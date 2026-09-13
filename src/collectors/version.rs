//! Version：本程序自己的版本与编译目标。
//!
//! 数值全部来自编译期常量，运行时不会变——所以它同时也是「这份输出是谁产生的」
//! 的自证。fastfetch 的 Version 模块做的是同一件事。
//!
//! 平台用 `std::env::consts`，不是 `uname`：这里说的是「这个二进制是为哪个目标编译的」，
//! 而不是「现在跑在什么内核上」（那是 Kernel 模块的事）。

use std::env::consts::{ARCH, OS};

use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 程序名。`CARGO_PKG_NAME` 是 crate 名（`vitals-rs`），用户看到的二进制叫 `vitals`。
const NAME: &str = "vitals";

/// 版本。
pub struct Version;

impl Collector for Version {
    fn name(&self) -> &'static str {
        "version"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let version = env!("CARGO_PKG_VERSION");
        let platform = format!("{ARCH}-{OS}");

        Ok(vec![
            Info::new(
                self.name(),
                "Version",
                format!("{NAME} {version} ({platform})"),
            )
            .with_variable("program", NAME.to_owned())
            .with_variable("version", version.to_owned())
            .with_variable("platform", platform),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_line_identifies_the_program() {
        let entries = Version.collect(&Context::for_tests()).unwrap();
        let info = &entries[0];

        assert_eq!(info.key, "Version");
        assert!(
            info.value.starts_with("vitals "),
            "得说清是谁：{}",
            info.value
        );
        assert_eq!(
            info.variable("version").map(str::to_owned),
            Some(env!("CARGO_PKG_VERSION").to_owned())
        );
    }

    #[test]
    fn the_platform_is_the_compile_target() {
        let entries = Version.collect(&Context::for_tests()).unwrap();

        assert_eq!(
            entries[0].variable("platform"),
            Some(format!("{ARCH}-{OS}").as_str())
        );
    }
}
