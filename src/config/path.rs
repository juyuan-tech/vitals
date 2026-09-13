//! 配置文件该放在哪。
//!
//! Linux/Unix 实现：`$XDG_CONFIG_HOME/vitals/config.toml`，
//! 没设 `XDG_CONFIG_HOME` 时回退 `$HOME/.config/vitals/config.toml`。
//!
//! **为什么不用 `etcetera` 这类库**：它们把环境变量的读取藏在内部，
//! 而 edition 2024 里 `std::env::set_var` 是 `unsafe fn`（本 crate `forbid(unsafe_code)`），
//! 于是用了库就没法给路径解析写测试。这里把纯逻辑抽成 [`resolve_in`]，
//! 由它接收注入的环境值——既好测，总共也只多十几行。
//!
//! Windows / macOS 的分支留给 v1.0（`PLAN.md` §2.8 说平台差异要收敛到少数几个函数，
//! 这就是其中一个：将来只改这一个文件）。

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// 配置目录下的相对路径。
const RELATIVE: &str = "vitals/config.toml";

/// 解析默认配置文件路径。
///
/// 返回 `None` 表示环境里既没有 `XDG_CONFIG_HOME` 也没有 `HOME`。
/// 调用方（[`crate::config::load`]）此时应当安静地用内置默认，而不是报错。
#[must_use]
pub fn config_path() -> Option<PathBuf> {
    resolve_in(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

/// [`config_path`] 的纯逻辑部分。
///
/// 环境变量在测试里没法安全地改（`set_var` 是 unsafe，且它是进程全局的，
/// 并行测试会互相踩），所以把值当参数传进来。
#[must_use]
fn resolve_in(config_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    // XDG 规范：XDG_CONFIG_HOME 必须是绝对路径，相对路径一律忽略。
    // 忽略之后继续往下走回退链，而不是直接返回 None。
    if let Some(dir) = config_home.filter(|value| Path::new(value).is_absolute()) {
        return Some(Path::new(&dir).join(RELATIVE));
    }

    home.map(|home| Path::new(&home).join(".config").join(RELATIVE))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(config_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
        resolve_in(config_home.map(OsString::from), home.map(OsString::from))
    }

    fn assert_is(actual: Option<PathBuf>, expected: &str) {
        assert_eq!(actual.as_deref(), Some(Path::new(expected)));
    }

    #[test]
    fn xdg_config_home_wins() {
        assert_is(
            resolve(Some("/xdg"), Some("/home/u")),
            "/xdg/vitals/config.toml",
        );
    }

    #[test]
    fn falls_back_to_home_dot_config() {
        assert_is(
            resolve(None, Some("/home/u")),
            "/home/u/.config/vitals/config.toml",
        );
    }

    #[test]
    fn relative_xdg_config_home_is_ignored() {
        // XDG 规范说相对路径无效。此时不该拼出一个相对路径，而该走回退。
        assert_is(
            resolve(Some("relative/dir"), Some("/home/u")),
            "/home/u/.config/vitals/config.toml",
        );
    }

    #[test]
    fn relative_xdg_without_home_yields_nothing() {
        assert_eq!(resolve(Some("relative/dir"), None), None);
    }

    #[test]
    fn no_environment_means_no_path() {
        assert_eq!(resolve(None, None), None);
    }
}
