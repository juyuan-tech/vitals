//! JSON 渲染：给脚本读的。
//!
//! 形状是**契约**（`PLAN.md` §6.3）：键名不随版本漂移，脚本才敢 `jq`。
//! 加字段不算破坏兼容——读者忽略不认识的键就行；只有不兼容的变化才动
//! [`SCHEMA_VERSION`]。
//!
//! 这一份输出不画 Logo、也不上色：它是给程序看的。
//! `Settings::resolve` 在 `--json` 时已经把这两样关掉了。

use std::io::Write;

use serde::Serialize;

use crate::core::info::Info;
use crate::core::render::{RenderError, Renderer, Report};

/// 输出 schema 的版本。
pub const SCHEMA_VERSION: u32 = 1;

/// 一份 JSON 输出。
///
/// 字段顺序就是输出顺序：`schema_version` 在最前，谁都能一眼看出这是哪一版。
#[derive(Debug, Serialize)]
struct Document<'a> {
    schema_version: u32,
    entries: Vec<Entry<'a>>,
    failures: Vec<Problem<'a>>,
}

/// 一条信息。
///
/// 这里刻意**不复用** `Info` 的 `Serialize`：JSON 的形状是渲染器的事，
/// 不是数据模型的事。分开之后，形状要改（比如加一个 `variables`）不必动核心接口，
/// 核心也不必为了输出而依赖 serde。
#[derive(Debug, Serialize)]
struct Entry<'a> {
    /// 模块名。`type` 是 JSON 里的键名，写成 `kind` 是为了避开 Rust 关键字。
    #[serde(rename = "type")]
    kind: &'a str,
    key: &'a str,
    value: &'a str,
}

/// 一个失败的模块。
#[derive(Debug, Serialize)]
struct Problem<'a> {
    /// 模块名。
    #[serde(rename = "type")]
    kind: &'a str,
    /// 失败原因，就是终端上会印的那一句。
    error: String,
}

impl<'a> From<&'a Info> for Entry<'a> {
    fn from(info: &'a Info) -> Self {
        Self {
            kind: info.module,
            key: &info.key,
            value: &info.value,
        }
    }
}

/// JSON 渲染器。
///
/// 没有状态，所以是个单元结构体。
#[derive(Debug, Clone, Copy)]
pub struct JsonRenderer;

impl Renderer for JsonRenderer {
    fn render(&self, report: &Report<'_>, out: &mut dyn Write) -> Result<(), RenderError> {
        let document = Document {
            schema_version: SCHEMA_VERSION,
            entries: report.entries.iter().map(Entry::from).collect(),
            failures: report
                .failures
                .iter()
                .map(|failure| Problem {
                    kind: &failure.module,
                    error: failure.error.to_string(),
                })
                .collect(),
        };

        // 带缩进：人也会扫一眼（`vitals --json | head`），而 `jq` 两种都吃。
        serde_json::to_writer_pretty(&mut *out, &document).map_err(to_io)?;
        writeln!(out)?;

        Ok(())
    }
}

/// 把 serde_json 的错误收敛成 `RenderError::Write`。
///
/// 这里不需要一个新的错误 variant：文档里全是字符串和整数，**串行化本身不会失败**，
/// 唯一的失败来源就是往 `out` 写字节时出错。为它加一个 variant 只会让调用方
/// 多写一个永远走不到的分支。
fn to_io(error: serde_json::Error) -> RenderError {
    RenderError::Write(std::io::Error::other(error))
}
