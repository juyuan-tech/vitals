//! 文本渲染：Logo 在左、信息在右，画面垂直居中。
//!
//! 三条硬规矩：
//!
//! 1. **对齐按显示宽度算**，既不按字节数、也不按 `chars().count()`（`PLAN.md` §6.1）。
//!    `"a\u{0301}b"` 的字节数是 4、字符数是 3，而它在终端上占 **2** 列。
//!    拿前两个数去补空格，遇到 CJK 宽字符或组合字符就会歪掉。
//! 2. **渲染器只描述颜色，不决定要不要上色**。这里永远带转义码，由 anstream
//!    在写出去时按「是不是终端」决定去留，所以 `vitals | cat` 干干净净。
//! 3. **不知道终端多宽就不隐藏 Logo**。宁可多画一张图，也不要因为猜了个 80
//!    就把用户的 Logo 悄悄吃掉。

use std::io::{self, Write};

use anstyle::{AnsiColor, Style};
use unicode_width::UnicodeWidthStr;

use crate::collectors::env;
use crate::core::info::Info;
use crate::core::render::{Logo, RenderError, Renderer, Report};
use crate::render::logo as logos;
use crate::render::theme::Theme;

/// 键与值之间的分隔。
const SEPARATOR: &str = ": ";

/// Logo 与信息之间留几列。
const GAP: usize = 2;

/// 文本渲染器。
#[derive(Debug)]
pub struct TextRenderer {
    theme: Theme,
    /// 写死的列数。`None` 表示渲染时去问终端。
    fixed_columns: Option<usize>,
}

impl TextRenderer {
    /// 按终端实际情况排版。
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            fixed_columns: None,
        }
    }

    /// 把列数写死，测试用。
    #[must_use]
    pub fn with_columns(theme: Theme, columns: usize) -> Self {
        Self {
            theme,
            fixed_columns: Some(columns),
        }
    }

    /// 这次渲染能用多少列。`None` 表示问不出来。
    #[must_use]
    pub fn columns(&self) -> Option<usize> {
        self.fixed_columns.or_else(terminal_columns)
    }
}

impl Renderer for TextRenderer {
    fn render(&self, report: &Report<'_>, out: &mut dyn Write) -> Result<(), RenderError> {
        let lines = layout(report.entries);
        let info_width = lines.iter().map(Line::width).max().unwrap_or(0);

        // 放不下就不画 Logo；问不出宽度则照画（见文件头第 3 条）。
        let logo = report.logo.filter(|logo| match self.columns() {
            Some(columns) => logo_width(logo) + GAP + info_width <= columns,
            None => true,
        });

        let art: Vec<&str> = logo
            .map(|logo| logo.art.lines().collect())
            .unwrap_or_default();
        let art_width = logo.map_or(0, logo_width);
        let color = logo.map(|logo| logos::color_of(logo.id));

        // 信息行比画面高时，画面上下各留一点，看起来才是居中的。
        let top = lines.len().saturating_sub(art.len()) / 2;
        let height = lines.len().max(art.len());

        for index in 0..height {
            let art_line = index
                .checked_sub(top)
                .and_then(|position| art.get(position).copied())
                .map(str::trim_end);
            let line = lines.get(index);

            match (art_line.zip(color), line) {
                (Some((art_line, color)), Some(line)) => {
                    // 画面按最宽那行补齐，右边的信息列才会是一条直线。
                    let pad = art_width.saturating_sub(display_width(art_line)) + GAP;
                    write_art(out, art_line, color)?;
                    write_spaces(out, pad)?;
                    write_row(out, line, self.theme)?;
                }
                (Some((art_line, color)), None) => {
                    write_art(out, art_line, color)?;
                    writeln!(out)?;
                }
                (None, Some(line)) => write_row(out, line, self.theme)?,
                (None, None) => {}
            }
        }

        Ok(())
    }
}

/// 一条待显示的信息行。
#[derive(Debug)]
struct Line {
    /// 键，还没补空格。
    key: String,
    /// 键左边要补几格，才能和所有键右对齐。
    padding: usize,
    /// 值。
    value: String,
}

impl Line {
    /// 这一行占多少列。
    ///
    /// 算的是**纯文本**：颜色转义码不占列，所以这里不把它们算进去。
    fn width(&self) -> usize {
        self.padding + display_width(&self.key) + SEPARATOR.len() + display_width(&self.value)
    }
}

/// 把所有键按最宽的那个右对齐。
fn layout(entries: &[Info]) -> Vec<Line> {
    let widest = entries
        .iter()
        .map(|info| display_width(&info.key))
        .max()
        .unwrap_or(0);

    entries
        .iter()
        .map(|info| Line {
            key: info.key.clone(),
            padding: widest.saturating_sub(display_width(&info.key)),
            value: info.value.clone(),
        })
        .collect()
}

/// 一段文本在终端上占多少列。
///
/// `PLAN.md` §6.1 点名的坑：同一个 `"a\u{0301}b"`，字节数是 4、字符数是 3、
/// 显示宽度是 **2**。只有最后一个数能用来补空格。
///
/// 注意 `unicode-width` 0.2 起 `str::width()` 返回 `usize`，
/// 而 `char::width()` 返回 `Option<usize>`——照 0.1 的教程写会编译不过。
///
/// 它也不认转义码。这里无所谓：版式永远在**还没上色**的文本上算，
/// 颜色是写出去的那一刻才加的。真出现自带转义码的值要靠别的办法量。
#[must_use]
pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Logo 最宽那一行占多少列。
fn logo_width(logo: &Logo) -> usize {
    logo.art.lines().map(display_width).max().unwrap_or(0)
}

/// 终端有多少列。
///
/// 顺序：问内核 → 看 `COLUMNS` → 承认不知道。
/// 走 stdout 而不是 `/dev/tty`：输出是管道时「多少列」本来就没有确定答案，
/// 而那正是「不知道」该有的回答（调用方据此不隐藏 Logo）。
fn terminal_columns() -> Option<usize> {
    if let Ok(size) = rustix::termios::tcgetwinsize(std::io::stdout()) {
        let columns = usize::from(size.ws_col);
        if columns > 0 {
            return Some(columns);
        }
    }

    env::var("COLUMNS")
        .and_then(|columns| columns.parse::<usize>().ok())
        .filter(|columns| *columns > 0)
}

/// 写画面的左半边。不带换行——后面可能还要接信息。
fn write_art(out: &mut dyn Write, art: &str, color: AnsiColor) -> io::Result<()> {
    let style = Style::new().fg_color(Some(color.into()));
    write!(out, "{}{art}{}", style.render(), style.render_reset())
}

/// 写一条信息行：键右对齐、上色，然后分隔符，再是值。
fn write_row(out: &mut dyn Write, line: &Line, theme: Theme) -> io::Result<()> {
    write_spaces(out, line.padding)?;
    write!(
        out,
        "{}{}{}",
        theme.key.render(),
        line.key,
        theme.key.render_reset()
    )?;
    out.write_all(SEPARATOR.as_bytes())?;
    writeln!(
        out,
        "{}{}{}",
        theme.value.render(),
        line.value,
        theme.value.render_reset()
    )
}

/// 补空格。
fn write_spaces(out: &mut dyn Write, count: usize) -> io::Result<()> {
    for _ in 0..count {
        out.write_all(b" ")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_uses_display_columns() {
        // 计划里点名的那个例子：三个数各不相同，只有显示宽度是对的。
        let text = "a\u{0301}b";
        assert_eq!(text.len(), 4, "字节数");
        assert_eq!(text.chars().count(), 3, "字符数");
        assert_eq!(display_width(text), 2, "这才是终端上占的列数");
    }

    #[test]
    fn wide_characters_count_as_two_columns() {
        assert_eq!(display_width("中文"), 4);
        assert_eq!(display_width("OS"), 2);
        assert_eq!(display_width(""), 0);
    }
}
