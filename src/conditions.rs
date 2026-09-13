//! 声明式条件：什么时候**不**采集一个模块。
//!
//! 配置里可以给模块挂条件（`PLAN.md` §2.5）。不满足就跳过它，而且**不报错**：
//! 「这台机器没有电池」不是错误，只是没什么可显示。
//!
//! 一条铁律：`when-command-exists` 只查 `PATH`，**绝不执行命令**。
//! 否则「判断有没有 nvidia-smi」本身就要起一个进程——而 v0.1 全体模块
//! 一个子进程都不开（`PLAN.md` §5.1）。
//!
//! 条件在这里**统一评估**：调度器只拿到最终名单，它不必知道条件这回事。

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::config::{HostOs, ModuleEntry};

/// 这一趟要采集什么，以及跳过了什么。
#[derive(Debug, Default)]
pub struct Plan {
    /// 通过条件的模块名，顺序不变。
    pub names: Vec<&'static str>,
    /// 被条件挡下来的模块。
    pub skipped: Vec<Skipped>,
}

/// 一条被跳过的模块。
#[derive(Debug, PartialEq, Eq)]
pub struct Skipped {
    /// 模块名。
    pub module: &'static str,
    /// 为什么跳过。
    pub reason: SkipReason,
}

/// 跳过的原因。留着具体信息是为了 `--verbose` 能说清楚，
/// 否则「某个模块没出来」只能靠猜。
#[derive(Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// 平台不符。
    Platform,
    /// 命令不在 `PATH` 里。
    Command(String),
    /// 路径不存在。
    Path(PathBuf),
}

impl SkipReason {
    /// `--verbose` 里给人看的一句话。
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Platform => match HostOs::current() {
                Some(current) => format!("当前平台是 {}", current.name()),
                None => "当前平台不在支持的名单里".to_owned(),
            },
            Self::Command(command) => format!("命令 `{command}` 不在 PATH 里"),
            Self::Path(path) => format!("路径 {} 不存在", path.display()),
        }
    }
}

/// 按条件筛出这一趟要采集的模块。
#[must_use]
pub fn plan(modules: &[ModuleEntry]) -> Plan {
    let mut plan = Plan::default();

    for entry in modules {
        match skip_reason(entry) {
            None => plan.names.push(entry.module_type.name()),
            Some(reason) => plan.skipped.push(Skipped {
                module: entry.module_type.name(),
                reason,
            }),
        }
    }

    plan
}

/// 这一项该不该跳过。`None` 表示条件都满足。
///
/// 三个条件的关系是**与**：任一不满足就跳过。
#[must_use]
pub fn skip_reason(entry: &ModuleEntry) -> Option<SkipReason> {
    if !platform_matches(&entry.platforms) {
        return Some(SkipReason::Platform);
    }

    if let Some(command) = &entry.when_command_exists {
        if !command_exists(command) {
            return Some(SkipReason::Command(command.clone()));
        }
    }

    if let Some(path) = &entry.when_file_exists {
        if !expand_tilde(path).exists() {
            return Some(SkipReason::Path(path.clone()));
        }
    }

    None
}

/// 平台条件是否满足。
///
/// 空列表表示不限。反过来，如果本程序跑在一个 [`HostOs`] 名单之外的目标上，
/// 任何非空的 `platforms` 都不满足——这是有意的：与其在一个没验证过的平台上
/// 硬跑，不如让用户显式去掉这个条件。
fn platform_matches(platforms: &[HostOs]) -> bool {
    platforms.is_empty() || HostOs::current().is_some_and(|current| platforms.contains(&current))
}

/// 命令是否在 `PATH` 里。
///
/// 只查，不执行。
#[must_use]
pub fn command_exists(command: &str) -> bool {
    command_in_path(command, std::env::var_os("PATH").as_deref())
}

/// [`command_exists`] 的本体。
///
/// `PATH` 由调用方传进来，测试才能塞一个自己造的目录——
/// 2024 edition 里 `set_var` 已经是 `unsafe`，不该为了测试去动进程环境。
#[must_use]
pub fn command_in_path(command: &str, path: Option<&OsStr>) -> bool {
    // 空名字永远匹配不上。配置里写成空串多半是手滑，不必特意报错，
    // 但也别让它意外匹配到目录本身。
    if command.is_empty() {
        return false;
    }

    // 带斜杠的名字不是 PATH 查找，直接当路径看——shell 也是这个规矩。
    if command.contains('/') {
        return is_executable_file(Path::new(command));
    }

    let Some(path) = path else {
        return false;
    };

    // `split_paths` 把 PATH 里的空项当成空路径，`空路径.join(cmd)` 就是相对当前目录
    // 查找——正是 POSIX 给空项规定的语义，不用自己处理。
    std::env::split_paths(path).any(|directory| is_executable_file(&directory.join(command)))
}

/// 是不是一个可执行文件。
///
/// 光看 `is_file()` 不够：`PATH` 上躺着一个没有执行位的同名文件，
/// shell 也执行不了它，那就不该算「命令存在」。
fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };

    metadata.is_file() && has_execute_bit(&metadata)
}

#[cfg(unix)]
fn has_execute_bit(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn has_execute_bit(_metadata: &std::fs::Metadata) -> bool {
    // 非 Unix 没有执行位这回事（Windows 看的是扩展名），能读到就算数。
    true
}

/// 把开头的 `~/` 展开成 `$HOME`。
///
/// 配置里写 `~/` 是人之常情。不展开的话那个路径**永远不存在**，
/// 模块就被静默跳过了——而「配置里写了却没生效」是最难查的一类问题。
#[must_use]
pub fn expand_tilde(path: &Path) -> PathBuf {
    let Some(rest) = path.to_str().and_then(|text| text.strip_prefix("~/")) else {
        return path.to_path_buf();
    };

    match std::env::var_os("HOME") {
        Some(home) => Path::new(&home).join(rest),
        // 连 HOME 都没有时不要凭空猜一个家目录，原样返回。
        None => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModuleType;

    fn entry(module_type: ModuleType) -> ModuleEntry {
        ModuleEntry::new(module_type)
    }

    #[test]
    fn no_conditions_never_skips() {
        assert_eq!(skip_reason(&entry(ModuleType::Os)), None);
    }

    #[test]
    fn the_current_platform_matches_itself() {
        let mut os = entry(ModuleType::Os);
        os.platforms = vec![HostOs::current().expect("测试机该在名单里")];

        assert_eq!(skip_reason(&os), None);
    }

    #[test]
    fn another_platform_is_skipped() {
        let mut os = entry(ModuleType::Os);
        os.platforms = vec![HostOs::Windows];

        assert_eq!(skip_reason(&os), Some(SkipReason::Platform));
    }

    #[test]
    fn an_empty_platform_list_means_no_restriction() {
        let mut os = entry(ModuleType::Os);
        os.platforms = Vec::new();

        assert_eq!(skip_reason(&os), None);
    }

    #[test]
    fn a_missing_command_is_skipped() {
        let mut disk = entry(ModuleType::Disk);
        disk.when_command_exists = Some("vitals-这个命令肯定不存在".to_owned());

        assert_eq!(
            skip_reason(&disk),
            Some(SkipReason::Command("vitals-这个命令肯定不存在".to_owned()))
        );
    }

    #[test]
    fn a_command_that_exists_is_not_skipped() {
        // `sh` 是 POSIX 保证会有的，用它当「存在」的样本最稳。
        let mut disk = entry(ModuleType::Disk);
        disk.when_command_exists = Some("sh".to_owned());

        assert_eq!(skip_reason(&disk), None);
    }

    #[test]
    fn a_missing_path_is_skipped() {
        let mut disk = entry(ModuleType::Disk);
        disk.when_file_exists = Some(PathBuf::from("/nonexistent/vitals-test-path"));

        assert_eq!(
            skip_reason(&disk),
            Some(SkipReason::Path(PathBuf::from(
                "/nonexistent/vitals-test-path"
            )))
        );
    }

    #[test]
    fn an_existing_path_is_not_skipped() {
        let mut disk = entry(ModuleType::Disk);
        // 目录也算存在——`when-file-exists` 问的是「在不在」，
        // 电池那类模块要判断的正是一个目录。
        disk.when_file_exists = Some(PathBuf::from("/proc"));

        assert_eq!(skip_reason(&disk), None);
    }

    #[test]
    fn all_three_conditions_must_hold() {
        let mut os = entry(ModuleType::Os);
        os.platforms = vec![HostOs::Linux];
        os.when_command_exists = Some("vitals-这个命令肯定不存在".to_owned());
        os.when_file_exists = Some(PathBuf::from("/proc"));

        // 前面两条都满足了，卡在命令上。
        assert_eq!(
            skip_reason(&os),
            Some(SkipReason::Command("vitals-这个命令肯定不存在".to_owned()))
        );
    }

    #[test]
    fn planning_keeps_the_order_and_records_the_skips() {
        let mut memory = entry(ModuleType::Memory);
        memory.when_file_exists = Some(PathBuf::from("/nonexistent"));

        let plan = plan(&[entry(ModuleType::Os), memory, entry(ModuleType::Kernel)]);

        assert_eq!(plan.names, vec!["os", "kernel"]);
        assert_eq!(plan.skipped.len(), 1);
        assert_eq!(plan.skipped[0].module, "memory");
    }

    #[test]
    fn an_empty_command_name_never_matches() {
        assert!(!command_in_path("", None));
        assert!(!command_in_path("", Some(OsStr::new("/bin"))));
    }

    #[test]
    fn a_command_with_a_slash_is_checked_as_a_path() {
        // 不按 PATH 拼接，直接看这个路径本身。
        assert!(command_in_path("/bin/sh", None));
        assert!(!command_in_path("/bin/vitals-不存在", None));
    }

    #[test]
    fn a_path_that_is_not_executable_is_not_a_command() {
        // `/etc/hostname` 是个普通文件；`/etc` 是目录。两个都不算命令。
        assert!(!command_in_path("/etc/hostname", None));
        assert!(!command_in_path("/etc", None));
    }

    #[test]
    fn a_missing_path_variable_matches_nothing() {
        assert!(!command_in_path("sh", None));
    }

    #[test]
    fn tilde_expands_to_home() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };

        let expanded = expand_tilde(Path::new("~/.config/vitals/config.toml"));
        assert!(expanded.starts_with(Path::new(&home)));
        assert!(expanded.ends_with(".config/vitals/config.toml"));
    }

    #[test]
    fn paths_without_a_leading_tilde_are_left_alone() {
        // 只展开开头的 `~/`：`~` 出现在中间、或者单独一个 `~` 都不是家目录的意思。
        for path in ["/etc/os-release", "./x", "a/~/b", "~"] {
            assert_eq!(expand_tilde(Path::new(path)), PathBuf::from(path));
        }
    }

    #[test]
    fn every_platform_name_round_trips() {
        // 名字是对外契约（配置里写的就是它），逐个往返钉住。
        for platform in HostOs::ALL {
            assert_eq!(HostOs::from_name(platform.name()), Some(platform));
        }

        assert_eq!(HostOs::from_name("linx"), None);
        // 编译目标一定在名单里，否则 `platforms` 条件在本机永远不满足。
        assert!(
            HostOs::current().is_some(),
            "当前平台：{}",
            std::env::consts::OS
        );
    }
}
