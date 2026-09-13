//! WM：窗口管理器 / 合成器。
//!
//! 同样沿用 fastfetch 的短名（`--structure wm`）。
//!
//! 值后面会带上会话类型：同一个 Mutter 在 Wayland 与 X11 上是两套东西，
//! fastfetch 也这么标。会话类型来自 `XDG_SESSION_TYPE`，读不到就不写括号。

use crate::collectors::{env, session};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 窗口管理器。
pub struct WindowManager;

impl Collector for WindowManager {
    fn name(&self) -> &'static str {
        "wm"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(session) = session::window_manager() else {
            return Ok(Vec::new());
        };

        let mut value = session.describe();
        if let Some(kind) = env::var("XDG_SESSION_TYPE") {
            value.push_str(&format!(" ({kind})"));
        }

        let mut info = Info::new(self.name(), "WM", value).with_variable("name", session.name);
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
    fn collects_on_this_machine_if_there_is_a_compositor() {
        let entries = WindowManager.collect(&Context::for_tests()).unwrap();

        for info in &entries {
            assert_eq!(info.module, "wm");
            assert_eq!(info.key, "WM");
            assert!(!info.value.is_empty());
        }
    }

    #[test]
    fn the_module_name_matches_the_config_vocabulary() {
        assert_eq!(WindowManager.name(), "wm");
    }
}
