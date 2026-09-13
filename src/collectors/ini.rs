//! 带 section 的 ini 文件，以及这套桌面配置散落在哪些路径。
//!
//! 第三批的五个模块（WM Theme、Theme、Icons、Font、Cursor）读的是同一类文件：
//! GTK 的 `settings.ini`、KDE 的 `kdeglobals` / `kwinrc`、光标主题的 `index.theme`。
//! 格式都是「`[section]` + `键=值`」，所以解析只写一份——写五份必然漂移
//! （一边认 `;` 注释、另一边忘了；或者重复键一个取第一个、一个取最后一个）。
//!
//! **为什么读 ini 而不是问桌面环境**：`gsettings`、`kreadconfig6`、`xfconf-query`
//! 都是子进程，其中两个还要连 D-Bus。这一批的口径是「能读文件就不 fork」，
//! 读不到就是无数据、不猜（每个模块的文档里写了它那条链为什么到头）。
//!
//! 路径也放在这儿：`$XDG_CONFIG_HOME`、`$XDG_CONFIG_DIRS`、`$XDG_DATA_DIRS`
//! 的规矩（必须是绝对路径、空值按默认）五个模块共用，抄五遍同样会漂移。
//! 纯逻辑都抽成了接收注入值的 `*_in` / `split_dirs`：`set_var` 在 edition 2024
//! 里是 `unsafe fn`（本 crate `forbid(unsafe_code)`），不抽就没法给路径写测试
//! ——这条理由与 `config/path.rs` 里那段一样。

use std::path::Path;

use crate::collectors::read;
use crate::core::collector::CollectError;

// ---------------------------------------------------------------------------
// 解析
// ---------------------------------------------------------------------------

/// ini 里的一条键值对。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    /// 所属 section。文件开头、还没遇到 `[section]` 的键没有归属。
    section: Option<String>,
    key: String,
    value: String,
}

/// 一份解析好的 ini。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ini {
    entries: Vec<Entry>,
}

impl Ini {
    /// 解析文本。
    ///
    /// 认的东西刻意只有三样：`[section]`、`键=值`、整行的 `#` / `;` 注释。
    /// 别的行一律丢掉——这些文件是别人写的，遇到看不懂的行要能接着往下读。
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut entries: Vec<Entry> = Vec::new();
        let mut section: Option<String> = None;

        for line in text.lines() {
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            // `[section]` 要求整行以 `]` 收尾：`[foo] bar` 不是合法的 section 头，
            // 与其把 `foo] bar` 当 section 名，不如丢掉这一行。
            // 括号内侧的空白去掉（有的文件写成 `[ Settings ]`）。
            if let Some(name) = line
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                section = Some(name.trim().to_owned());
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            let key = key.trim();
            if key.is_empty() {
                continue;
            }
            let value = unquote(value.trim());

            // 重复键取最后一个。同一个键在 `kdeglobals` 里被多个来源写过是常态，
            // 「后写的赢」是 KConfig 与 GKeyFile 一致的口径。
            if let Some(existing) = entries
                .iter_mut()
                .find(|entry| entry.section == section && entry.key == key)
            {
                existing.value = value;
                continue;
            }

            entries.push(Entry {
                section: section.clone(),
                key: key.to_owned(),
                value,
            });
        }

        Self { entries }
    }

    /// 取一个键的值。
    ///
    /// section 名与键名都**区分大小写**：GTK 的键全小写，而 KDE 的 `ColorScheme`、
    /// `[Icon Theme]` 的大小写就是它的身份，抹平只会读错地方。
    #[must_use]
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.section.as_deref() == Some(section) && entry.key == key)
            .map(|entry| entry.value.as_str())
    }
}

/// 去掉值两侧成对的引号。
///
/// GKeyFile 允许 `键="值"`（值里有空格时常用这种写法），KConfig 不允许。
/// 只在两侧是同一个引号时去掉：`"a` 这种残缺写法原样留着，它多半是文件坏了，
/// 而不是有意为之。
fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    let quoted = bytes.len() >= 2
        && (bytes[0] == b'"' || bytes[0] == b'\'')
        && bytes[bytes.len() - 1] == bytes[0];

    if quoted {
        // 两侧都是 ASCII 引号，所以 1 和 len-1 一定落在字符边界上。
        return value[1..value.len() - 1].to_owned();
    }

    value.to_owned()
}

/// 读并解析一个 ini 文件。
///
/// - 文件不存在 → `Ok(None)`，这是**无数据**
/// - 存在但读不了 → `Err`，这是**真失败**
///
/// 规矩来自 [`crate::collectors::read`]：这一层不重复判断，只把「有没有文件」
/// 翻译成「有没有数据」。
pub fn load(path: &str) -> Result<Option<Ini>, CollectError> {
    Ok(read::text(path)?.map(|text| Ini::parse(&text)))
}

// ---------------------------------------------------------------------------
// 查找线索
// ---------------------------------------------------------------------------

/// 一个值是从哪一类配置里读到的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// GTK 的 `settings.ini`。
    Gtk,
    /// KDE 的 KConfig 文件（`kdeglobals`、`kwinrc`）。
    Kde,
    /// 光标主题的 `index.theme`——libXcursor 认的那一份。
    Xcursor,
}

impl Origin {
    /// 显示在值后面的来源标签。
    ///
    /// 标出来是因为同一项在 GTK 与 KDE 下是两套不同的东西，不标的话
    /// 「Adwaita」到底是从哪儿读的没人看得出来（fastfetch 标成 `[GTK2/3]`，
    /// 我们只读了 GTK3/4，所以按实际的标）。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Gtk => "GTK",
            Self::Kde => "KDE",
            Self::Xcursor => "Xcursor",
        }
    }
}

/// 一条查找线索：在哪个文件的哪个 section 读哪个键。
#[derive(Debug, Clone)]
pub struct Probe {
    path: String,
    section: &'static str,
    key: &'static str,
    origin: Origin,
}

impl Probe {
    /// 显式指定一个文件（光标主题的 `index.theme` 走这条）。
    #[must_use]
    pub fn at(path: String, section: &'static str, key: &'static str, origin: Origin) -> Self {
        Self {
            path,
            section,
            key,
            origin,
        }
    }

    /// 用户级 GTK `settings.ini` 链上的一个键（`$XDG_CONFIG_HOME` 下那两份）。
    #[must_use]
    pub fn gtk_user(key: &'static str) -> Vec<Self> {
        gtk_probes(&gtk_user_settings_paths(), key)
    }

    /// 系统级 GTK `settings.ini` 链上的一个键（配置目录、`/etc`、数据目录）。
    #[must_use]
    pub fn gtk_system(key: &'static str) -> Vec<Self> {
        gtk_probes(&gtk_system_settings_paths(), key)
    }

    /// 用户级的 KDE 配置：`$XDG_CONFIG_HOME/<file>`。
    #[must_use]
    pub fn kconfig_user(file: &str, section: &'static str, key: &'static str) -> Option<Self> {
        kconfig_user_path(file).map(|path| Self {
            path,
            section,
            key,
            origin: Origin::Kde,
        })
    }

    /// 系统级的 KDE 配置：`$XDG_CONFIG_DIRS/<file>`。
    #[must_use]
    pub fn kconfig_system(file: &str, section: &'static str, key: &'static str) -> Vec<Self> {
        kconfig_system_paths(file)
            .into_iter()
            .map(|path| Self {
                path,
                section,
                key,
                origin: Origin::Kde,
            })
            .collect()
    }

    /// 这条线索属于哪一类配置。
    ///
    /// 各模块的候选表是拼出来的（GTK 一段 + 它自己的那一段），顺序就是优先级，
    /// 所以要能断言它是怎么拼的。
    #[must_use]
    pub const fn origin(&self) -> Origin {
        self.origin
    }

    /// 这条线索指向哪个文件。
    ///
    /// 与 [`origin`](Self::origin) 分开给是因为「用户级排在系统级前面」这条规矩
    /// **只有比路径才断言得了**：两段的来源标签都是 `GTK`。
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// 把一组 GTK `settings.ini` 路径变成查找线索。
///
/// 单独抽出来是因为「用户级」与「系统级」两段用的是同一套 section 与来源标签，
/// 只有路径不同——两处各写一遍迟早会漏掉一个字段。
fn gtk_probes(paths: &[String], key: &'static str) -> Vec<Probe> {
    /// GTK 的界面设置都在这个 section 里。
    const SECTION: &str = "Settings";

    paths
        .iter()
        .map(|path| Probe {
            path: path.clone(),
            section: SECTION,
            key,
            origin: Origin::Gtk,
        })
        .collect()
}

/// 一次命中的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// 文件里写的值（已去掉两侧空白与成对引号）。
    pub value: String,
    /// 从哪一类配置读到的。
    pub origin: Origin,
    /// 具体是哪个文件。模块不把它放进输出，留着是为了测试与排错。
    pub path: String,
}

/// 按优先级找第一个「文件在、section 在、键在、值非空」的线索。
///
/// 值非空是硬条件：`gtk-theme-name=` 与「根本没这个键」是同一件事，
/// 拿它顶替只会印出一个空主题名。
///
/// 文件**存在但读不了**时直接报错（[`load`] 的规矩），不偷偷跳到下一个：
/// 权限问题被掩掉之后，用户会以为「这台机器没配主题」。
///
/// # Errors
///
/// 候选文件里任一存在却读不了时，把它包装成 [`CollectError`] 返回。
pub fn probe(candidates: &[Probe]) -> Result<Option<Hit>, CollectError> {
    for candidate in candidates {
        let Some(ini) = load(&candidate.path)? else {
            continue;
        };
        let Some(value) = ini
            .get(candidate.section, candidate.key)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        return Ok(Some(Hit {
            value: value.to_owned(),
            origin: candidate.origin,
            path: candidate.path.clone(),
        }));
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// 路径
// ---------------------------------------------------------------------------

/// 只认绝对路径。XDG 规范这么要求，`HOME=""` 或写成相对值也不该被当成目录。
fn absolute(value: Option<&str>) -> Option<&str> {
    value.filter(|dir| Path::new(dir).is_absolute())
}

/// 拼路径。
///
/// 用 `Path::join` 而不是字符串相加：`$HOME/` 带尾斜杠时手拼会多出一个 `/`。
fn join(base: &str, rest: &str) -> String {
    Path::new(base).join(rest).to_string_lossy().into_owned()
}

/// 追加一个还没出现过的路径。
///
/// 去重不是洁癖：`XDG_CONFIG_DIRS=/etc` 时 `/etc/gtk-3.0/settings.ini`
/// 会被两条不同的规则各拼一次，同一个文件查两遍白费 I/O。
fn push_unique(paths: &mut Vec<String>, path: String) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

/// [`config_home`] 的纯逻辑部分。
fn config_home_in(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<String> {
    if let Some(dir) = absolute(xdg_config_home) {
        return Some(dir.to_owned());
    }

    // XDG_CONFIG_HOME 是相对路径时忽略它，继续走回退，而不是直接放弃。
    absolute(home).map(|home| join(home, ".config"))
}

/// 用户配置目录：`$XDG_CONFIG_HOME`，没设（或不是绝对路径）时 `$HOME/.config`。
///
/// 两个都没有就是 `None`——那种环境（`env -i`）下用户级的配置一律跳过，
/// 系统级的还读得到。
#[must_use]
pub fn config_home() -> Option<String> {
    config_home_in(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

/// 把一个目录列表环境变量拆成目录。
///
/// - 没设**或为空** → 用默认值（规范就这么定的，空串不算「指定了空列表」）；
/// - 只保留绝对路径；
/// - 去过重。
///
/// 变量设了但里面的项全不合法时返回空表：那是用户明说的「不要这些目录」，
/// 不该把默认值硬塞回去。
fn split_dirs(value: Option<&str>, defaults: &[&str]) -> Vec<String> {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return defaults.iter().map(|dir| (*dir).to_owned()).collect();
    };

    let mut dirs: Vec<String> = Vec::new();
    for dir in value
        .split(':')
        .filter_map(|dir| absolute(Some(dir.trim())))
    {
        push_unique(&mut dirs, dir.to_owned());
    }

    dirs
}

/// 系统配置目录：`$XDG_CONFIG_DIRS`，默认 `/etc/xdg`。
#[must_use]
pub fn config_dirs() -> Vec<String> {
    split_dirs(
        std::env::var("XDG_CONFIG_DIRS").ok().as_deref(),
        &["/etc/xdg"],
    )
}

/// 数据目录：`$XDG_DATA_DIRS`，默认 `/usr/local/share:/usr/share`。
#[must_use]
pub fn data_dirs() -> Vec<String> {
    split_dirs(
        std::env::var("XDG_DATA_DIRS").ok().as_deref(),
        &["/usr/local/share", "/usr/share"],
    )
}

/// GTK 的 `settings.ini` 后缀。同一件事在这里有两个版本，共用一份表免得漏掉一个。
const GTK_SUFFIXES: [&str; 2] = ["gtk-3.0/settings.ini", "gtk-4.0/settings.ini"];

/// 用户级 GTK `settings.ini`：`$XDG_CONFIG_HOME/gtk-3.0` 与 `gtk-4.0`。
///
/// 为什么 GTK3 排在 GTK4 前面：两者写的是同一件事，而发行版把默认值放在哪一份上
/// 并不一致。先 GTK3 是为了先按更常被写的那份下结论。
#[must_use]
pub fn gtk_user_settings_paths() -> Vec<String> {
    let Some(home) = config_home() else {
        return Vec::new();
    };

    GTK_SUFFIXES
        .iter()
        .map(|suffix| join(&home, suffix))
        .collect()
}

/// 系统级 GTK `settings.ini`：`$XDG_CONFIG_DIRS`、`/etc`、`$XDG_DATA_DIRS`。
///
/// 为什么要一直翻到数据目录：发行版的 gtk3 包把默认主题就放在
/// `/usr/share/gtk-3.0/settings.ini`（本机 Arch 就是），而它在 `$XDG_DATA_DIRS` 里。
/// 不翻这一层，一台从没手动改过主题的机器上这几项就全是空的。
/// `/etc/gtk-3.0/settings.ini` 也认，那是发行版用过的另一种布局。
///
/// **这些都是「发行版默认」**，所以调用方必须把它们排在任何用户级配置之后
/// ——见各模块的候选表。
#[must_use]
pub fn gtk_system_settings_paths() -> Vec<String> {
    let mut roots: Vec<String> = config_dirs();
    push_unique(&mut roots, "/etc".to_owned());
    roots.extend(data_dirs());

    let mut paths: Vec<String> = Vec::new();
    for root in &roots {
        for suffix in GTK_SUFFIXES {
            push_unique(&mut paths, join(root, suffix));
        }
    }

    paths
}

/// 整条 GTK 链（用户级 → 系统级）。列出所有候选时用它，**判定优先级时不要**：
/// 那样会让发行版默认压在用户自己写的 KDE 配置上面。
#[must_use]
pub fn gtk_settings_paths() -> Vec<String> {
    let mut paths = gtk_user_settings_paths();
    for path in gtk_system_settings_paths() {
        push_unique(&mut paths, path);
    }

    paths
}

/// 用户级的 KConfig 文件：`$XDG_CONFIG_HOME/<file>`。
#[must_use]
pub fn kconfig_user_path(file: &str) -> Option<String> {
    config_home().map(|home| join(&home, file))
}

/// 系统级的 KConfig 文件：`$XDG_CONFIG_DIRS/<file>`。
///
/// 发行版常把默认色板放在 `/etc/xdg/kdeglobals`。和 GTK 那边一样，
/// 这属于「系统默认」，要排在所有用户级配置之后。
#[must_use]
pub fn kconfig_system_paths(file: &str) -> Vec<String> {
    config_dirs().iter().map(|dir| join(dir, file)).collect()
}

/// 整条 KConfig 链（用户级 → 系统级）。
#[must_use]
pub fn kconfig_paths(file: &str) -> Vec<String> {
    let mut paths: Vec<String> = kconfig_user_path(file).into_iter().collect();
    for path in kconfig_system_paths(file) {
        push_unique(&mut paths, path);
    }

    paths
}

/// 用户自己装的光标主题：`$HOME/.icons/default/index.theme`。
///
/// `~/.icons` 是这套约定里唯一没跟上 XDG 的地方，只能按家目录拼。
#[must_use]
pub fn cursor_user_index_path() -> Option<String> {
    absolute(std::env::var("HOME").ok().as_deref())
        .map(|home| join(home, ".icons/default/index.theme"))
}

/// 发行版自带的光标主题：`$XDG_DATA_DIRS/icons/default/index.theme`
/// （本机是 `/usr/share/icons/default/index.theme`，内容只有一行 `Inherits=Adwaita`）。
#[must_use]
pub fn cursor_system_index_paths() -> Vec<String> {
    data_dirs()
        .iter()
        .map(|dir| join(dir, "icons/default/index.theme"))
        .collect()
}

/// 整条光标主题链（用户级 → 系统级）。
#[must_use]
pub fn cursor_index_paths() -> Vec<String> {
    let mut paths: Vec<String> = cursor_user_index_path().into_iter().collect();
    for path in cursor_system_index_paths() {
        push_unique(&mut paths, path);
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // 解析
    // -----------------------------------------------------------------------

    #[test]
    fn reads_a_key_from_the_right_section() {
        let ini =
            Ini::parse("[Settings]\ngtk-theme-name=Adwaita\n[Other]\ngtk-theme-name=Breeze\n");

        assert_eq!(ini.get("Settings", "gtk-theme-name"), Some("Adwaita"));
        assert_eq!(ini.get("Other", "gtk-theme-name"), Some("Breeze"));
        // 认错了 section 就等于读错了值，所以这条不能「找不到就全局找」。
        assert_eq!(ini.get("Missing", "gtk-theme-name"), None);
    }

    #[test]
    fn section_and_key_names_are_case_sensitive() {
        // KDE 的 `[General] ColorScheme` 与 GTK 的 `[Settings] gtk-theme-name`
        // 大小写都是身份的一部分。
        let ini = Ini::parse("[General]\nColorScheme=BreezeDark\n");

        assert_eq!(ini.get("General", "ColorScheme"), Some("BreezeDark"));
        assert_eq!(ini.get("general", "ColorScheme"), None);
        assert_eq!(ini.get("General", "colorscheme"), None);
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let ini = Ini::parse(
            "# 整行注释\n\
             \n\
             [Settings]\n\
             ; 另一种注释\n\
             gtk-theme-name=Adwaita\n",
        );

        assert_eq!(ini.get("Settings", "gtk-theme-name"), Some("Adwaita"));
    }

    #[test]
    fn whitespace_around_separators_is_trimmed() {
        // 本机 `/usr/share/gtk-3.0/settings.ini` 的实际写法就是 ` = `。
        let ini = Ini::parse("[ Settings ]\n  gtk-theme-name  =  Adwaita  \n");

        assert_eq!(ini.get("Settings", "gtk-theme-name"), Some("Adwaita"));
    }

    #[test]
    fn the_last_duplicate_key_wins() {
        // `kdeglobals` 被多个来源写过时是常态，后写的赢。
        let ini = Ini::parse("[General]\nColorScheme=Old\nColorScheme=New\n");
        assert_eq!(ini.get("General", "ColorScheme"), Some("New"));

        // 不同 section 的同名键互不影响。
        let ini = Ini::parse("[A]\nx=1\n[B]\nx=2\n[A]\nx=3\n");
        assert_eq!(ini.get("A", "x"), Some("3"));
        assert_eq!(ini.get("B", "x"), Some("2"));
    }

    #[test]
    fn a_value_may_contain_equals_signs() {
        let ini = Ini::parse("[Settings]\ngtk-theme-name=a=b\n");
        assert_eq!(ini.get("Settings", "gtk-theme-name"), Some("a=b"));
    }

    #[test]
    fn quoted_values_lose_their_quotes() {
        let ini = Ini::parse("[Settings]\ngtk-font-name=\"Cantarell 11\"\nplain=Adwaita\n");

        assert_eq!(ini.get("Settings", "gtk-font-name"), Some("Cantarell 11"));
        assert_eq!(ini.get("Settings", "plain"), Some("Adwaita"));
    }

    #[test]
    fn a_lone_quote_is_left_alone() {
        // 残缺的引号多半意味着文件坏了，这时原样带出来比悄悄吞掉一个引号好查。
        let ini = Ini::parse("[Settings]\nx=\"a\n");
        assert_eq!(ini.get("Settings", "x"), Some("\"a"));
    }

    #[test]
    fn an_inline_hash_is_part_of_the_value() {
        // GKeyFile 只认整行注释：`#` 出现在值中间时是值的一部分。
        // 自作主张当注释切掉，会让一个真有 `#` 的主题名少半截。
        let ini = Ini::parse("[Settings]\ngtk-theme-name=Adwaita #3\n");
        assert_eq!(ini.get("Settings", "gtk-theme-name"), Some("Adwaita #3"));
    }

    #[test]
    fn junk_lines_survive() {
        // 不是注释、不是 section、也没有 `=` 的行丢掉，后面的照读。
        let ini = Ini::parse("[Settings]\n这不是键值对\ngtk-theme-name=Adwaita\n=值没有键\n");

        assert_eq!(ini.get("Settings", "gtk-theme-name"), Some("Adwaita"));
        assert_eq!(ini.get("Settings", ""), None);
    }

    #[test]
    fn a_malformed_section_header_is_not_a_section() {
        // `[foo] bar` 不是合法的 section 头：宁可丢掉，也别把 `foo] bar` 当 section 名。
        let ini = Ini::parse("[foo] bar\nx=1\n");

        assert_eq!(ini.get("foo] bar", "x"), None);
        assert_eq!(ini.get("foo", "x"), None);
    }

    #[test]
    fn keys_before_any_section_belong_to_no_section() {
        // 这些文件里键都在 section 下；没有归属的键读不到，而不是被算进某个 section。
        let ini = Ini::parse("loose=1\n[Settings]\ngtk-theme-name=Adwaita\n");

        assert_eq!(ini.get("Settings", "loose"), None);
        assert_eq!(ini.get("", "loose"), None);
    }

    #[test]
    fn an_empty_value_is_a_missing_key_for_the_lookup() {
        // 解析层不抹平：`键=` 与「没这个键」在这一层分得开（`Some("")` 与 `None`）。
        let ini = Ini::parse("[Settings]\ngtk-theme-name=\n");
        assert_eq!(ini.get("Settings", "gtk-theme-name"), Some(""));

        // 抹平发生在查找层：空值等于没配，`probe` 会继续往下找。
        let empty = scratch("empty-is-missing", "[Settings]\ngtk-theme-name=\n");
        assert_eq!(
            probe(&[Probe::at(
                empty.clone(),
                "Settings",
                "gtk-theme-name",
                Origin::Gtk,
            )])
            .unwrap(),
            None
        );
        let _ = std::fs::remove_file(empty);
    }

    // -----------------------------------------------------------------------
    // 路径
    // -----------------------------------------------------------------------

    #[test]
    fn xdg_config_home_wins_over_home() {
        assert_eq!(
            config_home_in(Some("/xdg"), Some("/home/u")).as_deref(),
            Some("/xdg")
        );
        assert_eq!(
            config_home_in(None, Some("/home/u")).as_deref(),
            Some("/home/u/.config")
        );
    }

    #[test]
    fn a_relative_config_home_falls_through_to_home() {
        // XDG 规范：相对路径无效。忽略它之后继续走回退，而不是凭空拼一个相对路径。
        assert_eq!(
            config_home_in(Some("relative/dir"), Some("/home/u")).as_deref(),
            Some("/home/u/.config")
        );
        assert_eq!(config_home_in(Some("relative/dir"), None), None);
    }

    #[test]
    fn no_environment_means_no_user_directories() {
        assert_eq!(config_home_in(None, None), None);
        assert_eq!(config_home_in(Some(""), Some("")), None);
    }

    #[test]
    fn an_unset_or_empty_variable_falls_back_to_the_defaults() {
        for value in [None, Some(""), Some("   ")] {
            assert_eq!(split_dirs(value, &["/etc/xdg"]), vec!["/etc/xdg"]);
        }
    }

    #[test]
    fn directory_lists_are_split_and_filtered() {
        let dirs = split_dirs(Some("/a:/b:/c"), &["/default"]);

        assert_eq!(dirs, vec!["/a", "/b", "/c"]);

        // 相对路径与空项丢掉，绝对路径留着；去过重。
        let dirs = split_dirs(Some("/a::relative:/a"), &["/default"]);
        assert_eq!(dirs, vec!["/a"]);

        // 全是非法项时是空表，而不是把默认值硬塞回来。
        assert!(split_dirs(Some("relative:also/relative"), &["/default"]).is_empty());
    }

    // -----------------------------------------------------------------------
    // 查找
    // -----------------------------------------------------------------------

    /// 一个一定不存在的路径。
    fn nonexistent() -> &'static str {
        "/definitely/not/here/vitals-settings.ini"
    }

    /// 临时文件。单元测试拿不到 `CARGO_TARGET_TMPDIR`（那是集成测试的），
    /// 所以用系统临时目录 + 每个测试自己的文件名，互不干扰。
    fn scratch(name: &str, content: &str) -> String {
        let path = std::env::temp_dir().join(format!("vitals-ini-{name}.ini"));
        std::fs::write(&path, content).expect("临时文件该写得进去");
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn a_missing_file_is_no_data_not_a_failure() {
        assert_eq!(load(nonexistent()).unwrap(), None);
        assert_eq!(probe(&[]).unwrap(), None);
    }

    #[test]
    fn probing_stops_at_the_first_hit() {
        // 后一个文件写了别的值：谁先命中就是谁，不能两个都读。
        let first = scratch("first-hit", "[Settings]\ngtk-theme-name=Adwaita\n");
        let second = scratch("second-hit", "[Settings]\ngtk-theme-name=Breeze\n");

        let hit = probe(&[
            Probe::at(first.clone(), "Settings", "gtk-theme-name", Origin::Gtk),
            Probe::at(second, "Settings", "gtk-theme-name", Origin::Kde),
        ])
        .unwrap()
        .expect("第一个文件里有这个键");

        assert_eq!(hit.value, "Adwaita");
        assert_eq!(hit.origin, Origin::Gtk);
        assert_eq!(hit.path, first);

        let _ = std::fs::remove_file(&first);
    }

    #[test]
    fn probing_skips_empty_values_and_missing_keys() {
        let empty = scratch("empty-value", "[Settings]\ngtk-theme-name=\n");
        let other = scratch("other-section", "[Other]\ngtk-theme-name=Adwaita\n");
        let good = scratch("good-value", "[Settings]\ngtk-theme-name=Adwaita\n");

        // 空值 → 下一个；section 不对 → 下一个；都没有就回头找写着的那个。
        let hit = probe(&[
            Probe::at(empty.clone(), "Settings", "gtk-theme-name", Origin::Gtk),
            Probe::at(other.clone(), "Settings", "gtk-theme-name", Origin::Gtk),
        ])
        .unwrap();

        assert_eq!(hit, None, "两个候选都不算命中");

        let hit = probe(&[
            Probe::at(
                nonexistent().to_owned(),
                "Settings",
                "gtk-theme-name",
                Origin::Gtk,
            ),
            Probe::at(good.clone(), "Settings", "gtk-theme-name", Origin::Gtk),
        ])
        .unwrap()
        .expect("第三个候选里有值");

        assert_eq!(hit.value, "Adwaita");

        for path in [empty, other, good] {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn every_real_settings_file_we_find_parses_without_failing() {
        // 真机 smoke：这台机器上有就真读一遍，一个都没有也算通过
        // ——「无数据」本来就是合法结局。
        // 顺带钉住「文件里有的键值对，解析器得认出来」：本机
        // `~/.config/gtk-3.0/settings.ini` 只有一行 `gtk-im-module=fcitx`，
        // 而 `/usr/share/gtk-3.0/settings.ini` 里才有主题那三行。
        for path in gtk_settings_paths() {
            let Some(text) = read::text(&path).expect("读这些设置文件不该失败") else {
                continue;
            };

            let has_pairs = text.lines().any(|line| {
                let line = line.trim();
                !line.is_empty()
                    && !line.starts_with('#')
                    && !line.starts_with(';')
                    && !line.starts_with('[')
                    && line.contains('=')
            });

            if has_pairs {
                assert!(
                    !Ini::parse(&text).entries.is_empty(),
                    "{path} 里有键值对，却一条都没解析出来"
                );
            }
        }
    }

    #[test]
    fn the_gtk_chain_puts_the_user_first_and_gtk3_before_gtk4() {
        let paths = gtk_settings_paths();

        // 顺序是可断言的：用户级一定在系统级前面，gtk-3.0 一定在同级的 gtk-4.0 前面。
        let user = config_home();
        if let Some(home) = user {
            let gtk3 = join(&home, "gtk-3.0/settings.ini");
            let gtk4 = join(&home, "gtk-4.0/settings.ini");
            let position = |path: &str| paths.iter().position(|candidate| candidate == path);

            assert_eq!(position(&gtk3), Some(0));
            assert_eq!(position(&gtk4), Some(1));
            assert!(paths.iter().all(|path| path.ends_with("settings.ini")));
        }
        // HOME 都没有的环境里也只是这条断言跳过，不该 panic。
    }

    #[test]
    fn user_level_and_system_level_paths_do_not_mix() {
        // 这条守着整个优先级规则的物理基础：用户级那份必须来自 `$XDG_CONFIG_HOME`，
        // 系统级那份必须来自配置目录 / `/etc` / 数据目录，两边不能有交集
        // ——有交集就意味着某个文件会被查两遍、或者用户的配置被判成了默认值。
        let user = gtk_user_settings_paths();
        let system = gtk_system_settings_paths();

        assert!(user.len() <= 2, "用户级只有 gtk-3.0 与 gtk-4.0 两份");
        for path in &user {
            assert!(!system.contains(path), "{path} 同时出现在用户级与系统级里");
        }

        // 本机 Arch 的 gtk3 包把默认主题放在数据目录里，那是系统级。
        if let Some(home) = config_home() {
            assert!(user.iter().all(|path| path.starts_with(&home)));
        }
        if data_dirs().iter().any(|dir| dir == "/usr/share") {
            assert!(
                system.contains(&"/usr/share/gtk-3.0/settings.ini".to_owned()),
                "数据目录里那份发行版默认值要在系统级候选里"
            );
            assert!(
                !user.contains(&"/usr/share/gtk-3.0/settings.ini".to_owned()),
                "发行版默认值不能被当成用户级"
            );
        }
    }

    #[test]
    fn kconfig_candidates_are_user_then_system() {
        let paths = kconfig_paths("kdeglobals");

        if let Some(home) = config_home() {
            assert_eq!(
                paths.first().map(String::as_str),
                Some(join(&home, "kdeglobals").as_str()),
                "用户配置目录要排在最前"
            );
            assert_eq!(
                kconfig_user_path("kdeglobals").as_deref(),
                Some(join(&home, "kdeglobals").as_str())
            );
        }

        let mut unique = paths.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(paths.len(), unique.len(), "同一个文件不该查两遍");
    }

    #[test]
    fn cursor_candidates_cover_the_home_and_the_data_dirs() {
        let paths = cursor_index_paths();

        assert!(paths.iter().all(|path| path.ends_with("index.theme")));

        if let Some(home) = absolute(std::env::var("HOME").ok().as_deref()) {
            assert_eq!(
                paths.first().map(String::as_str),
                Some(join(home, ".icons/default/index.theme").as_str()),
                "用户自己装的光标主题优先"
            );
            assert_eq!(
                cursor_user_index_path().as_deref(),
                Some(join(home, ".icons/default/index.theme").as_str())
            );
        }
        // 系统级的都来自数据目录。
        for path in cursor_system_index_paths() {
            assert!(
                data_dirs().iter().any(|dir| path.starts_with(dir)),
                "{path}"
            );
        }
    }
}
