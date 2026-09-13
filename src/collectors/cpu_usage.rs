//! CPUUsage：整体 CPU 占用率。
//!
//! ## 数据来源与口径
//!
//! `/proc/stat` 的 `cpu...` 行是**自开机以来累计的 jiffies**，所以占用率同样只能
//! 靠两次采样作差。本模块照 upstream 的 `cpuusage_linux.c` 走，口径逐条如下：
//!
//! 1. **跳过第一行**。`cpu  a b c ...` 是所有核的合计，upstream 明确跳过它，
//!    只收 `cpu0`、`cpu1`… 每个核一行。
//! 2. 每行只取**前 7 个数**（`user nice system idle iowait irq softirq`），
//!    后面 `steal guest guest_nice` 不管（upstream 的 `sscanf` 正好 7 个转换）。
//! 3. `in_use = user + nice + system + irq + softirq`，`total = in_use + idle + iowait`。
//!    注意 `iowait` 算在 `total` 里但**不算** `in_use`——I/O 等待是「没在干活」。
//! 4. 每个核算出自己的百分比，最后取**算术平均**（`cpuusage.c` 里
//!    `avgValue = sumValue / valueCount`）。
//! 5. 印成整数百分比（`ffPercentAppendNum` 用显示配置的 `percentNdigits`，
//!    fastfetch 默认值就是 0 → `%.0f%%` → `18%`）。
//!
//! ### 「跳过第一行」和「直接用第一行」差多少
//!
//! 父指令里写的是「`/proc/stat` 第一行」，两种算法在**各核 total 增量相同**时
//! 完全等价。本机 16 核实测（同一窗口，两种算法各算一次，5 个窗口）：
//!
//! ```text
//! 逐核平均(upstream)  第一行聚合   差
//!      15.58%          16.36%    -0.78pp
//!      16.40%          15.48%    +0.92pp
//!      14.06%          13.93%    +0.13pp
//!      16.01%          15.53%    +0.49pp
//!      16.76%          16.31%    +0.45pp
//! ```
//!
//! 取整之后多数窗口一样，但确实出现过 16% / 15% 这种一位之差。既然目标是
//! 对齐 fastfetch，这里就按 upstream 的逐核平均来（也解释了这个模块为什么要
//! 逐行解析而不是只读一行）。
//!
//! ## 代价
//!
//! 两次采样之间睡 [`SAMPLE_WINDOW`]（200 ms），顺序调度下整体慢 200 ms。
//! upstream 这个模块的默认等待**也是 200 ms**（`cpuusage.c`:
//! `options->waitTime = 200`），所以这一处和 fastfetch 一致，不是妥协。
//!
//! ## 没做的那部分
//!
//! upstream 在「某个核的 total 没有增长」时会再等一轮、最多重试 3 次
//! （`retryCount <= 3`），仍然不行才报错。我们直接报错：重试会把等待时间
//! 变成不可控的 4 倍，和「等待必须有上限」的约定冲突。

use std::time::{Duration, Instant};

use crate::collectors::read;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 两次采样之间的固定窗口。见模块文档里的「代价」。
pub const SAMPLE_WINDOW: Duration = Duration::from_millis(200);

/// 累计 CPU 时间的来源。
const PROC_STAT: &str = "/proc/stat";

/// 整体 CPU 占用率。
pub struct CpuUsage;

impl Collector for CpuUsage {
    fn name(&self) -> &'static str {
        "cpu-usage"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(text) = read::text(PROC_STAT)? else {
            // 没有 /proc/stat（非 Linux）：无数据。
            return Ok(Vec::new());
        };
        let before = parse(&text);
        if before.is_empty() {
            return Ok(Vec::new());
        }

        let started = Instant::now();
        sleep_until(started, SAMPLE_WINDOW);
        // 与 upstream 同一时点：先量间隔，再读第二次。
        let elapsed = started.elapsed();

        let Some(text) = read::text(PROC_STAT)? else {
            return Err(CollectError::new(
                "/proc/stat 在采样窗口里消失了，量不到 CPU 占用率",
            ));
        };
        let after = parse(&text);

        let elapsed_ms = elapsed.as_millis() as u64;
        if elapsed_ms == 0 {
            return Err(CollectError::new(
                "cpu-usage 的采样间隔是 0 毫秒，算不出占用率",
            ));
        }

        if before.len() != after.len() {
            return Err(CollectError::new(format!(
                "/proc/stat 的核数在采样窗口里从 {} 变成了 {}（CPU 热插拔？），这次算不出占用率",
                before.len(),
                after.len()
            )));
        }

        let Some(percent) = usage_percent(&before, &after) else {
            return Err(CollectError::new(
                "/proc/stat 里有核的累计时间没有增长，这次算不出占用率",
            ));
        };

        Ok(vec![
            Info::new(self.name(), "CPU Usage", format_percent(percent))
                .with_variable("cores", before.len().to_string())
                .with_variable("percent", format!("{percent:.2}")),
        ])
    }
}

/// 一个核（或第一行的合计）在某次读取时的累计时间。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CpuTimes {
    /// `user + nice + system + irq + softirq`
    in_use: u64,
    /// `in_use + idle + iowait`
    total: u64,
}

/// 解析 `/proc/stat` 里的 `cpu` 行，返回**每个核**一条。
///
/// 第一行（`cpu `，注意那个空格）是合计，跳过；后面的 `cpuN` 按出现顺序收。
/// 遇到第一个不是 `cpuN` 的行（`intr`、`ctxt`…）就停——后面的行和 CPU 无关。
///
/// upstream 用的是 `sscanf("cpu%*d" + 7 个 %lu + "%*[^\n]\n")`——`%*d` 把**核号**
/// 丢掉、不赋值。本机 `cpu0 ...` 与 `cpu10 ...` 的核号位数还不一样，所以这个
/// 前缀必须单独吃掉：不丢的话核号会被当成 `user`，每个核的 `in_use` 都偏了。
///
/// 转换数不足 7 就 `break`（整个循环结束，不是跳过这一行）。这里同样：
/// 一行里不足 7 个数，就认为文件到此为止。
fn parse(text: &str) -> Vec<CpuTimes> {
    let mut cores = Vec::new();

    for line in text.lines() {
        let Some(rest) = line.strip_prefix("cpu") else {
            break;
        };
        if rest.starts_with(' ') {
            continue; // 合计行
        }

        let mut tokens = rest.split_whitespace();
        // 第一个 token 是核号（`cpu0` 的 `0`、`cpu10` 的 `10`），照 upstream 的 `%*d` 丢掉。
        if tokens.next().is_none() {
            break;
        }

        let fields: Vec<&str> = tokens.take(7).collect();
        if fields.len() < 7 {
            break;
        }

        let Some(numbers) = fields
            .iter()
            .take(7)
            .map(|field| field.parse::<u64>().ok())
            .collect::<Option<Vec<u64>>>()
        else {
            break;
        };

        let in_use = numbers[0] + numbers[1] + numbers[2] + numbers[5] + numbers[6];
        let total = in_use + numbers[3] + numbers[4];
        cores.push(CpuTimes { in_use, total });
    }

    cores
}

/// 逐核百分比再取算术平均（upstream 的两步）。
///
/// 任何一个核的 `total` 没有增长（`<=`）都返回 `None`：此时那一格的分母是 0，
/// upstream 会在这种情况下重试，我们直接认输。
fn usage_percent(before: &[CpuTimes], after: &[CpuTimes]) -> Option<f64> {
    if before.is_empty() || before.len() != after.len() {
        return None;
    }

    let mut sum = 0.0;
    for (old, new) in before.iter().zip(after) {
        let total = new.total.checked_sub(old.total)?;
        if total == 0 {
            return None;
        }

        let in_use = new.in_use.checked_sub(old.in_use)?;
        sum += in_use as f64 / total as f64 * 100.0;
    }

    Some(sum / before.len() as f64)
}

/// 把百分比印成 fastfetch 的样子。
///
/// `ffPercentAppendNum` 用的是显示配置里的 `percentNdigits`，fastfetch 默认 0，
/// 于是格式是 `%.0f%%`：没有小数、没有空格、带百分号，例如 `18%`。
/// Rust 的 `{:.0}` 与 C 的 `%.0f` 都是「就近取整、恰好半个走偶数」，所以直接用它。
fn format_percent(percent: f64) -> String {
    format!("{percent:.0}%")
}

/// 睡到距 `started` 满 `window` 为止。理由见 `net_io.rs` 里的同名函数。
fn sleep_until(started: Instant, window: Duration) {
    if let Some(rest) = window.checked_sub(started.elapsed()) {
        std::thread::sleep(rest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机 `/proc/stat` 的**前 4 行原文**（16 核，这里只抄前两核，其余省略——
    /// 单元测试只关心解析规则，不需要把 16 行都抄进来）。
    const PROC_STAT: &str = "\
cpu  15788096 1558 971857 51925257 124734 292376 120905 0 0 0
cpu0 495839 249 79244 3734969 11949 17522 31512 0 0 0
cpu1 431987 214 86491 3757989 8539 40644 14966 0 0 0
intr 76894404 17 9 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
";

    #[test]
    fn skips_the_aggregate_line_and_stops_at_intr() {
        let cores = parse(PROC_STAT);

        assert_eq!(cores.len(), 2, "只该有 cpu0、cpu1 两核");

        // cpu0 原文：495839 249 79244 3734969 11949 17522 31512
        //   in_use = user+nice+system+irq+softirq = 495839+249+79244+17522+31512
        //   total  = in_use + idle + iowait = in_use + 3734969 + 11949
        let in_use = 495_839 + 249 + 79_244 + 17_522 + 31_512;
        assert_eq!(cores[0].in_use, in_use);
        assert_eq!(cores[0].total, in_use + 3_734_969 + 11_949);

        // cpu1 原文：431987 214 86491 3757989 8539 40644 14966
        let in_use = 431_987 + 214 + 86_491 + 40_644 + 14_966;
        assert_eq!(cores[1].in_use, in_use);
        assert_eq!(cores[1].total, in_use + 3_757_989 + 8_539);
    }

    #[test]
    fn a_core_line_with_too_few_numbers_stops_the_parse() {
        // upstream 的 sscanf 转换数不足 7 时是整个循环 break（不是跳过这一行），
        // 所以第一行残缺就意味着一个核都收不到。
        let truncated = "\
cpu0 1 2 3
cpu1 1 2 3 4 5 6 7 8 9 0
";
        assert_eq!(parse(truncated).len(), 0);

        // 没有 cpu 行（空文件、或者文件系统不对）也是空。
        assert_eq!(parse("").len(), 0);
        assert_eq!(parse("intr 1 2 3\n").len(), 0);
    }

    #[test]
    fn the_core_number_is_not_mistaken_for_user_time() {
        // `cpu0 ...` 与 `cpu10 ...` 的核号位数不同，丢掉核号之后两行的第 1 个数
        // 才是 user。不丢的话 cpu10 那行会把 `10` 当成 user。
        let two_cores = "\
cpu0 7 0 0 100 0 0 0 0 0 0
cpu10 9 0 0 100 0 0 0 0 0 0
";
        let cores = parse(two_cores);
        assert_eq!(cores.len(), 2);
        assert_eq!(cores[0].in_use, 7);
        assert_eq!(cores[1].in_use, 9);
        assert_eq!(cores[0].total, 107);
    }

    #[test]
    fn the_percent_is_the_average_of_the_cores() {
        // 两个核：一个从 0/100 走到 100/200（用掉全部增量 → 100%），
        // 另一个从 0/100 走到 0/200（增量全是 idle → 0%）。
        // 平均 = 50%。
        let before = vec![
            CpuTimes {
                in_use: 0,
                total: 100,
            },
            CpuTimes {
                in_use: 0,
                total: 100,
            },
        ];
        let after = vec![
            CpuTimes {
                in_use: 100,
                total: 200,
            },
            CpuTimes {
                in_use: 0,
                total: 200,
            },
        ];

        assert_eq!(usage_percent(&before, &after), Some(50.0));
    }

    #[test]
    fn iowait_counts_as_idle() {
        // 一个核：in_use 没动，total 涨了 100（全是 iowait/idle）→ 0%。
        let before = vec![CpuTimes {
            in_use: 10,
            total: 100,
        }];
        let after = vec![CpuTimes {
            in_use: 10,
            total: 200,
        }];

        assert_eq!(usage_percent(&before, &after), Some(0.0));
    }

    #[test]
    fn a_shrinking_or_reshaped_cpu_set_is_not_computable() {
        let one = vec![CpuTimes {
            in_use: 0,
            total: 100,
        }];

        // 两次的核数不同（热插拔）。
        assert_eq!(usage_percent(&one, &[]), None);
        assert_eq!(usage_percent(&[], &one), None);

        // total 没增长（复用同一个采样）→ 分母为 0，算不出来。
        assert_eq!(usage_percent(&one, &one), None);

        // in_use 回退（理论上不该发生）→ 也算不出来。
        let backed = vec![CpuTimes {
            in_use: 0,
            total: 200,
        }];
        let weird = vec![CpuTimes {
            in_use: 50,
            total: 300,
        }];
        assert_eq!(usage_percent(&weird, &backed), None);
    }

    #[test]
    fn the_value_is_an_integer_percent() {
        // fastfetch 默认的 percentNdigits 是 0，所以是 "18%" 这种整数（没有空格）。
        assert_eq!(format_percent(18.4), "18%");
        assert_eq!(format_percent(18.6), "19%");
        // 本机实测的三个真值都落在整数上。
        assert_eq!(format_percent(0.0), "0%");
        assert_eq!(format_percent(100.0), "100%");
    }

    #[test]
    fn collects_on_this_machine() {
        // 会真的睡 200 ms —— 和 fastfetch 的默认等待一样长。
        let entries = CpuUsage.collect(&Context::for_tests()).unwrap();

        // 没有 /proc/stat（非 Linux）就是空的，也算通过。
        for info in &entries {
            assert_eq!(info.module, "cpu-usage");
            assert_eq!(info.key, "CPU Usage");
            assert!(info.value.ends_with('%'), "实际是 {}", info.value);
            assert!(
                info.variable("cores").is_some_and(|cores| cores != "0"),
                "核数该大于 0"
            );
        }
    }
}
