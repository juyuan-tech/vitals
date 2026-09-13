//! Memory：内存用量。数据来自 `/proc/meminfo`（解析在 `meminfo`）。

use crate::collectors::{meminfo, units};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 内存。
pub struct Memory;

impl Collector for Memory {
    fn name(&self) -> &'static str {
        "memory"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let mem = meminfo::Meminfo::read()?;
        if mem.total == 0 {
            // 文件在但连总量都读不出来，属于无数据。
            return Ok(Vec::new());
        }

        let used = mem.used();
        let total = mem.total;
        let percent = units::percent(used, total);
        let value = format!(
            "{} / {} ({percent}%)",
            units::bytes(used),
            units::bytes(total)
        );

        Ok(vec![
            Info::new(self.name(), "Memory", value)
                .with_variable("used_bytes", used.to_string())
                .with_variable("total_bytes", total.to_string())
                .with_variable("available_bytes", mem.available().to_string())
                .with_variable("percent", percent.to_string()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_on_this_machine() {
        let entries = Memory.collect(&Context::for_tests()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "Memory");
        assert!(
            entries[0].value.contains(" / "),
            "该是「已用 / 总量」的形状"
        );
        assert!(entries[0].value.ends_with("%)"));
    }
}
