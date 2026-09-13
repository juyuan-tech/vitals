//! Display：接了哪些显示器、各自多大、多少赫兹。
//!
//! 数据源是 DRM 的 sysfs：`/sys/class/drm/card<N>-<连接器>/`，一个连接器一个目录。
//!
//! - `status` 为 `connected` 才算接了显示器；
//! - `modes` 的第一行是内核按 EDID 排出来的**首选模式**。sysfs 里没有「当前模式」
//!   这个字段——要拿它得打开 DRM 设备、走 ioctl 与权限，代价与收益不成比例。
//!   首选模式在绝大多数机器上就是当前模式，注释里说清楚，不装作知道得更多；
//! - `edid` 是显示器的能力块，头一个 detailed timing descriptor 里有像素时钟与
//!   行/场消隐，能算出这个模式跑在多少赫兹。fastfetch 走 libdrm 拿这个数，
//!   我们直接算，而且**只在算出来的分辨率与首选模式一致时才敢报**，
//!   免得拿 A 模式的节奏去标 B 模式的分辨率。
//!
//! 键形如 `Display (eDP-1)`，值形如 `2880x1800 @ 120Hz (Built-in)`。
//! 分辨率拿不到就只报连接器，刷新率拿不到就不写 `@`——宁可少一个数，不猜。

use std::path::PathBuf;

use crate::collectors::read;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// DRM 连接器的 sysfs 目录。
const DRM: &str = "/sys/class/drm";

/// 显示器。
pub struct Display;

impl Collector for Display {
    fn name(&self) -> &'static str {
        "display"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let mut entries = Vec::new();

        for connector in connectors() {
            if let Some(info) = describe(&connector, self.name())? {
                entries.push(info);
            }
        }

        Ok(entries)
    }
}

/// 一个 DRM 连接器。
struct Connector {
    /// 连接器名，如 `eDP-1`。
    name: String,
    /// `/sys/class/drm/card1-eDP-1`。
    dir: PathBuf,
}

/// 列出**接了显示器**的连接器。
///
/// 目录名形如 `card1-eDP-1`：`card1` 只是哪张显卡，`eDP-1` 才是连接器。
/// `card1`（没有连接器后缀）、`renderD128`、`version` 这些都不是，跳过。
/// 读不到目录就是无数据。
fn connectors() -> Vec<Connector> {
    let Ok(entries) = std::fs::read_dir(DRM) else {
        return Vec::new();
    };

    let mut connectors: Vec<Connector> = entries
        .flatten()
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let dir_name = file_name.to_str()?;
            let (card, name) = dir_name.split_once('-')?;

            card.starts_with("card").then(|| Connector {
                name: name.to_owned(),
                dir: entry.path(),
            })
        })
        .collect();

    // 顺序稳定：同一台机器每次输出一致。
    connectors.sort_by(|left, right| left.name.cmp(&right.name));

    connectors
}

/// 描述一个连接器；没接显示器就是 `None`。
fn describe(connector: &Connector, module: &'static str) -> Result<Option<Info>, CollectError> {
    let status = read::text(&path_of(connector, "status"))?;
    if status.as_deref() != Some("connected") {
        return Ok(None);
    }

    let mode = read::text(&path_of(connector, "modes"))?
        .as_deref()
        .and_then(first_line)
        .and_then(parse_mode);

    // 刷新率只在 EDID 的首选时序与 `modes` 第一行对得上时才用。
    let refresh = match mode {
        Some((width, height)) => read::bytes(&path_of(connector, "edid"))?
            .as_deref()
            .and_then(|edid| refresh_of(edid, width, height)),
        None => None,
    };

    let mut value = match mode {
        Some((width, height)) => format!("{width}x{height}"),
        // 极少数连接器有状态却没报模式，这时至少让人看见它接着东西。
        None => "Connected".to_owned(),
    };
    if let Some(refresh) = refresh {
        value.push_str(&format!(" @ {refresh}Hz"));
    }
    value.push_str(if is_builtin(&connector.name) {
        " (Built-in)"
    } else {
        " (External)"
    });

    let mut info = Info::new(module, format!("Display ({})", connector.name), value)
        .with_variable("connector", connector.name.clone());
    if let Some((width, height)) = mode {
        info = info
            .with_variable("width", width.to_string())
            .with_variable("height", height.to_string());
    }
    if let Some(refresh) = refresh {
        info = info.with_variable("refresh", refresh.to_string());
    }

    Ok(Some(info))
}

/// 连接器目录下的一个文件路径。
fn path_of(connector: &Connector, file: &str) -> String {
    connector.dir.join(file).to_string_lossy().into_owned()
}

/// 内建屏？eDP、LVDS、DSI 都是笔记本里直接焊在主板上的那条路。
fn is_builtin(connector: &str) -> bool {
    ["eDP", "LVDS", "DSI"]
        .iter()
        .any(|prefix| connector.starts_with(prefix))
}

/// 取第一行非空内容。
fn first_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|line| !line.is_empty())
}

/// 解析 `2880x1800`。
fn parse_mode(mode: &str) -> Option<(u32, u32)> {
    let (width, height) = mode.split_once('x')?;

    Some((width.parse().ok()?, height.parse().ok()?))
}

/// 从 EDID 里算出这个分辨率的刷新率。
///
/// 只认**头一个** detailed timing descriptor（偏移 54，长 18 字节）：它描述的就是
/// 显示器的首选时序。像素时钟字段为 0 表示这一格是空的。
///
///   - 偏移 0..2：像素时钟，单位 10 kHz，小端
///   - 偏移 2/3：水平有效像素、水平消隐的低 8 位
///   - 偏移 4：高 4 位是水平有效像素的高 4 位，低 4 位是水平消隐的高 4 位
///   - 偏移 5/6、7：垂直方向同上
///
/// 刷新率 = 像素时钟 / (行总数 × 场总数)。算出来的分辨率与 `modes` 里的对不上就
/// 返回 `None`——那说明这个 descriptor 描述的不是当前这个模式。
fn refresh_of(edid: &[u8], width: u32, height: u32) -> Option<u32> {
    /// detailed timing descriptor 在 EDID 里的偏移与长度。
    const DTD_OFFSET: usize = 54;
    const DTD_LEN: usize = 18;

    let dtd = edid.get(DTD_OFFSET..DTD_OFFSET + DTD_LEN)?;

    let pixel_clock = u32::from(u16::from_le_bytes([dtd[0], dtd[1]])) * 10_000;
    if pixel_clock == 0 {
        return None;
    }

    let active_width = u32::from(dtd[2]) | (u32::from(dtd[4] & 0xf0) << 4);
    let blank_width = u32::from(dtd[3]) | (u32::from(dtd[4] & 0x0f) << 8);
    let active_height = u32::from(dtd[5]) | (u32::from(dtd[7] & 0xf0) << 4);
    let blank_height = u32::from(dtd[6]) | (u32::from(dtd[7] & 0x0f) << 8);

    if (active_width, active_height) != (width, height) {
        return None;
    }

    let pixels = u32::checked_mul(active_width + blank_width, active_height + blank_height)?;
    if pixels == 0 {
        return None;
    }

    // 四舍五入到整数赫兹：EDID 里的时钟是 10 kHz 量化的，本来就不是精确值。
    let refresh = (f64::from(pixel_clock) / f64::from(pixels)).round();
    (refresh > 0.0 && refresh < 1000.0).then_some(refresh as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_mode_line() {
        assert_eq!(parse_mode("2880x1800"), Some((2880, 1800)));
        assert_eq!(parse_mode("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_mode("2880"), None);
        assert_eq!(parse_mode("2880x"), None);
        assert_eq!(parse_mode(""), None);
    }

    #[test]
    fn takes_the_first_non_empty_line() {
        assert_eq!(first_line("2880x1800\n1920x1080\n"), Some("2880x1800"));
        assert_eq!(first_line("\n\n1920x1080\n"), Some("1920x1080"));
        assert_eq!(first_line(""), None);
    }

    #[test]
    fn built_in_connectors_are_recognised() {
        assert!(is_builtin("eDP-1"));
        assert!(is_builtin("LVDS-1"));
        assert!(is_builtin("DSI-1"));
        assert!(!is_builtin("HDMI-A-1"));
        assert!(!is_builtin("DP-1"));
    }

    /// 造一份只有头一个 detailed timing descriptor 有内容的 EDID。
    ///
    /// 数取真实 eDP 面板会用的值：2880x1800、行消隐 48、场消隐 30 → 行总数 2928、
    /// 场总数 1830；要跑 120 Hz 得 120 × 2928 × 1830 ≈ 643 MHz 像素时钟。
    /// 这个字段的单位是 10 kHz、位宽 16，所以**上限 655.35 MHz**——144 Hz 以上的
    /// 高分屏会顶到天花板，那时算出来的数会偏小，这是 EDID 格式自己的限制。
    fn edid_with(pixel_clock_10khz: u16, width: u32, height: u32) -> Vec<u8> {
        let (blank_width, blank_height) = (48, 30);
        let mut edid = vec![0u8; 128];

        let dtd = &mut edid[54..72];
        dtd[0..2].copy_from_slice(&pixel_clock_10khz.to_le_bytes());
        dtd[2] = (width & 0xff) as u8;
        dtd[3] = (blank_width & 0xff) as u8;
        dtd[4] = (((width >> 8) as u8 & 0x0f) << 4) | ((blank_width >> 8) as u8 & 0x0f);
        dtd[5] = (height & 0xff) as u8;
        dtd[6] = (blank_height & 0xff) as u8;
        dtd[7] = (((height >> 8) as u8 & 0x0f) << 4) | ((blank_height >> 8) as u8 & 0x0f);

        edid
    }

    #[test]
    fn reads_the_refresh_rate_out_of_a_real_shaped_edid() {
        // 64_299 × 10 kHz / (2928 × 1830) ≈ 120.0 Hz。
        let edid = edid_with(64_299, 2880, 1800);

        assert_eq!(refresh_of(&edid, 2880, 1800), Some(120));
    }

    #[test]
    fn a_descriptor_for_another_mode_is_ignored() {
        let edid = edid_with(64_299, 2880, 1800);

        // 同一个 EDID 换个分辨率问，答不上来——宁可少一个数，不能标错。
        assert_eq!(refresh_of(&edid, 1920, 1080), None);
    }

    #[test]
    fn malformed_edids_are_no_data() {
        assert_eq!(refresh_of(&[], 2880, 1800), None, "太短");
        assert_eq!(
            refresh_of(&edid_with(0, 2880, 1800), 2880, 1800),
            None,
            "像素时钟为 0 表示这一格是空的"
        );
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Display.collect(&Context::for_tests()).unwrap();

        for info in &entries {
            assert_eq!(info.module, "display");
            assert!(info.key.starts_with("Display ("), "实际是 {}", info.key);
            assert!(!info.value.is_empty());
        }
    }
}
