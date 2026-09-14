//! 块设备共用的两件事：这块是不是**物理盘**、盘名怎么拼。
//!
//! `physical-disk`（列出有哪些盘）与 `disk-io`（每张盘的读写速率）两处都要筛「哪些是
//! 物理盘」，也都要拼盘名。盘名规则必须一致，否则**同一块盘会在两行里显示成两个名字**。
//! 以前是两份拷贝（upstream 也是两份），改一处忘一处就会对不上。

use std::path::Path;

use crate::collectors::read;
use crate::core::collector::CollectError;

/// 这块设备是不是**物理盘**：判据是 `device` 这个符号链接在不在。
///
/// 虚拟设备（`loop0`、`zram0`、`dm-0`）在 `/sys/block/<盘>/` 下没有 `device`。
/// 「不存在」是**正常情况**——它不是物理盘，不是错误；别的失败（没权限）才是真失败，
/// 与 `read::` 那一套错误规矩一致。
///
/// 归并时这里收紧了一处：`physical_disk.rs` 原先用 `.is_dir()`，任何失败都当成
/// 「不是物理盘」；现在两边统一走这条更严的规矩。
/// 这张盘有没有 `device` 链接（upstream 的 `openat(dfd, "device", ...)`）。
///
/// 三种结局分得很清：
///
/// - 链接在 → 物理盘
/// - 链接不在 → `Ok(false)`，跳过这张盘，**这不是错误**
/// - 看不了（权限之类）→ `Err`，不能装作它不存在
pub(crate) fn is_physical(dir: &Path) -> Result<bool, CollectError> {
    match std::fs::metadata(dir.join("device")) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(CollectError::caused_by(
            crate::i18n::now().view_failed(dir.join("device").display()),
            source,
        )),
    }
}

/// 盘名：`vendor`（非空时加一个空格）+ `model`，都空则退回设备名。
///
/// 两个文件都在 **`device/` 下面**（`/sys/block/sda/device/vendor`），
/// 不在 `/sys/block/sda/vendor`——本机实测：`/sys/block/nvme0n1/` 这一层既没有
/// `vendor` 也没有 `model`，而 `device/` 下有 `model`
/// （`SAMSUNG MZVL21T0HCLR-00BH1`，带一长串尾随空白）。`read::text` 会去掉首尾空白，
/// 所以型号不用再 trim。
///
/// **与 upstream 逐字一致**（`physicaldisk_linux.c`）：`vendor` 非空就拼上它加一个空格，
/// 再拼 `model`。这里**没有**做「厂商已经在型号里就不重复」这种聪明处理——本机那块
/// NVMe 盘**根本没有** `vendor` 文件，而 U 盘的型号 `DataTraveler 3.0` 里也不含厂商名。
pub(crate) fn display_name(device: &str, vendor: Option<&str>, model: Option<&str>) -> String {
    let vendor = vendor.map(str::trim).filter(|value| !value.is_empty());
    let model = model.map(str::trim).filter(|value| !value.is_empty());

    let name = match (vendor, model) {
        (Some(vendor), Some(model)) => format!("{vendor} {model}"),
        (Some(vendor), None) => vendor.to_owned(),
        (None, Some(model)) => model.to_owned(),
        (None, None) => device.to_owned(),
    };

    if name.is_empty() {
        device.to_owned()
    } else {
        name
    }
}

/// 从 `/sys/block/<盘>/device/{vendor,model}` 读出字段并拼成盘名。
///
/// 两个模块都要这一步，顺手收在这里；拼接规则仍是 [`display_name`]，只有一份。
pub(crate) fn name_from_sysfs(dir: &Path, device: &str) -> Result<String, CollectError> {
    let read_field = |field: &str| read::text(&dir.join("device").join(field).to_string_lossy());

    let vendor = read_field("vendor")?;
    let model = read_field("model")?;

    Ok(display_name(device, vendor.as_deref(), model.as_deref()))
}
