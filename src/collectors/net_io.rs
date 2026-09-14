//! NetIO：网卡的收发速率。
//!
//! ## 为什么要采两次
//!
//! `/sys/class/net/<网卡>/statistics/{rx,tx}_bytes` 是**开机至今的累计计数器**，
//! 读一次只能知道「一共收了多少字节」，那不是速率。要得到「每秒多少」，
//! 只能隔一段时间再读一次、拿差值除以间隔。本模块的全部算法就是这个。
//!
//! ## 代价（写在这里，不藏）
//!
//! 两次采样之间要睡 [`SAMPLE_WINDOW`]（200 ms），而 `core::dispatch` 是
//! **顺序**执行采集器的，所以只要这个模块出现在输出里，整体就慢 200 ms。
//!
//! upstream 的默认等待是 500 ms（`netio.c` 的 `options->waitTime = 500`），
//! 但它有一个「准备阶段」：`ffPrepareNetIO` 在任何一个模块开始打印**之前**
//! 就把第一张快照拍好，于是同一轮里几个采样型模块共用一个窗口
//! （本机实测 `fastfetch -s NetIO:DiskIO:CPUUsage:Top` 总共 512 ms，
//! 只跑 NetIO 也是 503 ms——四个模块并没有各等 500 ms）。
//! 我们没有准备阶段，四个模块会各等一次，所以把窗口压到 200 ms 作补偿。
//!
//! ## 为什么只报默认路由那一张网卡
//!
//! upstream 的 `defaultRouteOnly` **默认是 true**（`netio.c` 的
//! `ffInitNetIOOptions`），所以它只读 `ffNetifGetDefaultRouteV4()` 那一张。
//! 本机实测（`/sys/class/net` 全集）：
//!
//! | 网卡 | operstate | `device` 链接 | 是默认路由 |
//! |---|---|---|---|
//! | dae0 | up | 无 | |
//! | docker0 | up | 无 | |
//! | enp5s0f4u1u3c2 | up | 有 | ✓ |
//! | lo | unknown | 无 | |
//! | veth970a806 | up | 无 | |
//! | wlan0 | down | 有 | |
//!
//! 拿「`device` 链接在不在」当判据会选出 **enp5s0f4u1u3c2 与 wlan0 两张**
//! （wlan0 是物理网卡但此刻 down、也不是默认路由），比 fastfetch 多一行；
//! 只拿 `operstate` 当判据会选出 **5 张**（`lo` 的 operstate 是 `unknown`，
//! 首字节也是 `u`）。真正让它只剩一行的就是 `defaultRouteOnly`。
//! 所以本模块两步都做：先取默认路由那张网卡，再按 upstream 的
//! `operstate` 判据（首字节 `u`）确认它确实是 up。
//!
//! ## 与 upstream 的结构差异
//!
//! 默认路由的查找在 upstream 里是共用的 `common/netif`，而这里的
//! 默认出口网卡的解析在 [`crate::collectors::routing`]：`local-ip` 要用同一份判断，
//! 两边各写一份、改一处忘一处，迟早会对不上。
//! 抽公共件时这两处是第一候选；本次不动 `local_ip.rs`。

use std::time::{Duration, Instant};

use crate::collectors::{read, routing, units};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 两次采样之间的固定窗口。
///
/// 模块文档里的「代价」说的就是它：出现本模块 → 整体慢这么多。
pub const SAMPLE_WINDOW: Duration = Duration::from_millis(200);

/// 默认路由表。
const ROUTE: &str = "/proc/net/route";

/// 网络吞吐。
pub struct NetIo;

impl Collector for NetIo {
    fn name(&self) -> &'static str {
        "net-io"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(route) = read::text(ROUTE)? else {
            // 连路由表都没有（内核没开 IPv4 路由），无数据。
            return Ok(Vec::new());
        };
        let Some(interface) = routing::default_route_interface(&route) else {
            // 没有默认路由 = 没有要报的网卡。
            return Ok(Vec::new());
        };

        // upstream 打开网卡目录后第一件事就是读 `operstate`，只认首字节 `u`
        // （`up` 和 `unknown` 都算）。读不到这个文件说明这张网卡刚消失，无数据。
        let Some(state) = read::text(&sys_net(&interface, "operstate"))? else {
            return Ok(Vec::new());
        };
        if !is_usable(&state) {
            return Ok(Vec::new());
        }

        let Some(before) = counters(&interface)? else {
            // 没有 statistics 就不是我们能测的网卡；**不印 0 B/s 充数**。
            return Ok(Vec::new());
        };

        let started = Instant::now();
        sleep_until(started, SAMPLE_WINDOW);
        // 与 upstream 同一时点：先量间隔，再读第二次。
        // 间隔因此只包含等待，不包含第二次读取的耗时（`ffDetectNetIO` 里
        // `time2` 也取在第二次 `ffNetIOGetIoCounters` 之前）。
        let elapsed = started.elapsed();

        let after = counters(&interface)?
            .ok_or_else(|| CollectError::new(crate::i18n::now().interface_gone(&interface)))?;

        let elapsed_ms = elapsed.as_millis() as u64;
        if elapsed_ms == 0 {
            return Err(CollectError::new(crate::i18n::now().net_io_zero_interval()));
        }

        let rx_rate = rate(before.rx_bytes, after.rx_bytes, elapsed_ms)
            .ok_or_else(|| CollectError::new(crate::i18n::now().rx_regressed(&interface)))?;
        let tx_rate = rate(before.tx_bytes, after.tx_bytes, elapsed_ms)
            .ok_or_else(|| CollectError::new(crate::i18n::now().tx_regressed(&interface)))?;

        Ok(vec![
            describe(
                self.name(),
                &interface,
                after.rx_bytes,
                after.tx_bytes,
                rx_rate,
                tx_rate,
            )
            .with_variable("elapsed-ms", elapsed_ms.to_string()),
        ])
    }
}

/// 拼出 sysfs 里某个网卡下的路径。
fn sys_net(interface: &str, leaf: &str) -> String {
    format!("/sys/class/net/{interface}/{leaf}")
}

/// `operstate` 是否算「能测」。
///
/// upstream 的做法是读 1 个字节、比 `'u'`（`netio_linux.c` 里的
/// `operstate != 'u'` 直接返回）。Linux 的取值里以 `u` 开头的只有
/// `up` 和 `unknown`，所以这个判据比它看起来要宽：**lo 也会通过**
/// （本机 `lo` 的 operstate 就是 `unknown`）。
fn is_usable(operstate: &str) -> bool {
    operstate.starts_with('u')
}

/// 一张网卡的累计收发字节数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Counters {
    rx_bytes: u64,
    tx_bytes: u64,
}

/// 读一张网卡的累计计数器。
///
/// 两个文件都是 sysfs 里的十进制整数：
///
/// - 文件不存在 → `Ok(None)`（无数据）
/// - 文件在、内容不是数字 → `Err`（这是解析失败，不能装作没有）
fn counters(interface: &str) -> Result<Option<Counters>, CollectError> {
    let Some(rx) = read::text(&sys_net(interface, "statistics/rx_bytes"))? else {
        return Ok(None);
    };
    let Some(tx) = read::text(&sys_net(interface, "statistics/tx_bytes"))? else {
        return Ok(None);
    };

    let parse = |what: &str, text: &str| {
        text.parse::<u64>().map_err(|source| {
            CollectError::caused_by(
                crate::i18n::now().counter_parse_failed(interface, what, text),
                source,
            )
        })
    };

    Ok(Some(Counters {
        rx_bytes: parse("rx_bytes", &rx)?,
        tx_bytes: parse("tx_bytes", &tx)?,
    }))
}

/// `(后 - 前) * 1000 / 间隔毫秒`。
///
/// 与 upstream 的 `(*currValue - *prevValue) * 1000 / (time2 - time1)` 同一套算术，
/// 包括那个**整数截断**——多出的零头就是不要了，这样和 fastfetch 的数对得上。
///
/// 返回 `None` 表示计数回退了（`u64` 减法会下溢）或间隔为 0，两种情况都算不出来。
fn rate(before: u64, after: u64, elapsed_ms: u64) -> Option<u64> {
    if elapsed_ms == 0 {
        return None;
    }

    let delta = after.checked_sub(before)?;
    Some(delta * 1000 / elapsed_ms)
}

/// 组装那一条信息。
///
/// upstream 的格式（`modules/netio/netio.c` 里 `detectTotal == false` 的那一支）：
/// `"<rx>/s (IN) - <tx>/s (OUT)"`，键是 `"Network I/O (<网卡>)"`。
fn describe(
    module: &'static str,
    interface: &str,
    rx_bytes: u64,
    tx_bytes: u64,
    rx_rate: u64,
    tx_rate: u64,
) -> Info {
    Info::new(
        module,
        format!("Network I/O ({interface})"),
        format!(
            "{}/s (IN) - {}/s (OUT)",
            units::bytes(rx_rate),
            units::bytes(tx_rate)
        ),
    )
    .with_variable("ifname", interface.to_owned())
    .with_variable("rx-bytes", rx_bytes.to_string())
    .with_variable("tx-bytes", tx_bytes.to_string())
    .with_variable("rx-rate", rx_rate.to_string())
    .with_variable("tx-rate", tx_rate.to_string())
}

/// 睡到距 `started` 满 `window` 为止。
///
/// upstream 是 `while (now - t1 < waitTime) sleep(...)` 的循环；这里一次睡掉剩下的
/// 时间就够了：`std::thread::sleep` 保证**不会早于**给定时长返回
/// （可能更晚），所以量到的间隔只会 ≥ 200 ms。
fn sleep_until(started: Instant, window: Duration) {
    if let Some(rest) = window.checked_sub(started.elapsed()) {
        std::thread::sleep(rest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机 `/proc/net/route` 的原文（制表符照抄，表头也留着）。
    #[test]
    fn the_operstate_rule_keeps_up_and_unknown() {
        // 本机 `/sys/class/net/*/operstate` 的原文：dae0/docker0/enp5s0f4u1u3c2/
        // veth970a806 是 "up"，lo 是 "unknown"，wlan0 是 "down"。
        // upstream 只读 1 个字节、只比 'u'，所以 `lo` 这种 "unknown" 也算「可用」。
        assert!(is_usable("up"));
        assert!(is_usable("unknown"));
        assert!(!is_usable("down"));

        // 这条判据单独用是**不够**的：本机 6 张网卡里有 5 张（上面那 5 张）通过，
        // 只有 down 掉的 wlan0 被挡下。真正把输出压到一行的是「默认路由」
        // ——见模块文档里的那张表。
        let all = [
            "dae0",
            "docker0",
            "enp5s0f4u1u3c2",
            "lo",
            "veth970a806",
            "wlan0",
        ];
        let states = ["up", "up", "up", "unknown", "up", "down"];
        let passing = states.iter().filter(|state| is_usable(state)).count();
        assert_eq!(passing, 5, "本机会有 5 张网卡通过 operstate 判据");
        assert_eq!(all.len(), 6);
    }

    #[test]
    fn the_rate_is_the_delta_over_the_window() {
        // 500 ms 里多了 1_000_000 字节 → 每秒 2_000_000。
        assert_eq!(rate(1_000_000, 2_000_000, 500), Some(2_000_000));
        // 没流量就是 0，不是错误。
        assert_eq!(rate(7, 7, 200), Some(0));
        // 整数截断（和 upstream 一样）：3 字节 / 200 ms = 15 B/s。
        assert_eq!(rate(0, 3, 200), Some(15));
        // 计数回退 / 间隔为 0 → 算不出来。
        assert_eq!(rate(5, 4, 200), None);
        assert_eq!(rate(0, 1, 0), None);
    }

    #[test]
    fn the_value_is_formatted_like_upstream() {
        // 用本机实测过的量级：fastfetch 印的 "29.03 KiB/s" 对应 29727 B/s
        // （29727 / 1024 = 29.030…，两位小数就是 29.03）。
        let info = describe("net-io", "enp5s0f4u1u3c2", 14195589432, 999, 29_727, 4_876);

        assert_eq!(info.value, "29.03 KiB/s (IN) - 4.76 KiB/s (OUT)");
        assert_eq!(info.key, "Network I/O (enp5s0f4u1u3c2)");
        assert_eq!(info.module, "net-io");
        assert_eq!(info.variable("ifname"), Some("enp5s0f4u1u3c2"));
        assert_eq!(info.variable("rx-bytes"), Some("14195589432"));
    }

    #[test]
    fn nothing_to_measure_is_not_an_error() {
        // 零字节也是合法输出（网卡挂着但没流量）。
        let info = describe("net-io", "eth0", 0, 0, 0, 0);
        assert_eq!(info.value, "0 B/s (IN) - 0 B/s (OUT)");
    }

    #[test]
    fn collects_on_this_machine() {
        // 会真的睡 200 ms —— 这是本模块的既定代价，见模块文档。
        let entries = NetIo.collect(&Context::for_tests()).unwrap();

        // 没有默认路由（纯 IPv6、容器里没网卡）就是空的，也算通过。
        for info in &entries {
            assert_eq!(info.module, "net-io");
            assert!(
                info.key.starts_with("Network I/O (") && info.key.ends_with(')'),
                "实际是 {}",
                info.key
            );
            assert!(info.value.contains("/s (IN) - "), "{}", info.value);
            assert!(info.value.ends_with("/s (OUT)"), "{}", info.value);
        }
    }
}
