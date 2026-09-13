//! TerminalSize：终端尺寸（列 × 行）。
//!
//! 数据来自 `tcgetwinsize(stdout)`——和文本渲染器量宽度用的是同一个调用
//! （`tty` 模块），所以「渲染器以为有多少列」和「这里报多少列」不会打架。
//!
//! 拿不到（输出被重定向、或者根本不在终端里）就返回无数据：那种情况下
//! 「终端多大」没有确定答案，报一个猜的数字不如不报。

use crate::collectors::tty;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 终端尺寸。
pub struct TerminalSize;

impl Collector for TerminalSize {
    fn name(&self) -> &'static str {
        "terminal-size"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some((columns, rows)) = tty::size() else {
            return Ok(Vec::new());
        };

        Ok(vec![
            Info::new(self.name(), "Terminal Size", format!("{columns}x{rows}"))
                .with_variable("columns", columns.to_string())
                .with_variable("rows", rows.to_string()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_only_when_stdout_is_a_terminal() {
        let entries = TerminalSize.collect(&Context::for_tests()).unwrap();

        // `cargo test` 把 stdout 接到管道上，所以这里通常是无数据；
        // 在终端里直接跑测试则会有。两种都对。
        match (entries.first(), tty::size()) {
            (None, None) => {}
            (Some(info), Some((columns, rows))) => {
                assert_eq!(info.key, "Terminal Size");
                assert_eq!(info.value, format!("{columns}x{rows}"));
            }
            (entries, size) => panic!("采集结果与 tty::size() 不一致：{entries:?} / {size:?}"),
        }
    }
}
