//! 会话识别：桌面环境与窗口管理器的共用件。
//!
//! 两条线索，按可靠性排序：
//!
//! 1. **环境变量**：freedesktop 定了标准（`XDG_CURRENT_DESKTOP`、`XDG_SESSION_DESKTOP`、
//!    `DESKTOP_SESSION`），各家还有自己的（`HYPRLAND_INSTANCE_SIGNATURE`、`SWAYSOCK`、
//!    `I3SOCK`）。`XDG_CURRENT_DESKTOP` 的值可以是 `ubuntu:GNOME` 这样的冒号列表，
//!    也可以是 `Budgie:GNOME` 这种「谁坐在谁上面」的写法——所以先整串比对，再拆开比对。
//! 2. **父进程链**：桌面与合成器几乎一定是我们的祖先（shell 由终端拉起，终端在会话里），
//!    走 [`proc_chain`] 就够，不必扫 `/proc` 下几百个进程（那要 6.9 ms，见 `processes`）。
//!
//! 版本号**不跑 `--version`**（那是子进程），而是查包数据库：`gnome-shell`、`kwin`、
//! `sway` 在发行版里都是包，`pkgdb::version_of` 从 pacman 的目录名、dpkg 的状态文件里
//! 就能拿到版本。fastfetch 只在 Debian 系读得到，我们在 Arch 上同样能报版本。
//!
//! 认不出来的情况（比如裸 X11 会话里的无名 WM）就是无数据——不猜。

use crate::collectors::{env, pkgdb, proc_chain};

/// 一条识别规则。
struct Rule {
    /// 显示名。
    name: &'static str,
    /// 查版本用的包名。
    package: &'static str,
    /// 环境变量里可能出现的值，大小写不敏感。
    env: &'static [&'static str],
    /// 父进程链上认得出的 `comm`。内核把它截断到 15 个字符，所以长名字要有截断版。
    process: &'static [&'static str],
}

/// 认出来的会话。
pub struct Session {
    /// 显示名。
    pub name: &'static str,
    /// 从包数据库读到的版本；读不到就是 `None`。
    pub version: Option<String>,
}

impl Session {
    /// `名称` 或 `名称 版本`。
    #[must_use]
    pub fn describe(&self) -> String {
        match &self.version {
            Some(version) => format!("{} {version}", self.name),
            None => self.name.to_owned(),
        }
    }
}

/// 读哪几个环境变量，顺序即优先级。
const ENV_KEYS: [&str; 3] = [
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_DESKTOP",
    "DESKTOP_SESSION",
];

/// 桌面环境。
#[must_use]
pub fn desktop() -> Option<Session> {
    identify(&DESKTOPS)
}

/// 窗口管理器 / 合成器。
#[must_use]
pub fn window_manager() -> Option<Session> {
    identify(&COMPOSITORS)
}

/// 环境变量优先，父进程链兜底，最后查包数据库补版本。
fn identify(rules: &'static [Rule]) -> Option<Session> {
    let rule = from_env(rules).or_else(|| from_chain(rules))?;

    Some(Session {
        name: rule.name,
        // 查不到版本不影响识别结果：`GNOME` 也比什么都没有强。
        version: pkgdb::version_of(rule.package).ok().flatten(),
    })
}

/// 从环境变量认。
///
/// 按 [`ENV_KEYS`] 的顺序找第一个认得出的值：`XDG_CURRENT_DESKTOP` 是标准答案，
/// 另外两个是它没设时的补充。
fn from_env(rules: &'static [Rule]) -> Option<&'static Rule> {
    ENV_KEYS
        .iter()
        .filter_map(|key| env::var(key))
        .find_map(|value| match_value(rules, &value))
}

/// 拿一个值去撞所有规则，**先整串、再拆冒号**。
///
/// 顺序不能反：`XDG_CURRENT_DESKTOP=Budgie:GNOME` 说的是「Budgie 坐在 GNOME 上面」，
/// 整串对上 Budgie 才对；先拆开的话就会先撞上 `GNOME`，把桌面认错。
/// 而 `ubuntu:GNOME` 整串谁都不认识，拆开之后 `GNOME` 才是答案。
fn match_value(rules: &'static [Rule], value: &str) -> Option<&'static Rule> {
    match_alias(rules, value).or_else(|| {
        value
            .split(':')
            .find_map(|token| match_alias(rules, token.trim()))
    })
}

/// 拿一个字符串去撞所有规则的别名（整串相等，大小写不敏感）。
fn match_alias(rules: &'static [Rule], value: &str) -> Option<&'static Rule> {
    if value.is_empty() {
        return None;
    }

    rules.iter().find(|rule| {
        rule.env
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(value))
    })
}

/// 顺着父进程链认：最近的那个祖先说了算。
fn from_chain(rules: &'static [Rule]) -> Option<&'static Rule> {
    proc_chain::ancestors(proc_chain::DEFAULT_DEPTH)
        .into_iter()
        .find_map(|ancestor| {
            rules
                .iter()
                .find(|rule| rule.process.contains(&ancestor.command.as_str()))
        })
}

/// 桌面环境表。
///
/// `process` 里带 `-` 结尾或少尾巴的名字都是**内核的 15 字符截断**，不是笔误：
/// `gnome-session-binary` 在 `comm` 里只有 `gnome-session-b`。
static DESKTOPS: [Rule; 14] = [
    Rule {
        name: "GNOME",
        package: "gnome-shell",
        env: &["GNOME", "GNOME-Classic", "GNOME-Flashback", "gnome"],
        process: &["gnome-shell", "gnome-session-b"],
    },
    Rule {
        name: "KDE Plasma",
        package: "plasma-desktop",
        env: &["KDE", "kde", "plasma", "plasma-wayland", "plasmawayland"],
        process: &["plasmashell", "kwin_wayland", "kwin_x11"],
    },
    Rule {
        name: "XFCE",
        package: "xfce4-session",
        env: &["XFCE", "xfce", "xfce4"],
        process: &["xfce4-session", "xfwm4"],
    },
    Rule {
        name: "Cinnamon",
        package: "cinnamon",
        env: &["Cinnamon", "X-Cinnamon", "cinnamon"],
        process: &["cinnamon"],
    },
    Rule {
        name: "MATE",
        package: "mate-session-manager",
        env: &["MATE", "mate"],
        process: &["mate-session", "marco"],
    },
    Rule {
        name: "LXQt",
        package: "lxqt-session",
        env: &["LXQt", "lxqt"],
        process: &["lxqt-session"],
    },
    Rule {
        name: "LXDE",
        package: "lxde-common",
        env: &["LXDE", "lxde"],
        process: &["lxsession"],
    },
    Rule {
        name: "Deepin",
        package: "dde-daemon",
        env: &["Deepin", "DDE", "deepin"],
        process: &["dde-session", "dde-daemon"],
    },
    Rule {
        name: "Budgie",
        package: "budgie-desktop",
        env: &["Budgie", "Budgie:GNOME", "budgie"],
        process: &["budgie-desktop", "budgie-wm"],
    },
    Rule {
        name: "Pantheon",
        package: "pantheon-shell",
        env: &["Pantheon", "pantheon"],
        // `io.elementary.gala` 在 `comm` 里是 `io.elementary.g`。
        process: &["gala", "io.elementary.g"],
    },
    Rule {
        name: "COSMIC",
        package: "cosmic-session",
        env: &["COSMIC", "cosmic"],
        process: &["cosmic-session", "cosmic-comp"],
    },
    Rule {
        name: "Unity",
        package: "unity",
        env: &["Unity", "unity"],
        process: &["unity", "unity-panel-ser"],
    },
    Rule {
        name: "Enlightenment",
        package: "enlightenment",
        env: &["Enlightenment", "enlightenment"],
        process: &["enlightenment"],
    },
    Rule {
        name: "UKUI",
        package: "ukui-session",
        env: &["UKUI", "ukui"],
        process: &["ukui-session"],
    },
];

/// 窗口管理器 / 合成器表。
static COMPOSITORS: [Rule; 22] = [
    Rule {
        name: "Mutter",
        package: "mutter",
        env: &[],
        process: &["gnome-shell"],
    },
    Rule {
        name: "KWin",
        package: "kwin",
        env: &[],
        process: &["kwin_wayland", "kwin_x11", "kwin"],
    },
    Rule {
        name: "Muffin",
        package: "muffin",
        env: &[],
        process: &["cinnamon"],
    },
    Rule {
        name: "Xfwm4",
        package: "xfwm4",
        env: &[],
        process: &["xfwm4"],
    },
    Rule {
        name: "Marco",
        package: "marco",
        env: &[],
        process: &["marco"],
    },
    Rule {
        name: "Openbox",
        package: "openbox",
        env: &["openbox"],
        process: &["openbox"],
    },
    Rule {
        name: "i3",
        package: "i3-wm",
        env: &["I3SOCK", "i3"],
        process: &["i3"],
    },
    Rule {
        name: "Sway",
        package: "sway",
        env: &["SWAYSOCK", "sway"],
        process: &["sway"],
    },
    Rule {
        name: "Hyprland",
        package: "hyprland",
        env: &["HYPRLAND_INSTANCE_SIGNATURE", "Hyprland", "hyprland"],
        process: &["Hyprland"],
    },
    Rule {
        name: "Niri",
        package: "niri",
        env: &["NIRI_SOCKET", "niri"],
        process: &["niri"],
    },
    Rule {
        name: "bspwm",
        package: "bspwm",
        env: &["BSPWM_SOCKET", "bspwm"],
        process: &["bspwm"],
    },
    Rule {
        name: "dwm",
        package: "dwm",
        env: &["dwm"],
        process: &["dwm"],
    },
    Rule {
        name: "awesome",
        package: "awesome",
        env: &["awesome"],
        process: &["awesome"],
    },
    Rule {
        name: "xmonad",
        package: "xmonad",
        env: &["xmonad"],
        process: &["xmonad"],
    },
    Rule {
        name: "River",
        package: "river",
        env: &["river"],
        process: &["river"],
    },
    Rule {
        name: "Wayfire",
        package: "wayfire",
        env: &["wayfire"],
        process: &["wayfire"],
    },
    Rule {
        name: "labwc",
        package: "labwc",
        env: &["labwc"],
        process: &["labwc"],
    },
    Rule {
        name: "IceWM",
        package: "icewm",
        env: &["icewm"],
        process: &["icewm"],
    },
    Rule {
        name: "Fluxbox",
        package: "fluxbox",
        env: &["fluxbox"],
        process: &["fluxbox"],
    },
    Rule {
        name: "Weston",
        package: "weston",
        env: &["weston"],
        process: &["weston"],
    },
    Rule {
        name: "Cage",
        package: "cage",
        env: &["cage"],
        process: &["cage"],
    },
    Rule {
        name: "COSMIC Comp",
        package: "cosmic-comp",
        env: &[],
        process: &["cosmic-comp"],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_value_beats_a_token_inside_it() {
        // `Budgie:GNOME` 说的是「Budgie 坐在 GNOME 上面」，认成 GNOME 就错了。
        assert_eq!(
            match_value(&DESKTOPS, "Budgie:GNOME").map(|rule| rule.name),
            Some("Budgie")
        );
        // 而 `ubuntu:GNOME` 整串谁都不认识，拆开之后 GNOME 才是答案。
        assert_eq!(
            match_value(&DESKTOPS, "ubuntu:GNOME").map(|rule| rule.name),
            Some("GNOME")
        );
    }

    #[test]
    fn aliases_are_matched_without_case() {
        assert_eq!(
            match_value(&DESKTOPS, "gnome").map(|rule| rule.name),
            Some("GNOME")
        );
        assert_eq!(
            match_value(&COMPOSITORS, "hyprland").map(|rule| rule.name),
            Some("Hyprland")
        );
    }

    #[test]
    fn unknown_names_are_no_data() {
        assert!(match_value(&DESKTOPS, "vitals-not-a-desktop").is_none());
        assert!(match_value(&DESKTOPS, "").is_none());
        assert!(match_value(&DESKTOPS, "ubuntu:").is_none());
    }

    #[test]
    fn the_tables_have_no_duplicate_aliases() {
        // 同一条线索认到两个桌面，说明表写重了——结果是顺序决定的，不能这么干。
        for rules in [&DESKTOPS[..], &COMPOSITORS[..]] {
            for rule in rules {
                for alias in rule.env {
                    let hits = rules
                        .iter()
                        .filter(|other| other.env.iter().any(|a| a.eq_ignore_ascii_case(alias)))
                        .count();

                    assert_eq!(hits, 1, "别名 {alias} 撞了 {hits} 条规则");
                }
            }
        }
    }

    #[test]
    fn session_describes_with_and_without_a_version() {
        let named = Session {
            name: "GNOME",
            version: Some("47.1".to_owned()),
        };
        let nameless = Session {
            name: "GNOME",
            version: None,
        };

        assert_eq!(named.describe(), "GNOME 47.1");
        assert_eq!(nameless.describe(), "GNOME");
    }

    #[test]
    fn identifies_this_machine_if_there_is_a_session() {
        // 在有桌面的机器上该认得出东西；跑在 CI/容器里认不出也正常，
        // 所以只断言「认出来时名字非空」。
        for session in [desktop(), window_manager()].into_iter().flatten() {
            assert!(!session.name.is_empty());
            assert!(!session.describe().is_empty());
        }
    }
}
