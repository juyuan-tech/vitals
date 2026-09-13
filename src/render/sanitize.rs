//! 把**外部字符串**里的终端控制字符去掉再显示。
//!
//! 为什么必须有这一层：vitals 显示的东西里有相当一部分不归我们管——挂载点的卷标、
//! utmp 里的用户名与来源主机、EDID 里的型号、由 `$HOME`/`$TMPDIR` 这类环境变量拼出来的
//! 路径。这些字符串里只要混进一个 ESC（`\u{1b}`），就能在终端上移动光标、改窗口标题、
//! 甚至触发 OSC 52 之类的序列：屏幕上写着「系统信息」，画面却由那串字符编排。
//!
//! 去掉的是**控制字符**，不是「非 ASCII」：
//!
//! - C0（U+0000–U+001F）、DEL（U+007F）、C1（U+0080–U+009F）——转义序列的材料，
//!   顺带也挡掉换行与制表符：它们会把「一行一项」的版式撑破；
//! - 双向文本控制符（U+202A–U+202E、U+2066–U+2069）——能让 `/usr/bin/x` 看起来像别的
//!   路径，属于经典的显示欺骗。
//!
//! 可打印字符与正常宽字符（中文、emoji）一律保留。`--json` 那条路不需要它：
//! `serde_json` 会把控制字符转义成 `\u001b` 这样的形式。

use std::borrow::Cow;

/// 去掉终端控制字符；字符串本来就干净时原样借用，不做多余分配。
///
/// 返回 `Cow` 是因为绝大多数值都是干净的：每次渲染都分配一份新 `String` 没必要。
pub fn sanitize(value: &str) -> Cow<'_, str> {
    if !value.chars().any(is_control_like) {
        return Cow::Borrowed(value);
    }

    Cow::Owned(value.chars().filter(|c| !is_control_like(*c)).collect())
}

/// 显示前要去掉的字符。
fn is_control_like(c: char) -> bool {
    matches!(
        c,
        '\u{0}'..='\u{1f}' | '\u{7f}' | '\u{80}'..='\u{9f}'
            | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_do_not_reach_the_terminal() {
        // 这是本模块存在的理由：卷标里带一个「清屏 + 变红」，屏幕上就成了另一个画面。
        let hostile = "Kingston\u{1b}[2J\u{1b}[31mPWNED";

        assert_eq!(sanitize(hostile), "Kingston[2J[31mPWNED");
    }

    #[test]
    fn newlines_and_tabs_are_dropped_too() {
        // 值里带换行会把「一行一项」撑破，顺带也让 `--explain` 那种对齐失效。
        assert_eq!(sanitize("a\nb\tc"), "abc");
    }

    #[test]
    fn bidi_controls_are_dropped() {
        // U+202E 之后的字符在屏幕上会被反着显示：`/usr/bin/x` 能装成别的路径。
        assert_eq!(sanitize("safe\u{202e}gnp.txt"), "safegnp.txt");
    }

    #[test]
    fn normal_wide_text_survives() {
        assert_eq!(
            sanitize("中文 e\u{301} \u{1f600}"),
            "中文 e\u{301} \u{1f600}"
        );
    }

    #[test]
    fn clean_values_are_borrowed() {
        // 干净的值不该付出一次分配——渲染每台机器都会走到这条路径。
        assert!(matches!(sanitize("plain"), Cow::Borrowed(_)));
        assert!(matches!(sanitize("bad\u{1b}"), Cow::Owned(_)));
    }
}
