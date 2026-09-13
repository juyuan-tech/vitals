//! DMI：x86 固件提供的机器信息表（`/sys/devices/virtual/dmi/id/`）。
//!
//! Host、BIOS、Board、Chassis 四个模块都读它，所以抽出来一处实现。
//! ARM 板子与部分虚拟机没有这张表，各模块自己决定留什么回退。

use crate::collectors::read;
use crate::core::collector::CollectError;

/// DMI 字段所在目录（`/sys/class/dmi/id` 是它的符号链接，用哪个都一样）。
const DIR: &str = "/sys/devices/virtual/dmi/id";

/// 读一个 DMI 字段。
///
/// 文件不存在就是 `None`——**不是错误**：容器与 ARM 上这些文件本来就不在。
pub fn field(name: &str) -> Result<Option<String>, CollectError> {
    read::text(&format!("{DIR}/{name}"))
}

/// 把厂商名并进型号里。
///
/// 厂商名常常已经含在型号里（本机：厂商 `HP`、产品 `HP Pavilion Plus Laptop 14-ey1xxx`），
/// 直接拼会印成 `HP HP Pavilion...`，所以型号里出现过厂商名就不再重复。
#[must_use]
pub fn join_vendor(vendor: Option<&str>, name: Option<&str>) -> Option<String> {
    match (vendor, name) {
        (Some(vendor), Some(name)) => {
            if name
                .to_ascii_lowercase()
                .contains(&vendor.to_ascii_lowercase())
            {
                Some(name.to_owned())
            } else {
                Some(format!("{vendor} {name}"))
            }
        }
        (Some(vendor), None) => Some(vendor.to_owned()),
        (None, Some(name)) => Some(name.to_owned()),
        (None, None) => None,
    }
}

/// SMBIOS 机箱类型码 → 名字。
///
/// 码表出自 SMBIOS 规范 7.4.1 节（System Enclosure or Chassis Types）。
/// 认不出来就返回 `None`，由调用方决定怎么显示那个码——总比假装知道强。
#[must_use]
pub fn chassis_name(code: &str) -> Option<&'static str> {
    let name = match code.trim().parse::<u32>().ok()? {
        1 => "Other",
        2 => "Unknown",
        3 => "Desktop",
        4 => "Low Profile Desktop",
        5 => "Pizza Box",
        6 => "Mini Tower",
        7 => "Tower",
        8 => "Portable",
        9 => "Laptop",
        10 => "Notebook",
        11 => "Hand Held",
        12 => "Docking Station",
        13 => "All in One",
        14 => "Sub Notebook",
        15 => "Space-saving",
        16 => "Lunch Box",
        17 => "Main Server Chassis",
        18 => "Expansion Chassis",
        19 => "Sub Chassis",
        20 => "Bus Expansion Chassis",
        21 => "Peripheral Chassis",
        22 => "RAID Chassis",
        23 => "Rack Mount Chassis",
        24 => "Sealed-case PC",
        25 => "Multi-system",
        26 => "CompactPCI",
        27 => "AdvancedTCA",
        28 => "Blade",
        29 => "Blade Enclosure",
        30 => "Tablet",
        31 => "Convertible",
        32 => "Detachable",
        33 => "IoT Gateway",
        34 => "Embedded PC",
        35 => "Mini PC",
        36 => "Stick PC",
        _ => return None,
    };

    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vendor_already_in_the_name_is_not_repeated() {
        // 本机的真实情况。
        assert_eq!(
            join_vendor(Some("HP"), Some("HP Pavilion Plus Laptop 14-ey1xxx")).as_deref(),
            Some("HP Pavilion Plus Laptop 14-ey1xxx")
        );
    }

    #[test]
    fn a_missing_vendor_gets_prepended() {
        assert_eq!(
            join_vendor(Some("LENOVO"), Some("ThinkPad X1 Carbon Gen 9")).as_deref(),
            Some("LENOVO ThinkPad X1 Carbon Gen 9")
        );
    }

    #[test]
    fn the_vendor_check_ignores_case() {
        assert_eq!(
            join_vendor(Some("hp"), Some("HP 8C6B")).as_deref(),
            Some("HP 8C6B")
        );
    }

    #[test]
    fn half_the_data_is_still_something() {
        assert_eq!(join_vendor(Some("HP"), None).as_deref(), Some("HP"));
        assert_eq!(
            join_vendor(None, Some("Standard PC")).as_deref(),
            Some("Standard PC")
        );
        assert_eq!(join_vendor(None, None), None, "两个都没有才是无数据");
    }

    #[test]
    fn the_chassis_codes_we_care_about_are_named() {
        assert_eq!(chassis_name("10"), Some("Notebook"));
        assert_eq!(chassis_name("3"), Some("Desktop"));
        assert_eq!(chassis_name("31"), Some("Convertible"));
        // 空白与换行不该影响判定：sysfs 读出来常常带换行。
        assert_eq!(chassis_name(" 10\n"), Some("Notebook"));
    }

    #[test]
    fn an_unknown_chassis_code_is_not_guessed() {
        assert_eq!(chassis_name("0"), None);
        assert_eq!(chassis_name("99"), None);
        assert_eq!(chassis_name(""), None);
        assert_eq!(chassis_name("笔记本"), None);
    }

    #[test]
    fn reads_the_real_dmi_on_this_machine() {
        // sys_vendor 在绝大多数 x86 机器上都有；没有也不算失败。
        let vendor = field("sys_vendor").expect("读 sysfs 不该失败");
        if let Some(vendor) = vendor {
            assert!(!vendor.is_empty(), "读到了就不该是空串");
        }
    }
}
