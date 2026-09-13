//! Mouse：指针设备（鼠标、触摸板、轨迹球、触摸屏）。
//!
//! 名字与分类都来自 [`crate::collectors::input`]。判据是**指针那一带的按键**
//! （`BTN_LEFT`..`BTN_TASK`、`BTN_TOUCH`、`BTN_TOOL_*`），不是名字：本机触摸板
//! 名字里带 `Touchpad`，但那只是驱动给它起的名字，真正说明身份的是它带 `BTN_LEFT`。
//!
//! 两条从真数据里核出来的边界：
//!
//! - **无线接收器里那份键盘不算鼠标**：本机 `G3 Mouse Keyboard` 带 `BTN_0`（0x100）
//!   却不带 `BTN_LEFT`（0x110），所以它只报在 Keyboard 那边。fastfetch 也是这么分的
//!   （它的 Mouse 三条里没有它）。早先我把整个 `BTN_MISC` 都当指针判据，就错在这里。
//! - **键盘不算鼠标**：本机 AT 键盘的位图里 `BTN_LEFT` 那一位是 0，所以它不出现在这里。
//!
//! 真机与 fastfetch 逐字一致：
//!
//! ```text
//! Mouse 1: G3 Mouse
//! Mouse 2: ELAN07D0:00 04F3:321A Mouse
//! Mouse 3: ELAN07D0:00 04F3:321A Touchpad
//! ```
//!
//! 值就是设备名。多块指针设备才编号（`Mouse 1:`、`Mouse 2:`），只有一块时不写序号。

use crate::collectors::input;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// Mouse。
pub struct Mouse;

impl Collector for Mouse {
    fn name(&self) -> &'static str {
        "mouse"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let names: Vec<String> = input::devices()?
            .into_iter()
            .filter(input::is_pointer)
            .map(|device| device.name)
            .collect();

        let total = names.len();

        Ok(names
            .into_iter()
            .enumerate()
            .map(|(index, name)| Info::new(self.name(), key(index, total), name))
            .collect())
    }
}

/// 键：多块指针设备才带序号。
fn key(index: usize, total: usize) -> String {
    if total > 1 {
        format!("Mouse {}", index + 1)
    } else {
        "Mouse".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_mice_only_when_there_are_several() {
        assert_eq!(key(0, 1), "Mouse");
        assert_eq!(key(0, 3), "Mouse 1");
        assert_eq!(key(2, 3), "Mouse 3");
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Mouse.collect(&Context::for_tests()).unwrap();

        for (index, info) in entries.iter().enumerate() {
            assert_eq!(info.module, "mouse");
            assert!(!info.value.is_empty());
            if entries.len() > 1 {
                assert_eq!(info.key, format!("Mouse {}", index + 1));
            }
        }

        // 本机那块「键盘 + 鼠标」合体的无线接收器**不该**出现在这里：
        // 它带 BTN_0 却不带 BTN_LEFT。这条与 fastfetch 的分法一致。
        assert!(
            !entries.iter().any(|info| info.value == "G3 Mouse Keyboard"),
            "合体接收器不该同时算鼠标：{entries:?}"
        );
    }
}
