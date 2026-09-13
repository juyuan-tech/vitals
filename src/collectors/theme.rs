//! Theme：界面主题名（控件那一套）。
//!
//! 这项值在 Linux 上没有统一来源，主流是两支：
//!
//! - **GTK 系**把 `gtk-theme-name` 写在 `gtk-3.0/settings.ini`（GTK4 同名的键在
//!   `gtk-4.0/settings.ini`）；
//! - **KDE 系**把配色方案名写在 `kdeglobals` 的 `[General] ColorScheme`。
//!
//! 两边都读，顺序的规矩只有一条：**用户写下的配置排在发行版默认前面**。
//! 用户级是 `~/.config/gtk-3.0|gtk-4.0/settings.ini` 与 `~/.config/kdeglobals`；
//! 系统级是 `$XDG_CONFIG_DIRS`、`/etc`、`$XDG_DATA_DIRS` 下的那几份
//! （发行版常放一份 `/usr/share/gtk-3.0/settings.ini`，本机 Arch 就是）。
//! 同一级之内 GTK 先于 KDE：两边写的是两套东西，都配了的机器上 GTK 那份管着更多应用。
//!
//! 特意**不**让系统级的 GTK 默认压在用户的 `kdeglobals` 上面：那样一个 KDE 用户
//! 自己挑的 `BreezeDark` 会被 `/usr/share/gtk-3.0/settings.ini` 里的 `Adwaita` 顶掉，
//! 而后者只是「这台机器没配过 GTK」时的一份发行版默认。
//!
//! 值后面标出**确实读到**的那一边（`(GTK)` / `(KDE)`）——只标一个，
//! 不写成 fastfetch 的 `[GTK2/3]`：我们读的是 GTK3/4 的 settings.ini，
//! 没读 GTK2，标成 2/3 就是编。
//!
//! 读不到就是**无数据**：为这一项去 fork `gsettings`（还要连 D-Bus）不划算。
//! 宁可不显示，也不拿 `de` / `wm` 的名字倒推一个主题名。

use crate::collectors::ini::{self, Probe};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// GTK 侧的键。
const GTK_KEY: &str = "gtk-theme-name";
/// KDE 侧的 section 与键。
const KDE_SECTION: &str = "General";
/// 见 [`KDE_SECTION`]。KDE 把配色方案名放在这里，等价于 GTK 的主题名。
const KDE_KEY: &str = "ColorScheme";

/// 界面主题。
pub struct Theme;

impl Theme {
    /// 查找线索，按优先级：用户 GTK → 用户 KDE → 系统 GTK → 系统 KDE。
    fn candidates() -> Vec<Probe> {
        let mut candidates = Probe::gtk_user(GTK_KEY);
        candidates.extend(Probe::kconfig_user("kdeglobals", KDE_SECTION, KDE_KEY));
        candidates.extend(Probe::gtk_system(GTK_KEY));
        candidates.extend(Probe::kconfig_system("kdeglobals", KDE_SECTION, KDE_KEY));
        candidates
    }
}

impl Collector for Theme {
    fn name(&self) -> &'static str {
        "theme"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(hit) = ini::probe(&Self::candidates())? else {
            return Ok(Vec::new());
        };

        let value = format!("{} ({})", hit.value, hit.origin.label());

        Ok(vec![
            Info::new(self.name(), "Theme", value)
                .with_variable("name", hit.value)
                .with_variable("origin", hit.origin.label()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_on_this_machine_if_a_theme_is_configured() {
        // 两种结局都算通过：这台机器没配主题时就是无数据。
        let entries = Theme.collect(&Context::for_tests()).unwrap();

        for info in &entries {
            assert_eq!(info.module, "theme");
            assert_eq!(info.key, "Theme");
            assert!(!info.value.is_empty());
            assert!(!info.value.contains('\n'), "值里不许有换行");
            assert!(
                info.variable("origin").is_some(),
                "读到了就得说清是从 GTK 还是 KDE 读的"
            );
        }
    }

    #[test]
    fn user_level_sources_come_before_distribution_defaults() {
        // 这条钉住整个模块的优先级规则，也是与「GTK 全排在 KDE 前面」那种排法
        // 最关键的区别：用户自己写的 kdeglobals 必须排在系统级 GTK 默认之前，
        // 否则发行版塞在 /usr/share/gtk-3.0/settings.ini 里的 Adwaita 会顶掉
        // 一个 KDE 用户挑的 BreezeDark。
        //
        // 只能比路径：用户级与系统级的来源标签都是 `GTK` / `KDE`。
        let candidates = Theme::candidates();
        let position = |path: &str| candidates.iter().position(|probe| probe.path() == path);

        let system = ini::gtk_system_settings_paths();
        let system_gtk = system
            .first()
            .map(String::as_str)
            .and_then(position)
            .expect("系统级 GTK 的候选总该有");

        if let Some(user_gtk) = ini::gtk_user_settings_paths().first().map(String::as_str) {
            assert!(
                position(user_gtk).expect("用户级 GTK 候选一定在表里") < system_gtk,
                "用户 GTK 要排在系统 GTK 前面"
            );
        }
        if let Some(user_kde) = ini::kconfig_user_path("kdeglobals")
            .as_deref()
            .and_then(position)
        {
            assert!(
                user_kde < system_gtk,
                "用户的 kdeglobals 要排在系统 GTK 默认前面"
            );
        }
    }

    #[test]
    fn within_a_level_gtk_is_asked_first() {
        // 同级之内 GTK 先于 KDE：这是口味上的选择（都配了的机器上 GTK 那份管着
        // 更多应用），不是硬规则，但它也得稳定，否则同一台机器两次运行可能不一样。
        let candidates = Theme::candidates();
        let position = |path: &str| candidates.iter().position(|probe| probe.path() == path);
        let first = |paths: Vec<String>| -> Option<usize> {
            paths.first().map(String::as_str).and_then(position)
        };

        if let (Some(gtk), Some(kde)) = (
            first(ini::gtk_user_settings_paths()),
            ini::kconfig_user_path("kdeglobals")
                .as_deref()
                .and_then(position),
        ) {
            assert!(gtk < kde, "用户级里 GTK 要排在 KDE 前面");
        }
        if let (Some(gtk), Some(kde)) = (
            first(ini::gtk_system_settings_paths()),
            first(ini::kconfig_system_paths("kdeglobals")),
        ) {
            assert!(gtk < kde, "系统级里 GTK 也要排在 KDE 前面");
        }
    }

    #[test]
    fn the_key_is_worded_like_fastfetch() {
        // 键名与 fastfetch 对齐，从 fastfetch 过来的人不用重新学。
        assert_eq!(Theme.name(), "theme");
    }
}
