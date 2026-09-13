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
    // 四舍五入到整数赫兹：EDID 里的时钟是 10 kHz 量化的，本来就不是精确值。
    let refresh = refresh_exact(edid, width, height)?.round();

    (refresh > 0.0 && refresh < 1000.0).then_some(refresh as u32)
}

/// 与 [`refresh_of`] 同一份计算，但**不四舍五入**。
///
/// 同一个数有两种用法：`Display` 印 `120 Hz`（人看的一行），`Monitor` 印 `120.001`
/// （fastfetch 的口径，真机比对过）。取数层给精确值，怎么舍由各自的渲染决定——
/// 所以这里不重复解析，只把最后的 `.round()` 留在上面那层。
pub(crate) fn refresh_exact(edid: &[u8], width: u32, height: u32) -> Option<f64> {
    #[allow(clippy::items_after_statements)]
    {
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

        let refresh = f64::from(pixel_clock) / f64::from(pixels);
        (refresh > 0.0 && refresh < 1000.0).then_some(refresh)
    }
}

// 这两个解析是 `Monitor` 的前置：`display.rs` 已经读了 EDID，`Monitor` 要的是同一份
// 数据（名字与物理尺寸）。**`monitor` 接上时把这两行 `#[allow(dead_code)]` 删掉**——
// 现在留着它只是因为那个模块还没落地，不是打算长期不用。
#[allow(dead_code)]
/// EDID 的**厂商字母 + 产品码**，例如 `SDC4197`。
///
/// 这不是那个 `00 00 00 FC` 的「显示器名描述符」——本机的 EDID 里根本没有那一段
/// （四个描述符槽位全是详细时序），而 fastfetch 照样印出了 `SDC4197`。对着它的输出
/// 反查真机 EDID 才知道它拼的是：
///
///   - 偏移 8..10：厂商 ID，大端，三个 5 位字母
///   - 偏移 10..12：产品码，小端
///
/// 两段任一为零就是没有。
pub(crate) fn name_of(edid: &[u8]) -> Option<String> {
    let manufacturer = u16::from_be_bytes([*edid.get(8)?, *edid.get(9)?]);
    let product = u16::from_le_bytes([*edid.get(10)?, *edid.get(11)?]);

    if manufacturer == 0 || product == 0 {
        return None;
    }

    // 三个字母，每 5 位一个，从高位开始（偏移 15、10、5 起，各取 5 位）。
    let letters: String = [10, 5, 0]
        .into_iter()
        .map(|shift| char::from_u32(u32::from((manufacturer >> shift) & 0x1f) + 0x40))
        .collect::<Option<String>>()?;

    Some(format!("{letters}{product:04X}"))
}

// 这两个解析是 `Monitor` 的前置：`display.rs` 已经读了 EDID，`Monitor` 要的是同一份
// 数据（名字与物理尺寸）。**`monitor` 接上时把这两行 `#[allow(dead_code)]` 删掉**——
// 现在留着它只是因为那个模块还没落地，不是打算长期不用。
#[allow(dead_code)]
/// 屏幕物理尺寸（毫米），来自 EDID 基础显示参数里的**厘米**字段（偏移 21、22）再乘 10。
///
/// 为什么不取详细时序描述符里那对毫米（偏移 54+12/13）：本机 DTD 写的是 `302x189`，
/// 而 fastfetch 印 `300x190 mm`——它用的是厘米那对（`30x19` cm），据此算出的
/// `13.98 inches / 242.93 ppi` 也正好对上。差 2 毫米看着无所谓，但这一行是拿它的
/// 输出逐字比对的，口径就得跟它一样。
///
/// 两个字段都为 0 视为没有（投影仪、虚拟屏常见）。
pub(crate) fn physical_size_mm(edid: &[u8]) -> Option<(u32, u32)> {
    let width = u32::from(*edid.get(21)?);
    let height = u32::from(*edid.get(22)?);

    (width > 0 && height > 0).then(|| (width * 10, height * 10))
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
    fn the_exact_refresh_keeps_the_third_decimal() {
        // 这几个数**抄自本机 eDP 的 EDID**（不是编的）：像素时钟 65227（单位 10 kHz）、
        // 行消隐 100、场消隐 24 → 行总数 2980、场总数 1824 → 120.0014 Hz。
        // `Monitor` 要印 `120.001`，`Display` 印 `120`——同一个数，两种舍法。
        let mut edid = edid_with(65_227, 2880, 1800);
        edid[54 + 3] = 100;
        edid[54 + 6] = 24;

        let exact = refresh_exact(&edid, 2880, 1800).unwrap();

        assert_eq!(format!("{exact:.3}"), "120.001");
        assert_eq!(refresh_of(&edid, 2880, 1800), Some(120), "显示那条仍是整数");
    }

    #[test]
    fn the_name_is_the_vendor_letters_plus_the_product_code() {
        // 合成 fixture（不是真 EDID 的副本）：照真机那块的字段布局填，
        // 厂商 `SDC`、产品码 `0x4197`——与 fastfetch 印出的 `SDC4197` 对应。
        let mut edid = edid_with(64_299, 2880, 1800);
        let manufacturer: u16 =
            (('S' as u16 - 64) << 10) | (('D' as u16 - 64) << 5) | ('C' as u16 - 64);
        edid[8..10].copy_from_slice(&manufacturer.to_be_bytes());
        edid[10..12].copy_from_slice(&0x4197u16.to_le_bytes());

        assert_eq!(name_of(&edid).as_deref(), Some("SDC4197"));
    }

    #[test]
    fn the_physical_size_comes_from_the_centimetre_fields() {
        // 同样合成：偏移 21/22 是厘米，30×19 cm → 300×190 mm。
        let mut edid = edid_with(64_299, 2880, 1800);
        edid[21] = 30;
        edid[22] = 19;

        assert_eq!(physical_size_mm(&edid), Some((300, 190)));
    }

    #[test]
    fn a_blank_edid_has_no_name_and_no_size() {
        let edid = vec![0u8; 128];

        assert_eq!(name_of(&edid), None);
        assert_eq!(physical_size_mm(&edid), None);
        assert_eq!(name_of(&edid[..4]), None, "短到读不出字段也是 None");
        assert_eq!(physical_size_mm(&edid[..8]), None);
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
