//! 阶段 1 验收：用一个假采集器跑通「采集 → 渲染」。
//!
//! 这里刻意**不碰真实系统**：数据全是构造的，所以这套测试在任何机器上都稳定，
//! 现在也不需要 `fixture`——真正读 `/proc` 的模块要到阶段 4 才出现。
//!
//! 被验证的是四条契约（都在 `PLAN.md` 里定过）：
//!
//! 1. 顺序即配置顺序；
//! 2. 无数据**不是**错误，不留警告；
//! 3. 单模块失败不中断整体；
//! 4. 采集与渲染真的分开了——渲染器只吃 `Report`，不认识任何模块。

use std::io::Write;
use std::time::Duration;

use vitals_rs::{
    CollectError, Collector, Context, Dispatcher, Info, Logo, Platform, RenderError, Renderer,
    Report,
};

// ---------------------------------------------------------------------------
// 假采集器：三种行为，分别对应模块的三种结局
// ---------------------------------------------------------------------------

/// 假采集器表现出来的三种结局。
enum Behavior {
    /// 有数据。
    Data,
    /// 无数据：文件不存在、字段缺失都算这类。**不是错误。**
    NoData,
    /// 真失败：权限不足、解析失败、命令超时。
    Fails,
}

struct FakeCollector {
    name: &'static str,
    behavior: Behavior,
}

impl FakeCollector {
    const fn new(name: &'static str, behavior: Behavior) -> Self {
        Self { name, behavior }
    }
}

impl Collector for FakeCollector {
    fn name(&self) -> &'static str {
        self.name
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        match self.behavior {
            Behavior::Data => Ok(vec![
                Info::new(self.name, "Key", "Value").with_variable("key", "Value"),
            ]),
            Behavior::NoData => Ok(Vec::new()),
            Behavior::Fails => Err(CollectError::new("假装的失败")),
        }
    }
}

// 用 static 而不是 const：这样 `&ALPHA` 拿到的才是真正的 'static 引用。
static ALPHA: FakeCollector = FakeCollector::new("alpha", Behavior::Data);
static BETA: FakeCollector = FakeCollector::new("beta", Behavior::NoData);
static GAMMA: FakeCollector = FakeCollector::new("gamma", Behavior::Fails);
static DELTA: FakeCollector = FakeCollector::new("delta", Behavior::Data);

static COLLECTORS: &[&dyn Collector] = &[&ALPHA, &BETA, &GAMMA, &DELTA];

fn dispatcher() -> Dispatcher<'static> {
    Dispatcher::new(COLLECTORS)
}

fn context() -> Context {
    Context::new(
        Platform {
            os_id: Some("arch".to_owned()),
            os_id_like: None,
        },
        Duration::from_secs(2),
    )
}

// ---------------------------------------------------------------------------
// 假渲染器：把 Report 原样写成文本，方便断言
// ---------------------------------------------------------------------------

struct RecordingRenderer;

impl Renderer for RecordingRenderer {
    fn render(&self, report: &Report<'_>, out: &mut dyn Write) -> Result<(), RenderError> {
        if let Some(logo) = report.logo {
            writeln!(out, "LOGO:{}", logo.id)?;
        }
        for info in report.entries {
            writeln!(out, "{}: {}", info.key, info.value)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 契约
// ---------------------------------------------------------------------------

#[test]
fn data_module_produces_entries() {
    let outcome = dispatcher().run(&["alpha"], &context());

    assert_eq!(outcome.entries.len(), 1);
    assert_eq!(outcome.entries[0].module, "alpha");
    assert_eq!(outcome.entries[0].key, "Key");
    assert_eq!(outcome.entries[0].value, "Value");
    assert!(outcome.failures.is_empty());
}

#[test]
fn template_variables_travel_with_the_entry() {
    let outcome = dispatcher().run(&["alpha"], &context());
    let info = &outcome.entries[0];

    assert_eq!(info.variable("key"), Some("Value"));
    assert_eq!(info.variable("不存在的变量"), None);
}

#[test]
fn plan_order_is_the_display_order() {
    // 故意让 delta 排在 alpha 前面，并夹一个会失败的模块。
    let outcome = dispatcher().run(&["delta", "gamma", "alpha"], &context());

    let order: Vec<&str> = outcome.entries.iter().map(|info| info.module).collect();
    assert_eq!(order, ["delta", "alpha"]);
}

#[test]
fn no_data_is_not_a_failure() {
    let outcome = dispatcher().run(&["beta"], &context());

    assert!(outcome.entries.is_empty());
    // 这条是重点：文件不存在不该在 stderr 上留警告。
    assert!(outcome.failures.is_empty(), "无数据不该产生警告");
}

#[test]
fn one_failing_module_does_not_stop_the_others() {
    let outcome = dispatcher().run(&["alpha", "gamma", "delta"], &context());

    // gamma 挂了，但排在它后面的 delta 照跑。
    let order: Vec<&str> = outcome.entries.iter().map(|info| info.module).collect();
    assert_eq!(order, ["alpha", "delta"]);

    assert_eq!(outcome.failures.len(), 1);
    assert_eq!(outcome.failures[0].module, "gamma");
    assert!(outcome.failures[0].error.to_string().contains("假装的失败"));
}

#[test]
fn unknown_module_is_reported_not_ignored() {
    let outcome = dispatcher().run(&["nope", "alpha"], &context());

    assert_eq!(outcome.failures.len(), 1);
    assert_eq!(outcome.failures[0].module, "nope");
    assert!(outcome.failures[0].error.to_string().contains("nope"));
    // 不认识的模块不该顺带弄丢认识的那个。
    assert_eq!(outcome.entries.len(), 1);
}

// ---------------------------------------------------------------------------
// 采集 → 渲染 全流程
// ---------------------------------------------------------------------------

#[test]
fn pipeline_collect_then_render() {
    static ARCH: Logo = Logo {
        id: "arch",
        art: "  /\\  \n /  \\ \n",
    };

    let outcome = dispatcher().run(&["alpha", "delta"], &context());
    assert!(outcome.failures.is_empty());

    let report = Report::new(Some(&ARCH), &outcome.entries, &[]);
    let mut buffer = Vec::new();
    RecordingRenderer.render(&report, &mut buffer).unwrap();

    let text = String::from_utf8(buffer).unwrap();
    assert_eq!(text, "LOGO:arch\nKey: Value\nKey: Value\n");
}

#[test]
fn no_logo_means_no_logo_block() {
    let outcome = dispatcher().run(&["alpha"], &context());
    let report = Report::new(None, &outcome.entries, &[]);

    let mut buffer = Vec::new();
    RecordingRenderer.render(&report, &mut buffer).unwrap();

    assert_eq!(String::from_utf8(buffer).unwrap(), "Key: Value\n");
}

#[test]
fn render_errors_reach_the_caller() {
    /// 每次写入都失败的输出流，模拟「管道被下游关掉」。
    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("模仿管道断开"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let outcome = dispatcher().run(&["alpha"], &context());
    let report = Report::new(None, &outcome.entries, &[]);

    let error = RecordingRenderer
        .render(&report, &mut FailingWriter)
        .unwrap_err();

    // 渲染器不许自己吞掉写失败——exit code 得由 main 决定。
    assert!(matches!(error, RenderError::Write(_)));
}
