//! Battery：笔记本电池的电量与状态。
//!
//! 数据源是 `/sys/class/power_supply/`（见 [`crate::collectors::power_supply`]），
//! 只认 `type` 为 `Battery` 的设备：`Mains` 归 Power Adapter，`USB`/`Wireless`
//! 这些是外设或充电头，报出来只会让人以为笔记本有两块电池。
//!
//! 键里的型号优先取 `model_name`（`Battery (WE04068XL)`），它才是用户认得出的
//! 那个名字；没有型号时退回 sysfs 设备名（`Battery (BAT0)`），总比两块电池
//! 都印成 `Battery` 强。
//!
//! 电量优先用 `capacity`——内核算好的，和 `upower` 的口径一致；没有才拿
//! `energy_now` / `energy_full` 自己除。fastfetch 是**没有 `capacity` 就整个不报**的，
//! 而这类固件（只给能量值）其实有数可报，所以这里做了回退。
//!
//! `status` 里只有三个值会印出来，`Full` 不在其中：电池满 + 接着电源时，
//! 同一个事实已经在 `AC Connected` 里说过了，再印一遍 `Full` 是同一句话说两遍
//! （fastfetch 的状态位里也没有 `Full`）。于是本机满电接着电源的结果就是
//! `100% [AC Connected]`，放电时是 `76% [Discharging]`。
//!
//! 光看 `type` 还不够，还要两道过滤（fastfetch 也有，理由见 [`is_machine_battery`]）：
//! `scope` 为 `Device` 的外设电池、`present` 为 `0` 的空电池仓都不算这台机器的电池。
//!
//! 台式机没有电池 → 无数据。

use crate::collectors::power_supply::{self, Supply};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// `status` 里值得单独印出来的值，按显示顺序。
///
/// 缺 `Full` 与 `Not charging` 的理由见模块文档：它们描述的「接着电源但没在充」
/// 已经由 `AC Connected` 表达，多印一个词只是噪音。
const STATUS: [&str; 3] = ["Charging", "Discharging", "Unknown"];

/// 电池。
pub struct Battery;

impl Collector for Battery {
    fn name(&self) -> &'static str {
        "battery"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let supplies = power_supply::supplies()?;
        let ac = power_supply::ac_connected(&supplies);

        // 一个属性都没有的「电池」（只有 type 文件）不该产出空行，
        // 所以在这里就把报不出来的滤掉，而不是等渲染器替我们判空。
        Ok(supplies
            .iter()
            .filter(|supply| is_machine_battery(supply))
            .filter_map(|supply| self.info(supply, ac))
            .collect())
    }
}

/// 这台设备算不算「机器的电池」。
///
/// 除了 `type` 为 `Battery`，还要排掉两类**读得到才判**的干扰项：
///
/// - `scope == "Device"`：外设自己的电池。蓝牙鼠标、键盘、耳机都会在
///   `/sys/class/power_supply/` 下注册一个 `type=Battery` 的设备，不排掉的话
///   用户会以为笔记本多了一块电池，`--json | jq` 的脚本也会多数一行。
/// - `present == 0`：可拆电池的空仓。设备节点还在，但里面没有电池，
///   报出来就是一块「0% 的电池」。
///
/// 两个文件**读不到时不跳过**：老内核根本不提供它们，因为读不到就把真电池
/// 扔掉，比多报一个外设严重得多。
fn is_machine_battery(supply: &Supply) -> bool {
    supply.is_battery()
        && supply.scope.as_deref() != Some("Device")
        && supply.present != Some(false)
}

impl Battery {
    /// 把一块电池变成一条信息。没有任何可报内容时返回 `None`。
    fn info(&self, supply: &Supply, ac: bool) -> Option<Info> {
        let mut parts: Vec<&str> = Vec::new();
        if ac {
            parts.push("AC Connected");
        }
        if let Some(status) = supply.status.as_deref() {
            if let Some(known) = STATUS.iter().find(|known| **known == status) {
                parts.push(known);
            }
        }

        let value = match (supply.percentage(), parts.is_empty()) {
            (Some(percent), false) => format!("{percent}% [{}]", parts.join(", ")),
            (Some(percent), true) => format!("{percent}%"),
            // 连百分比都没有的机器（只有 status）就整行只印状态：这时加方括号反而
            // 让人去找它前面本该有的那个数。fastfetch 也是这个取舍。
            (None, false) => parts.join(", "),
            (None, true) => return None,
        };

        let device = supply.name.clone();
        let mut info = Info::new(self.name(), key(supply), value)
            .with_variable("device", device)
            .with_variable("ac_connected", ac.to_string());

        if let Some(percent) = supply.percentage() {
            info = info.with_variable("capacity", percent.to_string());
        }
        if let Some(status) = supply.status.as_deref() {
            info = info.with_variable("status", status);
        }

        Some(info)
    }
}

/// 显示键：`Battery (型号)`。
fn key(supply: &Supply) -> String {
    match supply.model_name.as_deref() {
        Some(model) => format!("Battery ({model})"),
        None => format!("Battery ({})", supply.name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一块电池。
    fn battery(fields: impl FnOnce(&mut Supply)) -> Supply {
        let mut supply = Supply {
            name: "BAT0".to_owned(),
            kind: Some("Battery".to_owned()),
            ..Supply::default()
        };
        fields(&mut supply);
        supply
    }

    #[test]
    fn the_model_name_goes_in_the_key() {
        let supply = battery(|supply| supply.model_name = Some("WE04068XL".to_owned()));
        assert_eq!(key(&supply), "Battery (WE04068XL)");
    }

    #[test]
    fn a_battery_without_a_model_falls_back_to_the_device_name() {
        // 直接印成 `Battery` 的话，两台电池会撞成一个键。
        assert_eq!(key(&battery(|_| {})), "Battery (BAT0)");
    }

    // -----------------------------------------------------------------------
    // 两道过滤：fixture 直接给出 sysfs 里会读到的那几个值，不依赖真机
    // -----------------------------------------------------------------------

    #[test]
    fn a_peripheral_battery_is_not_the_machines_battery() {
        // 蓝牙鼠标的电池就是长这样：type=Battery、scope=Device。
        let mouse = battery(|supply| {
            supply.name = "hid-00:1a:7d:da:71:13-battery".to_owned();
            supply.scope = Some("Device".to_owned());
            supply.capacity = Some(80);
        });

        assert!(!is_machine_battery(&mouse), "外设电池不该混进 battery 模块");
    }

    #[test]
    fn a_system_scoped_battery_is_kept() {
        // 笔记本电池的 scope 是 System（有的内核干脆不给这个文件）。
        let system = battery(|supply| supply.scope = Some("System".to_owned()));
        assert!(is_machine_battery(&system));
    }

    #[test]
    fn a_battery_that_is_not_present_is_skipped() {
        // `present=0`：可拆电池的空仓，设备节点在但里面没电池。
        let empty_bay = battery(|supply| {
            supply.present = Some(false);
            supply.capacity = Some(0);
        });

        assert!(!is_machine_battery(&empty_bay));
        assert!(is_machine_battery(&battery(
            |supply| supply.present = Some(true)
        )));
    }

    #[test]
    fn a_battery_without_scope_or_present_is_kept() {
        // 老内核这两个文件都没有。读不到 ≠ 不是电池——扔真电池比多报一个严重。
        assert!(is_machine_battery(&battery(|_| {})));

        // 半读得到也一样：只有 present 没有 scope，或反过来。
        let only_present = battery(|supply| supply.present = Some(true));
        let only_scope = battery(|supply| supply.scope = Some("System".to_owned()));

        assert!(is_machine_battery(&only_present));
        assert!(is_machine_battery(&only_scope));
    }

    #[test]
    fn a_non_battery_is_never_reported() {
        // 过滤的入口条件还是 type：Mains/USB 设备即使 scope 与 present 都正常也不行。
        let mut mains = battery(|supply| supply.present = Some(true));
        mains.kind = Some("Mains".to_owned());
        assert!(!is_machine_battery(&mains));
    }

    #[test]
    fn a_full_battery_on_ac_is_reported_as_ac_connected() {
        // 本机的真实组合：capacity=100、status=Full、ADP1 在线。
        let supply = battery(|supply| {
            supply.capacity = Some(100);
            supply.status = Some("Full".to_owned());
        });

        let info = Battery.info(&supply, true).expect("有数据");
        assert_eq!(info.key, "Battery (BAT0)");
        assert_eq!(info.value, "100% [AC Connected]");
        assert_eq!(info.variable("capacity"), Some("100"));
        assert_eq!(info.variable("status"), Some("Full"), "原始状态还是留着");
    }

    #[test]
    fn discharging_on_battery_lists_the_status() {
        let supply = battery(|supply| {
            supply.capacity = Some(76);
            supply.status = Some("Discharging".to_owned());
        });

        assert_eq!(
            Battery.info(&supply, false).unwrap().value,
            "76% [Discharging]"
        );
    }

    #[test]
    fn charging_is_listed_after_ac_connected() {
        // fastfetch 的顺序是「先电源、后状态」，照抄它。
        let supply = battery(|supply| {
            supply.capacity = Some(42);
            supply.status = Some("Charging".to_owned());
        });

        assert_eq!(
            Battery.info(&supply, true).unwrap().value,
            "42% [AC Connected, Charging]"
        );
    }

    #[test]
    fn a_battery_without_any_numbers_shows_only_its_status() {
        // 这种机器上「没有百分比」是事实，印 `0%` 就是编。
        let supply = battery(|supply| supply.status = Some("Discharging".to_owned()));
        assert_eq!(Battery.info(&supply, false).unwrap().value, "Discharging");
    }

    #[test]
    fn a_battery_with_nothing_to_say_is_dropped() {
        // 只有 type 文件的设备：产出空值会让渲染器多印一行空的。
        assert!(Battery.info(&battery(|_| {}), false).is_none());
    }

    #[test]
    fn an_unrecognised_status_is_not_printed() {
        // 内核将来加个新状态，宁可少印一个词，也不要原样把内核的枚举值漏出去。
        let supply = battery(|supply| {
            supply.capacity = Some(50);
            supply.status = Some("Calibrating".to_owned());
        });

        assert_eq!(Battery.info(&supply, false).unwrap().value, "50%");
    }

    #[test]
    fn collects_on_this_machine_if_there_is_a_battery() {
        let entries = Battery.collect(&Context::for_tests()).unwrap();

        // 台式机 / 容器里是空的——那也是正确答案。
        for info in &entries {
            assert_eq!(info.module, "battery");
            assert!(info.key.starts_with("Battery ("));
            assert!(!info.value.is_empty());
            assert!(!info.value.contains('\n'));
        }
    }
}
