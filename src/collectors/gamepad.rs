//! Gamepad：游戏手柄。
//!
//! 判据是 `/proc/bus/input/devices` 里 `H: Handlers=` 有没有 `js*`：内核给每个
//! 注册成 joystick 接口的设备开一个 `/dev/input/jsN`，手柄走的就是这条路
//! （`eventN` 不算——鼠标键盘也都开着 `eventN`）。
//!
//! 本机**没有手柄** → 无数据（fastfetch 在这台机器上也不输出这一行）。
//! 所以这个模块只有单元测试能覆盖判据，真机那一半在本机验证不了——按
//! 「宁可报告做不到，也不许猜着写」，这里只写清楚判据与它的依据，不编数据。
//!
//! 值就是设备名。多个手柄才编号（`Gamepad 1:`、`Gamepad 2:`）。

use crate::collectors::input;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// Gamepad。
pub struct Gamepad;

impl Collector for Gamepad {
    fn name(&self) -> &'static str {
        "gamepad"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let names: Vec<String> = input::devices()?
            .into_iter()
            .filter(input::is_gamepad)
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

/// 键：多个手柄才带序号。
fn key(index: usize, total: usize) -> String {
    if total > 1 {
        format!("Gamepad {}", index + 1)
    } else {
        "Gamepad".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_gamepads_only_when_there_are_several() {
        assert_eq!(key(0, 1), "Gamepad");
        assert_eq!(key(0, 2), "Gamepad 1");
        assert_eq!(key(1, 2), "Gamepad 2");
    }

    #[test]
    fn collects_nothing_when_there_is_no_gamepad() {
        // 本机没有手柄，所以这里必须是空——不是 `Err`，也不是一行空字符串。
        // （别的机器上手柄设备自己会带 `js0`，那时才该有数据。）
        let entries = Gamepad.collect(&Context::for_tests()).unwrap();

        for info in &entries {
            assert_eq!(info.module, "gamepad");
            assert!(!info.value.is_empty());
        }
    }
}
