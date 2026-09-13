//! Wi-Fi：无线网卡的状态与信号。
//!
//! 判「哪些接口是无线」看 `/sys/class/net/<接口>/wireless/` 这个目录在不在——
//! 比看名字（`wlan*`）靠谱，也比 `iw` 省一个子进程。状态取 `operstate`：
//! `down`（没启用）、`dormant`（启用了但没连上，即载波为 0）、`up`（已连接）。
//! 信号质量与电平（dBm）在 `/proc/net/wireless` 里，**但只有连上时才有数据行**：
//! 本机 wlan0 是 `down`，那个文件只有一个表头，所以 fastfetch 也只印得出 `Wi-Fi: down`。
//!
//! **报不出 SSID**：它在 cfg80211/nl80211 里，要么走 netlink（自己实现一套协议）、
//! 要么调 `iw`（子进程），两条都不走。连上时我们给的是「状态 + 质量 + 电平」，
//! 比 fastfetch 少一个网络名——这是这一行诚实的边界，写在 `variables` 里的 `interface`
//! 至少让人知道说的是哪块网卡。
//!
//! 一块无线网卡都没有（台式机走网线）→ 无数据，不报。

use crate::collectors::read;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 网卡目录。
const NET: &str = "/sys/class/net";

/// 无线统计（只有已连接时才有一行数据）。
const WIRELESS: &str = "/proc/net/wireless";

/// Wi-Fi。
pub struct Wifi;

impl Collector for Wifi {
    fn name(&self) -> &'static str {
        "wifi"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let interfaces = interfaces()?;
        if interfaces.is_empty() {
            return Ok(Vec::new());
        }

        let stats = match read::text(WIRELESS)? {
            Some(text) => parse_stats(&text),
            None => Vec::new(),
        };

        let total = interfaces.len();
        let mut entries = Vec::new();

        for interface in interfaces {
            let state = read::text(&format!("{NET}/{interface}/operstate"))?
                .map(|text| text.trim().to_owned())
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| "unknown".to_owned());

            let stat = stats.iter().find(|stat| stat.interface == interface);
            let info = Info::new(self.name(), key(&interface, total), value(&state, stat))
                .with_variable("interface", interface.clone());

            let info = match stat {
                Some(stat) => info
                    .with_variable("quality", stat.quality.to_string())
                    .with_variable("level", format!("{} dBm", stat.level)),
                None => info,
            };

            entries.push(info);
        }

        Ok(entries)
    }
}

/// 键：只有一块无线网卡时就是 `Wi-Fi`（fastfetch 也是这么印的），
/// 多块时带上网卡名，否则两行长得一模一样。
fn key(interface: &str, total: usize) -> String {
    if total > 1 {
        format!("Wi-Fi ({interface})")
    } else {
        "Wi-Fi".to_owned()
    }
}

/// 值：状态，连上时补上质量与电平。
fn value(state: &str, stat: Option<&Stat>) -> String {
    match stat {
        Some(stat) => format!("{state} ({}%, {} dBm)", stat.quality, stat.level),
        None => state.to_owned(),
    }
}

/// 一块无线网卡的统计。
#[derive(Debug, Clone, PartialEq)]
struct Stat {
    interface: String,
    /// 链接质量，0 到 70（内核的 `link` 列，表头里写着上限 70）。
    quality: u32,
    /// 信号电平，dBm，通常是负数。
    level: i32,
}

/// `/sys/class/net` 下的无线接口，按名字排序。
fn interfaces() -> Result<Vec<String>, CollectError> {
    let Ok(entries) = std::fs::read_dir(NET) else {
        return Ok(Vec::new());
    };

    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().join("wireless").is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .collect();
    names.sort();

    Ok(names)
}

/// 解析 `/proc/net/wireless`。
///
/// 形状（前两行是表头，列的对应关系看表头自己）：
///
/// ```text
/// Inter-| sta-|   Quality        |   Discarded packets               | Missed | WE
///  face | tus | link level noise |  nwid  crypt   frag  retry   misc | beacon | 22
/// wlan0: 0000   70.  -40.  -256        0      0      0      0      0        0
/// ```
///
/// 内核把 `link`、`level`、`noise` 打成**带小数点的浮点**（`70.`、`-40.`），
/// 末尾那个点要去掉才解得出来。解不出来的行直接跳过：内核加列是正常的，
/// 没必要为一行看不懂的数字让整个模块失败。
fn parse_stats(text: &str) -> Vec<Stat> {
    let mut stats = Vec::new();

    for line in text.lines() {
        let Some((interface, rest)) = line.split_once(':') else {
            continue;
        };
        let interface = interface.trim();
        // 表头里有 `Inter-| sta-|` 这种带冒号的怪东西，接口名不该带空格或竖线。
        if interface.is_empty() || interface.contains(' ') || interface.contains('|') {
            continue;
        }

        let mut fields = rest.split_whitespace();
        let _status = fields.next();
        // 质量是个非负计数；负数（内核不会给，但别信内核）当解析失败跳过。
        let Some(quality) = fields
            .next()
            .and_then(number)
            .filter(|value| *value >= 0)
        else {
            continue;
        };
        let Some(level) = fields.next().and_then(number) else {
            continue;
        };

        stats.push(Stat {
            interface: interface.to_owned(),
            quality: quality as u32,
            level,
        });
    }

    stats
}

/// 解一个内核打的浮点列：`70.` → 70、`-40.` → -40。
fn number(field: &str) -> Option<i32> {
    field.trim_end_matches('.').parse::<f64>().ok().map(|value| value as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIRELESS: &str = "\
Inter-| sta-|   Quality        |   Discarded packets               | Missed | WE
 face | tus | link level noise |  nwid  crypt   frag  retry   misc | beacon | 22
 wlan0: 0000   70.  -40.  -256        0      0      0      0      0        0
";

    #[test]
    fn parses_the_kernels_float_columns() {
        assert_eq!(
            parse_stats(WIRELESS),
            [Stat {
                interface: "wlan0".to_owned(),
                quality: 70,
                level: -40,
            }]
        );
    }

    #[test]
    fn a_header_only_file_has_no_stats() {
        // 本机的真实情况：wlan0 是 down，这个文件只有表头。
        let header = WIRELESS.lines().take(2).collect::<Vec<_>>().join("\n");
        assert!(parse_stats(&header).is_empty());
    }

    #[test]
    fn skips_lines_that_do_not_make_sense() {
        assert!(parse_stats("").is_empty());
        assert!(parse_stats("garbage\n").is_empty());
        assert!(parse_stats("wlan0: 0000\n").is_empty(), "列不够就跳过");
        assert!(parse_stats("wlan0: 0000 abc def\n").is_empty());
    }

    #[test]
    fn keys_are_numbered_only_when_there_are_several() {
        assert_eq!(key("wlan0", 1), "Wi-Fi");
        assert_eq!(key("wlan0", 2), "Wi-Fi (wlan0)");
    }

    #[test]
    fn values_carry_the_state_and_the_signal() {
        assert_eq!(value("down", None), "down");
        assert_eq!(
            value(
                "up",
                Some(&Stat {
                    interface: "wlan0".to_owned(),
                    quality: 70,
                    level: -40,
                })
            ),
            "up (70%, -40 dBm)"
        );
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Wifi.collect(&Context::for_tests()).unwrap();

        // 本机有无线网卡（且是 down）；台式机没有则两种结局都通过。
        for info in &entries {
            assert_eq!(info.module, "wifi");
            assert!(info.key.starts_with("Wi-Fi"), "实际是 {}", info.key);
            assert!(!info.value.is_empty());
        }
    }
}
