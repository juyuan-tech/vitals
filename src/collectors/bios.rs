//! BIOS/UEFI：固件信息，来自 DMI。
//!
//! 键里带上 `UEFI` / `BIOS`：判断依据是 `/sys/firmware/efi` 在不在——只有 EFI
//! 引导时内核才挂它。同一条固件信息，两种引导方式的含义完全不同（能不能改引导项、
//! 能不能签内核），所以宁可把它写进键里，也不藏进变量里。

use std::path::Path;

use crate::collectors::dmi;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// EFI 运行时目录：在不在就是「是不是 EFI 引导」。
const EFI: &str = "/sys/firmware/efi";

/// 固件。
pub struct Bios;

impl Collector for Bios {
    fn name(&self) -> &'static str {
        "bios"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        // 版本是主信息，没有它这个模块就没什么可说的（部分虚拟机就是这样）。
        let Some(version) = dmi::field("bios_version")? else {
            return Ok(Vec::new());
        };
        let vendor = dmi::field("bios_vendor")?;
        let date = dmi::field("bios_date")?;

        let value = describe(vendor.as_deref(), &version, date.as_deref());
        let mut info = Info::new(self.name(), key(Path::new(EFI).exists()), value);

        if let Some(vendor) = vendor {
            info = info.with_variable("vendor", vendor);
        }
        info = info.with_variable("version", version);
        if let Some(date) = date {
            info = info.with_variable("release_date", date);
        }

        Ok(vec![info])
    }
}

/// 键带上引导方式。
fn key(efi: bool) -> &'static str {
    if efi { "BIOS (UEFI)" } else { "BIOS (Legacy)" }
}

/// 拼显示值：`Insyde F.09 (12/05/2025)`。
///
/// 日期比厂商的版本号有用得多（固件更新基本按日期比新旧），所以带上它。
fn describe(vendor: Option<&str>, version: &str, date: Option<&str>) -> String {
    let mut value = String::new();

    if let Some(vendor) = vendor.filter(|vendor| !vendor.is_empty()) {
        value.push_str(vendor);
        value.push(' ');
    }
    value.push_str(version);
    if let Some(date) = date.filter(|date| !date.is_empty()) {
        value.push_str(&format!(" ({date})"));
    }

    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_says_how_it_boots() {
        assert_eq!(key(true), "BIOS (UEFI)");
        assert_eq!(key(false), "BIOS (Legacy)");
    }

    #[test]
    fn the_value_is_vendor_version_date() {
        // 本机的真实形状。
        assert_eq!(
            describe(Some("Insyde"), "F.09", Some("12/05/2025")),
            "Insyde F.09 (12/05/2025)"
        );
    }

    #[test]
    fn missing_pieces_are_left_out_cleanly() {
        assert_eq!(describe(None, "F.09", None), "F.09");
        assert_eq!(describe(Some(""), "F.09", Some("")), "F.09");
        assert_eq!(describe(Some("Insyde"), "F.09", None), "Insyde F.09");
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Bios.collect(&Context::for_tests()).unwrap();

        if let Some(info) = entries.first() {
            assert!(info.key.starts_with("BIOS ("), "键该说明引导方式");
            assert!(!info.value.is_empty());
        }
    }
}
