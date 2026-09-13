//! WM：窗口管理器 / 合成器。
//!
//! 同样沿用 fastfetch 的短名（`--structure wm`）。
//!
//! 值后面会带上会话类型：同一个 Mutter 在 Wayland 与 X11 上是两套东西，
//! fastfetch 也这么标。会话类型来自 `XDG_SESSION_TYPE`，读不到就不写括号。

use crate::collectors::{env, session};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 会话类型的首字母大写：`wayland` → `Wayland`、`x11` → `X11`。
fn capitalize(kind: &str) -> String {
    let mut chars = kind.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

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
            // `XDG_SESSION_TYPE` 是小写的（`wayland`、`x11`），fastfetch 印
            // `(Wayland)`、`(X11)`。值是小写、显示也小写看着像没整理过。
            value.push_str(&format!(" ({})", capitalize(&kind)));
        }

        // 键用全名：fastfetch 默认视图里是 `Window Manager:`，
        // `WM` 那个短名（`--structure wm`）是这个模块自己的名字，不是显示键。
        let mut info =
            Info::new(self.name(), "Window Manager", value).with_variable("name", session.name);
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
            assert_eq!(info.key, "Window Manager");
            assert!(!info.value.is_empty());
        }
    }

    #[test]
    fn the_module_name_matches_the_config_vocabulary() {
        assert_eq!(WindowManager.name(), "wm");
    }

    #[test]
    fn the_session_type_is_capitalised() {
        assert_eq!(capitalize("wayland"), "Wayland");
        assert_eq!(capitalize("x11"), "X11");
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn the_module_name_is_the_short_one() {
        assert_eq!(WindowManager.name(), "wm");
    }
}
