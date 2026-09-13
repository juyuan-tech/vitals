//! Keyboard：键盘设备。
//!
//! 名字来自 `/proc/bus/input/devices`（设备自己报的字符串），分类见 [`crate::collectors::input`]。
//! 靠**按键位图**而不是 `Handlers` 里的 `kbd` 判键盘：本机的电源键也带 `kbd`，
//! 拿它当键盘会多报一行（fastfetch 没报它，这一点我们与它一致）。
//!
//! 值就是设备名。多块键盘才编号（`Keyboard 1:`、`Keyboard 2:`）——本机有两块
//! （笔记本内置的 AT 键盘与无线接收器里那份），fastfetch 也是这么印的。
//! 只有一块时不写序号，免得 `Keyboard 1` 这种「只有一号」的怪样子。

use crate::collectors::input;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// Keyboard。
pub struct Keyboard;

impl Collector for Keyboard {
    fn name(&self) -> &'static str {
        "keyboard"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let names: Vec<String> = input::devices()?
            .into_iter()
            .filter(input::is_keyboard)
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

/// 键：多块键盘才带序号。
fn key(index: usize, total: usize) -> String {
    if total > 1 {
        format!("Keyboard {}", index + 1)
    } else {
        "Keyboard".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_keyboards_only_when_there_are_several() {
        assert_eq!(key(0, 1), "Keyboard");
        assert_eq!(key(0, 2), "Keyboard 1");
        assert_eq!(key(1, 2), "Keyboard 2");
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Keyboard.collect(&Context::for_tests()).unwrap();

        for (index, info) in entries.iter().enumerate() {
            assert_eq!(info.module, "keyboard");
            assert!(!info.value.is_empty());
            if entries.len() > 1 {
                assert_eq!(info.key, format!("Keyboard {}", index + 1));
            }
        }

        // 本机有两块键盘（AT 键盘 + 无线接收器），电源键不该混进来。
        assert!(
            !entries.iter().any(|info| info.value.contains("Power Button")),
            "电源键不是键盘：{entries:?}"
        );
    }
}
