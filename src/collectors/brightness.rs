//! Brightness：背光亮度。
//!
//! 数据源 `/sys/class/backlight/`：一个子目录一个背光设备，值是
//! `brightness` / `max_brightness` 的百分比。可以有多个（内置屏 + 另一块面板），
//! 所以这个模块一行一个设备，不是把所有设备合成一行。
//!
//! 为什么不用 X11 / Wayland 的亮度接口：那要连 D-Bus 或跑 `xrandr` / `brightnessctl`，
//! 而 sysfs 是内核给的只读文件——TTY 里、没有会话、没有 D-Bus 时一样读得到，
//! 也不用管用户跑的是哪个桌面。
//!
//! 括号里放 sysfs 设备名（`intel_backlight`、`amdgpu_bl1`），不是从 EDID 里
//! 抠出来的显示器型号：fastfetch 会去读 EDID 拿型号名，但它拿不到时会退回
//! 设备名，而「为了一个显示名去解析 128 字节 EDID」不值得这一版的复杂度。
//!
//! `brightness` 与 `max_brightness` 缺一个就跳过这个设备：只有一个数时
//! 「400000 是亮还是暗」无从谈起，硬报一个百分比就是编。
//! 台式机没有背光 → 目录不存在或为空 → 无数据。

use std::io::ErrorKind;

use crate::collectors::{read, units};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 背光设备所在目录。和 power_supply 一样，子目录是符号链接，
/// 不能拿 `DirEntry::file_type()` 去筛。
const DIR: &str = "/sys/class/backlight";

/// 背光。
pub struct Brightness;

impl Collector for Brightness {
    fn name(&self) -> &'static str {
        "brightness"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let entries = match std::fs::read_dir(DIR) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(CollectError::caused_by(format!("读取 {DIR} 失败"), source)),
        };

        let mut devices = Vec::new();
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            devices.push(name);
        }
        // 多设备时输出顺序不能跟着 readdir 的哈希序变。
        devices.sort();

        let mut infos = Vec::new();
        for name in devices {
            if let Some(info) = self.info(&name)? {
                infos.push(info);
            }
        }

        Ok(infos)
    }
}

impl Brightness {
    /// 读一个背光设备，拼出一条信息。读不全就返回 `None`（跳过它，不是失败）。
    fn info(&self, name: &str) -> Result<Option<Info>, CollectError> {
        let dir = format!("{DIR}/{name}");
        let brightness: Option<u64> =
            read::text(&format!("{dir}/brightness"))?.and_then(|value| value.parse().ok());
        let max: Option<u64> =
            read::text(&format!("{dir}/max_brightness"))?.and_then(|value| value.parse().ok());

        let (Some(brightness), Some(max)) = (brightness, max) else {
            return Ok(None);
        };
        // max 为 0 的设备（驱动还没初始化完）没法算比例，也跳过。
        if max == 0 {
            return Ok(None);
        }

        let percent = units::percent(brightness, max);
        Ok(Some(
            Info::new(
                self.name(),
                format!("Brightness ({name})"),
                format!("{percent}%"),
            )
            .with_variable("device", name)
            .with_variable("brightness", brightness.to_string())
            .with_variable("max_brightness", max.to_string())
            .with_variable("percent", percent.to_string()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_this_machines_backlights_if_there_are_any() {
        let entries = Brightness.collect(&Context::for_tests()).unwrap();

        // 台式机与容器里没有背光，空表也是正确答案。
        for info in &entries {
            assert_eq!(info.module, "brightness");
            assert!(info.key.starts_with("Brightness ("));
            assert!(info.value.ends_with('%'), "值是百分比：{}", info.value);
            assert!(info.variable("max_brightness").is_some());
        }
    }

    #[test]
    fn the_percent_matches_the_two_files_in_sysfs() {
        // 拿模块的输出和直接读 sysfs 算出来的百分比对一遍。
        let Ok(entries) = std::fs::read_dir(DIR) else {
            return; // 没有背光目录，这条测试没事可做
        };

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let read_number = |file: &str| -> Option<u64> {
                std::fs::read_to_string(format!("{DIR}/{name}/{file}"))
                    .ok()?
                    .trim()
                    .parse()
                    .ok()
            };

            let expected = match (read_number("brightness"), read_number("max_brightness")) {
                (Some(brightness), Some(max)) if max > 0 => {
                    Some(format!("{}%", units::percent(brightness, max)))
                }
                _ => None,
            };

            let produced = Brightness
                .collect(&Context::for_tests())
                .unwrap()
                .into_iter()
                .find(|info| info.key == format!("Brightness ({name})"))
                .map(|info| info.value);

            assert_eq!(produced, expected, "设备 {name} 的输出要和 sysfs 对得上");
        }
    }

    #[test]
    fn a_device_without_a_maximum_is_skipped() {
        // 目录不存在时 `info` 该给出 None（跳过），而不是报错——这条路径
        // 也就是真机上「设备目录里少文件」的样子。
        assert!(Brightness.info("vitals-这个设备不存在").unwrap().is_none());
    }
}
