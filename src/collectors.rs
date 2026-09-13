//! 具体模块：v0.1 的十个。
//!
//! 每个模块一个文件、一个单元结构体、一个 [`Collector`] 实现。
//!
//! 依赖方向是单向的：模块依赖 `crate::core`，核心不认识任何模块。
//! 模块之间不互相调用，只共用这里几个无状态工具：
//!
//! - [`read`] / [`env`]：读文件与环境变量，并把「没有」和「失败」分开
//! - [`units`]：字节数与时长的格式化
//! - [`os_release`] / [`accounts`]：两个被多处使用的系统数据源

pub mod accounts;
pub mod cpu;
pub mod disk;
pub mod env;
pub mod host;
pub mod kernel;
pub mod memory;
pub mod os;
pub mod os_release;
pub mod read;
pub mod rust;
pub mod shell;
pub mod units;
pub mod uptime;
pub mod user;

use crate::core::collector::Collector;

/// 模块注册表。
///
/// 顺序无关紧要：**显示顺序由配置决定**（`Config::modules`），
/// 这里只是「有哪些模块」的名单，调度时按名字线性查找。
/// 模块数量是十几个量级，建哈希表纯属自找复杂度（`PLAN.md` §3）。
///
/// 全部是单元结构体，没有状态：模块的配置在阶段 8 才会进来，
/// 那时若真需要，再给它们加字段。
pub const COLLECTORS: &[&dyn Collector] = &[
    &os::Os,
    &host::Host,
    &kernel::Kernel,
    &uptime::Uptime,
    &shell::Shell,
    &user::User,
    &cpu::Cpu,
    &memory::Memory,
    &disk::Disk,
    &rust::Rust,
];
