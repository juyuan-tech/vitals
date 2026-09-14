//! Power supply：`/sys/class/power_supply/` 的公共读取。
//!
//! Battery 与 Power Adapter 两个模块都读这一处，所以抽出来一份实现。
//! 不抽的话两边各自 `read_dir` 同一个目录、各自认 `type`，迟早漂移
//! （一边认了 `Mains`，另一边忘了认）。
//!
//! 为什么读 sysfs 而不是 upower / ACPI 工具：那两样要连 D-Bus 或起子进程，
//! 而 `/sys/class/power_supply/` 是内核给的只读文件，零子进程、零依赖、零副作用。
//! 代价是属性集随内核版本、驱动、设备类型而变——所以这里的字段**全是可选**的。
//! 「少了它还能不能下结论」由各模块自己定，读取层不替它们假设：
//! 「没有 `capacity`」和「`capacity` 是 0」是两回事，在读取层抹平就再也分不出来了。
//!
//! 没有这个目录的机器（容器、部分台式机）不是错误：返回空表。

use std::io::ErrorKind;

use crate::collectors::{read, units};
use crate::core::collector::CollectError;

/// 电源设备所在目录。一个子目录 = 一个设备，**目录名就是设备名**（`BAT0`、`ADP1`）。
///
/// 注意这些子目录是符号链接，所以不能拿 `DirEntry::file_type()` 去筛
/// （它不跟随链接，会把全部设备判成非目录）。
const DIR: &str = "/sys/class/power_supply";

/// 一个电源设备。
#[derive(Debug, Clone, Default)]
pub struct Supply {
    /// 目录名，例如 `BAT0`、`ADP1`。两个模块都拿它做回退显示名。
    pub name: String,
    /// `type`：`Battery` / `Mains` / `USB` / `UPS` / `Wireless` …
    pub kind: Option<String>,
    /// `scope`：`System` 表示这是机器自己的电源，`Device` 表示外设的
    /// （蓝牙鼠标、耳机的电池都会以 power_supply 设备的身份出现）。
    pub scope: Option<String>,
    /// `present`：电池在不在位。可拆电池的空仓会报 `0`。
    ///
    /// 判定方向与 [`Supply::online`] 刻意相反：这里只有 `0` 才算「不在位」，
    /// 别的值（包括将来内核新增的写法）一律当在位——把一块真电池扔掉比多报一块严重。
    pub present: Option<bool>,
    /// `capacity`：内核算好的整百分比。只有部分设备给（不是所有固件都有）。
    pub capacity: Option<u64>,
    /// `model_name`：电池型号，例如 `WE04068XL`。台式机的适配器一般没有。
    pub model_name: Option<String>,
    /// `status`：`Charging` / `Discharging` / `Full` / `Not charging` / `Unknown`。
    pub status: Option<String>,
    /// `online`：Mains 设备「接上了没有」。`true` 只对应文件里的 `1`。
    pub online: Option<bool>,
    /// `energy_now` / `energy_full`（µWh）。部分设备不给 `capacity`，但给这两个。
    pub energy_now: Option<u64>,
    /// 见 [`Supply::energy_now`]。
    pub energy_full: Option<u64>,
    /// `charge_now` / `charge_full`（µAh）。与 `energy_*` 同为「现在 / 满」，
    /// 只是单位不同，比值一样。有的驱动只给这一对。
    pub charge_now: Option<u64>,
    /// 见 [`Supply::charge_now`]。
    pub charge_full: Option<u64>,
}

impl Supply {
    /// 是不是电池。
    #[must_use]
    pub fn is_battery(&self) -> bool {
        self.kind.as_deref() == Some("Battery")
    }

    /// 是不是外接电源。
    #[must_use]
    pub fn is_mains(&self) -> bool {
        self.kind.as_deref() == Some("Mains")
    }

    /// 电量百分比。
    ///
    /// 三级回退，顺序是有意的：
    ///
    /// 1. `capacity`——内核已经算好了，直接信它（本地口径和 `upower` 一致）；
    /// 2. `energy_now` / `energy_full`（µWh）；
    /// 3. `charge_now` / `charge_full`（µAh）。
    ///
    /// 后两级自己除：分子分母同单位，比值与单位无关，所以 µWh 和 µAh 共用一套算法。
    /// 三级都没有就返回 `None`，调用方据此「只显示状态」而不是编一个 0%。
    /// 分母为 0 也算无数据：除零会 panic，而「满电量是 0」本来就不是有效读数。
    #[must_use]
    pub fn percentage(&self) -> Option<u64> {
        if let Some(capacity) = self.capacity {
            // 超出 100 只可能是驱动写错了，不该把 300% 摆到用户脸上。
            return Some(capacity.min(100));
        }

        let (now, full) = match (self.energy_now, self.energy_full) {
            (Some(now), Some(full)) => (now, full),
            _ => (self.charge_now?, self.charge_full?),
        };

        (full > 0).then(|| units::percent(now, full))
    }
}

/// 读一遍所有电源设备，按设备名排序。
///
/// 排序是为了输出稳定：`read_dir` 给的是目录哈希序，同一台机器两次运行
/// 都可能换个顺序，多设备时输出会莫名其妙地跳。
pub fn supplies() -> Result<Vec<Supply>, CollectError> {
    let entries = match std::fs::read_dir(DIR) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(CollectError::caused_by(
                crate::i18n::now().cannot_read(DIR),
                source,
            ));
        }
    };

    let mut supplies = Vec::new();
    for entry in entries {
        // 迭代本身的错误（目录边读边消失）跳过它这一条，不影响别的设备。
        let Ok(entry) = entry else { continue };
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }

        supplies.push(read_supply(&name)?);
    }

    supplies.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(supplies)
}

/// 有没有任何一个 Mains 设备报「已接入」。
///
/// 电池自己的 `status` 只说明在充、在放还是满了，插没插电源得问适配器，
/// 而适配器可以有好几个（双口 USB-C 的机器就是两条）。任何一个在线就算接上了。
#[must_use]
pub fn ac_connected(supplies: &[Supply]) -> bool {
    supplies
        .iter()
        .any(|supply| supply.is_mains() && supply.online == Some(true))
}

/// 读一个设备的所有属性。
fn read_supply(name: &str) -> Result<Supply, CollectError> {
    let dir = format!("{DIR}/{name}");

    Ok(Supply {
        name: name.to_owned(),
        kind: text(&dir, "type")?,
        scope: text(&dir, "scope")?,
        present: text(&dir, "present")?.map(|value| value != "0"),
        capacity: number(&dir, "capacity")?,
        model_name: text(&dir, "model_name")?,
        status: text(&dir, "status")?,
        online: text(&dir, "online")?.map(|value| value == "1"),
        energy_now: number(&dir, "energy_now")?,
        energy_full: number(&dir, "energy_full")?,
        charge_now: number(&dir, "charge_now")?,
        charge_full: number(&dir, "charge_full")?,
    })
}

/// 读一个文本属性，空文件当作没有。
///
/// 不存在的属性走 [`read::text`] 的 `Ok(None)`——那是**无数据**，不是失败。
fn text(dir: &str, attribute: &str) -> Result<Option<String>, CollectError> {
    Ok(read::text(&format!("{dir}/{attribute}"))?.filter(|value| !value.is_empty()))
}

/// 读一个整数属性。
///
/// 内容不是数字就当没有：同一批属性里既有整数（`capacity`）也有文字
/// （`capacity_level` 是 `Full` / `Normal` / `Critical`），读串了不该 panic。
fn number(dir: &str, attribute: &str) -> Result<Option<u64>, CollectError> {
    Ok(text(dir, attribute)?.and_then(|value| value.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个设备，只填关心的那几个字段。
    fn supply(kind: &str, fields: impl FnOnce(&mut Supply)) -> Supply {
        let mut supply = Supply {
            name: "TEST".to_owned(),
            kind: Some(kind.to_owned()),
            ..Supply::default()
        };
        fields(&mut supply);
        supply
    }

    #[test]
    fn capacity_wins_over_the_energy_reading() {
        // 两个数都在时报 capacity：它是内核的口径，和 upower 显示的一致，
        // 自己拿 energy_* 除反而可能因为满电容量重新学习而对不上。
        let battery = supply("Battery", |supply| {
            supply.capacity = Some(76);
            supply.energy_now = Some(1);
            supply.energy_full = Some(100);
        });

        assert_eq!(battery.percentage(), Some(76));
    }

    #[test]
    fn energy_is_used_when_capacity_is_missing() {
        // 真机上这类设备存在：只有 energy_* 没有 capacity。
        let battery = supply("Battery", |supply| {
            supply.energy_now = Some(63_130_000);
            supply.energy_full = Some(63_130_000);
        });

        assert_eq!(battery.percentage(), Some(100));
    }

    #[test]
    fn charge_is_the_last_resort() {
        let battery = supply("Battery", |supply| {
            supply.charge_now = Some(2_400);
            supply.charge_full = Some(3_200);
        });

        assert_eq!(battery.percentage(), Some(75));
    }

    #[test]
    fn a_battery_without_any_reading_has_no_percentage() {
        assert_eq!(supply("Battery", |_| {}).percentage(), None);

        // 只有一半（缺分母）也是无数据，不能拿 now 当百分比。
        let half = supply("Battery", |supply| supply.energy_now = Some(100));
        assert_eq!(half.percentage(), None);
    }

    #[test]
    fn a_zero_full_capacity_does_not_divide_by_zero() {
        let battery = supply("Battery", |supply| {
            supply.energy_now = Some(100);
            supply.energy_full = Some(0);
        });

        assert_eq!(battery.percentage(), None, "满电量是 0 不是有效读数");
    }

    #[test]
    fn a_capacity_above_one_hundred_is_clamped() {
        let battery = supply("Battery", |supply| supply.capacity = Some(300));
        assert_eq!(battery.percentage(), Some(100));
    }

    #[test]
    fn the_kind_decides_what_a_supply_is() {
        assert!(supply("Battery", |_| {}).is_battery());
        assert!(!supply("Battery", |_| {}).is_mains());
        assert!(supply("Mains", |_| {}).is_mains());
        // USB 供电（部分 USB-C 口）两边都不算：它既不报电量也不能代表外接电源。
        assert!(!supply("USB", |_| {}).is_battery());
        assert!(!supply("USB", |_| {}).is_mains());
        assert!(!Supply::default().is_battery(), "没有 type 就什么都不是");
    }

    #[test]
    fn only_an_online_mains_counts_as_ac() {
        let offline = supply("Mains", |supply| supply.online = Some(false));
        let unknown = supply("Mains", |_| {});
        let online = supply("Mains", |supply| supply.online = Some(true));
        let battery = supply("Battery", |supply| supply.online = Some(true));

        assert!(!ac_connected(std::slice::from_ref(&offline)));
        assert!(!ac_connected(std::slice::from_ref(&unknown)));
        assert!(
            !ac_connected(std::slice::from_ref(&battery)),
            "电池不算适配器"
        );
        assert!(ac_connected(&[offline.clone(), online]), "有一个在线就算");
    }

    #[test]
    fn reads_this_machines_supplies_without_failing() {
        // 容器和台式机上会是空的——那也是正确答案。
        let supplies = supplies().expect("读 sysfs 不该失败");

        for supply in &supplies {
            assert!(!supply.name.is_empty());
            assert!(!supply.name.starts_with('.'), "点目录不该进来");
        }

        // 有设备就必须有 type：`type` 是设备的身份，读不到那个目录根本不成设备。
        if !supplies.is_empty() {
            assert!(
                supplies.iter().any(|supply| supply.kind.is_some()),
                "至少该有一个设备认得出来"
            );
        }
    }

    #[test]
    fn the_supplies_come_back_sorted_by_name() {
        let supplies = supplies().expect("读 sysfs 不该失败");
        let mut sorted: Vec<&str> = supplies.iter().map(|supply| supply.name.as_str()).collect();
        let names = sorted.clone();
        sorted.sort_unstable();

        assert_eq!(names, sorted, "输出顺序要稳定，不能跟着 readdir 的哈希序走");
    }
}
