//! Break：一个空行。
//!
//! 和 Separator 一样是渲染原语，但连特判都不需要：空键加空值，渲染器写一个
//! 换行就完事。存在的意义是把信息分组——fastfetch 默认视图里它排在 `Colors` 前面。

use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 空行。
pub struct Break;

impl Collector for Break {
    fn name(&self) -> &'static str {
        "break"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        Ok(vec![Info::new(self.name(), "", "")])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_break_is_an_empty_line() {
        let entries = Break.collect(&Context::for_tests()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].module, "break");
        assert!(entries[0].key.is_empty() && entries[0].value.is_empty());
    }
}
