//! `/etc/os-release` 的解析。
//!
//! 单独成文件，是因为它有两个使用者：OS 模块（显示发行版名），
//! 以及构建 `Platform` 的地方（阶段 5 选 Logo、阶段 7 评估 `platforms` 条件）。
//! 两边读的必须是同一份数据，否则会出现「显示的发行版和 Logo 对不上」。

use crate::collectors::read;
use crate::core::collector::CollectError;

/// 规范里说的两个位置，按优先级。
const PATHS: [&str; 2] = ["/etc/os-release", "/usr/lib/os-release"];

/// `os-release` 里我们关心的字段。
///
/// 都是 `Option`：这个文件缺字段是常事（`ID_LIKE`、`VERSION_ID` 尤其常见），
/// 缺了就是「不知道」，不是错误。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Release {
    /// 发行版 id，例如 `arch`。Logo 就是按它匹配的。
    pub id: Option<String>,
    /// 像哪些发行版，例如 `arch`。衍生版（cachyos 之类）靠它回退。
    pub id_like: Option<String>,
    /// 正式名，例如 `Arch Linux`。
    pub name: Option<String>,
    /// 给人看的名字，例如 `Arch Linux`，通常带版本。
    pub pretty_name: Option<String>,
    /// 版本号，滚动发行版往往没有。
    pub version_id: Option<String>,
}

impl Release {
    /// 显示用的名字：`PRETTY_NAME` → `NAME` → `ID`。
    ///
    /// 三级回退是规范推荐的顺序，不是随便排的。
    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        self.pretty_name
            .as_deref()
            .or(self.name.as_deref())
            .or(self.id.as_deref())
    }
}

/// 解析 `KEY=VALUE` 文本。
///
/// 只做规范要求的两件事：跳过注释与空行、去掉值两边**成对**的引号。
/// 没做转义与变量展开——这个文件里几乎用不到，真遇到再说。
#[must_use]
pub fn parse(text: &str) -> Release {
    let mut release = Release::default();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        let value = unquote(value.trim()).to_owned();
        match key.trim() {
            "ID" => release.id = Some(value),
            "ID_LIKE" => release.id_like = Some(value),
            "NAME" => release.name = Some(value),
            "PRETTY_NAME" => release.pretty_name = Some(value),
            "VERSION_ID" => release.version_id = Some(value),
            _ => {}
        }
    }

    release
}

/// 去掉值两边成对的引号。
fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner;
        }
    }

    value
}

/// 读系统的 `os-release`。文件不存在返回 `Ok(None)`。
pub fn read() -> Result<Option<Release>, CollectError> {
    Ok(read::first(&PATHS)?.map(|text| parse(&text)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 实测的 `/etc/os-release`（这台机器）。
    const ARCH: &str = r#"
NAME="Arch Linux"
PRETTY_NAME="Arch Linux"
ID=arch
BUILD_ID=rolling
ANSI_COLOR="38;2;23;147;209"
HOME_URL="https://archlinux.org/"
"#;

    #[test]
    fn parses_the_real_file() {
        let release = parse(ARCH);

        assert_eq!(release.id.as_deref(), Some("arch"));
        assert_eq!(release.name.as_deref(), Some("Arch Linux"));
        assert_eq!(release.pretty_name.as_deref(), Some("Arch Linux"));
        assert_eq!(release.id_like, None, "这个文件里没有 ID_LIKE");
        assert_eq!(release.version_id, None, "滚动发行版没有版本号");
    }

    #[test]
    fn the_display_name_falls_back_to_the_id() {
        let only_id = parse("ID=arch\n");
        assert_eq!(only_id.display_name(), Some("arch"), "ID 是最后的兜底");

        let with_name = parse("ID=arch\nNAME=\"Arch Linux\"\n");
        assert_eq!(with_name.display_name(), Some("Arch Linux"));

        let with_pretty = parse("ID=arch\nNAME=\"Arch\"\nPRETTY_NAME=\"Arch Linux 6.16\"\n");
        assert_eq!(with_pretty.display_name(), Some("Arch Linux 6.16"));
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let release = parse("# 注释\n\nID=cachyos\n\n# PRETTY_NAME=别被骗了\n");
        assert_eq!(release.id.as_deref(), Some("cachyos"));
        assert_eq!(release.pretty_name, None, "注释里的赋值不该被当成数据");
    }

    #[test]
    fn quotes_are_stripped_only_when_paired() {
        assert_eq!(unquote("\"arch\""), "arch");
        assert_eq!(unquote("'arch'"), "arch");
        assert_eq!(unquote("arch"), "arch");
        assert_eq!(unquote("\"arch"), "\"arch", "半边引号不动它");
    }

    #[test]
    fn a_file_without_any_known_key_yields_nothing() {
        let release = parse("HOME_URL=\"https://example.com\"\n");
        assert_eq!(release, Release::default());
        assert_eq!(release.display_name(), None);
    }

    #[test]
    fn a_line_without_equals_is_ignored_not_fatal() {
        let release = parse("这不是 KEY=VALUE\nID=arch\n");
        assert_eq!(release.id.as_deref(), Some("arch"));
    }
}
