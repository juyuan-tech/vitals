//! Board：主板型号，来自 DMI。

use crate::collectors::dmi;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 主板。
pub struct Board;

impl Collector for Board {
    fn name(&self) -> &'static str {
        "board"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let name = dmi::field("board_name")?;
        let vendor = dmi::field("board_vendor")?;
        let version = dmi::field("board_version")?;

        // 连名字都读不到就没什么可显示的——不拿厂商或版本单独凑一行。
        let Some(base) = dmi::join_vendor(vendor.as_deref(), name.as_deref()) else {
            return Ok(Vec::new());
        };

        let value = describe(&base, version.as_deref());
        let mut info = Info::new(self.name(), "Board", value);

        if let Some(name) = name {
            info = info.with_variable("name", name);
        }
        if let Some(vendor) = vendor {
            info = info.with_variable("vendor", vendor);
        }
        if let Some(version) = version {
            info = info.with_variable("version", version);
        }

        Ok(vec![info])
    }
}

/// 拼显示值：`HP 8C6B (05.17)`。
fn describe(base: &str, version: Option<&str>) -> String {
    match version.filter(|version| !version.is_empty()) {
        Some(version) => format!("{base} ({version})"),
        None => base.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_goes_in_parentheses() {
        assert_eq!(describe("HP 8C6B", Some("05.17")), "HP 8C6B (05.17)");
        assert_eq!(describe("HP 8C6B", None), "HP 8C6B");
        assert_eq!(describe("HP 8C6B", Some("")), "HP 8C6B");
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Board.collect(&Context::for_tests()).unwrap();

        if let Some(info) = entries.first() {
            assert_eq!(info.key, "Board");
            assert!(!info.value.is_empty());
        }
    }
}
