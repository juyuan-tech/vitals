//! Colors：一排 16 个色块。
//!
//! **采集器什么也不采**：这一块完全由渲染器画。做法与 `separator` 同一套——
//! 发一个空条目当标记，渲染器见到 `module == "colors"` 就铺两行色块。
//! 这么分是有理由的：颜色是**渲染器**的事（终端里还有没有颜色由 anstream 决定），
//! 采集器不该知道。JSON 那边也因此不用特殊照顾：空键空值的条目本来就被排除
//! （见 `render/json.rs` 的过滤条件）。
//!
//! 排法（与 fastfetch 实测一致）：8 格 × 2 行，每格三格宽；上排是标准 8 色
//! （ANSI `40`-`47`），下排是亮色（`100`-`107`），行末一个 reset。
//!
//! 一处**刻意偏差**：fastfetch 在第二排前面还加了一个 `\x1b[5m`（闪烁）。
//! 我们不跟——闪烁是用户会专门去关掉的东西，印在一屏系统信息里只是噪音。

use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 色块。
pub struct Colors;

impl Collector for Colors {
    fn name(&self) -> &'static str {
        "colors"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        // 键与值都是空的：这不是「键: 值」行，是给渲染器的一条标记。
        Ok(vec![Info::new(self.name(), "", "")])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_a_marker_without_content() {
        let entries = Colors.collect(&Context::for_tests()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].module, "colors");
        assert!(entries[0].key.is_empty(), "不是「键: 值」行");
        assert!(entries[0].value.is_empty(), "内容由渲染器画");
    }

    #[test]
    fn the_module_name_matches() {
        assert_eq!(Colors.name(), "colors");
    }
}
