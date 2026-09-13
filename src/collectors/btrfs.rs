//! BTRFS：每个 btrfs 文件系统的容量与空间分配。
//!
//! 全部来自 `/sys/fs/btrfs/<fsid>/`，**用户可读**（不像 DMI 那样要 root），也不用调
//! `btrfs filesystem usage`：
//!
//! - `label`：卷标
//! - `devices/*`：成员设备的符号链接，末段是块设备名，容量走
//!   `/sys/class/block/<名字>/size`（512 字节扇区数）
//! - `allocation/{data,metadata,system}/{disk_used,disk_total}`：三类空间的**实际占用**
//!   与**已分配**
//!
//! 两个容易搞错的地方：
//!
//! 1. **「已用」要用 `disk_used` 之和，不是 `bytes_used` 之和**。后者是文件系统内部的逻辑
//!    计数，和 `df` 看到的口径不是一回事（本机两者差 2.7 GiB，因为压缩与元数据记账）。
//!    fastfetch 用的也是 `disk_used`。
//! 2. **「已分配」不等于「已用」**：btrfs 按块（chunk）预留空间，预留了未必用掉。两个百分比
//!    各有意义，所以都要报。本机：已用 6%、已分配 12%。
//!
//! 设备容量之和 > 0 才算一个能报的文件系统：算不出总量就没法给百分比，宁可这一条没有。

use crate::collectors::{read, units};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// btrfs 在 sysfs 里的根。
const ROOT: &str = "/sys/fs/btrfs";

/// 三类空间。btrfs 只在这三类里分配块。
const SPACES: [&str; 3] = ["data", "metadata", "system"];

/// BTRFS 文件系统。
pub struct Btrfs;

impl Collector for Btrfs {
    fn name(&self) -> &'static str {
        "btrfs"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let mut entries = Vec::new();

        for fsid in filesystems()? {
            let Some(filesystem) = read_filesystem(&fsid, ROOT)? else {
                continue;
            };

            entries.push(Info::new(
                self.name(),
                describe_key(&filesystem),
                describe_value(&filesystem),
            ));
        }

        Ok(entries)
    }
}

/// 一个 btrfs 文件系统。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Filesystem {
    label: String,
    /// 设备容量之和。
    total: u64,
    /// 三类空间的 `disk_total` 之和。
    allocated: u64,
    /// 三类空间的 `disk_used` 之和。
    used: u64,
}

/// 键：`BTRFS (卷标)`；没有卷标就不写括号。
///
/// **不用 fsid 顶替卷标**：那串 UUID 对人不构成信息，宁可不写。
fn describe_key(filesystem: &Filesystem) -> String {
    if filesystem.label.is_empty() {
        "BTRFS".to_owned()
    } else {
        format!("BTRFS ({})", filesystem.label)
    }
}

/// 值：`52.4 GiB / 920.9 GiB (6%, 12% allocated)`。
fn describe_value(filesystem: &Filesystem) -> String {
    format!(
        "{} / {} ({}%, {}% allocated)",
        units::bytes(filesystem.used),
        units::bytes(filesystem.total),
        units::percent(filesystem.used, filesystem.total),
        units::percent(filesystem.allocated, filesystem.total),
    )
}

/// `/sys/fs/btrfs` 下的 fsid 目录（`features` 不是文件系统）。
fn filesystems() -> Result<Vec<String>, CollectError> {
    let Ok(entries) = std::fs::read_dir(ROOT) else {
        return Ok(Vec::new());
    };

    let mut ids: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .filter(|name| name != "features")
        .collect();
    ids.sort();

    Ok(ids)
}

/// 读一个文件系统。设备容量读不出来就返回 `None`（这一条不报）。
fn read_filesystem(fsid: &str, root: &str) -> Result<Option<Filesystem>, CollectError> {
    let dir = format!("{root}/{fsid}");

    let label = read::text(&format!("{dir}/label"))?
        .unwrap_or_default()
        .trim()
        .to_owned();

    let total = device_total(&format!("{dir}/devices"))?;
    if total == 0 {
        return Ok(None);
    }

    let mut allocated = 0;
    let mut used = 0;
    for space in SPACES {
        allocated += read_u64(&format!("{dir}/allocation/{space}/disk_total"))?.unwrap_or(0);
        used += read_u64(&format!("{dir}/allocation/{space}/disk_used"))?.unwrap_or(0);
    }

    Ok(Some(Filesystem {
        label,
        total,
        allocated,
        used,
    }))
}

/// 成员设备的容量之和。
fn device_total(devices: &str) -> Result<u64, CollectError> {
    let Ok(entries) = std::fs::read_dir(devices) else {
        return Ok(0);
    };

    let mut total = 0;
    for entry in entries.flatten() {
        // 符号链接指向 `/sys/devices/...`，末段就是块设备名（`nvme0n1p3`）。
        let Ok(target) = std::fs::read_link(entry.path()) else {
            continue;
        };
        let Some(name) = target.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        // 容量单位是 512 字节扇区，与 `queue/logical_block_size` 无关——
        // 这个 `size` 按内核的固定口径走。
        total += read_u64(&format!("/sys/class/block/{name}/size"))?.unwrap_or(0) * 512;
    }

    Ok(total)
}

/// 读一个十进制整数文件。缺失或不是数字都算 0，读不了才是失败。
fn read_u64(path: &str) -> Result<Option<u64>, CollectError> {
    Ok(read::text(path)?.and_then(|text| text.trim().parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filesystem(label: &str, total: u64, allocated: u64, used: u64) -> Filesystem {
        Filesystem {
            label: label.to_owned(),
            total,
            allocated,
            used,
        }
    }

    #[test]
    fn prints_the_label_in_the_key() {
        assert_eq!(
            describe_key(&filesystem("myArch", 1, 1, 1)),
            "BTRFS (myArch)"
        );
    }

    #[test]
    fn a_filesystem_without_a_label_gets_a_bare_key() {
        // 不用 fsid 顶替：UUID 对人不构成信息。（空白标签在读的时候就 trim 掉了。）
        assert_eq!(describe_key(&filesystem("", 1, 1, 1)), "BTRFS");
    }

    #[test]
    fn renders_used_total_and_both_percentages() {
        // 本机的真实形状（已用 6%、已分配 12%）。
        let filesystem = filesystem("myArch", 988_860_000_000, 117_060_000_000, 56_236_548_608);

        assert_eq!(
            describe_value(&filesystem),
            "52.4 GiB / 920.9 GiB (6%, 12% allocated)"
        );
    }

    #[test]
    fn a_filesystem_we_cannot_size_is_skipped() {
        // 设备容量之和是 0（目录不存在、或者链接指不到块设备）时不报这一条：
        // 连总量都没有，百分比无从谈起。
        assert!(read_filesystem("nope", "/tmp").unwrap().is_none());
        assert_eq!(device_total("/definitely/not/here").unwrap(), 0);
    }

    #[test]
    fn reads_a_decimal_file() {
        assert_eq!(read_u64("/definitely/not/here").unwrap(), None);
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Btrfs.collect(&Context::for_tests()).unwrap();

        // 本机是 btrfs，应当有；别的机器可能是 ext4，两种结局都通过。
        for info in &entries {
            assert_eq!(info.module, "btrfs");
            assert!(info.key.starts_with("BTRFS"), "实际是 {}", info.key);
            assert!(info.value.contains(" / "), "实际是 {}", info.value);
            assert!(info.value.ends_with("allocated)"), "实际是 {}", info.value);
        }
    }
}
