//! 阶段 6 验收：JSON 输出。
//!
//! 形状是契约（`PLAN.md` §6.3），所以这里逐条钉住：键名、schema 版本、
//! 失败与条目分开、空的时候是空数组。契约松动的后果是别人的脚本先坏。

use serde_json::Value;

use vitals_rs::Info;
use vitals_rs::core::collector::CollectError;
use vitals_rs::core::dispatch::Failure;
use vitals_rs::core::render::{Renderer, Report};
use vitals_rs::render::json::{JsonRenderer, SCHEMA_VERSION};

fn bytes(entries: &[Info], failures: &[Failure]) -> Vec<u8> {
    let mut buffer = Vec::new();
    JsonRenderer
        .render(&Report::new(None, entries, failures), &mut buffer)
        .expect("写进内存不会失败");

    buffer
}

/// 渲染，再解析回来——顺带证明「输出确实是合法 JSON」。
fn render(entries: &[Info], failures: &[Failure]) -> Value {
    serde_json::from_slice(&bytes(entries, failures)).expect("渲染出来的必须是合法 JSON")
}

#[test]
fn the_document_carries_a_schema_version() {
    assert_eq!(render(&[], &[])["schema_version"], SCHEMA_VERSION);
}

#[test]
fn an_entry_has_exactly_type_key_and_value() {
    let document = render(&[Info::new("os", "OS", "Arch Linux")], &[]);

    // 用整体相等来断言：多一个字段就会失败，形状不会悄悄漂移。
    assert_eq!(
        document["entries"][0],
        serde_json::json!({ "type": "os", "key": "OS", "value": "Arch Linux" })
    );
}

#[test]
fn the_order_follows_the_entries() {
    let document = render(
        &[
            Info::new("os", "OS", "Arch Linux"),
            Info::new("kernel", "Kernel", "7.2.4"),
        ],
        &[],
    );

    assert_eq!(document["entries"][0]["type"], "os");
    assert_eq!(document["entries"][1]["type"], "kernel");
}

#[test]
fn empty_runs_are_empty_arrays_not_null() {
    let document = render(&[], &[]);

    assert_eq!(document["entries"], serde_json::json!([]));
    assert_eq!(document["failures"], serde_json::json!([]));
    // 脚本会直接遍历它，`jq '.entries[]'` 不该炸。
    assert!(document["entries"].as_array().unwrap().is_empty());
}

#[test]
fn failures_are_kept_out_of_the_entries() {
    let failures = [Failure {
        module: "disk".to_owned(),
        error: CollectError::new("statvfs 失败"),
    }];
    let document = render(&[Info::new("os", "OS", "Arch Linux")], &failures);

    assert_eq!(
        document["entries"].as_array().unwrap().len(),
        1,
        "失败的模块不该混进条目里——脚本会把一条错误当成一条信息读"
    );
    assert_eq!(document["failures"][0]["type"], "disk");
    assert_eq!(document["failures"][0]["error"], "statvfs 失败");
}

#[test]
fn values_survive_the_escape_rules() {
    // 手写 JSON 拼接最容易在这些字符上出事，而错了不会报错、只会静默变形。
    let value = "引号\" 反斜杠\\ 换行\n 制表\t 中文 🎉";
    let document = render(&[Info::new("os", "OS", value)], &[]);

    assert_eq!(document["entries"][0]["value"], value);
}

#[test]
fn the_output_is_pretty_printed_and_newline_terminated() {
    let text = String::from_utf8(bytes(&[Info::new("os", "OS", "Arch")], &[])).unwrap();

    assert!(text.contains("\n  "), "带缩进，人也要能扫一眼：{text}");
    assert!(text.ends_with("}\n"), "末尾要有换行：{text:?}");
}

#[test]
fn json_output_never_carries_escape_codes() {
    let text = String::from_utf8(bytes(&[Info::new("os", "OS", "Arch")], &[])).unwrap();

    assert!(!text.contains('\u{1b}'));
}
