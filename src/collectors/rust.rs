//! Rust：当前 rustup 工具链。
//!
//! 只读文件、不开子进程。`rustc --version` 更准，但 `PLAN.md` 定的是
//! v0.1 任何模块都不许开子进程，所以这里报的是**工具链名**
//! （`stable-x86_64-unknown-linux-gnu`），不是编译器版本号——措辞上要诚实。

use serde::Deserialize;

use crate::collectors::{env, read};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// `RUSTUP_HOME` 没设时它在哪儿。
const RUSTUP_HOME: &str = ".rustup";

/// Rust。
pub struct Rust;

/// `settings.toml` 里我们唯一关心的键。
///
/// **不能**加 `deny_unknown_fields`：这个文件里还有 `version`、`toolchains`
/// 等其他键，认不出来是正常的。
#[derive(Debug, Deserialize)]
struct Settings {
    default_toolchain: Option<String>,
}

/// 当前工具链：环境变量 → `$RUSTUP_HOME/settings.toml`。
fn toolchain() -> Result<Option<String>, CollectError> {
    // 环境变量优先：`RUSTUP_TOOLCHAIN` 说的是「这次要用哪个」，
    // 比 settings.toml 里的默认值更贴近此刻正在发生的事。
    if let Some(toolchain) = env::var("RUSTUP_TOOLCHAIN") {
        return Ok(Some(toolchain));
    }

    let home = env::var("RUSTUP_HOME")
        .or_else(|| env::var("HOME").map(|home| format!("{home}/{RUSTUP_HOME}")));
    let Some(home) = home else {
        return Ok(None);
    };

    let path = format!("{home}/settings.toml");
    let Some(text) = read::text(&path)? else {
        return Ok(None);
    };

    // 文件确实是 TOML，就用 TOML 解析器读，不手撕字符串。
    let settings: Settings = toml::from_str(&text)
        .map_err(|error| CollectError::caused_by(format!("解析 {path} 失败"), error))?;

    Ok(settings.default_toolchain)
}

impl Collector for Rust {
    fn name(&self) -> &'static str {
        "rust"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(toolchain) = toolchain()? else {
            return Ok(Vec::new());
        };

        Ok(vec![
            Info::new(self.name(), "Rust", toolchain.clone()).with_variable("toolchain", toolchain),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 实测本机 `~/.rustup/settings.toml` 的开头。
    const FIXTURE: &str = "\
version = \"12\"
default_toolchain = \"nightly-x86_64-unknown-linux-gnu\"
profile = \"default\"

[overrides]
";

    #[test]
    fn picks_the_default_toolchain() {
        let settings: Settings = toml::from_str(FIXTURE).unwrap();
        assert_eq!(
            settings.default_toolchain.as_deref(),
            Some("nightly-x86_64-unknown-linux-gnu")
        );
    }

    #[test]
    fn settings_without_a_default_toolchain_are_not_an_error() {
        let settings: Settings = toml::from_str("version = \"12\"\n").unwrap();
        assert_eq!(settings.default_toolchain, None);
    }

    #[test]
    fn a_broken_settings_file_is_a_real_failure() {
        // 解析失败是真失败（会变成一条警告），不是「无数据」——
        // 否则用户永远不知道自己的 rustup 配置坏了。
        let broken = toml::from_str::<Settings>("default_toolchain = 没有引号\n");
        assert!(broken.is_err());
    }

    #[test]
    fn collects_a_toolchain_on_this_machine() {
        let entries = Rust.collect(&Context::for_tests()).unwrap();

        // 没装 rustup 的机器上会是空的；装了的话工具链名该带目标三元组。
        if let Some(info) = entries.first() {
            assert_eq!(info.key, "Rust");
            assert!(!info.value.is_empty());
            assert_eq!(info.value, info.variable("toolchain").unwrap());
        }
    }
}
