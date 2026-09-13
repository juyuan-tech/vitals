//! 阶段 4 验收：把十个采集器在**真实系统**上跑一遍。
//!
//! 与 `pipeline.rs` 相反，这里刻意碰真系统。代价是数值不可断言
//! （随机器、时间在变），所以只断言结构与契约：
//!
//! 1. 注册表与配置认识的模块一一对应；
//! 2. 一个模块都不报「真失败」；
//! 3. 由内核直接供数的模块一定有数据；
//! 4. 每条数据的形状合法，且顺序跟着计划走。
//!
//! 容器里 DMI、rustup、`HOME` 都可能没有，那些模块允许是空的——
//! 这正是「无数据不是错误」那条语义的实战检验。

use std::time::Duration;

use vitals_rs::config::ModuleType;
use vitals_rs::{COLLECTORS, Context, Dispatcher, Platform};

/// 任何 Linux 都拿得到的模块：数据直接来自内核。
const ALWAYS_AVAILABLE: [&str; 5] = ["kernel", "uptime", "cpu", "memory", "disk"];

fn context() -> Context {
    Context::new(Platform::default(), Duration::from_secs(5))
}

/// 注册表里的模块名，保持注册顺序。
fn registered() -> Vec<&'static str> {
    COLLECTORS
        .iter()
        .map(|collector| collector.name())
        .collect()
}

#[test]
fn the_registry_covers_exactly_the_modules_the_config_knows() {
    // 这条守两处漂移：加了 ModuleType 却忘了写采集器；
    // 或者写了采集器却忘了让配置认识它（那样配置里永远配不出这个模块）。
    let mut from_registry = registered();
    from_registry.sort_unstable();

    let mut from_config: Vec<&str> = ModuleType::ALL.iter().map(|module| module.name()).collect();
    from_config.sort_unstable();

    assert_eq!(from_registry, from_config);
    assert_eq!(from_config.len(), 10, "v0.1 就是这十个");
}

#[test]
fn no_module_reports_a_real_failure_on_this_machine() {
    let plan = registered();
    let outcome = Dispatcher::new(COLLECTORS).run(&plan, &context());

    let failures: Vec<String> = outcome
        .failures
        .iter()
        .map(|failure| format!("{}: {}", failure.module, failure.error))
        .collect();

    assert!(failures.is_empty(), "这些模块报了真失败：{failures:#?}");
}

#[test]
fn the_kernel_backed_modules_always_have_data() {
    let outcome = Dispatcher::new(COLLECTORS).run(&ALWAYS_AVAILABLE, &context());
    let produced: Vec<&str> = outcome.entries.iter().map(|info| info.module).collect();

    for module in ALWAYS_AVAILABLE {
        assert!(
            produced.contains(&module),
            "{module} 在任何 Linux 上都该有数据；实际只有：{produced:?}"
        );
    }
}

#[test]
fn every_entry_has_a_non_empty_shape() {
    let plan = registered();
    let outcome = Dispatcher::new(COLLECTORS).run(&plan, &context());

    assert!(!outcome.entries.is_empty());
    for info in &outcome.entries {
        assert!(!info.key.is_empty(), "{} 的 key 是空的", info.module);
        assert!(!info.value.is_empty(), "{} 的值是空的", info.module);
        assert!(plan.contains(&info.module), "{} 不在计划里", info.module);
        // 采集器不许把换行带进值里——渲染器要按行排版。
        assert!(!info.value.contains('\n'), "{} 的值里有换行", info.module);
    }
}

#[test]
fn the_display_order_follows_the_plan_not_the_registry() {
    // 注册表的顺序是 os, host, kernel...；这里故意反着点，输出必须跟着计划走。
    let plan = ["rust", "os", "kernel"];
    let outcome = Dispatcher::new(COLLECTORS).run(&plan, &context());
    let produced: Vec<&str> = outcome.entries.iter().map(|info| info.module).collect();

    // rust 没装 rustup 时会是空的，所以按「实际有数据的那些」比对顺序。
    let expected: Vec<&str> = plan
        .iter()
        .copied()
        .filter(|module| produced.contains(module))
        .collect();

    assert_eq!(produced, expected);
}

#[test]
fn the_os_module_agrees_with_the_platform_context() {
    // 阶段 5 会用 `Platform` 里的 id 去挑 Logo，而 OS 模块显示发行版名。
    // 两者必须来自同一份数据，否则会出现「显示 CachyOS、Logo 却是 Arch」。
    use vitals_rs::collectors::os_release;

    let release = os_release::read().unwrap();
    if let Some(release) = release {
        let outcome = Dispatcher::new(COLLECTORS).run(&["os"], &context());
        let info = &outcome.entries[0];

        assert_eq!(info.value, release.display_name().unwrap());
        assert_eq!(info.variable("id"), release.id.as_deref());
    }
}
