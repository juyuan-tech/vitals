//! Init System：1 号进程（init）的名字与版本。
//!
//! 名字读 `/proc/1/comm`。版本没有免 fork 的通用来源，于是去**包数据库**里找
//! （见 `pkgdb`）：`systemctl --version` 要开子进程，而这里的取舍始终是
//! 「能读文件就不 fork」。找不到版本就只显示名字——不编一个。
//!
//! 容器里 `/proc/1/comm` 是容器的 1 号进程，那时显示的其实是容器的 init。
//! 那也正是真话：这台机器上确实在跑它。

use crate::collectors::{pkgdb, read};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 1 号进程。
const COMM: &str = "/proc/1/comm";

/// init。
pub struct InitSystem;

impl Collector for InitSystem {
    fn name(&self) -> &'static str {
        "init-system"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(name) = read::text(COMM)? else {
            return Ok(Vec::new());
        };
        let name = name.trim();
        if name.is_empty() {
            return Ok(Vec::new());
        }

        let version = pkgdb::version_of(name)?;
        let value = match &version {
            Some(version) => format!("{name} {version}"),
            None => name.to_owned(),
        };

        let mut info = Info::new(self.name(), "Init System", value).with_variable("name", name);
        if let Some(version) = version {
            info = info.with_variable("version", version);
        }

        Ok(vec![info])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_on_this_machine() {
        let entries = InitSystem.collect(&Context::for_tests()).unwrap();

        if let Some(info) = entries.first() {
            assert_eq!(info.key, "Init System");
            assert!(!info.value.is_empty());
            // 有版本时形如 `systemd 261.3-1`，没版本时就是光名字。
            assert!(info.value.starts_with(info.variable("name").unwrap()));
        }
    }
}
