//! Icons：图标主题名。
//!
//! 和 [`super::theme`] 一样的两支来源，键不同：
//!
//! - GTK 的 `gtk-icon-theme-name`（`settings.ini`，同一个链）；
//! - KDE 的 `kdeglobals` `[Icons] Theme`。
//!
//! 顺序和 [`super::theme`] 一个规矩：用户级（GTK → KDE）在前，发行版默认在后。
//!
//! 特意**不**拿 `/usr/share/icons/default/index.theme` 的 `Inherits=` 顶替：
//! 那份是 **光标**主题的兜底声明（见 [`super::cursor`]），跟图标主题是两回事
//! ——本机它就写着 `Adwaita`，而图标主题压根没配，拿它顶上等于编一个。
//!
//! 读不到就是无数据。

use crate::collectors::ini::{self, Probe};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// GTK 侧的键。
const GTK_KEY: &str = "gtk-icon-theme-name";
/// KDE 侧的 section 与键。KDE 把图标主题单列一段，不像 GTK 那样混在 `[Settings]` 里。
const KDE_SECTION: &str = "Icons";
/// 见 [`KDE_SECTION`]。
const KDE_KEY: &str = "Theme";

/// 图标主题。
pub struct Icons;

impl Icons {
    /// 查找线索，按优先级：用户 GTK → 用户 KDE → 系统 GTK → 系统 KDE。
    fn candidates() -> Vec<Probe> {
        let mut candidates = Probe::gtk_user(GTK_KEY);
        candidates.extend(Probe::kconfig_user("kdeglobals", KDE_SECTION, KDE_KEY));
        candidates.extend(Probe::gtk_system(GTK_KEY));
        candidates.extend(Probe::kconfig_system("kdeglobals", KDE_SECTION, KDE_KEY));
        candidates
    }
}

impl Collector for Icons {
    fn name(&self) -> &'static str {
        "icons"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(hit) = ini::probe(&Self::candidates())? else {
            return Ok(Vec::new());
        };

        let value = format!("{} ({})", hit.value, hit.origin.label());

        Ok(vec![
            Info::new(self.name(), "Icons", value)
                .with_variable("name", hit.value)
                .with_variable("origin", hit.origin.label()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_on_this_machine_if_an_icon_theme_is_configured() {
        let entries = Icons.collect(&Context::for_tests()).unwrap();

        for info in &entries {
            assert_eq!(info.module, "icons");
            assert_eq!(info.key, "Icons");
            assert!(!info.value.is_empty());
            assert!(!info.value.contains('\n'));
            assert!(info.variable("origin").is_some());
        }
    }

    #[test]
    fn the_module_name_matches_the_config_vocabulary() {
        assert_eq!(Icons.name(), "icons");
    }
}
