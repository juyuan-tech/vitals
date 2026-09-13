//! 内置 Logo。
//!
//! 图形由 `include_str!` 在**编译期**嵌进二进制，运行时不读磁盘
//! （`PLAN.md` 第十条排除的正是「运行时读 Logo 文件」）。
//!
//! # 来源
//!
//! 取自 [fastfetch](https://github.com/fastfetch-cli/fastfetch) 的 `src/logo/ascii/`，
//! MIT 许可（Copyright (c) 2021-2023 Linus Dierheimer，2022-2026 Carter Li）。
//! 取来时去掉了它自己的 `$1`/`$2` 颜色占位符，改成「一个发行版一种颜色」
//! ——`PLAN.md` §6.2 就是这么定的，也省得实现一套分段着色。

use anstyle::AnsiColor;

use crate::collectors::os_release::Release;
use crate::core::render::Logo;

/// 一个内置 Logo，以及它该用的颜色。
#[derive(Debug)]
pub struct Entry {
    /// 图形本身。
    pub logo: Logo,
    /// 用哪种颜色画。整张图一种颜色，不分段。
    pub color: AnsiColor,
}

/// 声明一张内置图。
///
/// 宏在这里的价值是把**三处容易对不上的东西**绑成一个字面量：
/// Logo 的 id、要加载的文件名、以及 `include_str!` 的路径。
/// 手写三遍的话，加一张图时漏改一处就是运行期才发现的问题。
macro_rules! logo {
    ($id:literal, $color:ident) => {
        Entry {
            logo: Logo {
                id: $id,
                art: include_str!(concat!("logos/", $id, ".txt")),
            },
            color: AnsiColor::$color,
        }
    };
}

/// 内置 Logo 表。
///
/// 只放最常见的那些。衍生版靠 `ID_LIKE` 回退接住
/// （CachyOS → arch、Linux Mint → ubuntu、Rocky → centos），
/// 两条都接不住才落到通用 Linux。这样十来张图能覆盖绝大多数机器。
///
/// 颜色取各家标识的主体色，`AnsiColor` 只有八种基础色与八种亮色，
/// 所以是「最接近」而不是「精确」。
pub static LOGOS: &[Entry] = &[
    logo!("arch", BrightCyan),
    logo!("manjaro", BrightGreen),
    logo!("debian", Red),
    logo!("ubuntu", Yellow),
    logo!("fedora", BrightBlue),
    logo!("centos", Magenta),
    logo!("alpine", Cyan),
    logo!("gentoo", BrightMagenta),
    logo!("void", Green),
    logo!("nixos", Blue),
    logo!("opensuse", BrightGreen),
];

/// 通用回退：不是上面任何一种时用它。
pub static GENERIC: Entry = logo!("linux", BrightWhite);

/// 按 id 找 Logo。
///
/// 大小写不敏感：`os-release` 规范说 `ID` 是小写，但拿别人的数据时不必较真。
#[must_use]
pub fn find(id: &str) -> Option<&'static Entry> {
    LOGOS
        .iter()
        .find(|entry| entry.logo.id.eq_ignore_ascii_case(id))
}

/// 按 `os-release` 选 Logo：`ID` → `ID_LIKE` → 通用。
///
/// `ID_LIKE` 那一步不能省：`ID=cachyos`、`ID=endeavouros` 这类衍生版自己那张图
/// 不在表里，但它们的 `ID_LIKE` 是 `arch`，能接住；没有这一步就全掉到通用 Logo。
#[must_use]
pub fn for_release(release: Option<&Release>) -> &'static Entry {
    let Some(release) = release else {
        return &GENERIC;
    };

    if let Some(entry) = release.id.as_deref().and_then(find) {
        return entry;
    }

    // `ID_LIKE` 可以是多个，空格分隔（`ID_LIKE="rhel centos fedora"`），按写出来的顺序试。
    for id in release
        .id_like
        .as_deref()
        .unwrap_or_default()
        .split_whitespace()
    {
        if let Some(entry) = find(id) {
            return entry;
        }
    }

    &GENERIC
}

/// 某个 id 该用哪种颜色画。
///
/// 渲染器手里只有 `core::render::Logo`（核心不认识 `anstyle`），
/// 所以要按 id 回查一次颜色。查不到就按通用 Logo 的颜色画。
#[must_use]
pub fn color_of(id: &str) -> AnsiColor {
    find(id).map_or(GENERIC.color, |entry| entry.color)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个只有 id 和 ID_LIKE 的 `os-release`。
    fn release(id: Option<&str>, id_like: Option<&str>) -> Release {
        Release {
            id: id.map(str::to_owned),
            id_like: id_like.map(str::to_owned),
            ..Release::default()
        }
    }

    #[test]
    fn every_logo_can_be_found_by_its_id() {
        for entry in LOGOS {
            let found = find(entry.logo.id);
            assert!(found.is_some(), "{} 找不回来", entry.logo.id);
            assert_eq!(found.unwrap().logo.id, entry.logo.id);
        }
    }

    #[test]
    fn the_generic_logo_is_not_in_the_table() {
        // 否则 `ID=linux` 这种不存在的情况会跟通用回退撞车。
        assert!(find(GENERIC.logo.id).is_none());
    }

    #[test]
    fn an_exact_id_wins() {
        assert_eq!(
            for_release(Some(&release(Some("ubuntu"), None))).logo.id,
            "ubuntu"
        );
        assert_eq!(
            for_release(Some(&release(Some("arch"), None))).logo.id,
            "arch"
        );
    }

    #[test]
    fn id_like_catches_the_derivatives() {
        // 这几条是真实数据：CachyOS、EndeavourOS 的 ID_LIKE 都是 arch；
        // Linux Mint 是 "ubuntu debian"。没有这条回退它们就只剩通用 Logo 了。
        assert_eq!(
            for_release(Some(&release(Some("cachyos"), Some("arch"))))
                .logo
                .id,
            "arch"
        );
        assert_eq!(
            for_release(Some(&release(Some("endeavouros"), Some("arch"))))
                .logo
                .id,
            "arch"
        );
        assert_eq!(
            for_release(Some(&release(Some("linuxmint"), Some("ubuntu debian"))))
                .logo
                .id,
            "ubuntu",
            "按写出来的顺序试，第一个能接住的就是 ubuntu"
        );
        assert_eq!(
            for_release(Some(&release(Some("rocky"), Some("rhel centos fedora"))))
                .logo
                .id,
            "centos"
        );
    }

    #[test]
    fn anything_unknown_falls_back_to_the_generic_one() {
        assert_eq!(for_release(None).logo.id, GENERIC.logo.id);
        assert_eq!(
            for_release(Some(&release(Some("plan9"), None))).logo.id,
            GENERIC.logo.id
        );
        assert_eq!(
            for_release(Some(&release(Some("plan9"), Some("beos haiku"))))
                .logo
                .id,
            GENERIC.logo.id
        );
        assert_eq!(
            for_release(Some(&release(None, None))).logo.id,
            GENERIC.logo.id
        );
    }

    #[test]
    fn ids_are_matched_without_regard_to_case() {
        assert_eq!(
            for_release(Some(&release(Some("Arch"), None))).logo.id,
            "arch"
        );
    }

    #[test]
    fn every_color_lookup_answers() {
        for entry in LOGOS {
            assert_eq!(color_of(entry.logo.id), entry.color);
        }
        assert_eq!(color_of("不存在的发行版"), GENERIC.color);
    }

    #[test]
    fn no_art_still_carries_a_color_placeholder() {
        // 取回来的图带着 fastfetch 的 `$1`/`$2`，那些必须已经清掉——
        // 否则终端上会直接印出 `$2` 这种东西。这一条守着那次清洗。
        for entry in LOGOS.iter().chain(std::iter::once(&GENERIC)) {
            let mut characters = entry.logo.art.chars().peekable();
            while let Some(character) = characters.next() {
                if character == '$' {
                    assert!(
                        !characters.peek().is_some_and(char::is_ascii_digit),
                        "{} 的画面里还有颜色占位符",
                        entry.logo.id
                    );
                }
            }
        }
    }

    #[test]
    fn every_art_is_multi_line_and_has_content() {
        for entry in LOGOS.iter().chain(std::iter::once(&GENERIC)) {
            let lines: Vec<&str> = entry.logo.art.lines().collect();
            assert!(lines.len() >= 5, "{} 太矮了", entry.logo.id);
            assert!(
                lines.iter().any(|line| !line.trim().is_empty()),
                "{} 是空的",
                entry.logo.id
            );
            for line in &lines {
                assert!(
                    !line.contains('\t'),
                    "{} 里有制表符，对齐会乱",
                    entry.logo.id
                );
            }
        }
    }
}
