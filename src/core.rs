//! 核心抽象：采集器、信息、模块调度、渲染器。
//!
//! 这一层**不知道任何具体模块的存在**：没有 `cpu`、没有 `os`，
//! 只有「一个采集器能产出信息」这条契约。具体模块住在 `crate::collectors`（阶段 4）。
//!
//! 方向是单向的：模块依赖核心，核心不依赖模块。

pub mod collector;
pub mod dispatch;
pub mod info;
pub mod render;
