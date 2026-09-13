//! 采集器：所有模块的统一接口。

use std::time::Duration;

use crate::core::info::Info;

/// 模块名。
///
/// 它同时是配置里的类型标签和 JSON 输出里的 `type` 字段，所以必须是 `'static`：
/// 它来自代码，不来自运行时输入。
pub type ModuleName = &'static str;

/// 一个采集器。
///
/// 三条硬约束：
///
/// 1. `Send + Sync`——阶段 10 要并行采集，所以接口现在就定死，免得将来改签名。
/// 2. **只返回数据，绝不打印**。打印是渲染器的事。
/// 3. 结局只有三种，别混：
///    - 有数据 → `Ok(entries)`
///    - **无数据** → `Ok(空)`。文件不存在、字段缺失都算这类，**不是错误、不报警告**。
///    - 真失败 → `Err(..)`。权限不足、解析失败、外部命令失败或超时才算。
pub trait Collector: Send + Sync {
    /// 模块名，与配置里的类型标签一一对应。
    fn name(&self) -> ModuleName;

    /// 采集一次。
    ///
    /// 拿 `&self` 而不是 `&mut self`：并行采集时多个线程要同时借用它。
    /// 需要缓存就自己上内部可变性，别把 `&mut` 写进签名。
    ///
    /// 返回 `Vec` 而不是单个 `Info`，是因为一个模块可能产出多行
    /// （v0.2 的「多挂载点磁盘」就是几条 `disk` 条目）。
    fn collect(&self, ctx: &Context) -> Result<Vec<Info>, CollectError>;
}

/// 采集上下文：所有模块共享的、已经解析好的输入。
///
/// 这里只放「每个模块都需要、且重复计算不值得」的东西。
/// 阶段 2 会加一个 `settings` 字段（模块自己的配置块）——
/// 给结构体加字段不会破坏已有的 `Collector` 实现，所以现在不预造配置类型。
#[derive(Debug, Clone)]
pub struct Context {
    /// 已解析的平台信息，省得每个模块各自去读 `/etc/os-release`。
    pub platform: Platform,
    /// 外部命令的超时预算。阶段 7/9 用它，现在只是躺在上下文里。
    pub timeout: Duration,
}

impl Context {
    /// 组装一个上下文。
    #[must_use]
    pub const fn new(platform: Platform, timeout: Duration) -> Self {
        Self { platform, timeout }
    }
}

/// 平台信息。
///
/// 字段都是从 `/etc/os-release` 里读的那两个键：
/// `ID` 用来选 Logo，`ID_LIKE` 是选不到时的回退链
/// （没有它，cachyos、endeavouros 这类衍生版会全掉到通用 Logo）。
#[derive(Debug, Clone, Default)]
pub struct Platform {
    /// `/etc/os-release` 的 `ID`，例如 `arch`、`cachyos`。
    pub os_id: Option<String>,
    /// `/etc/os-release` 的 `ID_LIKE`，例如 `arch`。
    pub os_id_like: Option<String>,
}

/// 采集失败。
///
/// 这是**统一**的错误类型：模块自己在内部可以用更具体的 thiserror 枚举，
/// 但跨过模块边界时都收敛成它。目前只需要「能打印」，
/// 所以它是 message + source 的结构体而不是 enum——
/// 哪天真的需要按类别分支（比如只重试 Timeout），再升级成 enum。
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct CollectError {
    message: String,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl CollectError {
    /// 只说明白了什么事。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    /// 带上底层原因，保留完整错误链（`--verbose` 会打印出来）。
    pub fn caused_by(
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}
