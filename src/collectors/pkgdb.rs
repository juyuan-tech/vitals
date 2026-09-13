//! 包数据库：不 fork 包管理器，直接从文件系统读「装了什么」。
//!
//! 两件事，服务两个模块：查版本给 `init-system`（`systemd 261.3-1` 里那个版本号
//! 就是这么来的），数数量给 `packages`。
//!
//! Arch 的 `/var/lib/pacman/local/<包名>-<版本>/` 目录名自带版本；
//! Debian 系的 `/var/lib/dpkg/status` 是成对的 `Package:` / `Version:` 字段。
//! 别的包管理器（rpm 用 sqlite 库、nix 用 derivation）不做——
//! 不做「先写着，反正读不到就返回 None」的猜测。

use std::io::ErrorKind;

use crate::collectors::{env, read};
use crate::core::collector::CollectError;

/// pacman 的本地数据库。
const PACMAN: &str = "/var/lib/pacman/local";
/// dpkg 的状态文件。
const DPKG: &str = "/var/lib/dpkg/status";
/// flatpak 的系统级应用目录（一个应用一个子目录）。
const FLATPAK_SYSTEM: &str = "/var/lib/flatpak/app";
/// flatpak 的用户级应用目录，相对 `$HOME`。
const FLATPAK_USER: &str = ".local/share/flatpak/app";
/// snapd 的包文件目录（一个修订版一个 `<名字>_<修订号>.snap` 文件）。
const SNAP: &str = "/var/lib/snapd/snaps";

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
        .then(|| strip_epoch(rest).to_owned())
}

/// 去掉版本号里的 epoch 前缀：`2:7.1-3` → `7.1-3`、`1:1.6.8-1` → `1.6.8-1`。
///
/// epoch 是包管理器自己的**排序装置**（用来让某个版本永远排在新版本之后），
/// 不是上游项目版本号的一部分：`pipewire --version` 打的是 `1.6.8`，
/// 而 pacman 打的是 `1:1.6.8-1`。显示给用户看的版本号按上游口径走，
/// 这条定在 `PLAN.md` §5.6。发布号（`-1`、`-3`）保留，因为它是上游版本号的
/// 组成部分，pacman 与 apt 都把它算在版本里。
///
/// 只在冒号前面全是数字时才剥（`2:` 是 epoch，`CST-8` 这种不是）。
/// Debian 的 `/var/lib/dpkg/status` 同样会用 epoch（`1:2.3-4`），一起处理。
fn strip_epoch(version: &str) -> &str {
    match version.split_once(':') {
        Some((epoch, rest))
            if !epoch.is_empty() && epoch.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            rest
        }
        _ => version,
    }
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
                let version = strip_epoch(version.trim());
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

// ---------------------------------------------------------------------------
// 数数量（packages 模块）
// ---------------------------------------------------------------------------

/// 一个包管理器的数量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCount {
    /// 显示名（`pacman`、`flatpak`…），同时是排序键。
    pub name: &'static str,
    /// 数到的数量。调用方只会看到大于 0 的。
    pub count: u64,
}

/// 数一遍所有认识的包管理器，按名字字母序。
///
/// 一个都数不到 → 空表。这是**无数据**，不是错误：像 `pacman -Q` 那样去报错
/// 「没有找到包管理器」只会让容器和极简系统每次都多一行警告。
///
/// 四个数据库各自独立：一个读不了不影响另外三个（少报一路总比整行不报好）。
pub fn counts() -> Result<Vec<PackageCount>, CollectError> {
    let mut counts = Vec::new();

    // 分块的顺序就是字母序，不依赖「谁先被发现」；下面的 sort 只是兜底。
    if let Some(count) = count_dpkg()? {
        counts.push(PackageCount {
            name: "dpkg",
            count,
        });
    }
    if let Some(count) = count_flatpak()? {
        counts.push(PackageCount {
            name: "flatpak",
            count,
        });
    }
    if let Some(count) = count_dirs(PACMAN)? {
        counts.push(PackageCount {
            name: "pacman",
            count,
        });
    }
    if let Some(count) = count_snap_archives(SNAP)? {
        counts.push(PackageCount {
            name: "snap",
            count,
        });
    }

    counts.sort_by_key(|count| count.name);
    // 数量是 0 就不报：`0 (flatpak)` 看着像数据库坏了，实际只是没装应用。
    counts.retain(|count| count.count > 0);

    Ok(counts)
}

/// 数 `/var/lib/dpkg/status` 里的记录数。文件不在 → `None`。
fn count_dpkg() -> Result<Option<u64>, CollectError> {
    let Some(text) = read::text(DPKG)? else {
        return Ok(None);
    };

    Ok(Some(count_dpkg_records(&text)))
}

/// 数 dpkg 状态文件里的包记录。
///
/// 只认**行首**的 `Package:`：记录里每个字段都从第 0 列开始，而 `Description`
/// 的续行以空格开头——那些文字里完全可能出现一行 ` Package: xxx`。
/// 所以既不能用 `contains`，也不能先 `trim`。
///
/// 一条记录里 `Package:` 恰好出现一次（它还是记录的第一行），
/// 所以数它就等于数记录，不用去处理空行分隔。
#[must_use]
fn count_dpkg_records(text: &str) -> u64 {
    text.lines()
        .filter(|line| line.starts_with("Package:"))
        .count() as u64
}

/// 数 flatpak 应用：系统级 + 用户级两个目录之和。
///
/// 只用 `$HOME` 拼用户级路径，不认 `XDG_DATA_HOME`：这一版先照 fastfetch 的
/// 简化口径来，真有人把数据目录挪走了再按规范补。
/// 两个目录都不在 → `None`（这台机器没装 flatpak）。
fn count_flatpak() -> Result<Option<u64>, CollectError> {
    let mut total = 0;
    let mut found = false;

    if let Some(count) = count_dirs(FLATPAK_SYSTEM)? {
        total += count;
        found = true;
    }
    // 没有 `$HOME`（服务进程、`env -i`）时只是少数用户级的那些，不算失败。
    if let Some(home) = env::var("HOME") {
        if let Some(count) = count_dirs(&format!("{home}/{FLATPAK_USER}"))? {
            total += count;
            found = true;
        }
    }

    Ok(found.then_some(total))
}

/// 数 `*.snap` 文件。目录不在 → `None`。
fn count_snap_archives(dir: &str) -> Result<Option<u64>, CollectError> {
    let Some(entries) = entries_of(dir)? else {
        return Ok(None);
    };

    Ok(Some(
        entries
            .iter()
            .filter(|(name, is_dir)| !is_dir && name.ends_with(".snap"))
            .count() as u64,
    ))
}

/// 数目录项：pacman 与 flatpak 的数据库都是「一个包一个目录」。
///
/// 必须只数目录。pacman 的 `local/` 里就躺着一个 `ALPM_DB_VERSION` 文件，
/// 连它一起数会整整多报一个包。
fn count_dirs(dir: &str) -> Result<Option<u64>, CollectError> {
    let Some(entries) = entries_of(dir)? else {
        return Ok(None);
    };

    Ok(Some(
        entries.iter().filter(|(_, is_dir)| *is_dir).count() as u64
    ))
}

/// 列一个目录，返回 `(名字, 是不是目录)`，按名字排序。
///
/// - 目录不存在 → `Ok(None)`：这台机器没装这个包管理器，**不是错误**
/// - 读不了（权限、路径是文件）→ `Err`：数据库在那儿却读不到，数字就是错的，
///   该说出来。这和 [`read::text`] 对「不存在」与「读不了」的区分是同一条线。
///
/// 排序是为了输出稳定：`read_dir` 给的是文件系统的遍历顺序。
fn entries_of(dir: &str) -> Result<Option<Vec<(String, bool)>>, CollectError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(CollectError::caused_by(format!("读取 {dir} 失败"), source)),
    };

    let mut names = Vec::new();
    for entry in entries {
        // 迭代本身的错误（目录边读边变）跳过这一条，别的条目还是好的。
        let Ok(entry) = entry else { continue };
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
        names.push((name, is_dir));
    }
    names.sort();

    Ok(Some(names))
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
        // 带 epoch 的版本号：epoch 是包管理器的排序装置，对外显示要剥掉（PLAN §5.6）。
        assert_eq!(
            from_pacman_dir("ffmpeg-2:7.1-3", "ffmpeg").as_deref(),
            Some("7.1-3")
        );
        assert_eq!(
            from_pacman_dir("pipewire-1:1.6.8-1", "pipewire").as_deref(),
            Some("1.6.8-1")
        );
    }

    #[test]
    fn strips_only_a_numeric_epoch() {
        // 冒号前全是数字才算 epoch；`CST-8`、`a:b` 这种原样保留。
        assert_eq!(strip_epoch("2:7.1-3"), "7.1-3");
        assert_eq!(strip_epoch("7.1-3"), "7.1-3");
        assert_eq!(strip_epoch("CST-8"), "CST-8");
        assert_eq!(strip_epoch(":7.1"), ":7.1");
        assert_eq!(strip_epoch("a:b"), "a:b");
        assert_eq!(strip_epoch("1:"), "");
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

    // -----------------------------------------------------------------------
    // 数数量
    // -----------------------------------------------------------------------

    /// 造一个临时目录，用来钉住「数什么、不数什么」。
    ///
    /// 用 `std::env::temp_dir()` 而不是 `CARGO_TARGET_TMPDIR`：后者只对集成测试
    /// 生效（`cfg(test)` 里的单元测试拿不到它）。名字里带 pid，免得并行跑的两个
    /// 测试撞进同一个目录。
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vitals-pkgdb-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn counts_package_records_not_lines_that_mention_package() {
        // 真会踩的坑：Description 的续行以空格开头，里面完全可能写着
        // ` Package:` 这样的文字。只有行首的才算记录。
        let status = "\
Package: bash
Version: 5.2.21-2
Description: The GNU Bourne Again SHell
 Package: not-a-field
 Also see Package: nothing

Package: systemd
Version: 257.4-1

";
        assert_eq!(count_dpkg_records(status), 2);
    }

    #[test]
    fn an_empty_or_missing_database_counts_nothing() {
        assert_eq!(count_dpkg_records(""), 0);
        assert_eq!(count_dpkg_records("Status: install ok installed\n"), 0);
    }

    #[test]
    fn only_directories_count_as_packages() {
        let dir = scratch("dirs");
        std::fs::create_dir(dir.join("7zip-26.03-1")).unwrap();
        std::fs::create_dir(dir.join("aalib-1.4rc5-19")).unwrap();
        // pacman 的 `local/` 里真有这么个文件，连它一起数会多报一个包。
        std::fs::write(dir.join("ALPM_DB_VERSION"), "9\n").unwrap();

        assert_eq!(
            count_dirs(dir.to_str().unwrap()).unwrap(),
            Some(2),
            "ALPM_DB_VERSION 是文件，不是包"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_snap_archives_are_counted() {
        let dir = scratch("snaps");
        std::fs::write(dir.join("firefox_1234.snap"), "").unwrap();
        std::fs::write(dir.join("core22_5678.snap"), "").unwrap();
        // snapd 会在旁边留别的东西（`*.snap-revision` 这种残渣），不算包。
        std::fs::write(dir.join("firefox_1235.snap-revision"), "").unwrap();
        std::fs::write(dir.join("README"), "").unwrap();

        assert_eq!(count_snap_archives(dir.to_str().unwrap()).unwrap(), Some(2));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_directory_is_no_data_not_a_failure() {
        assert_eq!(count_dirs("/definitely/not/here").unwrap(), None);
        assert_eq!(count_snap_archives("/definitely/not/here").unwrap(), None);
        // 路径在那儿但根本不是目录：这是真失败，得说出来。
        assert!(count_dirs("/proc/sys/kernel/ostype").is_err());
    }

    #[test]
    fn counts_on_this_machine_are_sorted_and_positive() {
        let counts = counts().expect("数包数据库不该失败");

        let names: Vec<&str> = counts.iter().map(|count| count.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "按名字字母序");

        for count in &counts {
            assert!(count.count > 0, "数量为 0 的不该进结果：{count:?}");
        }

        // 本机是 Arch，`/var/lib/pacman/local` 一定在。
        if std::path::Path::new(PACMAN).is_dir() {
            assert!(names.contains(&"pacman"), "装了 pacman 就得报它：{names:?}");
        }
    }

    #[test]
    fn the_pacman_count_matches_the_directory_entries_on_this_machine() {
        // 拿模块的数和「自己去数目录」对一遍。这不是同义反复：它守住的是
        // 「数目录而不是数条目」这条——多算一个 ALPM_DB_VERSION 就会红。
        let Some(reported) = counts()
            .unwrap()
            .into_iter()
            .find(|count| count.name == "pacman")
        else {
            return; // 这台机器没有 pacman
        };

        let directories = std::fs::read_dir(PACMAN)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .count() as u64;
        let entries = std::fs::read_dir(PACMAN).unwrap().count() as u64;

        assert_eq!(reported.count, directories);
        assert!(directories <= entries);
    }
}
