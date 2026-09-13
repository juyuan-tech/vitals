//! Separator：一条横线。
//!
//! 它是**渲染原语**，不是采集到的数据：线该多长取决于别的信息行有多宽，
//! 那只有渲染器知道。所以这里只发一个空条目当标记——文本渲染器把它铺成横线，
//! JSON 渲染器直接跳过它（一串 `---` 对读 JSON 的程序没有意义）。

use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 分隔线。
pub struct Separator;

impl Collector for Separator {
    fn name(&self) -> &'static str {
        "separator"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        Ok(vec![Info::new(self.name(), "", "")])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_separator_is_an_empty_marker() {
        let entries = Separator.collect(&Context::for_tests()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].module, "separator");
        assert!(
            entries[0].key.is_empty() && entries[0].value.is_empty(),
            "内容交给渲染器决定，采集器只发标记"
        );
    }
}
