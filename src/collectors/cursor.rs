//! Cursor：光标（鼠标指针）主题名。
//!
//! 先看**会话环境**：`$XCURSOR_THEME`（配上 `$XCURSOR_SIZE`）。这是本次会话真正在用的
//! 那一个，Wayland 合成器与 Xcursor 客户端都认它，fastfetch 印的也是它。本机它写着
//! `breeze_cursors`，它印 `breeze (30px)`——后缀 `_cursors` 去掉，尺寸跟着主题写
//! （都是真机并排比对出来的）。
//!
//! 会话里没有才退回配置线索，顺序的规矩是「用户级在发行版默认之前」：
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
//! **这段注释改过一次，改的理由值得留着**：先前特意不读 `$XCURSOR_THEME`，理由是
//! 「它只说明这次会话是谁拉起来的，落盘的配置才是用户的选择」。真机并排比对后发现
//! 反了——它就是本次会话在用的那个（本机 `breeze_cursors`，而落盘的只是
//! `/usr/share/icons/default/index.theme` 里一行 `Inherits=Adwaita` 的系统兜底）。
//! 问「光标主题是什么」，前者才是答案。
//!
//! 值的写法：会话那条给 `breeze (30px)`；配置那条给 `Adwaita (Xcursor)`，括号里是来源
//! （`GTK` 是 `settings.ini`，`Xcursor` 是 `index.theme`）。两种括号装的不是一回事，
//! 所以各走各的：会话那条有尺寸可给，配置那条只能告诉你这是用户配的还是兜底的。

use crate::collectors::env;
use crate::collectors::ini::{self, Origin, Probe};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// GTK 侧的键。
const GTK_KEY: &str = "gtk-cursor-theme-name";
/// `index.theme` 里的 section 与键。
const INDEX_SECTION: &str = "Icon Theme";
/// 见 [`INDEX_SECTION`]。`Inherits` 声明「没指定主题时退到哪一个」。
const INDEX_KEY: &str = "Inherits";

/// 会话环境里的主题变量。**先问它**。
const ENV_THEME: &str = "XCURSOR_THEME";
/// 会话环境里的尺寸变量。
const ENV_SIZE: &str = "XCURSOR_SIZE";
/// 有些主题在变量里带这个后缀，而它自己叫不带后缀的名字（本机 `breeze_cursors`）。
const CURSORS_SUFFIX: &str = "_cursors";
/// 会话来源的标签，与 `Origin` 的两个标签并列。
const ORIGIN_SESSION: &str = "session";

/// 会话里在用的主题：`$XCURSOR_THEME` 去掉 `_cursors` 后缀。空值当没有。
fn session_theme() -> Option<String> {
    let raw = env::var(ENV_THEME)?;
    let theme = raw.trim();

    if theme.is_empty() {
        return None;
    }

    Some(
        theme
            .strip_suffix(CURSORS_SUFFIX)
            .unwrap_or(theme)
            .to_owned(),
    )
}

/// 会话里在用的尺寸：`$XCURSOR_SIZE` 不是正整数就当没有（有主题没尺寸是正常的）。
fn session_size() -> Option<String> {
    let raw = env::var(ENV_SIZE)?;
    let size: u32 = raw.trim().parse().ok()?;

    Some(format!("{size}px"))
}

/// 值的写法：有尺寸就 `breeze (30px)`，没有就只是主题名。
fn describe(theme: &str, size: Option<String>) -> String {
    match size {
        Some(size) => format!("{theme} ({size})"),
        None => theme.to_owned(),
    }
}

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
        // 会话里在用的那个最准，先问它；问不到才去翻配置文件。
        if let Some(theme) = session_theme() {
            let value = describe(&theme, session_size());

            return Ok(vec![
                Info::new(self.name(), "Cursor", value)
                    .with_variable("name", theme)
                    .with_variable("origin", ORIGIN_SESSION),
            ]);
        }

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
    fn the_cursors_suffix_is_stripped_not_the_theme() {
        assert_eq!(strip("breeze_cursors"), "breeze");
        assert_eq!(strip("Adwaita"), "Adwaita", "没后缀就原样");
        assert_eq!(strip("_cursors"), "", "整串就是后缀时给空串，不当成 panic");
    }

    /// 与 [`session_theme`] 同一套规则，但拿字符串当输入，便于用真实样本断言。
    fn strip(raw: &str) -> String {
        raw.strip_suffix(CURSORS_SUFFIX).unwrap_or(raw).to_owned()
    }

    #[test]
    fn the_size_rides_along_only_when_there_is_one() {
        assert_eq!(describe("breeze", Some("30px".to_owned())), "breeze (30px)");
        assert_eq!(describe("breeze", None), "breeze");
    }

    #[test]
    fn the_session_theme_wins_and_matches_this_machine() {
        let entries = Cursor.collect(&Context::for_tests()).unwrap();

        // 本机设了 `$XCURSOR_THEME`，所以走的必须是会话那条（而不是系统兜底的 Adwaita）。
        if let Some(theme) = session_theme() {
            assert_eq!(entries[0].value.split(' ').next(), Some(theme.as_str()));

            let expected = describe(&theme, session_size());
            assert_eq!(entries[0].value, expected);
            assert_eq!(entries[0].variable("origin"), Some(ORIGIN_SESSION));
        }
    }

    #[test]
    fn the_module_name_matches_the_config_vocabulary() {
        assert_eq!(Cursor.name(), "cursor");
    }
}
