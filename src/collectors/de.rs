//! DE：桌面环境。
//!
//! fastfetch 里这个模块的配置名就叫 `de`（`--structure de`），这里照抄它的词汇，
//! 免得从 fastfetch 过来的人要重新学一遍。
//!
//! 线索怎么来的、版本号从哪读，都在 [`session`] 里；这里只负责变成一行。
//! 没有桌面（纯 WM 会话、TTY、容器）就是无数据。

use crate::collectors::session;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 桌面环境。
pub struct Desktop;

impl Collector for Desktop {
    fn name(&self) -> &'static str {
        "de"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(session) = session::desktop() else {
            return Ok(Vec::new());
        };

        let mut info =
            Info::new(self.name(), "DE", session.describe()).with_variable("name", session.name);
        if let Some(version) = session.version {
            info = info.with_variable("version", version);
        }

        Ok(vec![info])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_on_this_machine_if_there_is_a_desktop() {
        let entries = Desktop.collect(&Context::for_tests()).unwrap();

        for info in &entries {
            assert_eq!(info.module, "de");
            assert_eq!(info.key, "DE");
            assert!(!info.value.is_empty());
        }
    }

    #[test]
    fn the_module_name_matches_the_config_vocabulary() {
        // fastfetch 的 `--structure de` 用的就是这个短名。
        assert_eq!(Desktop.name(), "de");
    }
}
