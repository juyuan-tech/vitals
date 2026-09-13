//! LM：登录管理器（display manager）。
//!
//! 系统里「谁来给你显示登录框」这件事存在好几个地方，各发行版各写各的，按可信度排：
//!
//! 1. `/etc/systemd/system/display-manager.service`——systemd 系的**正法**，一个指向
//!    `/usr/lib/systemd/system/sddm.service` 的符号链接，取它的文件名就是管理器名字；
//! 2. `/etc/X11/default-display-manager`——Debian 系，里面是可执行文件的绝对路径；
//! 3. `/etc/conf.d/xdm`（Gentoo）与 `/etc/sysconfig/displaymanager`（openSUSE）——
//!    里面是 `DISPLAYMANAGER="sddm"` 这样的赋值。
//!
//! 四处都没有就是**没装**显示管理器，那时你看到的是内核/agetty 的字符登录框——
//! 报 `login`（fastfetch 也是这么印的）。这不是「查不到」，是查到了「没有」。
//!
//! 版本查包数据库，包名跟管理器同名（`sddm`、`lightdm`…）；`login` 不是包，没有版本。
//!
//! **只能报「配的是哪个」，不能报「现在跑的是哪个」**：后者要问 systemd（D-Bus），
//! 与零子进程原则冲突。两者不一致的情况很少见，真不一致时 `--verbose` 也帮不上忙——
//! 这是这一行诚实的边界。

use crate::collectors::{pkgdb, read};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// systemd 的显示管理器别名（指向真正的 unit）。
const SYSTEMD_ALIAS: &str = "/etc/systemd/system/display-manager.service";

/// Debian 系的默认显示管理器，内容是绝对路径。
const DEBIAN: &str = "/etc/X11/default-display-manager";

/// Gentoo / openSUSE 的配置文件，内容是 `DISPLAYMANAGER="sddm"`。
const CONF_FILES: [&str; 2] = ["/etc/conf.d/xdm", "/etc/sysconfig/displaymanager"];

/// 没有显示管理器时的答案。
const CONSOLE: &str = "login";

/// 登录管理器。
pub struct Lm;

impl Collector for Lm {
    fn name(&self) -> &'static str {
        "lm"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let name = match locate()? {
            Some(name) => name,
            None => CONSOLE.to_owned(),
        };

        // 字符登录框不是包，没有版本可查。
        let value = if name == CONSOLE {
            name
        } else {
            match pkgdb::version_of(&name)? {
                Some(version) => format!("{name} {version}"),
                None => name,
            }
        };

        Ok(vec![Info::new(self.name(), "Login Manager", value)])
    }
}

/// 找出配的是哪个显示管理器。
fn locate() -> Result<Option<String>, CollectError> {
    if let Some(name) = from_systemd()? {
        return Ok(Some(name));
    }

    if let Some(name) = from_debian()? {
        return Ok(Some(name));
    }

    for path in CONF_FILES {
        if let Some(text) = read::text(path)? {
            if let Some(name) = from_assignment(&text) {
                return Ok(Some(name));
            }
        }
    }

    Ok(None)
}

/// systemd 的别名链接指向哪个 unit。
///
/// 用 `read_link` 而不是 `canonicalize`：后者会把 `/usr/lib/systemd/system/sddm.service`
/// 里可能存在的 `Alias=` 再解一层，而我们只要**别名指名的那个**。
fn from_systemd() -> Result<Option<String>, CollectError> {
    match std::fs::read_link(SYSTEMD_ALIAS) {
        Ok(target) => Ok(unit_name(&target.to_string_lossy())),
        // 不存在就是没有；别的错误（权限等）也不该让这一行消失——
        // 后面还有三个候选，逐个试。
        Err(_) => Ok(None),
    }
}

/// `/usr/lib/systemd/system/sddm.service` → `sddm`。
fn unit_name(path: &str) -> Option<String> {
    let file = path.rsplit('/').next()?.trim();
    let name = file.strip_suffix(".service").unwrap_or(file).trim();
    (!name.is_empty()).then(|| name.to_owned())
}

/// Debian 的路径 → 可执行文件名。
fn from_debian() -> Result<Option<String>, CollectError> {
    match read::text(DEBIAN)? {
        Some(text) => Ok(first_path_name(&text)),
        None => Ok(None),
    }
}

/// Debian 文件里第一行有效内容（跳过空行与注释）的末段。
///
/// 内容可能是 `/usr/sbin/lightdm`，也可能是 `lightdm`；末段就是要的名字。
fn first_path_name(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .and_then(|line| line.rsplit('/').next())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

/// `DISPLAYMANAGER="sddm"` → `sddm`。
fn from_assignment(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || !line.to_ascii_uppercase().starts_with("DISPLAYMANAGER") {
            continue;
        }

        let (_, value) = line.split_once('=')?;
        let value = unquote(value.trim());
        if !value.is_empty() {
            return Some(value);
        }
    }

    None
}

/// 去掉一层引号。这两个配置文件里值可能带双引号，也可能不带。
fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    let inner = trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(trimmed);

    inner.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_unit_name_from_a_symlink_target() {
        assert_eq!(
            unit_name("/usr/lib/systemd/system/sddm.service"),
            Some("sddm".to_owned())
        );
        assert_eq!(unit_name("gdm.service"), Some("gdm".to_owned()));
        assert_eq!(
            unit_name("/usr/lib/systemd/system/display-manager.service"),
            Some("display-manager".to_owned()),
            "没解到真名就把文件名给它，总比什么都不说强"
        );
        assert_eq!(unit_name(""), None);
        assert_eq!(unit_name("/usr/lib/systemd/system/.service"), None);
    }

    #[test]
    fn accepts_an_absolute_path_or_a_bare_name() {
        assert_eq!(
            first_path_name("/usr/sbin/lightdm"),
            Some("lightdm".to_owned())
        );
        assert_eq!(first_path_name("lightdm"), Some("lightdm".to_owned()));
        assert_eq!(
            first_path_name("\n# 注释\n/usr/sbin/gdm\n"),
            Some("gdm".to_owned()),
            "注释与空行不算"
        );
        assert_eq!(first_path_name("\n\n"), None);
    }

    #[test]
    fn strips_one_layer_of_quotes() {
        assert_eq!(unquote("\"sddm\""), "sddm");
        assert_eq!(unquote("'sddm'"), "sddm");
        assert_eq!(unquote("sddm"), "sddm");
        assert_eq!(
            unquote("  \" sddm \"  "),
            "sddm",
            "引号内的空白也去掉：配置里写了空格不是名字的一部分"
        );
        assert_eq!(unquote("\""), "\"", "落单的引号不是配对");
    }

    #[test]
    fn parses_the_gentoo_and_suse_assignments() {
        assert_eq!(
            from_assignment("DISPLAYMANAGER=\"sddm\"\n"),
            Some("sddm".to_owned())
        );
        assert_eq!(
            from_assignment("DISPLAYMANAGER=gdm\n"),
            Some("gdm".to_owned())
        );
        assert_eq!(
            from_assignment("# DISPLAYMANAGER=\"xdm\"\nDISPLAYMANAGER=\"lightdm\"\n"),
            Some("lightdm".to_owned()),
            "注释掉的那行不算"
        );
        assert_eq!(from_assignment("XCURSOR_THEME=\"x\"\n"), None);
        assert_eq!(from_assignment("DISPLAYMANAGER=\"\"\n"), None);
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Lm.collect(&Context::for_tests()).unwrap();

        assert_eq!(entries.len(), 1, "这一行无论如何都该有");
        assert_eq!(entries[0].key, "Login Manager");
        assert!(!entries[0].value.is_empty());
    }
}
