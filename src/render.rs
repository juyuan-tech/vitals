//! 渲染：把采集结果变成终端上的文字。
//!
//! `crate::core::render` 只定接口，这里放实现。
//! v0.1 只有一种文本渲染器；JSON 渲染器是 v0.2（阶段 6）的事，两者平级。

pub mod json;
pub mod logo;
pub mod sanitize;
pub mod text;
pub mod theme;
