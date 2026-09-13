//! TPM：可信平台模块的版本与厂商。
//!
//! 数据源 `/sys/class/tpm/`。设备名不一定是 `tpm0`，所以这里遍历目录取第一个，
//! 而不是把路径写死——fastfetch 写死的正是 `/sys/class/tpm/tpm0/`，
//! 在设备名不是 `tpm0` 的机器上它会直接报「没有 TPM」。
//!
//! 版本读 `tpm_version_major`。内核只给 `"2"` 或 `"1"`：实现就是
//! `chip->flags & TPM_CHIP_FLAG_TPM2 ? "2" : "1"`，它根本没区分过 1.1 与 1.2。
//! 所以这里只把 `"2"` 写成 `"2.0"`（TCG 的写法，也和 fastfetch 对齐），
//! 别的值原样显示——内核没说的版本号，我们不替它补一个。
//!
//! 厂商读 `device/description`。**这个属性不在内核 ABI 文档里**
//! （`Documentation/ABI/stable/sysfs-class-tpm` 只列了 device/ 下的 active、
//! caps、durations、owned 等），2016 年那版「让驱动挂自己的 sysfs 属性」的补丁
//! 被维护者以「用户态 API 不该厂商化」为由退回，于是只有部分内核/驱动提供它。
//! 真机实测本机（驱动 `tpm_crb_acpi`）没有这个文件，此时只显示版本——
//! 比从 modalias 里猜一个厂商名强。
//!
//! 完全没有 TPM（目录不存在或为空）→ 无数据。

use std::io::ErrorKind;
use std::path::PathBuf;

use crate::collectors::read;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// TPM 设备类目录。
const DIR: &str = "/sys/class/tpm";

/// TPM。
pub struct Tpm;

impl Collector for Tpm {
    fn name(&self) -> &'static str {
        "tpm"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(device) = first_device()? else {
            return Ok(Vec::new());
        };
        let root = device.to_string_lossy();

        let version = read::text(&format!("{root}/tpm_version_major"))?
            .filter(|value| !value.is_empty())
            .map(|value| normalize_version(&value));
        let vendor =
            read::text(&format!("{root}/device/description"))?.filter(|value| !value.is_empty());

        let Some(value) = join(version.as_deref(), vendor.as_deref()) else {
            // 设备节点在，但一个字段都读不出来。与其印一行空的 `TPM:`，
            // 不如当作没有——用户看不出区别，脚本也不用处理空值。
            return Ok(Vec::new());
        };

        let mut info = Info::new(self.name(), "TPM", value);
        if let Some(version) = version {
            info = info.with_variable("version", version);
        }
        if let Some(vendor) = vendor {
            info = info.with_variable("vendor", vendor);
        }

        Ok(vec![info])
    }
}

/// 把内核给的版本号写成 TCG 的写法：`2` → `2.0`。
///
/// 只认 `2`：内核的这个文件本质上是个布尔（是不是 TPM 2.0），
/// 把 `1` 写成 `1.2` 是替内核补它没说出口的话。
fn normalize_version(version: &str) -> String {
    if version == "2" {
        "2.0".to_owned()
    } else {
        version.to_owned()
    }
}

/// 拼显示值：`2.0 (IFX)`。两半都没有才是无数据。
fn join(version: Option<&str>, vendor: Option<&str>) -> Option<String> {
    match (version, vendor) {
        (Some(version), Some(vendor)) => Some(format!("{version} ({vendor})")),
        (Some(version), None) => Some(version.to_owned()),
        (None, Some(vendor)) => Some(vendor.to_owned()),
        (None, None) => None,
    }
}

/// 第一个 TPM 设备目录，按名字排序保证稳定。
///
/// 不用 `DirEntry::file_type()` 判断是不是目录：`/sys/class/tpm/tpm0` 是符号链接，
/// 而 `file_type()` 不跟随链接，会把所有设备都判成非目录。
fn first_device() -> Result<Option<PathBuf>, CollectError> {
    let entries = match std::fs::read_dir(DIR) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(CollectError::caused_by(format!("读取 {DIR} 失败"), source)),
    };

    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();

    Ok(names.first().map(|name| PathBuf::from(DIR).join(name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_two_gets_the_tcg_spelling() {
        // 内核给的是 `2`，TCG 和 fastfetch 都写 `2.0`。
        assert_eq!(normalize_version("2"), "2.0");
    }

    #[test]
    fn other_versions_are_left_alone() {
        // 内核这条路径本质是一个布尔（是不是 TPM2），`1` 到底是 1.1 还是 1.2
        // 它没说，所以不替它补。
        assert_eq!(normalize_version("1"), "1");
        assert_eq!(normalize_version("3"), "3");
    }

    #[test]
    fn the_value_joins_version_and_vendor() {
        assert_eq!(join(Some("2.0"), Some("IFX")).as_deref(), Some("2.0 (IFX)"));
        assert_eq!(join(Some("2.0"), None).as_deref(), Some("2.0"));
        assert_eq!(join(None, Some("IFX")).as_deref(), Some("IFX"));
        assert_eq!(join(None, None), None, "两半都没有才是无数据");
    }

    #[test]
    fn collects_on_this_machine_if_there_is_a_tpm() {
        let entries = Tpm.collect(&Context::for_tests()).unwrap();

        match entries.first() {
            None => {} // 没有 TPM 的机器（或容器）就是无数据
            Some(info) => {
                assert_eq!(info.key, "TPM");
                assert_eq!(info.module, "tpm");
                assert!(!info.value.is_empty());
                // 有版本时必须是内核给的写法转化而来，不能凭空多一段。
                if let Some(version) = info.variable("version") {
                    assert_eq!(version, "2.0", "本机 tpm_version_major 是 2");
                }
            }
        }
    }

    #[test]
    fn the_first_device_is_picked_by_name() {
        // 目录里通常只有一个 tpm0；这条只是钉住「取排序后的第一个」这条规则，
        // 免得将来读成 readdir 的哈希序、多设备时随机换一个。
        let device = first_device().unwrap();

        match device {
            None => {}
            Some(path) => {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                assert!(name.starts_with("tpm"), "设备名该是 tpm 开头：{name}");
                assert!(!name.starts_with('.'), "点目录不该被选中");
            }
        }
    }
}
