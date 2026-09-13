//! Camera：摄像头（V4L2 设备）。
//!
//! 名字来自 `/sys/class/video4linux/video*/name`，一个 V4L2 设备一个节点。
//!
//! 两个从真数据里看出来的坑：
//!
//! 1. **必须按名字去重**。本机 `video0..video3` 四个节点只有两个名字：
//!    `HP Wide Vision 5MP Camera: HP W` 与 `HP Wide Vision 5MP Camera: HP I`。
//!    内核把「哪种模式」写进了名字后缀，同一块摄像头会开出多个节点，不去重就会报 4 行。
//! 2. **块设备那种「是不是物理设备」的判据用不上**：这里列出来的就都是摄像头的节点，
//!    不需要额外过滤。
//!
//! 报不出分辨率与像素格式（fastfetch 值里 `- sRGB (2592x1944 px)` 那一段）：
//! 那要 V4L2 的 ioctl（`VIDIOC_ENUM_FMT` / `VIDIOC_G_FMT`），属 unsafe 的领域。
//! 所以我们的值就是内核给的名字本身——**前面那截与 fastfetch 逐字相同**，
//! 后面的格式信息我们没有，也不编一个。
//!
//! 没有摄像头 → 无数据。

use crate::collectors::read;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// V4L2 设备目录。
const VIDEO4LINUX: &str = "/sys/class/video4linux";

/// Camera。
pub struct Camera;

impl Collector for Camera {
    fn name(&self) -> &'static str {
        "camera"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let names = names()?;
        let total = names.len();

        Ok(names
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                Info::new(self.name(), key(index, total), name)
            })
            .collect())
    }
}

/// 键：`Camera 1`、`Camera 2`… 只有一块摄像头时不写序号。
fn key(index: usize, total: usize) -> String {
    if total > 1 {
        format!("Camera {}", index + 1)
    } else {
        "Camera".to_owned()
    }
}

/// 去重后的摄像头名字，按节点名排序（`video0` 在 `video2` 前面，顺序稳定）。
fn names() -> Result<Vec<String>, CollectError> {
    let Ok(entries) = std::fs::read_dir(VIDEO4LINUX) else {
        return Ok(Vec::new());
    };

    let mut nodes: Vec<String> = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("video"))
        })
        .map(|entry| entry.path().to_string_lossy().into_owned())
        .collect();
    nodes.sort();

    let mut names: Vec<String> = Vec::new();
    for node in nodes {
        let Some(name) = read::text(&format!("{node}/name"))? else {
            continue;
        };
        let name = name.trim().to_owned();

        // 名字为空的节点（驱动还没初始化完）不算一块摄像头。
        if name.is_empty() || names.contains(&name) {
            continue;
        }
        names.push(name);
    }

    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_the_cameras_only_when_there_are_several() {
        assert_eq!(key(0, 1), "Camera");
        assert_eq!(key(0, 2), "Camera 1");
        assert_eq!(key(1, 2), "Camera 2");
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Camera.collect(&Context::for_tests()).unwrap();

        // 本机有摄像头；没有摄像头的机器两种结局都通过。
        for (index, info) in entries.iter().enumerate() {
            assert_eq!(info.module, "camera");
            assert!(info.key.starts_with("Camera"), "实际是 {}", info.key);
            if entries.len() > 1 {
                assert_eq!(info.key, format!("Camera {}", index + 1));
            }
            assert!(!info.value.is_empty());
        }

        // 去重必须生效：本机 4 个 video 节点只有 2 个名字，报出来的不能超过节点数，
        // 更不能出现重复的名字。
        let mut seen: Vec<&str> = entries.iter().map(|info| info.value.as_str()).collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), before, "名字不该重复：{entries:?}");
    }
}
