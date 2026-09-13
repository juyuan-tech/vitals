//! Vitals —— 你的系统生命体征，一眼看全。
//!
//! 本 crate 全局遵守的设计约束（见仓库根目录 `PLAN.md`）：
//!
//! - **采集与渲染分离**：模块只返回数据，绝不直接打印。
//! - **单模块失败不影响整体**：警告进 stderr，其余模块继续。
//! - **自有代码零 `unsafe`**：系统调用一律经 `rustix` 的安全封装。
//! - **模块布局**：一律「自名文件 + 同名目录」，禁止 `mod.rs`。
//!
//! 结构上分成两层：`core` 是那四个抽象，`collectors`（阶段 4）是具体模块。
//! 依赖方向单一——模块依赖核心，核心不认识任何模块。

#![forbid(unsafe_code)]
#![warn(clippy::mod_module_files)]

pub mod core;

pub use crate::core::collector::{CollectError, Collector, Context, ModuleName, Platform};
pub use crate::core::dispatch::{Dispatcher, Failure, RunOutcome};
pub use crate::core::info::Info;
pub use crate::core::render::{Logo, RenderError, Renderer, Report};

/// 用户敲的命令名。
///
/// 注意它和包名 `vitals-rs` 不同：包名用于 crates.io，命令名用于终端。
pub const PROGRAM: &str = "vitals";

/// crate 版本，来自 `Cargo.toml`。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 一句话标语。
pub const TAGLINE: &str = "Your system's vital signs, at a glance.";

/// 模块注册表。
///
/// 阶段 4 会把十个采集器填进来。现在**故意是空的**：
/// 阶段 1 只定接口，不写实现。空数组让 `Dispatcher::new(COLLECTORS)`
/// 此刻就能编译通过，阶段 4 只是往数组里加元素，不动任何签名。
pub const COLLECTORS: &[&dyn Collector] = &[];
