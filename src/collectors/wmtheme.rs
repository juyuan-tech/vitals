//! WM Theme：窗口装饰主题（标题栏、边框那一套）。
//!
//! **只有 KDE 读得到**。KWin 把装饰主题写在 `~/.config/kwinrc` 的
//! `[org.kde.kdecoration2] theme`，那是个 ini，直接读就行，零子进程。
//!
//! 有的发行版/主题只写 `[org.kde.kdecoration2] library`（装饰库的名字，
//! 例如 `org.kde.breeze`）而没有 `theme`——那时是**无数据**：
//! 库名不是主题名，拿它顶替就是编一个用户没配过的值。
//!
//! 别的桌面为什么读不到（都在「不开子进程」这条线内）：
//!
//! - **GNOME / Mutter**：装饰主题存在 dconf 里，落盘形态是 `~/.config/dconf/user`
//!   这个 GVDB **二进制**库。要读它得自己实现 GVDB 解析或链 libdconf，两样都不划算；
//!   官方工具 `gsettings` 正是子进程，正是这一批不许用的东西。
//! - **Xfce**：走 XFConf 的 D-Bus 服务（`xfconf-query` 是子进程）。
//! - **平铺合成器**（Niri、Sway、Hyprland……）：根本没有「窗口装饰主题」这个概念
//!   ——本机跑的就是 Niri，窗口没有标题栏，这一项本来就没有答案。
//!
//! 所以这些机器上宁可不显示这一行，也不拿 `wm` 的名字去凑一个值。

use crate::collectors::ini::{self, Probe};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 装饰主题所在的文件与位置。
const FILE: &str = "kwinrc";
/// 见 [`FILE`]。KWin 的窗口装饰配置段。
const SECTION: &str = "org.kde.kdecoration2";
/// 见 [`SECTION`]。装饰主题名（例如 `Breeze`）。
const KEY: &str = "theme";

/// 窗口装饰主题。
pub struct WmTheme;

impl WmTheme {
    /// 查找线索：KWin 的配置链（用户配置目录 → 系统配置目录）。
    fn candidates() -> Vec<Probe> {
        let mut candidates: Vec<Probe> = Probe::kconfig_user(FILE, SECTION, KEY)
            .into_iter()
            .collect();
        candidates.extend(Probe::kconfig_system(FILE, SECTION, KEY));
        candidates
    }
}

impl Collector for WmTheme {
    fn name(&self) -> &'static str {
        "wmtheme"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(hit) = ini::probe(&Self::candidates())? else {
            return Ok(Vec::new());
        };

        // 这里不标来源：WMTheme 只有 KDE 一条路，标了也不带任何信息。
        Ok(vec![
            Info::new(self.name(), "WM Theme", hit.value.clone())
                .with_variable("name", hit.value)
                .with_variable("origin", hit.origin.label()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::ini::Origin;

    #[test]
    fn only_kwins_configuration_is_consulted() {
        let candidates = WmTheme::candidates();

        assert!(!candidates.is_empty(), "至少要有 ~/.config/kwinrc 这一条");
        assert!(
            candidates.iter().all(|probe| probe.origin() == Origin::Kde),
            "WMTheme 没有第二个数据源"
        );
        // 用户配置目录的 kwinrc 在系统配置目录的前面。
        if let Some(user) = ini::kconfig_user_path(FILE) {
            assert_eq!(
                ini::kconfig_paths(FILE).first(),
                Some(&user),
                "用户的 kwinrc 优先"
            );
        }
    }

    #[test]
    fn only_the_theme_key_counts_never_the_library() {
        // 只给 `library` 的发行版上必须是无数据：库名（`org.kde.breeze`）
        // 不是主题名，不能拿它顶上。这条靠「读的是 `theme` 这个键」保证。
        let section = "org.kde.kdecoration2";
        let ini = ini::Ini::parse(&format!(
            "[{section}]\nlibrary=org.kde.breeze\nlibraryVersion=2\n"
        ));

        assert_eq!(ini.get(section, "theme"), None);
        assert_eq!(ini.get(section, "library"), Some("org.kde.breeze"));
    }

    #[test]
    fn collects_on_this_machine_if_kwin_left_a_theme() {
        let entries = WmTheme.collect(&Context::for_tests()).unwrap();

        for info in &entries {
            assert_eq!(info.module, "wmtheme");
            assert_eq!(info.key, "WM Theme");
            assert!(!info.value.is_empty());
            assert!(!info.value.contains('\n'));
        }
    }

    #[test]
    fn the_module_name_matches_the_config_vocabulary() {
        assert_eq!(WmTheme.name(), "wmtheme");
    }
}
