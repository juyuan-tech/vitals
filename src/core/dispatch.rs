//! 模块调度：配置里的声明 → 映射到采集器 → 评估条件 → 执行 → 收集结果。
//!
//! 阶段 1 只实现中间三步里最简单的那个形状（查名字、顺序跑、收结果）。
//! 另外两步的落点已经标好：
//!
//! - **条件评估**（阶段 7）在 `find` 之后、`collect` 之前加一道 `matches`。
//! - **并行采集**（阶段 10）把 `for` 换成 `std::thread::scope`；
//!   `Collector: Send + Sync` 和 `&self` 就是为它准备的。

use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;
use crate::core::sources;

/// 一次调度的结果。
///
/// 条目和失败**都要**带回去：失败不能吞掉（要变成 stderr 上的警告），
/// 但也不能让它把已经采到的数据一起废掉。
#[derive(Debug, Default)]
pub struct RunOutcome {
    /// 所有模块的条目，顺序 = 配置顺序 = 显示顺序。
    pub entries: Vec<Info>,
    /// 失败的模块。空表示这次全部正常。
    pub failures: Vec<Failure>,
    /// 每个模块**实际读了哪些文件**（运行时记录，见 `core::sources`）。
    pub sources: Vec<ModuleSources>,
}

impl RunOutcome {
    /// 记录一个模块失败。
    fn push_failure(&mut self, module: &str, error: CollectError) {
        self.failures.push(Failure {
            module: module.to_owned(),
            error,
        });
    }
}

/// 一个模块的失败记录。
///
/// `module` 是 `String` 而不是 `&'static str`：模块名不在注册表里时
/// （配置写错了），我们只能记下用户写的那个名字。
#[derive(Debug)]
pub struct Failure {
    /// 出问题的模块名。
    pub module: String,
    /// 失败原因。
    pub error: CollectError,
}

/// 一个模块实际读了哪些文件。
///
/// `paths` 为空是正常情况：数据可能来自环境变量或系统调用（`uptime`、`hostname`）。
/// 报告里会照实说「没有读文件」，而不是编一个来源出来。
#[derive(Debug, Default)]
pub struct ModuleSources {
    /// 模块名。
    pub module: String,
    /// 碰过的路径，按首次碰到的顺序。
    pub paths: Vec<String>,
}

/// 模块调度器。
pub struct Dispatcher<'a> {
    collectors: &'a [&'a dyn Collector],
}

impl<'a> Dispatcher<'a> {
    /// 用一张注册表建调度器。
    ///
    /// 注册表就是一张静态数组，查找是线性的——模块数量是十几个，
    /// 建哈希表纯属浪费和自找复杂度。详见 `crate::COLLECTORS`。
    #[must_use]
    pub const fn new(collectors: &'a [&'a dyn Collector]) -> Self {
        Self { collectors }
    }

    /// 按 `plan` 给的顺序逐个执行。
    ///
    /// 三种情况的处理刻意不同，这是计划里定的规矩：
    ///
    /// - 名字对不上 → 记 `Failure`（阶段 2 会在配置校验时就先拦下来）
    /// - `Ok(空)` → 跳过，**不记警告**。这才叫「无数据不是错误」
    /// - `Err(..)` → 记 `Failure`，**继续跑后面的模块**，绝不中断整体
    #[must_use]
    pub fn run(&self, plan: &[&str], ctx: &Context) -> RunOutcome {
        let mut outcome = RunOutcome::default();

        for name in plan {
            let Some(collector) = self.find(name) else {
                outcome.push_failure(name, CollectError::new(format!("没有名为 `{name}` 的模块")));
                continue;
            };

            // 每个模块开始前清空记录、跑完取走：来源归属不会串到下一个模块。
            sources::clear();

            match collector.collect(ctx) {
                // 空 Vec 走到这里也一样：extend 什么都不做，不留痕。
                Ok(entries) => outcome.entries.extend(entries),
                Err(error) => outcome.push_failure(collector.name(), error),
            }

            outcome.sources.push(ModuleSources {
                module: collector.name().to_owned(),
                paths: sources::take(),
            });
        }

        outcome
    }

    /// 按名字找采集器。
    fn find(&self, name: &str) -> Option<&'a dyn Collector> {
        self.collectors
            .iter()
            .copied()
            .find(|collector| collector.name() == name)
    }
}
