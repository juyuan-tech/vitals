//! 配置的数据形态。
//!
//! 这里有两层结构，别混：
//!
//! - [`ConfigFile`]：**用户文件里能写什么**。字段都是 `Option`，
//!   因为「没写」和「写了等于默认的值」必须分得开——前者要保留内置默认，后者要生效。
//! - [`Config`]：**程序内部用的最终配置**。字段都是有值的，采集与渲染只看它。
//!
//! 每个结构都带 `deny_unknown_fields`：计划里明确要求未知字段报错，
//! 拼错一个键名不该静默通过。

use std::path::PathBuf;

use serde::Deserialize;

use crate::config::CURRENT_CONFIG_VERSION;

/// 去掉版本字段的默认值——省略 `config_version` 就按当前版本算。
const fn current_version() -> u32 {
    CURRENT_CONFIG_VERSION
}

/// 用户配置文件的反序列化形态。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    /// 配置版本。省略时按 [`CURRENT_CONFIG_VERSION`] 处理。
    #[serde(default = "current_version")]
    pub config_version: u32,

    /// 要显示的模块，**顺序即显示顺序**。
    ///
    /// 写了就**整体替换**内置默认，不是往默认列表里追加：列表的逐项合并
    /// 没有站得住的语义（顺序怎么排？重复算谁的？）。所以想少显示几个模块，
    /// 就得把要留的列全。
    pub modules: Option<Vec<ModuleEntry>>,
}

impl ConfigFile {
    /// 把用户文件叠到内置默认上，得到最终配置。
    pub(crate) fn into_config(self) -> Config {
        let mut config = Config {
            // 记下文件是针对哪个版本写的。现在没有迁移要跑，将来有了就靠它。
            config_version: self.config_version,
            ..Config::default()
        };

        if let Some(modules) = self.modules {
            config.modules = modules;
        }

        config
    }
}

/// 最终配置：采集与渲染只看这个。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// 这份配置针对的版本。
    pub config_version: u32,
    /// 要显示的模块，顺序即显示顺序。
    pub modules: Vec<ModuleEntry>,
}

impl Default for Config {
    /// 内置默认：`ModuleType::DEFAULT` 那一串，顺序就是显示顺序。
    ///
    /// 注意默认视图**不是**全部模块：全采一遍会让输出长得没人愿意看，
    /// 而且 `--list-modules` 已经把可选项都列出来了。
    fn default() -> Self {
        Self {
            config_version: CURRENT_CONFIG_VERSION,
            modules: ModuleType::DEFAULT
                .iter()
                .copied()
                .map(ModuleEntry::new)
                .collect(),
        }
    }
}

/// 数组表里的一项，对应一段 `[[modules]]`。
///
/// 为什么是 `[[modules]] type = "os"` 而不是 `modules = ["os"]`：
/// 计划里定的就是「数组表 + 类型标签」，而且类型标签走枚举后，
/// 写错一个字母 serde 会直接报 `unknown variant \`cpuu\`, expected one of ...`，
/// 比 untagged 枚举那句 “data did not match any variant” 有用得多。
/// 声明式条件（`PLAN.md` §2.5）也挂在这里。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleEntry {
    /// 模块的类型标签，例如 `"os"`。
    #[serde(rename = "type")]
    pub module_type: ModuleType,

    /// 只在这些平台上采集。空表示不限。
    #[serde(default)]
    pub platforms: Vec<HostOs>,

    /// 命令不在 `PATH` 里就跳过这个模块。**只查 PATH，不执行命令**（`PLAN.md` §2.5）。
    #[serde(default, rename = "when-command-exists")]
    pub when_command_exists: Option<String>,

    /// 路径不存在就跳过这个模块。
    #[serde(default, rename = "when-file-exists")]
    pub when_file_exists: Option<PathBuf>,
}

impl ModuleEntry {
    /// 建一项，不带任何条件。
    #[must_use]
    pub const fn new(module_type: ModuleType) -> Self {
        Self {
            module_type,
            platforms: Vec::new(),
            when_command_exists: None,
            when_file_exists: None,
        }
    }
}

/// 操作系统：`platforms` 条件的取值。
///
/// 同样是枚举而不是字符串（理由同 [`ModuleType`]）：写错一个字母，
/// serde 会报 ``unknown variant `linx`, expected one of ...``，把合法取值一并列出来。
///
/// 名字与 `std::env::consts::OS` 的拼法保持一致——判定「够不够得着这个平台」
/// 就是拿它和编译目标比一下。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostOs {
    /// Linux。绝大多数的发行版在这里；Android 单独算。
    Linux,
    /// macOS。
    Macos,
    /// Windows。
    Windows,
    /// FreeBSD。
    Freebsd,
    /// OpenBSD。
    Openbsd,
    /// NetBSD。
    Netbsd,
    /// Android。
    Android,
    /// Solaris。
    Solaris,
    /// illumos。
    Illumos,
}

impl HostOs {
    /// 全部平台，顺序稳定——错误信息里按这个顺序列出来。
    pub const ALL: [Self; 9] = [
        Self::Linux,
        Self::Macos,
        Self::Windows,
        Self::Freebsd,
        Self::Openbsd,
        Self::Netbsd,
        Self::Android,
        Self::Solaris,
        Self::Illumos,
    ];

    /// 平台名，与 `std::env::consts::OS` 同拼法。
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Freebsd => "freebsd",
            Self::Openbsd => "openbsd",
            Self::Netbsd => "netbsd",
            Self::Android => "android",
            Self::Solaris => "solaris",
            Self::Illumos => "illumos",
        }
    }

    /// 名字 → 平台。不另抄一份名单，避免两边漂移。
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|platform| platform.name() == name)
    }

    /// 本程序跑在哪个操作系统上。
    ///
    /// `env::consts::OS` 是编译期常量，所以这个问题的答案在编译时就已经定了。
    /// 返回 `None` 表示这个目标平台不在上面的名单里——那时带 `platforms` 的条件
    /// 一律不满足（见 `crate::conditions`）。
    #[must_use]
    pub fn current() -> Option<Self> {
        Self::from_name(std::env::consts::OS)
    }
}

/// 模块清单。**这是配置里 `type` 的合法取值，也是 JSON 里的 `type` 字段。**
///
/// 枚举而不是字符串：这样「配置里写了个不存在的模块」在解析期就报错，
/// 而不是等到运行时才发现少采了一个模块。
///
/// 目标是对标 fastfetch 的模块面（`PLAN.md` §5.4），所以这个枚举会长到几十个。
/// 加模块的顺序是：先在这里加一个变体、`name()` 加一行、`COLLECTORS` 注册一个，
/// `tests/config.rs` 的往返测试会盯着这三处别漏。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleType {
    /// 操作系统。
    Os,
    /// 机器型号。
    Host,
    /// 内核版本。
    Kernel,
    /// 固件（BIOS/UEFI）。
    Bios,
    /// 主板。
    Board,
    /// 机箱类型（笔记本还是台式机）。
    Chassis,
    /// 开机时长。
    Uptime,
    /// 平均负载。
    Loadavg,
    /// 进程数与线程数。
    Processes,
    /// CPU 型号与核心数。
    Cpu,
    /// 内存用量。
    Memory,
    /// 交换空间用量。
    Swap,
    /// 磁盘用量。
    Disk,
    /// 当前用户。
    User,
    /// 当前 shell。
    Shell,
    /// 当前终端。
    Terminal,
    /// 终端尺寸。
    TerminalSize,
    /// 区域设置。
    Locale,
    /// 默认编辑器。
    Editor,
    /// 本程序的版本。
    Version,
    /// 1 号进程（init）。
    InitSystem,
    /// `用户@主机名` 标题行。渲染原语之一，键是空的。
    Title,
    /// 一条横线。渲染原语，长度由渲染器决定。
    Separator,
    /// 一个空行。渲染原语。
    Break,
    /// Rust 工具链。
    Rust,
    /// 电池电量与状态。
    Battery,
    /// 外接电源接上了没有。
    PowerAdapter,
    /// 背光亮度。
    Brightness,
    /// 配置的 DNS 服务器。
    Dns,
    /// 可信平台模块（TPM）。
    Tpm,
    /// 已安装的包数。
    Packages,
    /// 显示器分辨率与刷新率。
    Display,
    /// 桌面环境。
    De,
    /// 窗口管理器 / 合成器。
    Wm,
    /// 窗口装饰主题（标题栏那一套）。
    ///
    /// 单独写 `serde(rename)`：kebab-case 会按驼峰边界把它切成 `wm-theme`，
    /// 而 fastfetch 里这个模块叫 `wmtheme`——一个词，中间没有连字符。
    /// 配置里的名字与 `--structure` 的口径必须一致，所以这里显式钉死。
    #[serde(rename = "wmtheme")]
    WmTheme,
    /// 界面主题名。
    Theme,
    /// 图标主题名。
    Icons,
    /// 界面字体。
    Font,
    /// 光标主题名。
    Cursor,
    /// 显卡型号与驱动。
    Gpu,
    /// 终端用的字体。
    TerminalFont,
    /// 默认路由那个本机地址。fastfetch 里叫 `LocalIp`（一个词），
    /// 我们的规范是 kebab-case，别名兼容交给 CLI 层。
    LocalIp,
    /// 当前登录的用户会话。
    Users,
    /// 物理盘。
    PhysicalDisk,
    /// 二级启动器。
    Bootmgr,
    /// 声音设备（服务端与声卡）。
    Sound,
    /// CPU 各级缓存。
    CpuCache,
    /// 登录管理器（显示管理器）。
    Lm,
    /// BTRFS 文件系统用量。
    Btrfs,
    /// 当前日期与时间。
    ///
    /// 单独写 `serde(rename)`：kebab-case 会把 `DateTime` 切成 `date-time`，
    /// 而我们的名字是 `datetime`（一个词，与 fastfetch 的模块名同形）。
    #[serde(rename = "datetime")]
    DateTime,
    /// Wi-Fi 无线网卡。
    Wifi,
    /// 摄像头。
    Camera,
    /// 键盘。
    Keyboard,
}

impl ModuleType {
    /// 全部模块，`--list-modules` 按这个顺序列出。
    pub const ALL: [Self; 53] = [
        Self::Os,
        Self::Host,
        Self::Kernel,
        Self::Bios,
        Self::Board,
        Self::Chassis,
        Self::Uptime,
        Self::Loadavg,
        Self::Processes,
        Self::Cpu,
        Self::Memory,
        Self::Swap,
        Self::Disk,
        Self::User,
        Self::Shell,
        Self::Terminal,
        Self::TerminalSize,
        Self::Locale,
        Self::Editor,
        Self::Version,
        Self::InitSystem,
        Self::Title,
        Self::Separator,
        Self::Break,
        Self::Rust,
        Self::Battery,
        Self::PowerAdapter,
        Self::Brightness,
        Self::Dns,
        Self::Tpm,
        Self::Packages,
        Self::Display,
        Self::De,
        Self::Wm,
        Self::WmTheme,
        Self::Theme,
        Self::Icons,
        Self::Font,
        Self::Cursor,
        Self::Gpu,
        Self::TerminalFont,
        Self::LocalIp,
        Self::Users,
        Self::PhysicalDisk,
        Self::Bootmgr,
        Self::Sound,
        Self::CpuCache,
        Self::Lm,
        Self::Btrfs,
        Self::DateTime,
        Self::Wifi,
        Self::Camera,
        Self::Keyboard,
    ];

    /// 默认视图：**没人写配置时显示这些**，顺序就是显示顺序。
    ///
    /// 顺序照着 fastfetch 2.68.1 的默认视图排（`OS → Host → Kernel → … → Locale`），
    /// 凡是它默认显示、我们也已经实现的，都在这儿。自己的东西只有三处：
    /// `processes`、`loadavg`（各一行，一眼看到机器忙不忙）和 `rust`（本程序自己的版本）。
    ///
    /// 其余模块配一句 `type = "bios"` 就能加进来，`--list-modules` 会列全。
    ///
    /// 后来补进来的 `bootmgr` 与 `local-ip` 也在 fastfetch 的默认视图里：
    /// 前者紧跟 `chassis`，后者在 `disk` 之后、`battery` 之前。`users` 与
    /// `physical-disk` 不进默认视图（fastfetch 默认也不显示它们）。
    pub const DEFAULT: [Self; 35] = [
        Self::Title,
        Self::Separator,
        Self::Os,
        Self::Host,
        Self::Kernel,
        Self::Bios,
        Self::Board,
        Self::Chassis,
        Self::Bootmgr,
        Self::Uptime,
        Self::Packages,
        Self::Shell,
        Self::Display,
        Self::De,
        Self::Wm,
        Self::WmTheme,
        Self::Theme,
        Self::Icons,
        Self::Font,
        Self::Cursor,
        Self::Terminal,
        Self::TerminalFont,
        Self::Cpu,
        Self::Gpu,
        Self::Memory,
        Self::Swap,
        Self::Disk,
        Self::LocalIp,
        Self::Processes,
        Self::Loadavg,
        Self::Battery,
        Self::PowerAdapter,
        Self::Locale,
        Self::Break,
        Self::Rust,
    ];

    /// 模块名。
    ///
    /// 必须和 `#[serde(rename_all = "kebab-case")]` 的结果一模一样，
    /// 也要和 `Collector::name()` 对齐——`tests/config.rs` 里有一条
    /// 逐个往返的测试守着这件事，改错了会红。
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Os => "os",
            Self::Host => "host",
            Self::Kernel => "kernel",
            Self::Bios => "bios",
            Self::Board => "board",
            Self::Chassis => "chassis",
            Self::Uptime => "uptime",
            Self::Loadavg => "loadavg",
            Self::Processes => "processes",
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::Swap => "swap",
            Self::Disk => "disk",
            Self::User => "user",
            Self::Shell => "shell",
            Self::Terminal => "terminal",
            Self::TerminalSize => "terminal-size",
            Self::Locale => "locale",
            Self::Editor => "editor",
            Self::Version => "version",
            Self::InitSystem => "init-system",
            Self::Title => "title",
            Self::Separator => "separator",
            Self::Break => "break",
            Self::Rust => "rust",
            Self::Battery => "battery",
            Self::PowerAdapter => "power-adapter",
            Self::Brightness => "brightness",
            Self::Dns => "dns",
            Self::Tpm => "tpm",
            Self::Packages => "packages",
            Self::Display => "display",
            Self::De => "de",
            Self::Wm => "wm",
            Self::WmTheme => "wmtheme",
            Self::Theme => "theme",
            Self::Icons => "icons",
            Self::Font => "font",
            Self::Cursor => "cursor",
            Self::Gpu => "gpu",
            Self::TerminalFont => "terminal-font",
            Self::LocalIp => "local-ip",
            Self::Users => "users",
            Self::PhysicalDisk => "physical-disk",
            Self::Bootmgr => "bootmgr",
            Self::Sound => "sound",
            Self::CpuCache => "cpu-cache",
            Self::Lm => "lm",
            Self::Btrfs => "btrfs",
            Self::DateTime => "datetime",
            Self::Wifi => "wifi",
            Self::Camera => "camera",
            Self::Keyboard => "keyboard",
        }
    }
}
