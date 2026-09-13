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
    /// 内置默认：十个基础模块，按 `PLAN.md` §5.1 的顺序。
    fn default() -> Self {
        Self {
            config_version: CURRENT_CONFIG_VERSION,
            modules: ModuleType::ALL
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
/// 阶段 7 的声明式条件（`platforms` / `when-command-exists`）也挂在这里。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleEntry {
    /// 模块的类型标签，例如 `"os"`。
    #[serde(rename = "type")]
    pub module_type: ModuleType,
}

impl ModuleEntry {
    /// 建一项。
    #[must_use]
    pub const fn new(module_type: ModuleType) -> Self {
        Self { module_type }
    }
}

/// v0.1 的十个基础模块。
///
/// 枚举而不是字符串：这样「配置里写了个不存在的模块」在解析期就报错，
/// 而不是等到运行时才发现少采了一个模块。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleType {
    /// 操作系统。
    Os,
    /// 主机名。
    Host,
    /// 内核版本。
    Kernel,
    /// 开机时长。
    Uptime,
    /// 当前 shell。
    Shell,
    /// 当前用户。
    User,
    /// CPU 型号与核心数。
    Cpu,
    /// 内存用量。
    Memory,
    /// 磁盘用量。
    Disk,
    /// Rust 工具链。
    Rust,
}

impl ModuleType {
    /// 全部模块，顺序即默认显示顺序。
    pub const ALL: [Self; 10] = [
        Self::Os,
        Self::Host,
        Self::Kernel,
        Self::Uptime,
        Self::Shell,
        Self::User,
        Self::Cpu,
        Self::Memory,
        Self::Disk,
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
            Self::Uptime => "uptime",
            Self::Shell => "shell",
            Self::User => "user",
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::Disk => "disk",
            Self::Rust => "rust",
        }
    }
}
