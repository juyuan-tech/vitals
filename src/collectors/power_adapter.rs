//! Power Adapter：外接电源接上了没有。
//!
//! 数据源与 Battery 同一个目录，见 [`crate::collectors::power_supply`]，
//! 这里只认 `type` 为 `Mains` 的设备。
//!
//! 为什么不是每个适配器一行：一台机器上可能有两条 Mains（双口 USB-C 供电），
//! 但用户问的是「现在是不是插着电」这一个问题。所以聚合成一行——
//! 任何一个在线就是 `AC Connected`，都读得到且都不在线就是 `Disconnected`。
//!
//! `online` **读不到的设备不投票**（见 [`online_state`]）：接没接上是个是非题，
//! 「读不到」不是「没插电」，把它算成 `Disconnected` 就是拿不知道冒充知道。
//! 所有 Mains 设备的 `online` 都读不到时，整个模块报无数据。
//!
//! 注意这里与 fastfetch 的取舍不同：fastfetch 只报在线的适配器，而且要
//! `input_power_limit`（功耗上限）也在才认，于是**没插电时它什么都不显示**。
//! 而「没插电」本身就是有用的信息（电池在放电），所以这里照报。
//!
//! 一台 Mains 设备都没有（台式机、容器）→ 无数据。

use crate::collectors::power_supply::{self, Supply};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 配置里的键。
const KEY: &str = "Power Adapter";

/// 外接电源。
pub struct PowerAdapter;

impl Collector for PowerAdapter {
    fn name(&self) -> &'static str {
        "power-adapter"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let supplies = power_supply::supplies()?;

        // 一个 `online` 都读不到 → 无数据，不猜。
        let Some(online) = online_state(&supplies) else {
            return Ok(Vec::new());
        };

        let value = if online {
            "AC Connected"
        } else {
            "Disconnected"
        };

        Ok(vec![
            Info::new(self.name(), KEY, value).with_variable("online", online.to_string()),
        ])
    }
}

/// 聚合所有 Mains 设备的 `online`。
///
/// - 一个 Mains 设备都没有 → `None`（这台机器没有外接电源这回事）
/// - 有 Mains，但 `online` 全都读不到 → 也是 `None`（**无数据**：接没接上不知道，
///   不该报 `Disconnected` 让人以为在放电）
/// - 只要有一个读到了 → `Some(任一在线)`：双口供电的机器上，有一条接着电就是接着电
fn online_state(supplies: &[Supply]) -> Option<bool> {
    let known: Vec<bool> = supplies
        .iter()
        .filter(|supply| supply.is_mains())
        .filter_map(|supply| supply.online)
        .collect();

    (!known.is_empty()).then(|| known.iter().any(|online| *online))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个 Mains 设备。
    fn adapter(online: Option<bool>) -> Supply {
        Supply {
            name: "ADP1".to_owned(),
            kind: Some("Mains".to_owned()),
            online,
            ..Supply::default()
        }
    }

    #[test]
    fn a_single_readable_adapter_decides_the_answer() {
        assert_eq!(online_state(&[adapter(Some(true))]), Some(true));
        assert_eq!(online_state(&[adapter(Some(false))]), Some(false));
    }

    #[test]
    fn any_online_adapter_counts_as_ac_connected() {
        // 双口 USB-C：一条插着、一条空着，结果就是接着电。
        let state = online_state(&[adapter(Some(false)), adapter(Some(true))]);
        assert_eq!(state, Some(true));
    }

    #[test]
    fn an_unreadable_online_does_not_vote() {
        // 混着一条读不到的：结论由读得到的那条决定，而不是被它拖成 Disconnected。
        let state = online_state(&[adapter(None), adapter(Some(false))]);
        assert_eq!(state, Some(false), "读不到的既不投赞成也不投反对");
    }

    #[test]
    fn all_unreadable_means_no_data_not_disconnected() {
        // 「不知道」不等于「没插电」——这时该报无数据。
        assert_eq!(online_state(&[adapter(None)]), None);
        assert_eq!(online_state(&[adapter(None), adapter(None)]), None);
    }

    #[test]
    fn no_mains_device_at_all_is_no_data() {
        assert_eq!(online_state(&[]), None);

        // 电池或 USB 供电设备都不是适配器，哪怕它们有 online。
        let battery = Supply {
            name: "BAT0".to_owned(),
            kind: Some("Battery".to_owned()),
            online: Some(true),
            ..Supply::default()
        };
        assert_eq!(online_state(&[battery]), None);
    }

    #[test]
    fn reports_the_connected_state_on_this_machine_if_there_is_an_adapter() {
        let entries = PowerAdapter.collect(&Context::for_tests()).unwrap();

        // 台式机 / 容器里没有 Mains 设备（或没有 online），那就是无数据。
        match entries.first() {
            None => {}
            Some(info) => {
                assert_eq!(info.key, "Power Adapter");
                assert_eq!(info.module, "power-adapter");
                assert!(
                    info.value == "AC Connected" || info.value == "Disconnected",
                    "只有这两种取值，实际是 {}",
                    info.value
                );
            }
        }
    }

    #[test]
    fn the_value_matches_what_sysfs_says() {
        // 这条不是重复上面的断言：它把模块的输出与「直接读 /sys 聚合一遍」对一遍，
        // 免得接线接反了（读得到/读不到、在线/离线）却两边都自洽。
        let expected = online_state(&power_supply::supplies().unwrap());
        let entries = PowerAdapter.collect(&Context::for_tests()).unwrap();

        assert_eq!(
            entries.first().map(|info| info.value.as_str()),
            expected.map(|online| if online {
                "AC Connected"
            } else {
                "Disconnected"
            }),
        );
        assert_eq!(
            entries
                .first()
                .and_then(|info| info.variable("online"))
                .map(str::to_owned),
            expected.map(|online| online.to_string()),
        );
    }
}
