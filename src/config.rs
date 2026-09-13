//! 配置：TOML 主配置的加载、覆盖与生成。
//!
//! 加载顺序（`PLAN.md` §2.3）：
//!
//! ```text
//! 内置默认  →  用户配置文件（--config 或 XDG 默认路径）  →  CLI 参数
//! ```
//!
//! 阶段 2 只做到「用户文件覆盖默认值」：不做多层系统级合并，CLI 覆盖要等阶段 3。
//!
//! 目前**只支持 TOML**。计划里的 JSONC 兼容是为 fastfetch 用户迁移预留的，
//! 但 fastfetch 的 schema 与本项目不同，所以单独的解析器在适配层（v0.6）存在之前
//! 服务不了任何人。等适配层一起做，别提前摊开两套解析路径。

mod path;
mod schema;

pub use crate::config::path::config_path;
pub use crate::config::schema::{Config, ModuleEntry, ModuleType};

use std::path::{Path, PathBuf};

use crate::config::schema::ConfigFile;

/// 本程序支持的配置版本。
pub const CURRENT_CONFIG_VERSION: u32 = 1;

/// 生成默认配置用的文本。
///
/// 它带注释，所以不是序列化 `Config::default()` 得来的。代价是这份文本可能和
/// schema 漂移——`tests/config.rs` 里有一条测试把它解析回来，逐字段对照
/// `Config::default()`，漂移就红。
#[must_use]
pub fn default_toml() -> &'static str {
    include_str!("config/default.toml")
}

/// 从 TOML 文本解析。
///
/// 测试用得到它。忘了哪来的文本会被记成「配置文本」，出现在错误信息里。
pub fn from_toml(text: &str) -> Result<Config, ConfigError> {
    parse(text, "配置文本")
}

/// 从文件加载。文件不存在是**错误**——显式指了路就该能读到东西。
pub fn load_file(path: &Path) -> Result<Config, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    parse(&text, &path.display().to_string())
}

/// 按加载顺序得到最终配置：内置默认 ← 用户配置文件。
///
/// `explicit` 是 `--config` 给的路径。区别在**文件不存在时**怎么办：
///
/// - 显式指定 → 报错。用户明确指了一个文件，读不到就得说。
/// - 默认路径 → 安静地用内置默认。第一次运行的人不该看到报错。
/// - 连 `HOME` 都没有 → 也用内置默认（`config_path()` 返回 `None`）。
pub fn load(explicit: Option<&Path>) -> Result<Config, ConfigError> {
    if let Some(path) = explicit {
        return load_file(path);
    }

    let Some(path) = config_path() else {
        return Ok(Config::default());
    };

    if path.exists() {
        load_file(&path)
    } else {
        Ok(Config::default())
    }
}

/// 真正干活的那个：解析 + 校验版本 + 叠到默认上。
fn parse(text: &str, origin: &str) -> Result<Config, ConfigError> {
    let file: ConfigFile = toml::from_str(text).map_err(|source| ConfigError::Parse {
        origin: origin.to_owned(),
        source: Box::new(source),
    })?;

    if file.config_version > CURRENT_CONFIG_VERSION {
        return Err(ConfigError::UnsupportedVersion {
            found: file.config_version,
            supported: CURRENT_CONFIG_VERSION,
        });
    }

    Ok(file.into_config())
}

/// 配置出错。
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// 读不到文件（不存在、没权限、不是 UTF-8）。
    ///
    /// 底层原因直接写进消息里：命令行上一行说完，比让人再去翻 `--verbose` 强。
    #[error("读取配置文件 {path} 失败：{source}")]
    Read {
        /// 出问题的路径。
        path: PathBuf,
        /// 底层原因。
        #[source]
        source: std::io::Error,
    },

    /// TOML 语法错、未知字段、未知模块类型、缺字段——都从这里出来。
    ///
    /// `toml::de::Error` 自带行号列号和「expected one of ...」，
    /// 直接把它显示出来比我们转述一遍有用得多，所以 `Box` 起来原样带。
    #[error("{origin} 解析失败\n{source}")]
    Parse {
        /// 出错的是哪个文件（或哪段文本）。
        origin: String,
        /// `toml` 的原始错误，含位置与期望值提示。
        #[source]
        source: Box<toml::de::Error>,
    },

    /// 配置文件比程序新。
    #[error("配置版本 {found} 高于本程序支持的 {supported}，请升级 vitals")]
    UnsupportedVersion {
        /// 文件里写的版本。
        found: u32,
        /// 本程序支持的版本。
        supported: u32,
    },
}
