//! Chassis：机箱类型（笔记本 / 台式机 / 服务器……），来自 DMI。
//!
//! 这个值有用在「怎么显示」之外——很多脚本想知道自己在不在笔记本上，
//! 而 DMI 的机箱类型比 `cat /sys/class/power_supply/*` 之类的间接推断可靠。

use crate::collectors::dmi;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 机箱。
pub struct Chassis;

impl Collector for Chassis {
    fn name(&self) -> &'static str {
        "chassis"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(code) = dmi::field("chassis_type")? else {
            return Ok(Vec::new());
        };

        let value = describe(&code);
        Ok(vec![
            Info::new(self.name(), "Chassis", value).with_variable("type", code.trim().to_owned()),
        ])
    }
}

/// 拼显示值：认识那个码就用名字，不认识就把码原样带出来。
///
/// 报 `Type 99` 比报 `Unknown` 好：前者至少让人能去查 SMBIOS 规范，
/// 后者什么信息都没有。
fn describe(code: &str) -> String {
    dmi::chassis_name(code).map_or_else(|| format!("Type {}", code.trim()), str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_codes_are_named() {
        assert_eq!(describe("10"), "Notebook");
        assert_eq!(describe(" 3\n"), "Desktop");
    }

    #[test]
    fn unknown_codes_keep_the_number() {
        assert_eq!(describe("99"), "Type 99");
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Chassis.collect(&Context::for_tests()).unwrap();

        if let Some(info) = entries.first() {
            assert_eq!(info.key, "Chassis");
            assert!(!info.value.is_empty());
        }
    }
}
