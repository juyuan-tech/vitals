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
//! - **`separator`** 印一条横线，**长度跟标题的值一样宽**（fastfetch 的口径），
//!   所以只有渲染器知道该多长：采集器发一个空条目当标记，线在 `render` 里才铺出来。

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

/// 标题模块。分隔线跟它的值一样宽。
const TITLE_MODULE: &str = "title";

/// 只发标记、由渲染器铺色块的模块。
const COLORS_MODULE: &str = "colors";

/// 一排几格色块。
const COLOR_BLOCKS: usize = 8;

/// 一格色块有多宽（跟着 fastfetch：三格）。
const COLOR_BLOCK: &str = "   ";

/// 无条件的重置。
///
/// **不能**用 `Style::render_reset()`：它是从「当前这个 Style 有没有属性」推出来的，
/// 默认 Style 渲染出的是空串。而色块的背景色是由**另一批** Style 设的，行末那个
/// Style 恰好是默认值——于是重置根本没写出去，最后一格背景会一路渗到行尾（真机
/// 字节比对抓到的：我们停在 `\x1b[47m   `，fastfetch 后面还有 `\x1b[m`）。
const COLOR_RESET: &str = "\x1b[0m";

/// 上排：标准 8 色，对应 ANSI 的 `40`-`47`。
const STANDARD_COLORS: [AnsiColor; COLOR_BLOCKS] = [
    AnsiColor::Black,
    AnsiColor::Red,
    AnsiColor::Green,
    AnsiColor::Yellow,
    AnsiColor::Blue,
    AnsiColor::Magenta,
    AnsiColor::Cyan,
    AnsiColor::White,
];

/// 下排：亮色，对应 `100`-`107`。
const BRIGHT_COLORS: [AnsiColor; COLOR_BLOCKS] = [
    AnsiColor::BrightBlack,
    AnsiColor::BrightRed,
    AnsiColor::BrightGreen,
    AnsiColor::BrightYellow,
    AnsiColor::BrightBlue,
    AnsiColor::BrightMagenta,
    AnsiColor::BrightCyan,
    AnsiColor::BrightWhite,
];

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
            // 分隔线不算：它会去够自己的长度。色块算——它真的占列。
            .filter(|line| line.kind != Kind::Rule)
            .map(Line::width)
            .max()
            .unwrap_or(0);

        // 线有多长：**跟标题一样宽**，不是跟信息列一样宽。fastfetch 就是这个口径——
        // 真机对比，标题 `gxyarch@MyArch` 是 14 列，它那条分隔线正好 14 个横线；
        // 我们先前按最宽的信息行铺，铺出 81 个，比标题长出一大截。
        // 没有标题（只选了别的模块）才退回信息列宽度。
        let rule_width = report
            .entries
            .iter()
            .find(|info| info.module == TITLE_MODULE)
            .map(|info| display_width(&info.value))
            .filter(|width| *width > 0)
            .unwrap_or(info_width);

        // 线有多长，现在才量得出来。
        for line in &mut lines {
            if line.kind == Kind::Rule {
                line.value = RULE.to_string().repeat(rule_width);
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
        // 与画面并排的行前面留的是「画面最宽那行 + GAP」。**没有画面同行的行
        // （画面上下留白的那几行）也必须留出一样宽的一列**，否则键的起始列会在中间
        // 跳一下：真机上就是这样——信息块上半部分在第 26 列、与画面并排的在第 65 列。
        let margin = if art.is_empty() { 0 } else { art_width + GAP };

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
                (None, Some(line)) => {
                    write_spaces(out, margin)?;
                    write_row(out, line, self.theme)?;
                }
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
    /// 值。分隔线的值在 `render` 里才填上，因为那时才知道该铺多长。
    value: String,
    /// 这一行是哪一种。三种行的画法互不相干，用一个枚举说清楚，
    /// 比堆两个互斥的 bool 好读。
    kind: Kind,
}

/// 行的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// `键: 值`。
    Info,
    /// 分隔线。值是空的，铺线的那一刻才知道该多长。
    Rule,
    /// 色块。`0` 是标准 8 色（`40`-`47`），`1` 是亮色（`100`-`107`）。
    Colors(u8),
}

impl Line {
    /// 一行色块。
    fn colors(row: u8) -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            kind: Kind::Colors(row),
        }
    }

    /// 这一行占多少列。
    ///
    /// 算的是**纯文本**：颜色转义码不占列，所以这里不把它们算进去。
    fn width(&self) -> usize {
        if let Kind::Colors(_) = self.kind {
            // 色块也占列：这一屏放不放得下 Logo 得把它算进去。
            return COLOR_BLOCKS * COLOR_BLOCK.len();
        }

        if self.key.is_empty() {
            // 无键行：既没有键也没有 `: `，值有多宽就多宽。
            return display_width(&self.value);
        }

        display_width(&self.key) + SEPARATOR.len() + display_width(&self.value)
    }
}

/// 把所有键**左对齐**。
///
/// 键从同一列开始，冒号因此参差——这是 fastfetch 的口径（实测它的键全部从第 42 列
/// 开始，冒号列 44/45/46/48/56…各不相同）。一开始我们做成右对齐（冒号一列），
/// 真机并排比过之后改成它这样：键长短不一时，右对齐会把标题和第一列的空白一起推远，
/// 看起来比它「散」。
fn layout(entries: &[Info]) -> Vec<Line> {
    entries
        .iter()
        .flat_map(|info| {
            // 一条 colors 标记铺成两行（上排标准色、下排亮色）。**在这里就展开**，
            // 后面「与画面并肩」那段才不用知道色块有几行——它只管按行号配画面。
            if info.module == COLORS_MODULE {
                return vec![Line::colors(0), Line::colors(1)];
            }

            let kind = if info.module == RULE_MODULE {
                Kind::Rule
            } else {
                Kind::Info
            };

            vec![Line {
                key: info.key.clone(),
                value: info.value.clone(),
                kind,
            }]
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
    // 色块不走「键: 值」那套：键与值都是空的，背景色由它自己带。
    if let Kind::Colors(row) = line.kind {
        return write_colors(out, row);
    }

    if line.key.is_empty() {
        // 无键行。空值就是空行（`break`），非空值当标题使——标题用键的样式，
        // 它本来就是这一段的主标题。
        if line.value.is_empty() {
            return writeln!(out);
        }

        let style = if line.kind == Kind::Rule {
            theme.value
        } else {
            theme.key
        };
        return writeln!(
            out,
            "{}{}{}",
            style.render(),
            line.value,
            style.render_reset()
        );
    }

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

/// 一行 8 格色块。
///
/// 上排标准 8 色、下排亮色，每格三格宽，行末一个 reset——与 fastfetch 的排法一致。
/// **不跟**它在第二排前面加的那个 `\x1b[5m`（闪烁）：闪烁是用户会专门去关掉的东西。
fn write_colors(out: &mut dyn Write, row: u8) -> io::Result<()> {
    for index in 0..COLOR_BLOCKS {
        let color = if row == 0 {
            STANDARD_COLORS[index]
        } else {
            BRIGHT_COLORS[index]
        };
        let style = Style::new().bg_color(Some(color.into()));
        write!(out, "{}{COLOR_BLOCK}", style.render())?;
    }

    // 用无条件的重置收尾：理由见 `COLOR_RESET`。
    writeln!(out, "{COLOR_RESET}")
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

    fn render_to_string(renderer: &TextRenderer, report: &Report<'_>) -> String {
        let mut out = Vec::new();
        renderer.render(report, &mut out).unwrap();

        String::from_utf8(out).unwrap()
    }

    /// 一屏「标题 + 分隔线 + 四行信息」，配一张三行的假画面。
    fn entries() -> Vec<Info> {
        vec![
            Info::new("title", "", "me@host"),
            Info::new("separator", "", ""),
            Info::new("os", "OS", "Arch Linux"),
            Info::new("host", "Host", "HP"),
            Info::new("kernel", "Kernel", "7.2.4"),
            Info::new("bios", "BIOS (UEFI)", "Insyde F.09"),
        ]
    }

    /// 去掉颜色转义码。
    ///
    /// 渲染器**永远**带转义码，由 anstream 在写出去时按「是不是终端」决定去留
    /// （文件头第 2 条），所以在这种直接接 `Vec<u8>` 的测试里必须先剥掉再量列，
    /// 否则 `\x1b[96m` 这 5 个字符会被当成 5 列宽度。
    fn visible(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();

        while let Some(ch) = chars.next() {
            if ch == '\u{1b}' {
                for next in chars.by_ref() {
                    if next == 'm' {
                        break;
                    }
                }
                continue;
            }
            out.push(ch);
        }

        out
    }

    #[test]
    fn every_key_starts_at_the_same_column_even_around_a_logo() {
        // 真机上被用户看到的问题：信息比画面高时画面会垂直居中，**画面上下留白那几行
        // 没有补画面那一列**，于是键的起始列在中间跳（上半部分与画面并排的差着
        // 「画面宽 + 间隔」）。这里用一张只有三行的假画面把那个分支逼出来。
        let art = Logo {
            id: "arch",
            art: "a\nbb\nccc",
        };
        let entries = entries();
        let report = Report {
            logo: Some(&art),
            entries: &entries,
            failures: &[],
        };
        let text = render_to_string(&TextRenderer::with_columns(Theme::default(), 80), &report);
        let text = visible(&text);

        // 键是**左对齐**的：`键: ` 这个串在哪一列出现，就是这一行的键起始列，
        // 必须处处相同。（右对齐时这个数会随键长变化——那正是先前出问题的地方。）
        let starts: Vec<usize> = ["OS", "Host", "Kernel", "BIOS (UEFI)"]
            .iter()
            .map(|key| {
                let needle = format!("{key}: ");
                let line = text
                    .lines()
                    .find(|line| line.contains(&needle))
                    .unwrap_or_else(|| panic!("没有这一行：{key}\n{text}"));

                line.find(&needle).unwrap()
            })
            .collect();

        assert!(
            starts.windows(2).all(|pair| pair[0] == pair[1]),
            "键该从同一列开始，实际是 {starts:?}：\n{text}"
        );
        // 而且那一列得在画面之后（画面最宽 3 列 + 间隔 2 列）——上下留白处也要让开。
        assert_eq!(starts[0], 5, "键该从画面右边开始：\n{text}");
    }

    #[test]
    fn the_colors_marker_becomes_two_rows_of_blocks() {
        let entries = vec![
            Info::new("os", "OS", "Arch Linux"),
            Info::new("colors", "", ""),
        ];
        let report = Report {
            logo: None,
            entries: &entries,
            failures: &[],
        };
        let text = render_to_string(&TextRenderer::with_columns(Theme::default(), 80), &report);

        // 上排标准色、下排亮色，与 fastfetch 的排法一致（实测它的字节就是这样）。
        for code in ["\u{1b}[40m", "\u{1b}[47m", "\u{1b}[100m", "\u{1b}[107m"] {
            assert!(text.contains(code), "缺少 {code:?}：{text:?}");
        }
        // 它在第二排前面加的闪烁（`\x1b[5m`）我们不跟。
        assert!(!text.contains("\u{1b}[5m"), "不跟着闪：{text:?}");

        let rows: Vec<&str> = text.lines().collect();
        assert_eq!(rows.len(), 3, "一行信息 + 两行色块：{text:?}");
        for row in &rows[1..] {
            assert_eq!(
                visible(row).chars().count(),
                8 * 3,
                "每排 8 格、每格三格宽：{row:?}"
            );
            // **必须**以重置收尾：否则最后一格的背景会渗到行尾。
            // 这里不能用 `Style::render_reset()` 之外的东西来「顺便」满足——
            // 就是因为它对默认 Style 返回空串，才出现过这个 bug。
            assert!(
                row.ends_with("\u{1b}[0m"),
                "色块行要以重置收尾，否则背景渗出：{row:?}"
            );
        }
    }

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
