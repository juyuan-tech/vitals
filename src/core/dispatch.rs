//! 模块调度：配置里的声明 → 映射到采集器 → 评估条件 → 执行 → 收集结果。
//!
//! 阶段 1 只实现中间三步里最简单的那个形状（查名字、顺序跑、收结果）。
//! 另外两步的落点已经标好：
//!
//! - **条件评估**（阶段 7）在 `find` 之后、`collect` 之前加一道 `matches`。
//! - **并行采集**：已经做了（`std::thread::scope`）。`Collector: Send + Sync` 与 `&self`
//!   就是为它准备的。最直接的收益是四个采样模块——它们各自要睡 200 ms 等窗口，
//!   串起来真机实测 821 ms，而这些等待期其实是空转。

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

/// 一个模块跑完带回来的东西。
///
/// 来源必须跟结果**一起**从线程里带出来：`core::sources` 是线程局部的，
/// 出了那个线程就取不到了（这也正是每个任务自己 `clear`、自己 `take` 的原因）。
struct Collected {
    /// 采集结果。
    result: Result<Vec<Info>, CollectError>,
    /// 这个模块实际碰过的路径。
    paths: Vec<String>,
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

        // 先按名单逐个找回采集器（找不到名字的照旧记一条失败，不进线程）。
        let mut runnable: Vec<&'a dyn Collector> = Vec::new();
        for name in plan {
            match self.find(name) {
                Some(collector) => runnable.push(collector),
                None => {
                    outcome
                        .push_failure(name, CollectError::new(format!("没有名为 `{name}` 的模块")));
                }
            }
        }

        // 并行跑，**但结果按名单顺序收回**——行序与配置顺序一致，与完成先后无关。
        //
        // 收益最明显的是采样模块：四个各自睡 200 ms 的模块串起来是 821 ms（真机实测），
        // 而它们的等待期是空转；并行之后总时长≈一次窗口。
        //
        // 来源记录用的是线程局部（`core::sources`），所以清空与取走必须在**同一个线程**
        // 里完成——这就是每个任务自己 `clear`、自己 `take` 的原因。
        let collected: Vec<Collected> = std::thread::scope(|scope| {
            let handles: Vec<_> = runnable
                .iter()
                .map(|collector| {
                    scope.spawn(move || {
                        sources::clear();
                        let result = collector.collect(ctx);
                        let paths = sources::take();

                        Collected { result, paths }
                    })
                })
                .collect();

            handles
                .into_iter()
                .map(|handle| {
                    // 某个模块 panic 不该拖垮整趟输出：把它记成这个模块的失败，
                    // 其余模块照常渲染（与「一个模块出错不中断整体」同一条规矩）。
                    handle.join().unwrap_or_else(|_| Collected {
                        result: Err(CollectError::new("采集线程异常结束")),
                        paths: Vec::new(),
                    })
                })
                .collect()
        });

        for (collector, collected) in runnable.iter().zip(collected) {
            match collected.result {
                // 空 Vec 走到这里也一样：extend 什么都不做，不留痕。
                Ok(entries) => outcome.entries.extend(entries),
                Err(error) => outcome.push_failure(collector.name(), error),
            }

            outcome.sources.push(ModuleSources {
                module: collector.name().to_owned(),
                paths: collected.paths,
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
