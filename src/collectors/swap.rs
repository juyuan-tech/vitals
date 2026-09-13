//! Swap：交换空间用量。和 Memory 一样读 `/proc/meminfo`。
//!
//! 没开 swap 的机器（`SwapTotal = 0`）返回**无数据**，而不是 `0 B / 0 B (0%)`：
//! 后者看着像 swap 坏了，前者才是事实——这台机器没有交换空间。

use crate::collectors::{meminfo, units};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 交换空间。
pub struct Swap;

impl Collector for Swap {
    fn name(&self) -> &'static str {
        "swap"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let mem = meminfo::Meminfo::read()?;
        if mem.swap_total == 0 {
            return Ok(Vec::new());
        }

        let used = mem.swap_used();
        let total = mem.swap_total;
        let percent = units::percent(used, total);
        let value = format!(
            "{} / {} ({percent}%)",
            units::bytes(used),
            units::bytes(total)
        );

        Ok(vec![
            Info::new(self.name(), "Swap", value)
                .with_variable("used_bytes", used.to_string())
                .with_variable("total_bytes", total.to_string())
                .with_variable("percent", percent.to_string()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_on_this_machine_if_swap_is_on() {
        let entries = Swap.collect(&Context::for_tests()).unwrap();

        // 没开 swap 的机器（或容器）这里就是空——那也是正确答案。
        match entries.first() {
            None => {}
            Some(info) => {
                assert_eq!(info.key, "Swap");
                assert!(info.value.contains(" / "));
                assert!(info.value.ends_with("%)"));
            }
        }
    }
}
