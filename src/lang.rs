//! 用哪种语言：帮助文本与运行期文案都看它。
//!
//! 字段名（`OS:`、`Memory:`）本来就是英文，翻译不到；`--gen-config` 打印的配置文件
//! 有中英两份（`config/default.toml` 与 `config/default.en.toml`），跟这里选。
//!
//! 生效语言是**进程级**的：启动时 [`Lang::resolve`] 定一次，之后只读。理由见
//! [`crate::i18n`]：消息在哪里产生（采集器深处的解析函数）和谁来显示离得远，
//! 让解析函数为显示背上语言参数不划算。

use std::sync::OnceLock;

/// 界面语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// 中文。
    Zh,
    /// 英文。
    En,
}

/// 显式指定语言的变量。
pub const LANG_ENV: &str = "VITALS_LANG";

/// 本次运行的生效语言。
static CURRENT: OnceLock<Lang> = OnceLock::new();

/// 定下本次运行的生效语言。进程里只认第一次调用。
///
/// 只有 `main` 调它：语言一旦定下来就不该再变，否则同一个进程里前后两条消息
/// 可能不是同一种语言。
pub fn set_current(lang: Lang) {
    let _ = CURRENT.set(lang);
}

/// 当前生效语言。
///
/// 没人设过就是中文——「拿不准一律中文」这条规矩在全局这一层同样成立，
/// 库的使用者不调 [`set_current`] 时行为与以前一致。
#[must_use]
pub fn current() -> Lang {
    CURRENT.get().copied().unwrap_or(Lang::Zh)
}

impl Lang {
    /// 按 `VITALS_LANG` → `LC_ALL` → `LC_MESSAGES` → `LANG` 的顺序定语言。
    ///
    /// 拿不准一律中文：变量没设、是 `C`/`POSIX`、或者压根不成语言标签时，
    /// 都按中文算。「什么都不设」必须和以前完全一样——不能因为换了台机器就变脸。
    #[must_use]
    pub fn resolve() -> Self {
        Self::from_env(
            std::env::var(LANG_ENV).ok().as_deref(),
            std::env::var("LC_ALL").ok().as_deref(),
            std::env::var("LC_MESSAGES").ok().as_deref(),
            std::env::var("LANG").ok().as_deref(),
        )
    }

    /// [`Self::resolve`] 的纯函数形式（不读环境），便于测试。
    ///
    /// `VITALS_LANG` 只认 `zh*` 与 `en*`；写别的（比如打错字）**当没写**，
    /// 继续往下看 locale，而不是悄悄给一份英文帮助。
    #[must_use]
    pub fn from_env(
        vitals_lang: Option<&str>,
        lc_all: Option<&str>,
        lc_messages: Option<&str>,
        lang: Option<&str>,
    ) -> Self {
        if let Some(tag) = language_tag(vitals_lang) {
            if tag.eq_ignore_ascii_case("zh") {
                return Self::Zh;
            }
            if tag.eq_ignore_ascii_case("en") {
                return Self::En;
            }
        }

        for candidate in [lc_all, lc_messages, lang] {
            if let Some(tag) = language_tag(candidate) {
                return if tag.eq_ignore_ascii_case("zh") {
                    Self::Zh
                } else {
                    Self::En
                };
            }
        }

        Self::Zh
    }
}

/// 取语言标签的主体：`zh_CN.UTF-8` → `zh`、`en-US` → `en`。
///
/// `C`、`POSIX`、空串和不成形的值一律 `None`，也就是「没表态」。
fn language_tag(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }

    // 先去掉 `.编码` 与 `@修饰`，再取 `_` / `-` 之前那段。
    let value = value.split('.').next().unwrap_or(value);
    let value = value.split('@').next().unwrap_or(value);
    let tag = value.split(['_', '-']).next().unwrap_or(value);

    if tag.len() < 2 || tag.len() > 8 || !tag.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return None;
    }
    if tag.eq_ignore_ascii_case("c") || tag.eq_ignore_ascii_case("posix") {
        return None;
    }

    Some(tag)
}

#[cfg(test)]
mod tests {
    use super::{Lang, current};

    /// 没人设过时是中文：库被当库用、或者测试里直接调，都不该突然变英文。
    #[test]
    fn the_effective_language_defaults_to_chinese() {
        assert_eq!(current(), Lang::Zh);
    }

    #[test]
    fn nothing_set_stays_chinese() {
        assert_eq!(Lang::from_env(None, None, None, None), Lang::Zh);
    }

    #[test]
    fn the_c_locale_is_not_a_language() {
        assert_eq!(Lang::from_env(None, None, None, Some("C")), Lang::Zh);
        assert_eq!(Lang::from_env(None, None, None, Some("POSIX")), Lang::Zh);
        assert_eq!(Lang::from_env(None, None, None, Some("")), Lang::Zh);
        assert_eq!(Lang::from_env(None, None, None, Some("   ")), Lang::Zh);
    }

    #[test]
    fn chinese_locales_stay_chinese() {
        assert_eq!(
            Lang::from_env(None, None, None, Some("zh_CN.UTF-8")),
            Lang::Zh
        );
        assert_eq!(Lang::from_env(None, None, None, Some("zh_TW")), Lang::Zh);
        assert_eq!(
            Lang::from_env(None, Some("zh_CN.UTF-8"), None, None),
            Lang::Zh
        );
    }

    #[test]
    fn any_other_locale_gets_english() {
        assert_eq!(
            Lang::from_env(None, None, None, Some("en_US.UTF-8")),
            Lang::En
        );
        assert_eq!(
            Lang::from_env(None, None, None, Some("de_DE.UTF-8")),
            Lang::En
        );
        assert_eq!(Lang::from_env(None, None, None, Some("ja-JP")), Lang::En);
    }

    #[test]
    fn explicit_override_wins() {
        assert_eq!(
            Lang::from_env(Some("en"), Some("zh_CN.UTF-8"), None, Some("zh_CN.UTF-8")),
            Lang::En
        );
        assert_eq!(
            Lang::from_env(Some("zh"), Some("en_US.UTF-8"), None, Some("en_US.UTF-8")),
            Lang::Zh
        );
        assert_eq!(Lang::from_env(Some("EN"), None, None, None), Lang::En);
        assert_eq!(
            Lang::from_env(Some("en_GB.UTF-8"), None, None, None),
            Lang::En
        );
    }

    #[test]
    fn a_typo_falls_through_to_the_locale() {
        // `english` 是个词、不是语言标签，认不出来 → 当没写。
        assert_eq!(
            Lang::from_env(Some("english"), None, None, Some("en_US.UTF-8")),
            Lang::En
        );
        assert_eq!(
            Lang::from_env(Some("1"), None, None, Some("zh_CN.UTF-8")),
            Lang::Zh
        );
        assert_eq!(Lang::from_env(Some("1"), None, None, None), Lang::Zh);
    }

    #[test]
    fn lc_all_beats_lc_messages_beats_lang() {
        assert_eq!(
            Lang::from_env(
                None,
                Some("en_US.UTF-8"),
                Some("zh_CN.UTF-8"),
                Some("zh_CN.UTF-8")
            ),
            Lang::En
        );
        assert_eq!(
            Lang::from_env(None, None, Some("zh_CN.UTF-8"), Some("en_US.UTF-8")),
            Lang::Zh
        );
        // 前面是 C（没表态）时继续往后看。
        assert_eq!(
            Lang::from_env(None, Some("C"), None, Some("en_US.UTF-8")),
            Lang::En
        );
    }
}
