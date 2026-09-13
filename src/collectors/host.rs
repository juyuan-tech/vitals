//! Host：机器型号。数据来自 DMI（`/sys/devices/virtual/dmi/id/`）。
//!
//! DMI 是 x86 固件提供的表；ARM 板子没有它，走设备树，所以留了那条回退。

use crate::collectors::{dmi, read};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 设备树平台（ARM）的回退数据源。
const DEVICE_TREE: &str = "/proc/device-tree/model";

/// 机器。
pub struct Host;

impl Collector for Host {
    fn name(&self) -> &'static str {
        "host"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let vendor = dmi::field("sys_vendor")?;
        let product_name = dmi::field("product_name")?;
        let product_version = dmi::field("product_version")?;
        let board_name = dmi::field("board_name")?;

        let value = match dmi::join_vendor(vendor.as_deref(), product_name.as_deref()) {
            Some(value) => value,
            // DMI 一个字段都没有：ARM 板子或某些虚拟机，试设备树。
            None => match read::text(DEVICE_TREE)? {
                // 设备树里这个字符串是 NUL 结尾的。
                Some(model) => model.trim_end_matches('\0').to_owned(),
                None => return Ok(Vec::new()),
            },
        };

        let mut info = Info::new(self.name(), "Host", value);
        for (key, field) in [
            ("vendor", &vendor),
            ("product_name", &product_name),
            ("product_version", &product_version),
            ("board_name", &board_name),
        ] {
            if let Some(field) = field {
                info = info.with_variable(key, field.clone());
            }
        }

        Ok(vec![info])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_on_this_machine() {
        let context = Context::for_tests();
        let entries = Host.collect(&context).unwrap();

        // 没有 DMI 也没有设备树的机器上会是空，不算失败。
        if let Some(info) = entries.first() {
            assert_eq!(info.key, "Host");
            assert!(!info.value.is_empty(), "有数据就不该是空串");
        }
    }
}
