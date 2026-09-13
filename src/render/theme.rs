//! 配色：哪些东西上什么色。
//!
//! 这里**只描述**样式，不决定要不要上色。降级是 `anstream` 的事：
//! 输出不是终端时它会把转义码剥掉，而且认得 `NO_COLOR` / `CLICOLOR_FORCE`。
//! 所以渲染器可以放心地总是带着颜色——`vitals | cat` 不会看到转义码。

use anstyle::{AnsiColor, Effects, Style};

/// 一套配色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// 键。
    pub key: Style,
    /// 值。
    pub value: Style,
}

impl Default for Theme {
    /// 键用青色加粗，值不上色。
    ///
    /// 值刻意留成终端默认前景色，而不是写死白色：白字在浅色背景的终端上等于看不见。
    /// 「键有色、值没色」已经足够把两者分开了，不需要给值也指定一个颜色。
    fn default() -> Self {
        Self {
            key: Style::new()
                .fg_color(Some(AnsiColor::Cyan.into()))
                .effects(Effects::BOLD),
            value: Style::new(),
        }
    }
}
