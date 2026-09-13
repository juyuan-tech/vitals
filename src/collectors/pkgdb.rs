//! 包数据库：不 fork 包管理器，直接从文件系统读「装了什么」。
//!
//! 目前只有「查某个包的版本」这件事，服务的模块是 `init-system`
//! （`systemd 261.3-1` 里那个版本号就是这么来的）。
//!
//! Arch 的 `/var/lib/pacman/local/<包名>-<版本>/` 目录名自带版本；
//! Debian 系的 `/var/lib/dpkg/status` 是成对的 `Package:` / `Version:` 字段。
//! 别的包管理器（rpm 用 sqlite 库、nix 用 derivation）等真做 packages 模块时再说——
//! 不做「先写着，反正读不到就返回 None」的猜测。

use crate::collectors::read;
use crate::core::collector::CollectError;

/// pacman 的本地数据库。
const PACMAN: &str = "/var/lib/pacman/local";
/// dpkg 的状态文件。
const DPKG: &str = "/var/lib/dpkg/status";

/// 查一个包的版本。查不到就是 `None`（没装、或者这个包管理器我们不认）。
pub fn version_of(package: &str) -> Result<Option<String>, CollectError> {
    if let Some(version) = pacman_version(package) {
        return Ok(Some(version));
    }

    Ok(read::text(DPKG)?
        .as_deref()
        .and_then(|text| from_dpkg(text, package)))
}

/// 从 pacman 的目录名里取版本。
///
/// 目录名是 `<包名>-<版本>-<发布号>`，比如 `systemd-261.3-1`。**必须校验版本号以数字开头**：
/// 光用 `strip_prefix` 的话，查 `systemd` 会把 `systemd-libs-261.3-1` 认成是它，
/// 于是报出 `libs-261.3-1` 这种假版本号。
fn from_pacman_dir(entry: &str, package: &str) -> Option<String> {
    let rest = entry.strip_prefix(package)?.strip_prefix('-')?;

    rest.starts_with(|first: char| first.is_ascii_digit())
        .then(|| rest.to_owned())
}

/// 在 `/var/lib/dpkg/status` 里找包版本。
///
/// 文件是若干条记录，记录之间用空行隔开，每条记录里 `Package:` 在前。所以看到
/// `Version:` 时，只要记住的那条记录正好是我们要的包，就命中。
fn from_dpkg(text: &str, package: &str) -> Option<String> {
    let mut current = None;

    for line in text.lines() {
        if line.is_empty() {
            current = None;
            continue;
        }

        if let Some(name) = line.strip_prefix("Package:") {
            current = Some(name.trim());
        } else if let Some(version) = line.strip_prefix("Version:") {
            if current == Some(package) {
                let version = version.trim();
                if !version.is_empty() {
                    return Some(version.to_owned());
                }
            }
        }
    }

    None
}

/// 在 pacman 的本地数据库里找包版本。
fn pacman_version(package: &str) -> Option<String> {
    // 读不到（没装 Arch、或者没有权限）不算错误：换个包管理器问就是了。
    let entries = std::fs::read_dir(PACMAN).ok()?;

    let mut versions: Vec<String> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| from_pacman_dir(&name, package))
        .collect();

    // 同一台机器上不该有两个版本。真撞上了取最小的那个，至少结果稳定。
    versions.sort();
    versions.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacman_directories_carry_the_version() {
        assert_eq!(
            from_pacman_dir("systemd-261.3-1", "systemd").as_deref(),
            Some("261.3-1")
        );
        // 带 epoch 的版本号。
        assert_eq!(
            from_pacman_dir("ffmpeg-2:7.1-3", "ffmpeg").as_deref(),
            Some("2:7.1-3")
        );
    }

    #[test]
    fn a_longer_package_name_is_not_mistaken_for_a_version() {
        // 这是真会踩的坑：systemd-libs 以 systemd 开头，但它的版本不是 `libs-261.3-1`。
        assert_eq!(from_pacman_dir("systemd-libs-261.3-1", "systemd"), None);
        assert_eq!(
            from_pacman_dir("systemd-sysvcompat-261.3-1", "systemd"),
            None
        );
        assert_eq!(from_pacman_dir("systemd", "systemd"), None, "没有版本段");
    }

    #[test]
    fn reads_a_version_out_of_dpkg_status() {
        let status = "\
Package: bash
Version: 5.2.21-2
Architecture: amd64

Package: systemd
Version: 257.4-1
Architecture: amd64
";

        assert_eq!(
            from_dpkg(status, "systemd").as_deref(),
            Some("257.4-1"),
            "要的是 systemd 那条，不是第一条"
        );
        assert_eq!(from_dpkg(status, "bash").as_deref(), Some("5.2.21-2"));
        assert_eq!(from_dpkg(status, "zsh"), None);
    }

    #[test]
    fn a_version_belonging_to_another_package_is_not_picked_up() {
        // `Version:` 在 `Package:` 之前出现过，不该被算到后面的包头上。
        let status = "Package: bash\nVersion: 5.2.21-2\n\nPackage: zsh\nVersion: 5.9-4\n";
        assert_eq!(from_dpkg(status, "zsh").as_deref(), Some("5.9-4"));
        assert_eq!(from_dpkg(status, "systemd"), None);
    }

    #[test]
    fn queries_the_real_databases_on_this_machine() {
        // 本机是 Arch：systemd 一定有版本；查一个不存在的包必须是 None，不是报错。
        let version = version_of("systemd").expect("读包数据库不该失败");

        if let Some(version) = version {
            assert!(!version.is_empty());
        }
        assert_eq!(version_of("vitals-这个包不存在").unwrap(), None);
    }
}
