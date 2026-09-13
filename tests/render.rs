//! 阶段 5 验收：文本渲染。
//!
//! 这里直接拿 `TextRenderer` 渲染到内存，所以能把版式一个字符一个字符地钉住。
//! 「真的把二进制跑起来」的那些在 `binary.rs`。

use std::io::Write;

use anstream::{AutoStream, ColorChoice};
use anstyle::Style;

use vitals_rs::Info;
use vitals_rs::core::render::{Logo, Renderer, Report};
use vitals_rs::render::text::TextRenderer;
use vitals_rs::render::theme::Theme;

/// 不上色的配色：这样输出里只剩版式，断言看得清。
fn plain() -> Theme {
    Theme {
        key: Style::new(),
        value: Style::new(),
    }
}

/// 一张两行的假图，宽度刚好手算得清。
const ART: &str = "##\n##";

fn logo() -> Logo {
    Logo {
        id: "test",
        art: ART,
    }
}

fn entries() -> Vec<Info> {
    vec![
        Info::new("os", "OS", "Arch Linux"),
        Info::new("kernel", "Kernel", "7.2.4-arch1-2"),
    ]
}

fn render_raw(renderer: &TextRenderer, logo: Option<&Logo>, entries: &[Info]) -> Vec<u8> {
    let mut buffer = Vec::new();
    renderer
        .render(&Report::new(logo, entries, &[]), &mut buffer)
        .expect("写进内存不会失败");

    buffer
}

/// 渲染成**管道里看到的样子**：颜色被 anstream 剥掉，只剩版式。
///
/// 版式断言都走这里，量的是用户真能看见的文本，断言里也不用塞转义码。
/// （Logo 的颜色来自发行版表而不是 `Theme`，所以就算用 `plain()` 它也会带色。）
fn render(renderer: &TextRenderer, logo: Option<&Logo>, entries: &[Info]) -> String {
    let mut buffer = Vec::new();
    {
        let mut stream = AutoStream::new(&mut buffer, ColorChoice::Never);
        renderer
            .render(&Report::new(logo, entries, &[]), &mut stream)
            .expect("写进内存不会失败");
        stream.flush().unwrap();
    }

    String::from_utf8(buffer).expect("渲染出来的是 UTF-8")
}

// ---------------------------------------------------------------------------
// 版式
// ---------------------------------------------------------------------------

#[test]
fn logo_on_the_left_information_on_the_right() {
    let text = render(
        &TextRenderer::with_columns(plain(), 40),
        Some(&logo()),
        &entries(),
    );

    // 键左对齐（fastfetch 的口径）：都从画面 2 列 + 空隙 2 列之后的同一列开始，
    // 冒号因此参差——`OS:` 在 4，`Kernel:` 也在 4。
    assert_eq!(
        text,
        "##  OS: Arch Linux\n\
         ##  Kernel: 7.2.4-arch1-2\n"
    );
}

#[test]
fn without_a_logo_the_information_keeps_its_own_alignment() {
    let text = render(&TextRenderer::with_columns(plain(), 40), None, &entries());

    assert_eq!(text, "OS: Arch Linux\nKernel: 7.2.4-arch1-2\n");
}

#[test]
fn a_narrow_terminal_drops_the_logo_but_keeps_the_information() {
    let text = render(
        &TextRenderer::with_columns(plain(), 20),
        Some(&logo()),
        &entries(),
    );

    assert!(!text.contains("##"), "放不下就不该画：\n{text}");
    assert!(text.contains("Kernel: 7.2.4-arch1-2"));
}

#[test]
fn the_logo_appears_exactly_when_it_fits() {
    // 需要 2(画面) + 2(空隙) + 21(最宽的 "Kernel: 7.2.4-arch1-2") = 25 列。
    let tight = render(
        &TextRenderer::with_columns(plain(), 24),
        Some(&logo()),
        &entries(),
    );
    assert!(!tight.contains("##"), "24 列差一列：\n{tight}");

    let fits = render(
        &TextRenderer::with_columns(plain(), 25),
        Some(&logo()),
        &entries(),
    );
    assert!(fits.contains("##"), "25 列刚好：\n{fits}");
}

#[test]
fn the_logo_is_centred_vertically() {
    let entries: Vec<Info> = ["OS", "Kernel", "Uptime", "CPU"]
        .iter()
        .map(|key| Info::new("m", *key, "v"))
        .collect();

    let text = render(
        &TextRenderer::with_columns(plain(), 40),
        Some(&logo()),
        &entries,
    );
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(lines.len(), 4, "信息有几行就出几行");
    assert_eq!(
        lines[0], "    OS: v",
        "（4-2)/2 = 1：第一行留给画面，但信息列仍要空出画面那一列（2 + 间隔 2）；键左对齐"
    );
    assert!(lines[1].starts_with("##"), "画面从第二行开始：{lines:?}");
    assert!(lines[2].starts_with("##"));
    assert_eq!(
        lines[3], "    CPU: v",
        "画面比信息短，最后一行只剩信息——依然要在同一列上"
    );
}

#[test]
fn a_logo_taller_than_the_information_keeps_its_extra_lines() {
    let tall = Logo {
        id: "test",
        art: "A\nB\nC\nD",
    };
    let text = render(
        &TextRenderer::with_columns(plain(), 40),
        Some(&tall),
        &[Info::new("m", "OS", "v")],
    );

    assert_eq!(text, "A  OS: v\nB\nC\nD\n");
}

#[test]
fn nothing_to_show_writes_nothing() {
    // 配置里 `modules = []` 是合法的「什么都不显示」，不是错误。
    assert_eq!(
        render(&TextRenderer::with_columns(plain(), 40), None, &[]),
        ""
    );
}

// ---------------------------------------------------------------------------
// 显示宽度
// ---------------------------------------------------------------------------

#[test]
fn the_width_math_uses_columns_not_bytes() {
    // 全是汉字：4 个字 = 8 列，但 12 字节。按字节算会得出 20 列才放得下，
    // 于是 16 列这条就通不过——这条测试正是用来分开这两种算法的。
    let entries = [Info::new("m", "OS", "中文中文")];
    let art = Logo { id: "t", art: "##" };

    let fits = render(
        &TextRenderer::with_columns(plain(), 16),
        Some(&art),
        &entries,
    );
    assert!(fits.contains("##"), "2+2+12 = 16 列刚好放得下：\n{fits}");

    let tight = render(
        &TextRenderer::with_columns(plain(), 15),
        Some(&art),
        &entries,
    );
    assert!(!tight.contains("##"), "15 列就不行：\n{tight}");
}

// ---------------------------------------------------------------------------
// 颜色：这里描述，anstream 决定去留
// ---------------------------------------------------------------------------

#[test]
fn colors_are_stripped_when_the_output_is_not_a_terminal() {
    // 阶段 5 的验收之一：管道输出无转义码。渲染器照常描述颜色……
    let colored = String::from_utf8(render_raw(
        &TextRenderer::with_columns(Theme::default(), 40),
        None,
        &entries(),
    ))
    .unwrap();
    assert!(colored.contains('\u{1b}'), "渲染器该照常带颜色");

    // ……anstream 在写出去的路上把它们剥掉。
    let piped = render(
        &TextRenderer::with_columns(Theme::default(), 40),
        None,
        &entries(),
    );
    assert!(!piped.contains('\u{1b}'), "管道里不该有转义码：{piped}");

    // 剥掉颜色之后，版式必须和不带配色的渲染一模一样。
    assert_eq!(
        piped,
        render(&TextRenderer::with_columns(plain(), 40), None, &entries())
    );
}

#[test]
fn no_color_strips_them_as_well() {
    let output = render(
        &TextRenderer::with_columns(Theme::default(), 40),
        Some(&logo()),
        &entries(),
    );

    assert!(!output.contains('\u{1b}'));
}

// ---------------------------------------------------------------------------
// 无键行与分隔线：标题、空行、横线
// ---------------------------------------------------------------------------

#[test]
fn a_keyless_line_prints_only_its_value() {
    // 标题行：没有键，就不该补空格、也不该印 `: `。
    let entries = vec![
        Info::new("title", "", "gxyarch@MyArch"),
        Info::new("os", "OS", "Arch Linux"),
    ];

    let output = render(&TextRenderer::with_columns(plain(), 80), None, &entries);

    assert_eq!(output, "gxyarch@MyArch\nOS: Arch Linux\n");
}

#[test]
fn a_keyless_line_does_not_widen_the_key_column() {
    // 空键要是参与了「谁是最宽的键」，别的行就会多出一段没意义的前置空格。
    let entries = vec![
        Info::new("title", "", "gxyarch@MyArch"),
        Info::new("kernel", "Kernel", "7.2.4-arch1-2"),
    ];

    let output = render(&TextRenderer::with_columns(plain(), 80), None, &entries);

    assert_eq!(output, "gxyarch@MyArch\nKernel: 7.2.4-arch1-2\n");
}

#[test]
fn the_separator_is_as_wide_as_the_title() {
    let entries = vec![
        Info::new("title", "", "gxyarch@MyArch"),
        Info::new("separator", "", ""),
        Info::new("os", "OS", "Arch Linux"),
        Info::new("kernel", "Kernel", "7.2.4-arch1-2"),
    ];

    let output = render(&TextRenderer::with_columns(plain(), 80), None, &entries);
    let lines: Vec<&str> = output.lines().collect();

    // 标题 `gxyarch@MyArch` 是 14 列，横线就跟它一样长（fastfetch 的口径）。
    // 信息列里有更宽的 `Kernel: 7.2.4-arch1-2`（21 列），但横线**不**跟着它。
    assert_eq!(lines[1].chars().count(), 14);
    assert_eq!(lines[1], "─".repeat(14));
    assert_eq!(lines[0], "gxyarch@MyArch");
    assert_eq!(
        lines[2], "OS: Arch Linux",
        "键左对齐：从画面之后的同一列开始"
    );
}

#[test]
fn a_separator_alone_is_empty() {
    // 没有别的行，横线就长不了——不能变成一条 0 宽的东西再画出一行空白。
    let entries = vec![Info::new("separator", "", "")];
    let output = render(&TextRenderer::with_columns(plain(), 80), None, &entries);

    assert_eq!(output, "\n");
}

#[test]
fn a_break_is_an_empty_line() {
    let entries = vec![
        Info::new("os", "OS", "Arch Linux"),
        Info::new("break", "", ""),
        Info::new("rust", "Rust", "stable"),
    ];

    let output = render(&TextRenderer::with_columns(plain(), 80), None, &entries);

    // 键左对齐：`Rust` 比 `OS` 宽不影响 OS 的位置，两者都从第 0 列开始。
    assert_eq!(output, "OS: Arch Linux\n\nRust: stable\n");
}

#[test]
fn the_rule_does_not_count_towards_the_logo_decision() {
    // 横线的长度是渲染器算出来的，不该反过来撑大信息列、把 Logo 挤掉。
    let entries = vec![
        Info::new("title", "", "gxyarch@MyArch"),
        Info::new("separator", "", ""),
        Info::new("os", "OS", "Arch Linux"),
    ];

    let output = render(
        &TextRenderer::with_columns(plain(), 40),
        Some(&logo()),
        &entries,
    );

    // Logo 宽 2 + 间隔 2 + 信息列 21 = 25 ≤ 40，画得下。
    assert!(output.contains("##"), "Logo 该画出来：{output}");
}
