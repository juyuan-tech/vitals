//! CPU：型号与核心数。
//!
//! 只做型号和核心数，**不做占用率**——理由见 `PLAN.md` §5.3：
//! 采集类工具给的是「身份与容量」的快照，利用率那种每时每刻都在变的东西属于 htop/btop。

use crate::collectors::read;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 型号与核数的来源。
const CPUINFO: &str = "/proc/cpuinfo";
/// 逻辑处理器数用内核自己维护的这个文件更可靠。
const PRESENT: &str = "/sys/devices/system/cpu/present";

/// CPU。
pub struct Cpu;

/// 从 `/proc/cpuinfo` 里挑出来的东西。
#[derive(Debug, Default, PartialEq, Eq)]
struct Cpuinfo {
    /// 型号，例如 `AMD Ryzen 7 8845H w/ Radeon(TM) 780M Graphics`。
    model: Option<String>,
    /// 物理核数（`cpu cores`）。
    cores: Option<u64>,
    /// `processor` 行的条数，也就是逻辑处理器数。
    logical: u64,
}

/// 解析 `/proc/cpuinfo`。
///
/// 行格式是 `model name\t: AMD Ryzen 7 8845H ...`，按第一个冒号切开就够。
/// 这个文件是「一个逻辑处理器一段」的结构，所以同一个键会出现多次：
/// 型号取第一个（同一颗 CPU 上它们都一样），核数取最大值。
fn parse_cpuinfo(text: &str) -> Cpuinfo {
    let mut cpuinfo = Cpuinfo::default();

    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();

        match key.trim() {
            "processor" => cpuinfo.logical += 1,
            "model name" if cpuinfo.model.is_none() => cpuinfo.model = Some(value.to_owned()),
            "cpu cores" => {
                if let Ok(count) = value.parse::<u64>() {
                    cpuinfo.cores = Some(cpuinfo.cores.map_or(count, |current| current.max(count)));
                }
            }
            _ => {}
        }
    }

    cpuinfo
}

/// 解析 CPU 列表：`0-15`、`0`、`0-3,8-11`。
///
/// 这是内核印 CPU 集合的标准写法，`present` / `online` / `possible` 都用它。
fn parse_cpu_list(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    let mut total = 0u64;

    for part in text.split(',') {
        let part = part.trim();
        match part.split_once('-') {
            Some((start, end)) => {
                let start: u64 = start.trim().parse().ok()?;
                let end: u64 = end.trim().parse().ok()?;
                total += end.checked_sub(start)? + 1;
            }
            None => {
                part.parse::<u64>().ok()?;
                total += 1;
            }
        }
    }

    Some(total)
}

/// 拼显示值，例如 `AMD Ryzen 7 8845H (8C/16T)`。
///
/// 只拿得到一样就只显示那一样；两样都没有才算无数据。
fn describe(model: Option<&str>, cores: Option<u64>, threads: u64) -> Option<String> {
    let counts = if threads == 0 {
        cores.map(|cores| format!("{cores}C"))
    } else if let Some(cores) = cores {
        Some(format!("{cores}C/{threads}T"))
    } else {
        Some(threads.to_string())
    };

    match (model, counts) {
        (Some(model), Some(counts)) => Some(format!("{model} ({counts})")),
        (Some(model), None) => Some(model.to_owned()),
        (None, counts) => counts,
    }
}

impl Collector for Cpu {
    fn name(&self) -> &'static str {
        "cpu"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(text) = read::text(CPUINFO)? else {
            return Ok(Vec::new());
        };
        let cpuinfo = parse_cpuinfo(&text);

        // 逻辑处理器数优先用 `/sys` 的 present：内核自己维护的，
        // 比数 `/proc/cpuinfo` 的行更可靠。
        let threads = match read::text(PRESENT)? {
            Some(text) => parse_cpu_list(&text),
            None => None,
        }
        .unwrap_or(cpuinfo.logical);

        let Some(value) = describe(cpuinfo.model.as_deref(), cpuinfo.cores, threads) else {
            return Ok(Vec::new());
        };

        let mut info = Info::new(self.name(), "CPU", value);
        if let Some(model) = &cpuinfo.model {
            info = info.with_variable("model", model.clone());
        }
        if let Some(cores) = cpuinfo.cores {
            info = info.with_variable("cores", cores.to_string());
        }
        if threads > 0 {
            info = info.with_variable("threads", threads.to_string());
        }

        Ok(vec![info])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 实测本机 `/proc/cpuinfo` 的骨架（八个核，十六个逻辑处理器）。
    fn fixture(processors: usize) -> String {
        let mut text = String::new();
        for index in 0..processors {
            text.push_str(&format!(
                "processor\t: {index}\n\
                 model name\t: AMD Ryzen 7 8845H w/ Radeon(TM) 780M Graphics\n\
                 physical id\t: 0\n\
                 cpu cores\t: 8\n\
                 \n"
            ));
        }
        text
    }

    #[test]
    fn picks_the_model_and_counts_processors() {
        let cpuinfo = parse_cpuinfo(&fixture(16));

        assert_eq!(
            cpuinfo.model.as_deref(),
            Some("AMD Ryzen 7 8845H w/ Radeon(TM) 780M Graphics")
        );
        assert_eq!(cpuinfo.cores, Some(8));
        assert_eq!(cpuinfo.logical, 16, "十六个 processor 段");
    }

    #[test]
    fn a_file_without_a_model_is_still_usable() {
        let cpuinfo = parse_cpuinfo("processor\t: 0\nprocessor\t: 1\n");

        assert_eq!(cpuinfo.model, None);
        assert_eq!(cpuinfo.logical, 2);
    }

    #[test]
    fn cpu_lists_are_parsed_as_ranges() {
        assert_eq!(parse_cpu_list("0-15"), Some(16));
        assert_eq!(parse_cpu_list("0"), Some(1));
        assert_eq!(parse_cpu_list("0-3,8-11"), Some(8));
        assert_eq!(parse_cpu_list(" 0-1 \n"), Some(2), "首尾空白与换行不影响");
    }

    #[test]
    fn broken_cpu_lists_are_no_data() {
        assert_eq!(parse_cpu_list(""), None);
        assert_eq!(parse_cpu_list("   "), None);
        assert_eq!(parse_cpu_list("abc"), None);
        assert_eq!(parse_cpu_list("5-1"), None, "倒着写的区间不算数");
    }

    #[test]
    fn the_display_value_prefers_cores_over_threads() {
        let model = Some("AMD Ryzen 7 8845H");

        assert_eq!(
            describe(model, Some(8), 16).as_deref(),
            Some("AMD Ryzen 7 8845H (8C/16T)")
        );
        assert_eq!(
            describe(model, Some(8), 8).as_deref(),
            Some("AMD Ryzen 7 8845H (8C/8T)"),
            "没有超线程时也照实写"
        );
        assert_eq!(
            describe(model, None, 16).as_deref(),
            Some("AMD Ryzen 7 8845H (16)")
        );
        assert_eq!(
            describe(None, Some(8), 16).as_deref(),
            Some("8C/16T"),
            "没有型号也还有核数"
        );
        assert_eq!(describe(None, None, 0), None, "什么都没有才是无数据");
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Cpu.collect(&Context::for_tests()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "CPU");
        assert!(entries[0].variable("threads").is_some());
    }
}
