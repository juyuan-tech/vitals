//! Top：CPU 占用最高的几个进程。
//!
//! ## 数据来源
//!
//! 扫 `/proc/<pid>/`，每个进程读三个文件：
//!
//! | 文件 | 取什么 | 用途 |
//! |---|---|---|
//! | `stat` | `comm`（第 2 字段）、`flags`(9)、`utime`(14)、`stime`(15)、`starttime`(22) | 名字、CPU 时间、认人 |
//! | `status` | `VmRSS:` | 常驻内存 |
//! | `io` | `read_bytes:`、`write_bytes:` | 真实存储读写（不是页缓存） |
//!
//! 之所以读 `status` 而不是 upstream 用的 `statm`：`statm` 给的是**页数**，
//! 要用它必须知道页大小（upstream 从 `sysconf(_SC_PAGESIZE)` 拿，Rust 标准库
//! 没有这个 API）。而 `status` 里的 `VmRSS` 正是同一个计数器换算出来的
//! （内核 `pages_to_kb()`：页数 × 页大小 / 1024，打印单位 kB），
//! 乘回 1024 就是**逐字节相同**的值——本机拿 Python 参考实现两边对过，
//! MEM 列与 fastfetch 一致到两位小数。这样既不猜页大小，也不用 `unsafe`。
//!
//! ## 口径（照 `detection/top/top.c` 与 `top_linux.c`）
//!
//! - **跳过内核线程**：`stat` 第 9 字段 `flags` 里带 `PF_KTHREAD`(0x00200000) 的
//!   一律不要。本机实测 `kthreadd` 是 `2129984`、`kworker/*` 是 `69238880`，
//!   都含这一位——不跳的话 kworker 会挤掉真实进程。
//! - **CPU% 是「采样窗口内占满一个核」的百分比**，不是「按整个 uptime 平均」：
//!   `(utime + stime) 的增量（毫秒）/ 窗口毫秒 × 100`。所以一个多线程进程
//!   可以超过 100%（本机 `WebKitWebProces` 常年 96%～102%）。
//! - **MEM 取第二次快照的值**（内存是瞬时量，不需要差值）。
//! - **DSK** `(后 - 前) × 1000 / 窗口毫秒`，单核一样是整数截断。
//! - **配对靠 pid + `starttime`**：`starttime` 是「进程启动时刻（自开机起的
//!   jiffies）」，pid 被回收后重用的进程会换一个值。值变了、或者任何计数器
//!   回退了，都跳过这一个（不是错误）。
//! - **排序**：CPU% 降序，并列时 pid 升序（`compareCpuResults` 的 0 分支）。
//! - **只印前 5 个**（`options->nProcesses = 5`），键是 `Top Processes 1`…
//!   如果只剩 1 条就印不带编号的 `Top Processes`（`total == 1 ? 0 : index + 1`）。
//!
//! ## 代价
//!
//! 两次采样之间睡 [`SAMPLE_WINDOW`]（200 ms）。upstream 这个模块默认 **500 ms**
//! （`options->waitTime = 500`），我们按约定压到 200 ms——分母变小会让
//! CPU% 的抖动略大，但形状与口径不变。顺序调度下整体慢 200 ms。
//!
//! ## 读不到就跳过，不让整个模块失败
//!
//! 扫 `/proc` 的过程里进程随时会消失，别人的 `io` 也读不了：
//!
//! - 单个 pid 的目录/`stat` 读不了（`ENOENT` 或权限）→ **跳过这一个进程**，
//!   upstream 同样是 `continue`。
//! - `status` 读不了 → 跳过这一个进程（upstream 在 `statm` 读不了时也是跳过，
//!   因为它算不出 MEM）。
//! - `io` 读不了 → **读写都算 0**，进程照样出现在榜上（本机实测：别人的进程
//!   `cat /proc/1/io` 是「权限不够」，而 fastfetch 对这类进程也照样印
//!   `DSK 0 B/s / 0 B/s`）。
//!
//! ## 没做的那部分
//!
//! - 不读线程数（`stat` 第 20 字段）：默认输出里没有 `THR`，fastfetch 的默认
//!   `showTypes` 只含 CPU|MEMORY|DISK。
//! - 不实现按内存/磁盘/启动时间排序（upstream 的 `sort` 选项），也不实现
//!   `processes` 条数与 `waitTime` 的可配置——我们的模块没有配置块。

use std::time::{Duration, Instant};

use crate::collectors::{read, units};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 两次采样之间的固定窗口。见模块文档里的「代价」。
pub const SAMPLE_WINDOW: Duration = Duration::from_millis(200);

/// 进程表。
const PROC: &str = "/proc";

/// 榜单长度（upstream 的 `options->nProcesses = 5`）。
const N_PROCESSES: usize = 5;

/// `PF_KTHREAD`（内核线程标志，`include/linux/sched.h`）。
const PF_KTHREAD: u64 = 0x0020_0000;

/// 占用最高的进程。
pub struct Top;

impl Collector for Top {
    fn name(&self) -> &'static str {
        "top"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let before = snapshot();
        if before.is_empty() {
            // 读不到 /proc（非 Linux）或者一个进程都没有：无数据。
            return Ok(Vec::new());
        }

        let started = Instant::now();
        sleep_until(started, SAMPLE_WINDOW);
        // 与 upstream 同一时点：先量间隔，再读第二次。
        let elapsed = started.elapsed();

        let after = snapshot();
        let elapsed_ms = elapsed.as_millis() as u64;
        if elapsed_ms == 0 {
            return Err(CollectError::new(crate::i18n::now().top_zero_interval()));
        }

        let mut rows = Vec::new();
        for old in &before {
            // upstream 是靠「第二次快照里找 pid」配对的，找不到就跳过。
            let Some(new) = after.iter().find(|item| item.pid == old.pid) else {
                continue;
            };

            // starttime 不同 = pid 被回收后换了进程；计数器回退 = 进程重启过。
            // 这两种情况算出来的差值没有意义，跳过（upstream 同样是 continue）。
            if new.start_time != old.start_time
                || new.cpu_ms < old.cpu_ms
                || new.read_bytes < old.read_bytes
                || new.write_bytes < old.write_bytes
            {
                continue;
            }

            rows.push(Row {
                // 名字取**第一次**快照的（upstream 用的是 `oldItem->name`）。
                name: old.name.clone(),
                pid: new.pid,
                cpu_percent: (new.cpu_ms - old.cpu_ms) as f64 / elapsed_ms as f64 * 100.0,
                // 内存取**第二次**的：内存是瞬时量。
                mem_bytes: new.mem_bytes,
                read_rate: (new.read_bytes - old.read_bytes) * 1000 / elapsed_ms,
                write_rate: (new.write_bytes - old.write_bytes) * 1000 / elapsed_ms,
            });
        }

        // CPU% 降序；并列时 pid 升序（`compareCpuResults` 的平局分支）。
        rows.sort_by(|left, right| {
            right
                .cpu_percent
                .total_cmp(&left.cpu_percent)
                .then(left.pid.cmp(&right.pid))
        });
        rows.truncate(N_PROCESSES);

        let total = rows.len();
        Ok(rows
            .iter()
            .enumerate()
            .map(|(index, row)| row.describe(self.name(), total, index))
            .collect())
    }
}

/// 一个进程的一次快照。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    pid: u32,
    name: String,
    /// `utime + stime`，单位毫秒。
    cpu_ms: u64,
    mem_bytes: u64,
    read_bytes: u64,
    write_bytes: u64,
    /// `stat` 第 22 字段：自开机起的 jiffies。pid 重用会换值。
    start_time: u64,
}

/// 榜单上的一行。
#[derive(Debug, Clone, PartialEq)]
struct Row {
    name: String,
    pid: u32,
    cpu_percent: f64,
    mem_bytes: u64,
    read_rate: u64,
    write_rate: u64,
}

impl Row {
    /// 组装那一条信息。
    ///
    /// upstream 的默认输出（`printTopResult`，`outputFormat` 为空那一支）：
    ///
    /// ```text
    /// Top Processes 1: WebKitWebProces (1703) - CPU 96% - MEM 3.76 GiB - DSK 0 B/s / 0 B/s
    /// ```
    ///
    /// 编号规则是 `total == 1 ? 0 : index + 1`：只有一条时不带编号。
    fn describe(&self, module: &'static str, total: usize, index: usize) -> Info {
        let key = if total == 1 {
            "Top Processes".to_owned()
        } else {
            format!("Top Processes {}", index + 1)
        };

        let name = if self.name.is_empty() {
            "<unknown>".to_owned()
        } else {
            self.name.clone()
        };

        Info::new(
            module,
            key,
            format!(
                "{name} ({}) - CPU {:.0}% - MEM {} - DSK {}/s / {}/s",
                self.pid,
                self.cpu_percent,
                units::bytes(self.mem_bytes),
                units::bytes(self.read_rate),
                units::bytes(self.write_rate),
            ),
        )
        .with_variable("name", name)
        .with_variable("pid", self.pid.to_string())
        .with_variable("cpu-percent", format!("{:.2}", self.cpu_percent))
        .with_variable("mem", self.mem_bytes.to_string())
        .with_variable("disk-read", self.read_rate.to_string())
        .with_variable("disk-write", self.write_rate.to_string())
    }
}

/// 扫一遍 `/proc`。
///
/// 目录读不了（没有 /proc）→ 空表，与 `processes.rs` 的约定一致。
fn snapshot() -> Vec<Snapshot> {
    let Ok(entries) = std::fs::read_dir(PROC) else {
        return Vec::new();
    };

    let mut snapshots = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        // 只认纯数字目录；`self`、`net`、`meminfo` 这些都不是进程。
        let Ok(pid) = name.parse::<u32>() else {
            // `name.parse::<u32>()` 会接受 `+5`、`007` 这类写法，`/proc` 里不会出现，
            // 但真出现了也无所谓：pid 目录就是纯十进制数字。
            continue;
        };
        if pid == 0 {
            continue;
        }

        // 单个进程读不到（正好退出、权限）就跳过它，绝不让整个模块失败。
        let Some(fields) = read_stat(pid) else {
            continue;
        };

        // upstream 在 MEMORY 类型打开时读不到 statm 就跳过这个进程；我们同理：
        // 读不到 VmRSS 就跳过（内核线程的 status 里**没有** VmRSS 这一行，
        // 但它们早就被上面的 PF_KTHREAD 挡掉了）。
        let Some(mem_bytes) = read_mem(pid) else {
            continue;
        };

        // io 读不了不是错误：读写都算 0（本机实测别人的进程就是读不了）。
        let (read_bytes, write_bytes) = read_io(pid).unwrap_or((0, 0));

        snapshots.push(Snapshot {
            pid,
            name: fields.name,
            cpu_ms: fields.cpu_ms,
            mem_bytes,
            read_bytes,
            write_bytes,
            start_time: fields.start_time,
        });
    }

    snapshots
}

/// `stat` 里我们要的字段。
#[derive(Debug, Clone, PartialEq, Eq)]
struct StatFields {
    name: String,
    cpu_ms: u64,
    start_time: u64,
}

/// 读并解析 `/proc/<pid>/stat`。
///
/// 任何失败都返回 `None`（调用方跳过这个进程）。
fn read_stat(pid: u32) -> Option<StatFields> {
    let text = read::text(&format!("{PROC}/{pid}/stat")).ok()??;
    parse_stat(&text)
}

/// 解析 `/proc/<pid>/stat` 的一行。
///
/// 字段编号是**从 1 开始的整文件编号**（`pid` 是 1、`comm` 是 2）：
///
/// ```text
/// 1 pid  2 comm  3 state  4 ppid  5 pgrp  6 session  7 tty_nr  8 tpgid
/// 9 flags  14 utime  15 stime  20 num_threads  22 starttime
/// ```
///
/// `comm` 在括号里，而且**可以有空格和括号**——本机的 `(Isolated Web Co)`
/// 和 `((sd-pam))` 都是真实存在的。所以名字取「第一个 `(`」到「**最后**一个
/// `)`」之间（upstream 用的是 `memrchr`，找最后一个 `)`）。
fn parse_stat(text: &str) -> Option<StatFields> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    if close <= open {
        return None;
    }

    let name = text[open + 1..close].to_owned();

    // `close` 指向 ')'，其后的第一个字符是空格，字段从再下一个字符开始。
    // `close + 2` 之后就是第 3 个字段（state）。
    let rest: Vec<&str> = text[close + 1..].split_whitespace().collect();
    let field = |number: usize| -> Option<u64> {
        // 第 N 个字段（N >= 3）在 rest 里的下标是 N - 3。
        rest.get(number.checked_sub(3)?)?.parse().ok()
    };

    let flags = field(9)?;
    if flags & PF_KTHREAD != 0 {
        return None; // 内核线程：整条不要
    }

    let utime = field(14)?;
    let stime = field(15)?;
    let start_time = field(22)?;

    Some(StatFields {
        name,
        // `_SC_CLK_TCK` 在 Linux 上恒为 100（USER_HZ，2.6 起固定），
        // upstream 的 `sysconf(_SC_CLK_TCK)` 拿到的就是这个值，
        // 所以这里直接写死，省掉一个 sysconf。
        cpu_ms: (utime + stime) * 1000 / 100,
        start_time,
    })
}

/// 读 `/proc/<pid>/status` 里的 `VmRSS`，换算成字节。
///
/// 行是 `<tab>3957436 kB` 这种形状（本机实测），单位固定是 kB。
/// 找不到这一行（内核线程）或读不了 → `None`。
fn read_mem(pid: u32) -> Option<u64> {
    let text = read::text(&format!("{PROC}/{pid}/status")).ok()??;
    parse_mem(&text)
}

/// 从 `status` 的原文里取 `VmRSS`。
fn parse_mem(text: &str) -> Option<u64> {
    let line = text.lines().find(|line| line.starts_with("VmRSS:"))?;
    let value = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    Some(value * 1024)
}

/// 读 `/proc/<pid>/io` 的 `read_bytes` / `write_bytes`。
///
/// 这两个是**真实存储 I/O**（不是 `rchar`/`wchar` 那种含页缓存的计数），
/// 与 upstream 取的一样。读不了（权限）→ `None`，调用方按 0 处理。
fn read_io(pid: u32) -> Option<(u64, u64)> {
    let text = read::text(&format!("{PROC}/{pid}/io")).ok()??;
    parse_io(&text)
}

/// 从 `io` 的原文里取两个字段。
///
/// 本机原文（顺序固定，但我们不依赖顺序，按行找）：
///
/// ```text
/// rchar: 44311025
/// wchar: 40186288
/// syscr: 4588716
/// syscw: 5023286
/// read_bytes: 81354752
/// write_bytes: 0
/// cancelled_write_bytes: 0
/// ```
///
/// 缺 `read_bytes` 或 `write_bytes` 都返回 `None`（整条当读不到）。
fn parse_io(text: &str) -> Option<(u64, u64)> {
    let value = |key: &str| -> Option<u64> {
        let line = text.lines().find(|line| line.starts_with(key))?;
        line.split_whitespace().nth(1)?.parse().ok()
    };

    Some((value("read_bytes:")?, value("write_bytes:")?))
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

    /// 本机 `/proc/1703/stat` 的原文（WebKit 的 Web 内容进程，一次真实读取）。
    /// `utime`=1679108、`stime`=20673、`flags`=4194304、`starttime`=2121。
    const WEBKIT_STAT: &str = "1703 (WebKitWebProces) R 1626 1626 1626 0 -1 4194304 34781765 0 230 0 1679108 20673 0 0 20 0 36 0 2121 80927399936 997215 18446744073709551615 94394721773072 94394721773381 140729838183504 0 0 0 0 4096 8390208 0 0 0 17 6 0 0 0 0 0 94394721782080 94394721782088 94395212963840 140729838185569 140729838185615 140729838185615 140729838186447 0";

    /// 本机 `/proc/2/stat` 的原文（kthreadd，内核线程）。
    /// `flags`=2129984 = 0x208040，含 `PF_KTHREAD`。
    const KTHREADD_STAT: &str = "2 (kthreadd) S 0 0 0 0 -1 2129984 0 0 0 0 0 7 0 0 20 0 1 0 11 0 0 18446744073709551615 0 0 0 0 0 0 0 2147483647 0 0 0 0 0 9 0 0 0 0 0 0 0 0 0 0 0 0 0";

    /// 本机 `/proc/101/stat` 的原文（kworker，也是内核线程）。
    /// `flags`=69238880 = 0x4208060，同样含 `PF_KTHREAD`。
    const KWORKER_STAT: &str = "101 (kworker/11:0H-kblockd) I 2 0 0 0 -1 69238880 0 0 0 0 0 0 0 0 0 -20 1 0 22 0 0 18446744073709551615 0 0 0 0 0 0 0 2147483647 0 0 0 0 17 11 0 0 0 0 0 0 0 0 0 0 0 0 0";

    /// 本机 `/proc/1097/stat` 的原文——comm 自己带括号，`((sd-pam))`。
    /// `flags`=4194624 = 0x400140，**不含** `PF_KTHREAD`，所以它是个正经进程。
    const SD_PAM_STAT: &str = "1097 ((sd-pam)) S 1095 1095 1095 0 -1 4194624 58 0 0 0 0 0 0 0 20 0 1 0 1288 19935232 792 18446744073709551615 1 1 0 0 0 0 0 4096 0 0 0 0 17 2 0 0 0 0 0 0 0 0 0 0 0 0 0";

    /// 本机 `/proc/1703/status` 里我们要的那两行（原文带制表符）。
    const WEBKIT_STATUS: &str = "Name:\tWebKitWebProces\nUmask:\t0022\nState:\tR (running)\nVmRSS:\t 3988788 kB\nThreads:\t36\n";

    /// 本机 `/proc/1703/io` 的原文。
    const WEBKIT_IO: &str = "\
rchar: 44556574
wchar: 40412304
syscr: 4614614
syscw: 5051538
read_bytes: 81408000
write_bytes: 0
cancelled_write_bytes: 0
";

    #[test]
    fn parses_the_fields_upstream_uses() {
        let fields = parse_stat(WEBKIT_STAT).expect("这是一个普通进程");

        assert_eq!(fields.name, "WebKitWebProces");
        // (1679108 + 20673) jiffies * 1000 / 100 = 16997810 ms
        assert_eq!(fields.cpu_ms, 16_997_810);
        assert_eq!(fields.start_time, 2_121);
    }

    #[test]
    fn kernel_threads_are_skipped() {
        // 本机实测：kthreadd 的 flags 是 2129984（0x208000）、kworker 是
        // 69238880（0x4208000），都含 PF_KTHREAD(0x200000)。
        assert_eq!(parse_stat(KTHREADD_STAT), None);
        assert_eq!(parse_stat(KWORKER_STAT), None);

        // 反过来验一下这一位确实是判据：把 PF_KTHREAD 去掉就能解析出来。
        let not_a_thread = KTHREADD_STAT.replacen(" 2129984 ", " 29696 ", 1);
        assert_eq!(parse_stat(&not_a_thread).unwrap().name, "kthreadd");
    }

    #[test]
    fn the_name_may_contain_spaces_and_parentheses() {
        // 本机真实存在这两种：`Isolated Web Co` 带空格、`(sd-pam)` 自带括号。
        // 取「第一个 (」到「最后一个 )」之间，两种都对。
        assert_eq!(parse_stat(SD_PAM_STAT).unwrap().name, "(sd-pam)");

        let spaced = WEBKIT_STAT.replace("(WebKitWebProces)", "(Isolated Web Co)");
        assert_eq!(parse_stat(&spaced).unwrap().name, "Isolated Web Co");
    }

    #[test]
    fn a_malformed_stat_line_is_not_parsed() {
        assert_eq!(parse_stat(""), None);
        assert_eq!(parse_stat("1703 no parens here"), None);
        // 括号反着来。
        assert_eq!(parse_stat("1703 )oops( S 1 2 3"), None);
        // 字段不够（只有 state 与 ppid）。
        assert_eq!(parse_stat("1703 (short) S 1"), None);
    }

    #[test]
    fn reads_vm_rss_from_status() {
        // 本机实测 VmRSS = 3988788 kB → 字节数（约 3.80 GiB）。
        assert_eq!(parse_mem(WEBKIT_STATUS), Some(3_988_788 * 1024));
        // 内核线程的 status 里没有 VmRSS（本机 `grep VmRSS /proc/101/status` 为空）。
        assert_eq!(parse_mem("Name:\tkworker/11:0H-kblockd\nState:\tI\n"), None);
    }

    #[test]
    fn reads_physical_io_not_the_cached_counters() {
        // read_bytes/write_bytes 是真实存储 I/O；rchar/wchar 含页缓存，不算。
        // 本机原文里 rchar=44556574 而 read_bytes=81408000——取错了数会完全不同。
        assert_eq!(parse_io(WEBKIT_IO), Some((81_408_000, 0)));

        // 缺字段就读不到（调用方按 0 处理）。
        assert_eq!(parse_io("rchar: 1\nwchar: 2\n"), None);
        assert_eq!(parse_io(""), None);
    }

    #[test]
    fn rows_are_numbered_and_formatted_like_upstream() {
        let row = Row {
            name: "WebKitWebProces".to_owned(),
            pid: 1703,
            cpu_percent: 96.4,
            mem_bytes: 3_988_788 * 1024,
            read_rate: 0,
            write_rate: 0,
        };

        // 5 条里排在第一个：键带编号 1。
        let info = row.describe("top", 5, 0);
        assert_eq!(info.key, "Top Processes 1");
        assert_eq!(
            info.value,
            "WebKitWebProces (1703) - CPU 96% - MEM 3.80 GiB - DSK 0 B/s / 0 B/s"
        );
        assert_eq!(info.module, "top");
        assert_eq!(info.variable("pid"), Some("1703"));

        // 只有一条时不带编号。
        assert_eq!(row.describe("top", 1, 0).key, "Top Processes");

        // 名字为空时 upstream 会印 `<unknown>`。
        let unknown = Row {
            name: String::new(),
            ..row.clone()
        };
        assert!(
            unknown
                .describe("top", 1, 0)
                .value
                .starts_with("<unknown> (1703)")
        );
    }

    #[test]
    fn the_cpu_percent_is_the_share_of_one_core_over_the_window() {
        // 200 ms 的窗口里用掉 100 ms CPU → 50%；用满 200 ms → 100%；
        // 多线程超出一个核就是 150%。
        assert_eq!(percent(100, 200), 50.0);
        assert_eq!(percent(200, 200), 100.0);
        assert_eq!(percent(300, 200), 150.0);
        assert_eq!(percent(0, 200), 0.0);

        /// 与 `collect` 里那行算术同形（窗口毫秒固定为正）。
        fn percent(cpu_ms: u64, elapsed_ms: u64) -> f64 {
            cpu_ms as f64 / elapsed_ms as f64 * 100.0
        }
    }

    #[test]
    fn the_disk_rate_is_the_delta_over_the_window() {
        // 与 net_io/disk_io 同一套：`(后 - 前) * 1000 / 窗口毫秒`。
        // 本机 fastfetch 印过 "62.99 KiB/s"：64500 B/s，正好是 12900 字节 / 200 ms。
        assert_eq!((12_900_u64) * 1000 / 200, 64_500);
        assert_eq!(units::bytes(64_500), "62.99 KiB");
    }

    #[test]
    fn sorts_by_cpu_and_ties_break_on_pid() {
        let row = |pid: u32, cpu_percent: f64| Row {
            name: format!("p{pid}"),
            pid,
            cpu_percent,
            mem_bytes: 0,
            read_rate: 0,
            write_rate: 0,
        };

        let mut rows = vec![row(3, 10.0), row(1, 50.0), row(2, 10.0)];
        rows.sort_by(|left, right| {
            right
                .cpu_percent
                .total_cmp(&left.cpu_percent)
                .then(left.pid.cmp(&right.pid))
        });
        rows.truncate(N_PROCESSES);

        assert_eq!(
            rows.iter().map(|row| row.pid).collect::<Vec<_>>(),
            [1, 2, 3]
        );
    }

    #[test]
    fn another_users_process_keeps_zero_disk_rates() {
        // 本机实测：普通用户 `cat /proc/1/io` 得到「权限不够」（pid 1 是 root 的）。
        // upstream 对这种情况也是把两个计数当 0，进程照样上榜、DSK 印 0 B/s。
        // 这条支路在「没有别的用户的进程」时根本走不到，所以两种结局都要能过。
        let snapshots = snapshot();
        assert!(!snapshots.is_empty(), "/proc 里该有进程");

        let Some(init) = snapshots.iter().find(|item| item.pid == 1) else {
            return; // 容器里可能看不到 pid 1
        };

        if read_io(1).is_none() {
            // 读不到 → 必须是 0，而不是让整个模块失败。
            assert_eq!((init.read_bytes, init.write_bytes), (0, 0));
        }
    }

    #[test]
    fn collects_on_this_machine() {
        // 会真的睡 200 ms —— 本模块的既定代价。
        let entries = Top.collect(&Context::for_tests()).unwrap();

        // 没有 /proc（非 Linux）就是空的，也算通过。
        assert!(entries.len() <= N_PROCESSES, "{}", entries.len());

        for (index, info) in entries.iter().enumerate() {
            assert_eq!(info.module, "top");
            // 只有一条时 upstream 不编号（`total == 1 ? 0 : index + 1`），
            // 单进程的容器里就会走到那一支。
            if entries.len() == 1 {
                assert_eq!(info.key, "Top Processes");
            } else {
                assert_eq!(info.key, format!("Top Processes {}", index + 1));
            }
            assert!(info.value.contains(" - CPU "), "{}", info.value);
            assert!(info.value.contains("% - MEM "), "{}", info.value);
            assert!(info.value.contains(" - DSK "), "{}", info.value);
            assert!(info.value.ends_with("/s"), "{}", info.value);
            // pid 必须是纯数字（` (1234) `）。
            let pid = info.variable("pid").expect("该有 pid 变量");
            assert!(pid.parse::<u32>().is_ok(), "{pid}");
        }

        // 榜单必须是降序的——本机实测 fastfetch 也是这个顺序。
        let percents: Vec<f64> = entries
            .iter()
            .map(|info| info.variable("cpu-percent").unwrap().parse().unwrap())
            .collect();
        for pair in percents.windows(2) {
            assert!(pair[0] >= pair[1], "CPU% 该降序：{percents:?}");
        }
    }
}
