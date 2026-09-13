//! Uptime：开机时长。数据来自 `/proc/uptime`。

use crate::collectors::{read, units};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 数据源。
const PATH: &str = "/proc/uptime";

/// 开机时长。
pub struct Uptime;

/// 解析 `/proc/uptime`。
///
/// 格式是两列浮点秒数：第一列是总时长，第二列是所有 CPU 的空闲累计。
/// 我们只要第一列。
fn parse_seconds(text: &str) -> Option<u64> {
    let first = text.split_whitespace().next()?;
    let seconds: f64 = first.parse().ok()?;

    // 负数在现实中不存在；就算出现，as 转换会饱和到 0，不会 panic。
    Some(seconds as u64)
}

impl Collector for Uptime {
    fn name(&self) -> &'static str {
        "uptime"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(text) = read::text(PATH)? else {
            return Ok(Vec::new());
        };

        // 文件在但读不出秒数，当作无数据：值本身没意义，报错反而噪音。
        let Some(seconds) = parse_seconds(&text) else {
            return Ok(Vec::new());
        };

        Ok(vec![
            Info::new(self.name(), "Uptime", units::duration(seconds))
                .with_variable("seconds", seconds.to_string()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_real_format() {
        // 实测这台机器：101546.17 452335.80
        assert_eq!(parse_seconds("101546.17 452335.80"), Some(101_546));
        assert_eq!(parse_seconds("0.00 0.00"), Some(0));
        assert_eq!(parse_seconds("  12.5  "), Some(12), "前后空白不影响");
    }

    #[test]
    fn garbage_is_no_data() {
        assert_eq!(parse_seconds(""), None);
        assert_eq!(parse_seconds("   "), None);
        assert_eq!(parse_seconds("不知道"), None);
    }

    #[test]
    fn collects_something_on_this_machine() {
        let entries = Uptime.collect(&Context::for_tests()).unwrap();

        // 容器里 /proc/uptime 也在，所以这里应当有值。
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "Uptime");
        assert!(
            entries[0].value.ends_with(['d', 'h', 'm', 's']),
            "时长该带单位：{}",
            entries[0].value
        );
    }
}
