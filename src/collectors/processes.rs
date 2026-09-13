//! Processes：进程数与线程数。
//!
//! 两个数都**不需要打开任何进程的文件**：
//!
//! - **进程数**：数 `/proc` 下的纯数字目录（一次 `getdents`）。
//! - **线程数**：读 `/proc/loadavg` 第 4 个字段里的总任务数（一次 `read`）。
//!   内核自己维护这个计数器，本机实测它与「逐个打开 `/proc/<pid>/stat` 累加
//!   `num_threads`」**完全相等**（2008 = 2008），而后者是 434 次 open、6.9 ms，
//!   前者 0.3 ms 量级。冷启动预算 20 ms，不该有一个模块吃掉三分之一。
//!
//! 「总任务数」就是线程数：每个进程至少算一个线程，内核把两者一起计。
//!
//! 某个进程中途退出之类的情况在这里根本不会发生——我们不去逐个碰它们。

use crate::collectors::read;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 进程表。
const PROC: &str = "/proc";
/// 总任务数的来源。
const LOADAVG: &str = "/proc/loadavg";

/// 进程数。
pub struct Processes;

impl Collector for Processes {
    fn name(&self) -> &'static str {
        "processes"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let processes = count_processes();
        if processes == 0 {
            // 没有 /proc（非 Linux）、或者一个都数不到，都是无数据。
            return Ok(Vec::new());
        }

        let threads = read::text(LOADAVG)?.as_deref().and_then(parse_tasks);
        let value = match threads {
            Some(threads) => format!("{processes} ({threads} threads)"),
            // 读不到总任务数就只报进程数，不编一个 0。
            None => processes.to_string(),
        };

        let mut info = Info::new(self.name(), "Processes", value)
            .with_variable("processes", processes.to_string());
        if let Some(threads) = threads {
            info = info.with_variable("threads", threads.to_string());
        }

        Ok(vec![info])
    }
}

/// 数 `/proc` 下有多少个进程目录。
///
/// `/proc` 里还混着 `self`、`meminfo`、`net` 这些名字，只认纯数字。
/// 读不到目录就是 0。
fn count_processes() -> u64 {
    let Ok(entries) = std::fs::read_dir(PROC) else {
        return 0;
    };

    entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()))
        })
        .count() as u64
}

/// 从 `/proc/loadavg` 里取总任务数。
///
/// 文件形如 `0.52 0.45 0.39 2/1986 265527`：第 4 个字段是「可运行 / 总数」，
/// 要的是斜杠后面那个数。取不到就是 `None`。
fn parse_tasks(loadavg: &str) -> Option<u64> {
    let field = loadavg.split_whitespace().nth(3)?;

    field.split_once('/')?.1.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_task_count_from_a_real_line() {
        assert_eq!(parse_tasks("2.85 2.63 2.54 2/1986 265527\n"), Some(1986));
    }

    #[test]
    fn a_broken_line_is_no_data() {
        assert_eq!(parse_tasks(""), None);
        assert_eq!(parse_tasks("0.52 0.45 0.39\n"), None, "字段不够");
        assert_eq!(parse_tasks("0.52 0.45 0.39 忙 265527\n"), None);
        assert_eq!(parse_tasks("0.52 0.45 0.39 2/不确定 265527\n"), None);
    }

    #[test]
    fn counts_processes_on_this_machine() {
        // 任何 Linux 上都至少有 1 号进程，所以这里一定是正数。
        assert!(count_processes() > 0);
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Processes.collect(&Context::for_tests()).unwrap();

        if let Some(info) = entries.first() {
            assert_eq!(info.key, "Processes");
            assert!(
                info.value.ends_with(" threads)"),
                "值形如 `434 (2008 threads)`，实际是 {}",
                info.value
            );
        }
    }
}
