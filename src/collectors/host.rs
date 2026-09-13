//! Host：机器型号。数据来自 DMI（`/sys/devices/virtual/dmi/id/`）。
//!
//! DMI 是 x86 固件提供的表；ARM 板子没有它，走设备树，所以留了那条回退。

use crate::collectors::read;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// DMI 字段所在目录。
const DMI: &str = "/sys/devices/virtual/dmi/id";
/// 设备树平台（ARM）的回退数据源。
const DEVICE_TREE: &str = "/proc/device-tree/model";

/// 机器。
pub struct Host;

/// DMI 里读到的原始字段。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Dmi {
    vendor: Option<String>,
    product_name: Option<String>,
    product_version: Option<String>,
    board_name: Option<String>,
}

impl Dmi {
    fn read() -> Result<Self, CollectError> {
        // 这几个文件在普通 Linux 上都是 0444，不需要 root。
        Ok(Self {
            vendor: read::text(&format!("{DMI}/sys_vendor"))?,
            product_name: read::text(&format!("{DMI}/product_name"))?,
            product_version: read::text(&format!("{DMI}/product_version"))?,
            board_name: read::text(&format!("{DMI}/board_name"))?,
        })
    }
}

/// 拼显示值。
///
/// 厂商名常常已经含在产品名里（本机就是这样：厂商 `HP`、产品
/// `HP Pavilion Plus Laptop 14-ey1xxx`），直接拼会印成 `HP HP Pavilion...`。
/// 所以产品名里已经出现过厂商名时就不再重复。
fn describe(dmi: &Dmi) -> Option<String> {
    let vendor = dmi.vendor.as_deref();
    let product = dmi.product_name.as_deref();

    match (vendor, product) {
        (Some(vendor), Some(product)) => {
            if product
                .to_ascii_lowercase()
                .contains(&vendor.to_ascii_lowercase())
            {
                Some(product.to_owned())
            } else {
                Some(format!("{vendor} {product}"))
            }
        }
        (Some(vendor), None) => Some(vendor.to_owned()),
        (None, Some(product)) => Some(product.to_owned()),
        (None, None) => None,
    }
}

impl Collector for Host {
    fn name(&self) -> &'static str {
        "host"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let dmi = Dmi::read()?;

        let value = match describe(&dmi) {
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
            ("vendor", &dmi.vendor),
            ("product_name", &dmi.product_name),
            ("product_version", &dmi.product_version),
            ("board_name", &dmi.board_name),
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

    fn dmi(vendor: Option<&str>, product: Option<&str>) -> Dmi {
        Dmi {
            vendor: vendor.map(str::to_owned),
            product_name: product.map(str::to_owned),
            ..Dmi::default()
        }
    }

    #[test]
    fn a_vendor_already_in_the_product_name_is_not_repeated() {
        // 本机的真实情况。
        let described = describe(&dmi(Some("HP"), Some("HP Pavilion Plus Laptop 14-ey1xxx")));

        assert_eq!(
            described.as_deref(),
            Some("HP Pavilion Plus Laptop 14-ey1xxx")
        );
    }

    #[test]
    fn a_missing_vendor_gets_prepended() {
        let described = describe(&dmi(Some("LENOVO"), Some("ThinkPad X1 Carbon Gen 9")));
        assert_eq!(
            described.as_deref(),
            Some("LENOVO ThinkPad X1 Carbon Gen 9")
        );
    }

    #[test]
    fn half_the_data_is_still_something() {
        assert_eq!(describe(&dmi(Some("HP"), None)).as_deref(), Some("HP"));
        assert_eq!(
            describe(&dmi(None, Some("Standard PC"))).as_deref(),
            Some("Standard PC")
        );
        assert_eq!(describe(&dmi(None, None)), None, "两个都没有才是无数据");
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Host.collect(&Context::for_tests()).unwrap();

        // 虚拟机/容器里可能没有 DMI，所以只在有数据时校验形状。
        if let Some(info) = entries.first() {
            assert_eq!(info.key, "Host");
            assert!(!info.value.is_empty());
        }
    }
}
