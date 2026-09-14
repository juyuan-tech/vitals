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
use std::sync::atomic::{AtomicUsize, Ordering};

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
                    outcome.push_failure(
                        name,
                        CollectError::new(crate::i18n::now().no_such_module(name)),
                    );
                }
            }
        }

        // 并行跑，**但结果按名单顺序收回**——行序与配置顺序一致，与完成先后无关。
        //
        // 收益最明显的是采样模块：四个各自睡 200 ms 的模块串起来是 821 ms（真机实测），
        // 而它们的等待期是空转；并行之后总时长≈一次窗口。
        //
        // 线程数**有上限**（[`WORKERS`]），不是「一条模块一个线程」：后者能被配置直接
        // 触发资源耗尽（写 3000 条重复模块就要 3000 个线程），而且 `scope.spawn` 在起不了
        // 线程时会 panic，那时连一句「哪个模块失败了」都报不出来。这里改成一个共享计数器
        // 派活，起不来线程就少起几个、当前线程把剩下的干掉——降级，不 panic。
        let work: &[&dyn Collector] = &runnable;
        let next = AtomicUsize::new(0);

        let batches: Vec<Vec<(usize, Collected)>> = std::thread::scope(|scope| {
            let mut handles = Vec::new();

            for _ in 0..work.len().min(WORKERS) {
                let spawned = std::thread::Builder::new()
                    .name("vitals-collect".to_owned())
                    .spawn_scoped(scope, || drain(&next, work, ctx));

                match spawned {
                    Ok(handle) => handles.push(handle),
                    Err(_) => break,
                }
            }

            // 当前线程一起干活：工作线程一个也没起来时，这一段就是全部的计算。
            let leftover = drain(&next, work, ctx);

            let mut batches: Vec<Vec<(usize, Collected)>> = handles
                .into_iter()
                .filter_map(|handle| handle.join().ok())
                .collect();
            batches.push(leftover);

            batches
        });

        // 按序号摆回配置顺序。缺席的序号意味着那个模块所在的线程异常结束了：
        // 记成它的失败，其余模块照常渲染（与「一个模块出错不中断整体」同一条规矩）。
        let mut slots: Vec<Option<Collected>> = (0..work.len()).map(|_| None).collect();
        for (index, item) in batches.into_iter().flatten() {
            if let Some(slot) = slots.get_mut(index) {
                *slot = Some(item);
            }
        }

        for (collector, slot) in work.iter().zip(slots) {
            let collected = slot.unwrap_or_else(|| Collected {
                result: Err(CollectError::new(
                    crate::i18n::now().collector_thread_died(),
                )),
                paths: Vec::new(),
            });

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

/// 同时跑几个模块。
///
/// 不按模块数起线程：配置里写 3000 条重复模块就得到 3000 个线程，那是配置能直接触发的
/// 资源耗尽。16 足够——真正需要并行的只有四个各自要等采样窗口的模块
/// （`net-io`/`disk-io`/`cpu-usage`/`top`，见各自的文档），其余都是毫秒级。
const WORKERS: usize = 16;

/// 从共享计数器上领活干，直到领完；每件活带着自己的序号回去。
///
/// 序号是行序的依据：线程按完成顺序往各自的 `Vec` 里写，最后按序号排回来，
/// 于是显示顺序 = 配置顺序，与完成先后无关。
///
/// 取号用 `Relaxed` 就够：这里只需要「每个号只被取到一次」这一个原子性，
/// 不需要用它去同步别的内存。
fn drain(next: &AtomicUsize, work: &[&dyn Collector], ctx: &Context) -> Vec<(usize, Collected)> {
    let mut done = Vec::new();

    loop {
        let index = next.fetch_add(1, Ordering::Relaxed);
        let Some(collector) = work.get(index) else {
            return done;
        };

        done.push((index, collect_one(*collector, ctx)));
    }
}

/// 跑一个模块，并把**这个线程**读过的文件路径一起带回来。
///
/// 来源记录是线程局部的（`core::sources`），所以清空与取走必须在同一个线程里完成。
fn collect_one(collector: &dyn Collector, ctx: &Context) -> Collected {
    sources::clear();
    let result = collector.collect(ctx);
    let paths = sources::take();

    Collected { result, paths }
}
