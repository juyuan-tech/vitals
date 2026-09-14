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
pub use crate::config::schema::{Config, HostOs, ModuleEntry, ModuleType};

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::collectors::read::MAX_READ;
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
    default_toml_for(crate::lang::current())
}

/// 指定语言的默认配置文本。
///
/// 两份带注释的文本内容必须一致：`tests/config.rs` 把中英各解析一遍，
/// 都要求等于 `Config::default()`，只改一份、或者哪份漏了个模块都会红。
#[must_use]
pub fn default_toml_for(lang: crate::lang::Lang) -> &'static str {
    match lang {
        crate::lang::Lang::Zh => include_str!("config/default.toml"),
        crate::lang::Lang::En => include_str!("config/default.en.toml"),
    }
}

/// 从 TOML 文本解析。
///
/// 测试用得到它。忘了哪来的文本会被记成「配置文本」，出现在错误信息里。
pub fn from_toml(text: &str) -> Result<Config, ConfigError> {
    parse(text, crate::i18n::now().config_text())
}

/// 从文件加载。文件不存在是**错误**——显式指了路就该能读到东西。
pub fn load_file(path: &Path) -> Result<Config, ConfigError> {
    let text = read_capped(path)?;
    parse(&text, &path.display().to_string())
}

/// 读配置文件，**带上限**。
///
/// 为什么不用 `fs::read_to_string`：`--config` 指到 `/dev/zero` 或者 FIFO 这类文件时，
/// 它会一直读到内存耗尽。采集器那边早就有这条上限（`read::MAX_READ`，那里的注释点名
/// 的正是 `/dev/zero`），配置文件走的是另一条读路径，曾经漏掉——同一个洞不该留两遍。
fn read_capped(path: &Path) -> Result<String, ConfigError> {
    let failed = |source: std::io::Error| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    };

    let file = File::open(path).map_err(failed)?;

    let mut bytes = Vec::new();
    // 多读一个字节：正好等于上限时也能分清「就是这么大」与「还有更多」。
    file.take(MAX_READ + 1)
        .read_to_end(&mut bytes)
        .map_err(failed)?;

    if bytes.len() as u64 > MAX_READ {
        return Err(ConfigError::TooLarge {
            path: path.to_path_buf(),
            limit: MAX_READ,
        });
    }

    String::from_utf8(bytes).map_err(|error| ConfigError::NotUtf8 {
        path: path.to_path_buf(),
        error,
    })
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
///
/// `Display` 自己写而不是交给 `thiserror` 的属性：消息要按语言给，
/// 而属性里只能写死字面量。
#[derive(Debug)]
pub enum ConfigError {
    /// 读不到文件（不存在、没权限、不是 UTF-8）。
    ///
    /// 底层原因直接写进消息里：命令行上一行说完，比让人再去翻 `--verbose` 强。
    Read {
        /// 出问题的路径。
        path: PathBuf,
        /// 底层原因。
        source: std::io::Error,
    },

    /// TOML 语法错、未知字段、未知模块类型、缺字段——都从这里出来。
    ///
    /// `toml::de::Error` 自带行号列号和「expected one of ...」，
    /// 直接把它显示出来比我们转述一遍有用得多，所以 `Box` 起来原样带。
    Parse {
        /// 出错的是哪个文件（或哪段文本）。
        origin: String,
        /// `toml` 的原始错误，含位置与期望值提示。
        source: Box<toml::de::Error>,
    },

    /// 配置文件超过读取上限。指到 `/dev/zero` 这类文件时就是它兜住的。
    TooLarge {
        /// 配置文件的路径。
        path: PathBuf,
        /// 上限，字节。
        limit: u64,
    },
    /// 配置文件不是 UTF-8。
    NotUtf8 {
        /// 配置文件的路径。
        path: PathBuf,
        /// `String::from_utf8` 的原始错误。
        error: std::string::FromUtf8Error,
    },
    /// 配置文件比程序新。
    UnsupportedVersion {
        /// 文件里写的版本。
        found: u32,
        /// 本程序支持的版本。
        supported: u32,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = crate::i18n::now();

        match self {
            Self::Read { path, source } => {
                formatter.write_str(&text.config_read_failed(path.display(), source))
            }
            Self::Parse { origin, source } => {
                formatter.write_str(&text.config_parse_failed(origin, source))
            }
            Self::TooLarge { path, limit } => {
                formatter.write_str(&text.too_large(path.display(), *limit))
            }
            Self::NotUtf8 { path, .. } => formatter.write_str(&text.not_utf8(path.display())),
            Self::UnsupportedVersion { found, supported } => {
                formatter.write_str(&text.config_version_too_new(*found, *supported))
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::TooLarge { .. } => None,
            Self::NotUtf8 { error, .. } => Some(error),
            Self::UnsupportedVersion { .. } => None,
        }
    }
}
