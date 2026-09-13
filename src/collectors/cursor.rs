//! Cursor：光标（鼠标指针）主题名。
//!
//! 两条线索，顺序的规矩是「用户级在发行版默认之前」：
//!
//! 1. 用户级：GTK 的 `gtk-cursor-theme-name`（`settings.ini`），
//!    然后 `$HOME/.icons/default/index.theme` 的 `[Icon Theme] Inherits=`；
//! 2. 系统级：发行版的 `settings.ini`，然后
//!    `$XDG_DATA_DIRS/icons/default/index.theme`（本机是
//!    `/usr/share/icons/default/index.theme`，内容只有一行 `Inherits=Adwaita`）。
//!
//! `index.theme` 是 libXcursor 认的标准文件：`Inherits=` 声明「没指定主题时退到哪一个」。
//! 两个来源都没有就是无数据。
//!
//! **特意不读 `$XCURSOR_THEME`**：它给的是「这一次会话是谁拉起来的」，
//! 本机上它是 `breeze_cursors`，而落盘的配置里写的是 Adwaita——两者不是一个问题
//! （一个是会话的临时值，一个是配置）。本模块回答后者。
//!
//! 值后面标出来源：`(GTK)` 是 `settings.ini`，`(Xcursor)` 是 `index.theme`。
//! 两者机制不同，混着看不出来就分不清「用户配的」和「兜底的」。

use crate::collectors::ini::{self, Origin, Probe};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// GTK 侧的键。
const GTK_KEY: &str = "gtk-cursor-theme-name";
/// `index.theme` 里的 section 与键。
const INDEX_SECTION: &str = "Icon Theme";
/// 见 [`INDEX_SECTION`]。`Inherits` 声明「没指定主题时退到哪一个」。
const INDEX_KEY: &str = "Inherits";

/// 光标主题。
pub struct Cursor;

impl Cursor {
    /// 查找线索：用户 GTK → 用户的 `index.theme` → 系统 GTK → 系统的 `index.theme`。
    fn candidates() -> Vec<Probe> {
        let mut candidates = Probe::gtk_user(GTK_KEY);
        candidates.extend(
            ini::cursor_user_index_path()
                .into_iter()
                .map(|path| Probe::at(path, INDEX_SECTION, INDEX_KEY, Origin::Xcursor)),
        );
        candidates.extend(Probe::gtk_system(GTK_KEY));
        candidates.extend(
            ini::cursor_system_index_paths()
                .into_iter()
                .map(|path| Probe::at(path, INDEX_SECTION, INDEX_KEY, Origin::Xcursor)),
        );
        candidates
    }
}

impl Collector for Cursor {
    fn name(&self) -> &'static str {
        "cursor"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(hit) = ini::probe(&Self::candidates())? else {
            return Ok(Vec::new());
        };

        let value = format!("{} ({})", hit.value, hit.origin.label());

        Ok(vec![
            Info::new(self.name(), "Cursor", value)
                .with_variable("name", hit.value)
                .with_variable("origin", hit.origin.label()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_user_level_comes_before_the_distribution_defaults() {
        let candidates = Cursor::candidates();
        let origins: Vec<Origin> = candidates.iter().map(Probe::origin).collect();

        assert_eq!(
            origins.first(),
            Some(&Origin::Gtk),
            "用户 GTK 的 settings.ini 要排在最前"
        );
        assert_eq!(
            origins.last(),
            Some(&Origin::Xcursor),
            "发行版自带的 index.theme 垫底"
        );

        // 用户自己的 `~/.icons` 那份要排在数据目录里那份之前。
        // 两段来源标签一样，只有比路径才分得出用户级与系统级。
        let position = |path: &str| candidates.iter().position(|probe| probe.path() == path);

        if let (Some(user), Some(system)) = (
            ini::cursor_user_index_path().as_deref().and_then(position),
            ini::cursor_system_index_paths()
                .first()
                .map(String::as_str)
                .and_then(position),
        ) {
            assert!(user < system, "用户自己装的 index.theme 优先");
        }
    }

    #[test]
    fn collects_on_this_machine_if_a_cursor_theme_is_configured() {
        let entries = Cursor.collect(&Context::for_tests()).unwrap();

        for info in &entries {
            assert_eq!(info.module, "cursor");
            assert_eq!(info.key, "Cursor");
            assert!(!info.value.is_empty());
            assert!(!info.value.contains('\n'));
            assert!(info.variable("origin").is_some());
        }
    }

    #[test]
    fn the_module_name_matches_the_config_vocabulary() {
        assert_eq!(Cursor.name(), "cursor");
    }
}
