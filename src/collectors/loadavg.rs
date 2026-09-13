//! Load Average：1 / 5 / 15 分钟的平均负载。数据来自 `/proc/loadavg`。
//!
//! 只做 Linux：`/proc/loadavg` 是 Linux 专有的。别的平台有 `getloadavg(3)`，
//! 那是 libc 调用——本项目的硬约束是零 unsafe，所以这个模块在非 Linux 上
//! 安静地不出数据，而不是硬凑一个。
//!
//! 值的原样保留内核给的两位小数，不做四舍五入：这一行是给人看「现在忙不忙」的，
//! 而 0.60 和 0.64 在直觉上是一回事，在数字上不是。

use crate::collectors::read;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 数据源。
const PATH: &str = "/proc/loadavg";

/// 平均负载。
pub struct Loadavg;

impl Collector for Loadavg {
    fn name(&self) -> &'static str {
        "loadavg"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(text) = read::text(PATH)? else {
            return Ok(Vec::new());
        };
        let Some((one, five, fifteen)) = parse(&text) else {
            return Ok(Vec::new());
        };

        let value = format!("{one}, {five}, {fifteen}");
        Ok(vec![
            Info::new(self.name(), "Load Average", value)
                .with_variable("one", one)
                .with_variable("five", five)
                .with_variable("fifteen", fifteen),
        ])
    }
}

/// 取前三个字段。
///
/// 文件形如 `0.52 0.45 0.39 2/1986 265527`，后面还有「运行中/总任务数」和
/// 最后一个进程号——这里用不上，长度不够或者不是数字就算无数据。
fn parse(text: &str) -> Option<(String, String, String)> {
    let mut fields = text.split_whitespace().map(str::to_owned);
    let one = fields.next()?;
    let five = fields.next()?;
    let fifteen = fields.next()?;

    let plausible = [&one, &five, &fifteen]
        .iter()
        .all(|value| value.parse::<f64>().is_ok());

    plausible.then_some((one, five, fifteen))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_first_three_fields() {
        let (one, five, fifteen) = parse("0.52 0.45 0.39 2/1986 265527\n").unwrap();

        assert_eq!(
            (one.as_str(), five.as_str(), fifteen.as_str()),
            ("0.52", "0.45", "0.39")
        );
    }

    #[test]
    fn a_short_or_broken_file_is_no_data() {
        assert!(parse("0.52 0.45\n").is_none());
        assert!(parse("").is_none());
        assert!(parse("忙 0.45 0.39\n").is_none(), "非数字不算数");
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Loadavg.collect(&Context::for_tests()).unwrap();

        if let Some(info) = entries.first() {
            assert_eq!(info.key, "Load Average");
            assert_eq!(info.value.matches(',').count(), 2, "三个值两个逗号");
        }
    }
}
