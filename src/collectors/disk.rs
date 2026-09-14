//! Disk：根文件系统的用量。
//!
//! v0.1 只看 `/`。多挂载点、按设备筛选是 v0.2「扩展模块」的事。

use rustix::fs::statvfs;

use crate::collectors::{read, units};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 看哪个挂载点。
const MOUNT: &str = "/";
/// 用来补上设备名与文件系统类型。
const MOUNTS: &str = "/proc/mounts";

/// 磁盘。
pub struct Disk;

/// 从 `/proc/mounts` 里找某个挂载点，返回 (设备, 文件系统类型)。
///
/// 每行六段：`设备 挂载点 类型 选项 0 0`。
/// 路径里的空格在文件里被转义成 `\040`，这里不还原——v0.1 只认 `/`，
/// 真要处理多挂载点时再说，那时也该连转义一起做对。
fn mount_of(text: &str, mount: &str) -> Option<(String, String)> {
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [device, mount_point, fs_type, ..] = fields[..] else {
            continue;
        };

        if mount_point == mount {
            return Some((device.to_owned(), fs_type.to_owned()));
        }
    }

    None
}

impl Collector for Disk {
    fn name(&self) -> &'static str {
        "disk"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let vfs = statvfs(MOUNT).map_err(|error| {
            CollectError::caused_by(crate::i18n::now().mount_read_failed(MOUNT), error)
        })?;

        // f_frsize 是「计量块的基本单位」，POSIX 允许它是 0，那时退回 f_bsize。
        let frsize = if vfs.f_frsize == 0 {
            vfs.f_bsize
        } else {
            vfs.f_frsize
        };

        let total = vfs.f_blocks.saturating_mul(frsize);
        let free = vfs.f_bfree.saturating_mul(frsize);
        let available = vfs.f_bavail.saturating_mul(frsize);
        // 已用 = 总量 - 空闲，和 `df` 一致：保留块也算「已用」。
        // f_bavail 是留给非特权用户的那部分，比 f_bfree 小，不能拿来算已用。
        let used = total.saturating_sub(free);

        let percent = units::percent(used, total);
        let value = format!(
            "{} / {} ({percent}%)",
            units::bytes(used),
            units::bytes(total)
        );

        let mut info = Info::new(self.name(), "Disk", value)
            .with_variable("mount_point", MOUNT.to_owned())
            .with_variable("used_bytes", used.to_string())
            .with_variable("total_bytes", total.to_string())
            .with_variable("available_bytes", available.to_string())
            .with_variable("percent", percent.to_string());

        // 设备与类型是锦上添花：读不到就少两个变量，不影响这条信息本身。
        if let Some(text) = read::text(MOUNTS)? {
            if let Some((device, fs_type)) = mount_of(&text, MOUNT) {
                info = info
                    .with_variable("device", device)
                    .with_variable("fs_type", fs_type);
            }
        }

        Ok(vec![info])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 实测本机 `/proc/mounts` 里的根那一行。
    const FIXTURE: &str = "\
proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
/dev/nvme0n1p3 / btrfs ro,nosuid,nodev,relatime,compress=zstd:3,ssd 0 0
tmpfs /tmp tmpfs rw,nosuid,nodev 0 0
";

    #[test]
    fn finds_the_root_entry() {
        assert_eq!(
            mount_of(FIXTURE, "/"),
            Some(("/dev/nvme0n1p3".to_owned(), "btrfs".to_owned()))
        );
    }

    #[test]
    fn does_not_confuse_proc_with_root() {
        // /proc 那行的挂载点是 /proc，不是 / ——按整段比较就不会误判。
        assert_eq!(
            mount_of(FIXTURE, "/proc"),
            Some(("proc".to_owned(), "proc".to_owned()))
        );
    }

    #[test]
    fn an_absent_mount_yields_nothing() {
        assert_eq!(mount_of(FIXTURE, "/mnt/nowhere"), None);
        assert_eq!(mount_of("", "/"), None);
        assert_eq!(mount_of("只有两段 也是\n", "/"), None, "段数不够就跳过");
    }

    #[test]
    fn collects_the_root_filesystem() {
        let entries = Disk.collect(&Context::for_tests()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "Disk");
        assert!(
            entries[0].value.contains(" / "),
            "该是「已用 / 总量」的形状"
        );
        assert_eq!(entries[0].variable("mount_point"), Some("/"));
        assert!(entries[0].variable("total_bytes").is_some());
    }
}
