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
//!
//! 另外两种行不是「键: 值」：
//!
//! - **空键**（`title`）只印值，不补键、不写分隔符——它就是标题。
//! - **`separator`** 印一条横线，长度取决于其它行有多宽，所以只有渲染器知道该多长：
//!   采集器发一个空条目当标记，线在 `render` 里才铺出来。

use std::io::{self, Write};

use anstyle::{AnsiColor, Style};
use unicode_width::UnicodeWidthStr;

use crate::collectors::{env, tty};
use crate::core::info::Info;
use crate::core::render::{Logo, RenderError, Renderer, Report};
use crate::render::logo as logos;
use crate::render::theme::Theme;

/// 键与值之间的分隔。
const SEPARATOR: &str = ": ";

/// Logo 与信息之间留几列。
const GAP: usize = 2;

/// 横线用的字符。
///
/// `─`（U+2500）在 Unicode 里是「宽度不定」的：终端按 CJK 宽字符模式算时它占两列。
/// 但只有这一行会因此变长，别的行的对齐不依赖它，所以代价只是横线可能比信息列长一点。
const RULE: char = '─';

/// 只发标记、由渲染器铺线的模块。
const RULE_MODULE: &str = "separator";

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
        let mut lines = layout(report.entries);

        // 信息列有多宽，只由「键: 值」这类行决定——分隔线自己不算数，
        // 否则它会去够自己的长度。
        let info_width = lines
            .iter()
            .filter(|line| !line.rule)
            .map(Line::width)
            .max()
            .unwrap_or(0);

        // 线有多长，现在才量得出来。
        for line in &mut lines {
            if line.rule {
                line.value = RULE.to_string().repeat(info_width);
            }
        }

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
    /// 键，还没补空格。**空键表示这不是「键: 值」行**：
    /// 标题就是空的键，空行与分隔线则是空键加空值。
    key: String,
    /// 键左边要补几格，才能和所有键右对齐。
    padding: usize,
    /// 值。分隔线的值在 `render` 里才填上，因为那时才知道该铺多长。
    value: String,
    /// 是不是分隔线。
    rule: bool,
}

impl Line {
    /// 这一行占多少列。
    ///
    /// 算的是**纯文本**：颜色转义码不占列，所以这里不把它们算进去。
    fn width(&self) -> usize {
        if self.key.is_empty() {
            // 无键行：既没有键也没有 `: `，值有多宽就多宽。
            return display_width(&self.value);
        }

        self.padding + display_width(&self.key) + SEPARATOR.len() + display_width(&self.value)
    }
}

/// 把所有键按最宽的那个右对齐。
fn layout(entries: &[Info]) -> Vec<Line> {
    // 无键行不参与键对齐：拿一个空键去和别人比宽窄，只会把最宽键的宽度算对，
    // 却让自己多出一段没意义的前置空格。
    let widest = entries
        .iter()
        .filter(|info| !info.key.is_empty())
        .map(|info| display_width(&info.key))
        .max()
        .unwrap_or(0);

    entries
        .iter()
        .map(|info| {
            let keyless = info.key.is_empty();
            Line {
                key: info.key.clone(),
                padding: if keyless {
                    0
                } else {
                    widest.saturating_sub(display_width(&info.key))
                },
                value: info.value.clone(),
                rule: info.module == RULE_MODULE,
            }
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
    tty::size().map(|(columns, _)| columns).or_else(|| {
        env::var("COLUMNS")
            .and_then(|columns| columns.parse::<usize>().ok())
            .filter(|columns| *columns > 0)
    })
}

/// 写画面的左半边。不带换行——后面可能还要接信息。
fn write_art(out: &mut dyn Write, art: &str, color: AnsiColor) -> io::Result<()> {
    let style = Style::new().fg_color(Some(color.into()));
    write!(out, "{}{art}{}", style.render(), style.render_reset())
}

/// 写一条信息行：键右对齐、上色，然后分隔符，再是值。
fn write_row(out: &mut dyn Write, line: &Line, theme: Theme) -> io::Result<()> {
    if line.key.is_empty() {
        // 无键行。空值就是空行（`break`），非空值当标题使——标题用键的样式，
        // 它本来就是这一段的主标题。
        if line.value.is_empty() {
            return writeln!(out);
        }

        let style = if line.rule { theme.value } else { theme.key };
        return writeln!(
            out,
            "{}{}{}",
            style.render(),
            line.value,
            style.render_reset()
        );
    }

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
