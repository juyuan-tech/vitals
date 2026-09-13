//! TerminalFont：终端自己用的那套字体。
//!
//! 和 fastfetch 一样，这个值得从**终端自己的配置文件**里读：终端把这个信息放在了
//! `kitty.conf` / `alacritty.toml` / `config`（ghostty）/ `foot.ini` 里，没有任何
//! 环境变量或 `/proc` 能问出「我在用哪套字体」。
//!
//! **不跑 `kitty +kitten`、不跑 `fc-match`**：读一个几百字节的文本文件就够了，
//! 为它开子进程不划算——而且 `kitty.conf` 里写的才是用户真正配的那套。
//!
//! 认不出终端、或者配置文件里没写字体，就是无数据。**不猜**：字体不是那种
//! 「有个大概值也比没有强」的信息，报错了比不报更糟。
//!
//! 已知拿不到的两类：
//!
//! - **GNOME Terminal / VTE 系**：字体存在 dconf 的二进制库里，读它要自己实现
//!   GVariant 解析，为一个字体不值当；
//! - **Konsole**：字体在 profile 文件里，得先解析 `konsolerc` 找到 profile 名，
//!   而且那份是 KDE 的字体描述串（与 `kdeglobals` 同一格式）。等主题那批的
//!   KDE 描述串解析落地后，可以考虑合成一个共用件再来接——现在接就是抄一份。

use crate::collectors::{env, read};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 终端字体。
pub struct TerminalFont;

impl Collector for TerminalFont {
    fn name(&self) -> &'static str {
        "terminal-font"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(config) = locate() else {
            return Ok(Vec::new());
        };
        let Some(text) = read::text(&config.path)? else {
            return Ok(Vec::new());
        };

        let Some(font) = (config.read)(&text) else {
            return Ok(Vec::new());
        };

        Ok(vec![Info::new(self.name(), "Terminal Font", font.render())])
    }
}

/// 一份字体配置。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Font {
    family: String,
    size: Option<String>,
}

impl Font {
    /// `JetBrainsMono Nerd Font 12pt`。
    ///
    /// 字号没有就不写，不做「默认 12」这种猜测。
    fn render(&self) -> String {
        match &self.size {
            Some(size) => format!("{} {}pt", self.family, size),
            None => self.family.clone(),
        }
    }
}

/// 认出来的终端：它的配置文件在哪、怎么读。
struct Candidate {
    path: String,
    read: fn(&str) -> Option<Font>,
}

/// 按环境变量认终端，返回它的字体配置。
///
/// 顺序是刻意的：先看**最具体**的标记（`KITTY_PID` 这种只有本终端会设的），
/// 再看 `TERM`。`TERM` 在嵌套环境下最不可靠（本机它是 `dumb`，
/// 而真正在跑的终端是 kitty）。
fn locate() -> Option<Candidate> {
    let config = config_dir()?;
    let term = env::var("TERM").unwrap_or_default();

    let kitty = env::var("KITTY_PID").is_some()
        || env::var("KITTY_WINDOW_ID").is_some()
        || term.contains("kitty");
    if kitty {
        return Some(Candidate {
            path: format!("{config}/kitty/kitty.conf"),
            read: kitty_font,
        });
    }

    let ghostty = env::var("GHOSTTY_RESOURCES_DIR").is_some()
        || env::var("TERM_PROGRAM").as_deref() == Some("ghostty")
        || term == "xterm-ghostty";
    if ghostty {
        return Some(Candidate {
            path: format!("{config}/ghostty/config"),
            read: ghostty_font,
        });
    }

    let alacritty = env::var("ALACRITTY_SOCKET").is_some()
        || env::var("ALACRITTY_LOG").is_some()
        || env::var("TERM_PROGRAM").as_deref() == Some("Alacritty")
        || term == "alacritty";
    if alacritty {
        return Some(Candidate {
            path: format!("{config}/alacritty/alacritty.toml"),
            read: alacritty_font,
        });
    }

    if term.starts_with("foot") {
        return Some(Candidate {
            path: format!("{config}/foot/foot.ini"),
            read: foot_font,
        });
    }

    None
}

/// `$XDG_CONFIG_HOME`，没设就用 `$HOME/.config`。
fn config_dir() -> Option<String> {
    if let Some(dir) = env::var("XDG_CONFIG_HOME") {
        return Some(dir);
    }

    env::var("HOME").map(|home| format!("{home}/.config"))
}

/// kitty：`font_family JetBrainsMono Nerd Font`，空格分隔，不带等号。
///
/// kitty 允许写多行 `font_family`（依次是粗体、斜体……），**第一行才是正体**，
/// 所以取第一个命中的。
fn kitty_font(text: &str) -> Option<Font> {
    let family = value_of(text, "font_family")?;
    let size = value_of(text, "font_size").map(|size| trim_size(&size));

    Some(Font { family, size })
}

/// ghostty：`font-family = JetBrains Mono`。
fn ghostty_font(text: &str) -> Option<Font> {
    let family = value_of(text, "font-family")?;
    let size = value_of(text, "font-size").map(|size| trim_size(&size));

    Some(Font { family, size })
}

/// alacritty：TOML，`[font.normal] family = "..."` 与 `[font] size = 13`。
fn alacritty_font(text: &str) -> Option<Font> {
    let family = toml_value(text, "font.normal", "family")?;
    let size = toml_value(text, "font", "size").map(|size| trim_size(&size));

    Some(Font { family, size })
}

/// foot：`font=monospace:size=11`，一格里把几项用冒号串起来。
fn foot_font(text: &str) -> Option<Font> {
    let spec = value_of(text, "font")?;

    let mut family = None;
    let mut size = None;
    for field in spec.split(':') {
        match field.split_once('=') {
            Some(("size", value)) => size = Some(trim_size(value)),
            Some(("style", _)) => {}
            Some(_) => {}
            None => family = Some(field.trim().to_owned()),
        }
    }

    let family = family.filter(|family| !family.is_empty())?;

    Some(Font { family, size })
}

/// 取一个 `键 值` / `键=值` / `键: 值` 里的值。
///
/// 注释（`#` 与 `;`）整行跳过——`kitty.conf`、`ghostty config`、`foot.ini`
/// 都有把配置注释掉的习惯，把注释里的字体当成真的就错了。
fn value_of(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        let (found, value) = match line.split_once('=') {
            Some((found, value)) => (found.trim(), value.trim()),
            None => match line.split_once(char::is_whitespace) {
                Some(pair) => (pair.0.trim(), pair.1.trim()),
                None => continue,
            },
        };

        if found != key {
            continue;
        }

        let value = unquote(value);
        if !value.is_empty() {
            return Some(value);
        }
    }

    None
}

/// 取一个 TOML 小节里的键：`[font.normal]` 下的 `family`。
///
/// 必须看小节——`[font]` 和 `[font.bold]` 里都有 `family`，不看小节就会把粗体
/// 的字体当成正体。
fn toml_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut current = String::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(header) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            current = header.trim().to_owned();
            continue;
        }

        if current != section {
            continue;
        }

        if let Some((found, value)) = line.split_once('=') {
            // 不用 let-chain：那是 Rust 1.88 才稳定的，本项目 MSRV 是 1.85。
            if found.trim() != key {
                continue;
            }
            let value = unquote(value.trim());
            if !value.is_empty() {
                return Some(value);
            }
        }
    }

    None
}

/// 去掉一层引号。
fn unquote(value: &str) -> String {
    let value = value.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner.to_owned();
        }
    }

    value.to_owned()
}

/// `12.0` → `12`，`11.5` → `11.5`。终端配置文件里字号常写成浮点。
fn trim_size(size: &str) -> String {
    let size = size.trim();
    match size.strip_suffix(".0") {
        Some(whole) if !whole.contains('.') => whole.to_owned(),
        _ => size.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KITTY: &str = "\
#: 注释里的字体不算
# font_family NotReal Font
font_family JetBrainsMono Nerd Font
bold_font JetBrainsMono Nerd Font Bold
font_size 12.0
";

    const ALACRITTY: &str = r#"
[font]
size = 13

[font.bold]
family = "Bold Family"

[font.normal]
family = "JetBrains Mono"
"#;

    #[test]
    fn reads_kitty() {
        assert_eq!(
            kitty_font(KITTY),
            Some(Font {
                family: "JetBrainsMono Nerd Font".to_owned(),
                size: Some("12".to_owned()),
            })
        );
    }

    #[test]
    fn reads_alacritty_and_ignores_the_bold_section() {
        // 最容易写错的一条：[font.bold] 在前，不看小节就会读到 "Bold Family"。
        assert_eq!(
            alacritty_font(ALACRITTY),
            Some(Font {
                family: "JetBrains Mono".to_owned(),
                size: Some("13".to_owned()),
            })
        );
    }

    #[test]
    fn reads_ghostty() {
        let text = "font-family = JetBrains Mono\nfont-size = 13\n# font-family = No\n";

        assert_eq!(
            ghostty_font(text),
            Some(Font {
                family: "JetBrains Mono".to_owned(),
                size: Some("13".to_owned()),
            })
        );
    }

    #[test]
    fn reads_foot() {
        let text = "[main]\nfont=monospace:size=11\n";

        assert_eq!(
            foot_font(text),
            Some(Font {
                family: "monospace".to_owned(),
                size: Some("11".to_owned()),
            })
        );
    }

    #[test]
    fn foot_without_a_family_is_no_data() {
        assert_eq!(foot_font("[main]\nfont=:size=11\n"), None);
    }

    #[test]
    fn a_commented_out_key_is_not_a_value() {
        assert_eq!(value_of("#font_family Foo\n", "font_family"), None);
        assert_eq!(value_of("; font = Foo\n", "font"), None);
        assert_eq!(
            value_of("font_family\n", "font_family"),
            None,
            "只有键没有值"
        );
        assert_eq!(
            value_of("font_family = \n", "font_family"),
            None,
            "值是空的"
        );
    }

    #[test]
    fn values_may_keep_their_inner_spaces() {
        // `font_family JetBrainsMono Nerd Font`：值里的空格要留住。
        assert_eq!(
            value_of("font_family JetBrainsMono Nerd Font", "font_family").as_deref(),
            Some("JetBrainsMono Nerd Font")
        );
    }

    #[test]
    fn quotes_are_stripped() {
        assert_eq!(unquote("\"JetBrains Mono\""), "JetBrains Mono");
        assert_eq!(unquote("'monospace'"), "monospace");
        assert_eq!(unquote("\"unbalanced"), "\"unbalanced");
    }

    #[test]
    fn trims_only_a_trailing_zero_fraction() {
        assert_eq!(trim_size("12.0"), "12");
        assert_eq!(trim_size("11.5"), "11.5");
        assert_eq!(trim_size("12"), "12");
        assert_eq!(trim_size(" 13 "), "13");
    }

    #[test]
    fn renders_with_and_without_a_size() {
        let full = Font {
            family: "Fira Code".to_owned(),
            size: Some("14".to_owned()),
        };
        assert_eq!(full.render(), "Fira Code 14pt");

        let no_size = Font {
            family: "Fira Code".to_owned(),
            size: None,
        };
        assert_eq!(no_size.render(), "Fira Code");
    }

    #[test]
    fn finds_the_kitty_config_on_this_machine() {
        // 本机在 kitty 里跑（KITTY_PID 有值），所以这条能拿到真实字体；
        // 换一台机器可能走不到，那就允许无数据——这里不假设环境。
        let candidate = locate();
        let Some(candidate) = candidate else {
            return;
        };

        assert!(
            candidate.path.ends_with(".conf")
                || candidate.path.ends_with(".toml")
                || candidate.path.ends_with("config")
                || candidate.path.ends_with(".ini"),
            "路径看着不像配置文件：{}",
            candidate.path
        );
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = TerminalFont.collect(&Context::for_tests()).unwrap();

        match entries.first() {
            None => {}
            Some(info) => {
                assert_eq!(info.key, "Terminal Font");
                assert!(!info.value.is_empty());
            }
        }
    }
}
