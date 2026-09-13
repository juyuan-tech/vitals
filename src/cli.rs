//! 命令行：参数定义与「有效设置」的解析。
//!
//! 刻意分成两步，别混：
//!
//! 1. [`Cli`]：clap 解析出来的原始参数，**只反映用户敲了什么**。
//! 2. [`Settings`]：把 CLI、配置文件、内置默认按优先级叠完之后的最终设置。
//!    采集与渲染只看它，不再回头看 `Cli` 或 `Config`。
//!
//! 优先级：CLI > 配置文件 > 内置默认（`PLAN.md` §4）。
//!
//! 放在 lib 而不是 `main.rs` 里，是为了能测：`main` 只负责退出码和往哪个流写。

use std::path::PathBuf;

use clap::Parser;

use crate::config::{Config, ModuleEntry, ModuleType};

// 注意：下面这个文档注释会被 clap 当成 `--help` 的详细说明，所以只写给用户看的话。
// 维护者提醒：`name = "vitals"` 不能省——clap 默认取包名，也就是 `vitals-rs`，
// 那样 `--version` 会打印成 `vitals-rs 0.1.0`，和用户敲的命令名对不上。
/// 你的系统生命体征，一眼看全。
///
/// 不带参数时按配置文件渲染；用 `--module` 只留其中几个。
#[derive(Debug, Parser)]
#[command(
    name = "vitals",
    version,
    about = crate::TAGLINE,
)]
pub struct Cli {
    /// 指定配置文件；不写就找 $XDG_CONFIG_HOME/vitals/config.toml
    #[arg(long, value_name = "路径")]
    pub config: Option<PathBuf>,

    /// 以 JSON 输出（自动关掉颜色与 Logo）
    #[arg(long)]
    pub json: bool,

    /// Logo：auto 按发行版自动选、none 不显示、或直接给名称
    #[arg(long, value_name = "auto|none|名称", default_value = "auto", value_parser = parse_logo)]
    pub logo: LogoChoice,

    /// 只显示这些模块，逗号分隔。顺序就是你写的顺序；配置里没有的也能点
    #[arg(long, value_name = "列表", value_delimiter = ',', value_parser = parse_module)]
    pub module: Option<Vec<ModuleType>>,

    /// 关闭颜色（等同设置 NO_COLOR）
    #[arg(long)]
    pub no_color: bool,

    /// 列出全部可用模块
    #[arg(long)]
    pub list_modules: bool,

    /// 把内置默认配置打印到 stdout
    #[arg(long)]
    pub gen_config: bool,

    /// 把诊断信息（含最终生效的设置）写到 stderr
    #[arg(long)]
    pub verbose: bool,

    /// 逐项说明每个模块为什么出现、或为什么没有出现。
    ///
    /// 与 `--verbose` 的分工：`--verbose` 只说「谁被条件挡住了」，而且写到 stderr；
    /// `--explain` 对着**结果**说话——显示 / 空 / 跳过 / 失败，四种状态各有理由。
    /// 要分辨「空」与「显示」，它得真跑一遍采集，所以会花一次采集的时间。
    #[arg(long)]
    pub explain: bool,
}

/// Logo 的选择。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogoChoice {
    /// 按 `/etc/os-release` 的 `ID` 自动匹配，匹配不到时用通用 Logo。
    Auto,
    /// 不显示 Logo。
    None,
    /// 指定名称，例如 `arch`。
    Named(String),
}

impl std::fmt::Display for LogoChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => f.write_str("auto"),
            Self::None => f.write_str("none"),
            Self::Named(name) => f.write_str(name),
        }
    }
}

/// clap 的取值解析器：`--logo` 的值 → [`LogoChoice`]。
///
/// `auto` 和 `none` 是关键字，其余一律当名称。所以这里**不存在非法取值**，
/// 拼错一个 Logo 名不会报错，只会在阶段 5 匹配不到时回退通用 Logo。
fn parse_logo(value: &str) -> Result<LogoChoice, String> {
    Ok(match value {
        "auto" => LogoChoice::Auto,
        "none" => LogoChoice::None,
        name => LogoChoice::Named(name.to_owned()),
    })
}

/// clap 的取值解析器：`--module` 的一项 → [`ModuleType`]。
///
/// 用 `value_parser` 而不是给 [`ModuleType`] 派生 `clap::ValueEnum`：
/// 前者把校验留在 CLI 边界，错误信息能自己写；后者会让配置模块反过来依赖 clap。
/// 合法取值从 [`ModuleType::ALL`] 现取，不另抄一份名单，避免漂移。
fn parse_module(name: &str) -> Result<ModuleType, String> {
    // 比较时忽略大小写与 `-`/`_`：`LocalIp`、`localip`、`local-ip` 都能用
    // （fastfetch 的长名与我们自己的名字各占一半江山，见 `ModuleType::from_name`）。
    if let Some(module) = ModuleType::from_name(name) {
        return Ok(module);
    }

    let valid: Vec<&str> = ModuleType::ALL.iter().map(|module| module.name()).collect();
    Err(format!("未知模块 `{name}`；可用：{}", valid.join(", ")))
}

/// 叠完优先级之后的最终设置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// 要采集的模块**连同它们的声明式条件**，顺序 = 采集顺序。
    ///
    /// 条件不在这里评估：条件要对着环境算（`PATH`、文件系统），那是
    /// `crate::conditions` 的事。但 `--module` 点名过的模块会**丢掉**配置里的条件——
    /// 显式点名是更强的意愿，用户既然点了它就说明他这会儿就要看它。
    pub modules: Vec<ModuleEntry>,
    /// Logo 选择。`--json` 时已被强制成 [`LogoChoice::None`]。
    pub logo: LogoChoice,
    /// 是否**允许**上色。
    ///
    /// 为 `true` 也只是允许——真正上不上色由阶段 5 的 anstream 按 TTY 决定
    /// （管道里自动无色）。`--json` 和 `--no-color` 会让它变成 `false`。
    pub allow_color: bool,
    /// 输出 JSON 而不是文本。
    pub json: bool,
    /// 打印诊断信息。
    pub verbose: bool,
    /// `--explain`：打印逐项诊断后结束，不渲染画面。
    pub explain: bool,
}

impl Settings {
    /// 按 CLI > 配置文件 > 内置默认 叠出最终设置。
    #[must_use]
    pub fn resolve(cli: &Cli, config: &Config) -> Self {
        // `--module` 是**选择**，不是过滤：点谁采谁，顺序就是你写的顺序。
        //
        // 这里以前是 `retain`（从配置视图里挑掉没点名的）。那样 `--module battery`
        // 在电池不在默认视图时一个字都不显示——看着像「这台机器没电池」，
        // 其实是自己把它滤掉了。现在点名的模块一定出现：配置里有它的用配置里那份，
        // 没有就按内置默认造一份（不带条件）。
        let modules: Vec<ModuleEntry> = match &cli.module {
            Some(only) => only
                .iter()
                .map(|module| ModuleEntry::new(*module))
                .collect(),
            None => config.modules.clone(),
        };

        Self {
            modules,
            logo: if cli.json {
                LogoChoice::None
            } else {
                cli.logo.clone()
            },
            allow_color: !cli.no_color && !cli.json,
            json: cli.json,
            verbose: cli.verbose,
            explain: cli.explain,
        }
    }

    /// 生效的模块名，按采集顺序。
    #[must_use]
    pub fn module_names(&self) -> Vec<&'static str> {
        self.modules
            .iter()
            .map(|entry| entry.module_type.name())
            .collect()
    }

    /// `--verbose` 用的诊断行。
    ///
    /// lib 不打印，交给 `main` 写 stderr——这样 `vitals > 文件` 时诊断不会污染结果。
    #[must_use]
    pub fn describe(&self) -> Vec<String> {
        vec![
            format!(
                "模块 {} 个：{}",
                self.modules.len(),
                self.module_names().join(", ")
            ),
            format!(
                "logo={} 颜色={} json={} verbose={}",
                self.logo,
                on_off(self.allow_color),
                on_off(self.json),
                on_off(self.verbose),
            ),
        ]
    }
}

/// `true` / `false` → `开` / `关`。
fn on_off(value: bool) -> &'static str {
    if value { "开" } else { "关" }
}
