//! Gpu：显卡型号、驱动、独显还是核显。
//!
//! 数据全在 sysfs 里：`/sys/class/drm/card<N>/device/` 是那张卡的 PCI 设备。
//!
//! - `vendor` / `device`：PCI 厂商号与设备号（`0x1002` / `0x15bf`）
//! - `uevent`：`DRIVER=amdgpu` 这样的驱动名，与 `PCI_ID`、槽位号
//! - `boot_vga`：写着 1 的那张是开机时点亮屏幕的卡，一般就是核显
//!
//! 型号名从 `pci.ids`（hwdata 包）里查：那个文件是「厂商号 → 厂商名 + 设备号 →
//! 设备名」的层级表。查不到就老老实实报 `1002:15bf`——PCI ID 本身就是准确信息，
//! 编一个名字才是撒谎。
//!
//! **不跑 `lspci`、不跑 `glxinfo`**：fastfetch 的 GPU 行里那串
//! `(radeonsi, gfx1103, LLVM 19.1.7, DRM 3.63, 7.2.4-arch1-2)` 是问 Mesa 要的，
//! 那要开子进程或加载图形栈。我们只报驱动模块名（`amdgpu`），
//! 想知道 Mesa 版本的人可以配 `opengl` 模块（那个会挂 `when-command-exists`）。
//!
//! 没有 PCI 显卡的机器（ARM SoC、纯虚拟帧缓冲）拿不到 vendor/device，就是无数据。

use crate::collectors::read;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// DRM 设备目录。
const DRM: &str = "/sys/class/drm";

/// AMD 的 PCI 厂商号，单独拿出来是因为核显那套判断要用它。
const AMD: u16 = 0x1002;

/// `pci.ids` 可能出现的位置，按顺序找。
///
/// 发行版放哪儿全凭自己：Arch 在 `hwdata` 包里，Debian 在 `pciutils` 包里。
const IDS_PATHS: [&str; 4] = [
    "/usr/share/hwdata/pci.ids",
    "/usr/share/misc/pci.ids",
    "/usr/share/pci.ids",
    "/usr/local/share/pci.ids",
];

/// 显卡。
pub struct Gpu;

impl Collector for Gpu {
    fn name(&self) -> &'static str {
        "gpu"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let cards = cards()?;
        if cards.is_empty() {
            return Ok(Vec::new());
        }

        // pci.ids 只在真有卡时才读：没有显卡的机器没必要碰这个几 MB 的文件。
        let ids = read::first(&IDS_PATHS)?;

        // 核显的营销名要做一次 `/proc/cpuinfo`（见 `igpu_marketing_name`），
        // 只有机器上真有一张 AMD 核显时才有必要读它。
        let marketing = if cards.iter().any(|card| card.vendor == AMD && card.boot_vga) {
            read::text("/proc/cpuinfo")?
                .as_deref()
                .and_then(igpu_marketing_name)
        } else {
            None
        };

        // 一张卡是 `GPU`，多张才编号——fastfetch 也是这么处理的。
        let numbered = cards.len() > 1;

        cards
            .iter()
            .enumerate()
            .map(|(index, card)| {
                let key = if numbered {
                    format!("GPU {}", index + 1)
                } else {
                    "GPU".to_owned()
                };

                Ok(card.describe(ids.as_deref(), marketing.as_deref(), self.name(), key))
            })
            .collect()
    }
}

/// 一张显卡的原始信息。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Card {
    /// PCI 厂商号，如 `0x1002`。
    vendor: u16,
    /// PCI 设备号，如 `0x15bf`。
    device: u16,
    /// 驱动模块名，如 `amdgpu`。
    driver: Option<String>,
    /// 开机时点亮屏幕的那张卡（一般就是核显）。
    boot_vga: bool,
}

impl Card {
    /// 组装成一行。
    ///
    /// `AMD Radeon 780M [HawkPoint1] (amdgpu)`、
    /// `NVIDIA GeForce RTX 4060 Max-Q / Mobile (nvidia) [Discrete]`。
    ///
    /// 营销名（`marketing`）只有在「AMD 厂商 + 开机点屏那张卡 + CPU 型号里真写着
    /// `w/ Radeon … Graphics`」三条都成立时才用，见 [`igpu_marketing_name`]。
    fn describe(
        &self,
        ids: Option<&str>,
        marketing: Option<&str>,
        module: &'static str,
        key: String,
    ) -> Info {
        let name = ids
            .and_then(|text| lookup(text, self.vendor, self.device))
            .unwrap_or_else(|| self.raw_id());

        // pci.ids 对 AMD 核显给的是代号（`HawkPoint1`）。代号是准确的，只是没人认得，
        // 所以把营销名放在前面、代号放进方括号——两个都留着，谁也不用猜。
        let name = match marketing {
            Some(marketing) if self.vendor == AMD && self.boot_vga && !name.contains("Radeon") => {
                format!("{marketing} [{name}]")
            }
            _ => name,
        };

        let mut value = match short_vendor(self.vendor) {
            Some(vendor) => format!("{vendor} {name}"),
            // 不认识的厂商就只报 pci.ids 里的名字，不硬套一个缩写。
            None => name,
        };
        if let Some(driver) = &self.driver {
            value.push_str(&format!(" ({driver})"));
        }
        if !self.boot_vga {
            // 不是开机点屏的那张，一般就是独显。这是启发式，不是铁律。
            value.push_str(" [Discrete]");
        }

        let mut info = Info::new(module, key, value)
            .with_variable("vendor", format!("{:04x}", self.vendor))
            .with_variable("device", format!("{:04x}", self.device));
        if let Some(driver) = &self.driver {
            info = info.with_variable("driver", driver.clone());
        }

        info
    }

    /// 查不到名字时用的原始 ID，形如 `1002:15bf`。
    fn raw_id(&self) -> String {
        format!("{:04x}:{:04x}", self.vendor, self.device)
    }
}

/// 把所有 DRM 显卡读出来。
fn cards() -> Result<Vec<Card>, CollectError> {
    let Ok(entries) = std::fs::read_dir(DRM) else {
        return Ok(Vec::new());
    };

    // 目录顺序不保证稳定，排序后再读，免得同一台机器两次输出换个顺序。
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            // `card0` 是显卡，`card0-eDP-1` 是它上面的连接器，`renderD128` 是渲染节点。
            let rest = name.strip_prefix("card")?;
            (!rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| name.to_owned())
        })
        .collect();
    names.sort();

    let mut cards = Vec::new();
    for name in names {
        if let Some(card) = read_card(&format!("{DRM}/{name}/device"))? {
            cards.push(card);
        }
    }

    Ok(cards)
}

/// 读一张卡；不是 PCI 设备（读不到 vendor/device）就是 `None`。
fn read_card(dir: &str) -> Result<Option<Card>, CollectError> {
    let Some(vendor) = read::text(&format!("{dir}/vendor"))? else {
        return Ok(None);
    };
    let Some(device) = read::text(&format!("{dir}/device"))? else {
        return Ok(None);
    };
    let (Some(vendor), Some(device)) = (parse_hex(&vendor), parse_hex(&device)) else {
        return Ok(None);
    };

    let uevent = read::text(&format!("{dir}/uevent"))?.unwrap_or_default();

    Ok(Some(Card {
        vendor,
        device,
        driver: uevent_value(&uevent, "DRIVER"),
        boot_vga: read::text(&format!("{dir}/boot_vga"))?.as_deref() == Some("1"),
    }))
}

/// 解析 sysfs 里的十六进制数：`0x1002`。
fn parse_hex(text: &str) -> Option<u16> {
    let digits = text.trim().strip_prefix("0x").unwrap_or(text.trim());

    u16::from_str_radix(digits, 16).ok()
}

/// 从 `uevent` 里取一个键的值。
fn uevent_value(uevent: &str, key: &str) -> Option<String> {
    uevent
        .lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
        .map(str::to_owned)
        .filter(|value| !value.is_empty())
}

/// 厂商号 → 大家认得的短名字。
///
/// `pci.ids` 里的厂商名太长（`Advanced Micro Devices, Inc. [AMD/ATI]`），
/// 而 fastfetch 印的是 `AMD`。表里没有的厂商就返回 `None`，由调用方决定怎么办。
fn short_vendor(vendor: u16) -> Option<&'static str> {
    match vendor {
        AMD => Some("AMD"),
        0x1010 => Some("ImgTec"),
        0x10de => Some("NVIDIA"),
        0x13b5 => Some("ARM"),
        0x1ae0 => Some("Google"),
        0x1af4 => Some("Red Hat"),
        0x1b36 => Some("Red Hat"),
        0x1234 => Some("QEMU"),
        0x15ad => Some("VMware"),
        0x8086 => Some("Intel"),
        0x5143 => Some("Qualcomm"),
        _ => None,
    }
}

/// 从 `/proc/cpuinfo` 的型号串里取出核显的营销名。
///
/// AMD 的 APU 会把核显名字写进 CPU 型号：
/// `AMD Ryzen 7 8845H w/ Radeon(TM) 780M Graphics`。
///
/// 这是**零子进程**拿到营销名的唯一可靠路子：内核 sysfs 与 `pci.ids` 都只给代号
/// （本机是 `HawkPoint1`），而 fastfetch 那串 `(radeonsi, gfx1103, LLVM 19.1.7)`
/// 得问图形栈。它只在三条**同时**成立时才被采用：厂商是 AMD、这张卡是
/// `boot_vga`（开机点屏的那张就是核显）、CPU 型号里真写着 `w/ Radeon … Graphics`。
/// 独显不会被套上核显的名字——它的 CPU 型号串里没有这一段。
///
/// Intel 的机器上这行会写成 `Intel(R) Core(TM) …`，压根没有 ` w/ `，
/// 所以这个函数返回 `None`，不参与命名。
fn igpu_marketing_name(cpuinfo: &str) -> Option<String> {
    let model = cpuinfo
        .lines()
        .find_map(|line| line.strip_prefix("model name"))?;
    let model = model.trim_start_matches([':', ' ', '\t']);

    let (_, igpu) = model.split_once(" w/ ")?;
    let igpu = igpu.strip_suffix(" Graphics").unwrap_or(igpu);

    // `Radeon(TM) 780M` → `Radeon 780M`。
    let cleaned = igpu
        .replace("(TM)", " ")
        .replace("(tm)", " ")
        .replace("(R)", " ")
        .replace("(r)", " ")
        .replace('™', " ");
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");

    // 只认带 Radeon 的：别的厂商在 `w/` 后面可能写别的东西。
    cleaned.contains("Radeon").then_some(cleaned)
}

/// 在 `pci.ids` 里查设备名。
///
/// 文件结构（真实文件用 TAB 缩进，下面按一层缩进四个空格画；层级就是缩进层数）：
///
/// ```text
/// 1002  Advanced Micro Devices, Inc. [AMD/ATI]     ← 厂商，顶格
///     15bf  Radeon 780M                            ← 设备，一层
///         103c 8b2c  Board name                    ← 子系统，两层
/// C 03  Display controller                         ← 设备类小节，顶格
///     00  VGA compatible controller                ← 类，不是设备
/// ```
///
/// 两条必须当心的：
///
/// 1. **设备类小节**（顶格 `C xx`）里也有缩进行，它们是「类」不是「设备」，
///    后者的设备号可能和显卡撞上——遇到顶格的 `C` 就要把「正在看的厂商」清掉；
/// 2. 子系统行是**两层**缩进，跳过。
fn lookup(text: &str, vendor: u16, device: u16) -> Option<String> {
    let mut in_vendor = false;

    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(rest) = line.strip_prefix('\t') {
            if !in_vendor || rest.starts_with('\t') {
                continue;
            }
            if let Some((id, name)) = split_id(rest) {
                // 这里不用 let-chain（那是 Rust 1.88 才稳定的，本项目的 MSRV 是 1.85）。
                if id == device {
                    return Some(name.to_owned());
                }
            }
            continue;
        }

        // 顶格行：厂商，或者设备类小节（`C ` 开头）。两者都要结束上一段厂商。
        in_vendor = split_id(line).is_some_and(|(id, _)| id == vendor && !line.starts_with("C "));
    }

    None
}

/// 拆一行 `15bf  Radeon 780M` → `(0x15bf, "Radeon 780M")`。
fn split_id(line: &str) -> Option<(u16, &str)> {
    let (id, name) = line.split_once(char::is_whitespace)?;
    let id = u16::from_str_radix(id, 16).ok()?;
    let name = name.trim();

    (!name.is_empty()).then_some((id, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一小段真形状的 pci.ids，含一个会撞号的设备类小节。
    const IDS: &str = "\
# 注释行
1002  Advanced Micro Devices, Inc. [AMD/ATI]
\t15bf  Radeon 780M
\t\t103c 8b2c  Some board
\t164e  Raphael
10de  NVIDIA Corporation
\t2882  GeForce RTX 4060 Max-Q / Mobile
C 03  Display controller
\t00  VGA compatible controller
\t80  Display controller
";

    #[test]
    fn finds_a_device_name() {
        assert_eq!(lookup(IDS, 0x1002, 0x15bf).as_deref(), Some("Radeon 780M"));
        assert_eq!(
            lookup(IDS, 0x10de, 0x2882).as_deref(),
            Some("GeForce RTX 4060 Max-Q / Mobile")
        );
        assert_eq!(lookup(IDS, 0x1002, 0x164e).as_deref(), Some("Raphael"));
    }

    #[test]
    fn the_class_section_does_not_leak_into_device_lookups() {
        // `C 03` 小节里的 `00` 与 `80` 是设备类，不是设备。厂商号撞上时最容易错。
        assert_eq!(lookup(IDS, 0x1002, 0x0000), None);
        assert_eq!(lookup(IDS, 0x1002, 0x0080), None);
    }

    #[test]
    fn a_subsystem_line_is_not_a_device() {
        // `103c 8b2c` 是两层缩进的子系统行，不该被当成设备号 0x103c。
        assert_eq!(lookup(IDS, 0x1002, 0x103c), None);
    }

    #[test]
    fn unknown_ids_are_no_data() {
        assert_eq!(lookup(IDS, 0xffff, 0x0001), None);
        assert_eq!(lookup(IDS, 0x1002, 0xffff), None);
        assert_eq!(lookup("", 0x1002, 0x15bf), None);
    }

    #[test]
    fn splits_an_id_line() {
        assert_eq!(split_id("15bf  Radeon 780M"), Some((0x15bf, "Radeon 780M")));
        assert_eq!(split_id("15bf\tRadeon 780M"), Some((0x15bf, "Radeon 780M")));
        assert_eq!(split_id("15bf"), None, "只有号没有名字");
        assert_eq!(split_id("15bf  "), None, "名字是空的");
        assert_eq!(split_id("zzzz  nope"), None);
    }

    #[test]
    fn parses_sysfs_hex() {
        assert_eq!(parse_hex("0x1002"), Some(0x1002));
        assert_eq!(parse_hex("0x15bf\n"), Some(0x15bf));
        assert_eq!(parse_hex("1002"), Some(0x1002), "没有 0x 前缀也认");
        assert_eq!(parse_hex(""), None);
        assert_eq!(parse_hex("0xzzzz"), None);
    }

    #[test]
    fn reads_values_out_of_uevent() {
        let uevent = "DRIVER=amdgpu\nPCI_CLASS=30000\nPCI_ID=1002:15BF\n";

        assert_eq!(uevent_value(uevent, "DRIVER").as_deref(), Some("amdgpu"));
        assert_eq!(uevent_value(uevent, "PCI_ID").as_deref(), Some("1002:15BF"));
        assert_eq!(uevent_value(uevent, "MISSING"), None);
        assert_eq!(uevent_value("DRIVER=\n", "DRIVER"), None, "空值当没有");
    }

    #[test]
    fn builds_a_line_with_driver_and_discrete_mark() {
        let card = Card {
            vendor: 0x1002,
            device: 0x15bf,
            driver: Some("amdgpu".to_owned()),
            boot_vga: true,
        };
        let info = card.describe(Some(IDS), None, "gpu", "GPU".to_owned());

        assert_eq!(info.key, "GPU");
        assert_eq!(info.value, "AMD Radeon 780M (amdgpu)");
        assert!(!info.value.contains("[Discrete]"), "开机点屏的是核显");

        let discrete = Card {
            boot_vga: false,
            driver: Some("nvidia".to_owned()),
            vendor: 0x10de,
            device: 0x2882,
        };
        assert_eq!(
            discrete
                .describe(Some(IDS), None, "gpu", "GPU 2".to_owned())
                .value,
            "NVIDIA GeForce RTX 4060 Max-Q / Mobile (nvidia) [Discrete]"
        );
    }

    #[test]
    fn reads_the_igpu_name_out_of_the_cpu_model() {
        let cpuinfo =
            "processor\t: 0\nmodel name\t: AMD Ryzen 7 8845H w/ Radeon(TM) 780M Graphics\n";

        assert_eq!(igpu_marketing_name(cpuinfo).as_deref(), Some("Radeon 780M"));
    }

    #[test]
    fn a_cpu_model_without_an_igpu_yields_nothing() {
        // Intel 的型号串里根本没有 ` w/ `。
        assert_eq!(
            igpu_marketing_name("model name\t: Intel(R) Core(TM) Ultra 7 155H\n"),
            None
        );
        // 没有核显的桌面上也拿不到。
        assert_eq!(
            igpu_marketing_name("model name\t: AMD Ryzen 5 5600X 6-Core Processor\n"),
            None
        );
        assert_eq!(igpu_marketing_name(""), None);
        assert_eq!(igpu_marketing_name("processor\t: 0\n"), None);
    }

    #[test]
    fn the_marketing_name_leads_and_the_codename_stays_in_brackets() {
        let card = Card {
            vendor: AMD,
            device: 0x1900,
            driver: Some("amdgpu".to_owned()),
            boot_vga: true,
        };
        let ids = "1002  Advanced Micro Devices, Inc. [AMD/ATI]\n\t1900  HawkPoint1\n";

        // 两条信息都留着：认得的人看前面的，想知道代号的人看方括号里的。
        assert_eq!(
            card.describe(Some(ids), Some("Radeon 780M"), "gpu", "GPU".to_owned())
                .value,
            "AMD Radeon 780M [HawkPoint1] (amdgpu)"
        );
    }

    #[test]
    fn a_name_that_already_says_radeon_is_not_annotated() {
        let card = Card {
            vendor: AMD,
            device: 0x744c,
            driver: None,
            boot_vga: true,
        };
        let ids = "1002  AMD\n\t744c  Radeon RX 7900 XTX\n";

        assert_eq!(
            card.describe(Some(ids), Some("Radeon 780M"), "gpu", "GPU".to_owned())
                .value,
            "AMD Radeon RX 7900 XTX"
        );
    }

    #[test]
    fn a_discrete_card_never_borrows_the_igpu_name() {
        let card = Card {
            vendor: AMD,
            device: 0x1900,
            driver: None,
            boot_vga: false,
        };
        let ids = "1002  AMD\n\t1900  HawkPoint1\n";

        assert_eq!(
            card.describe(Some(ids), Some("Radeon 780M"), "gpu", "GPU 2".to_owned())
                .value,
            "AMD HawkPoint1 [Discrete]"
        );
    }

    #[test]
    fn a_missing_pci_ids_falls_back_to_the_raw_id() {
        let card = Card {
            vendor: 0x1002,
            device: 0x15bf,
            driver: None,
            boot_vga: true,
        };

        // 没有 pci.ids 也不能没有名字：PCI ID 本身是准确信息。
        assert_eq!(
            card.describe(None, None, "gpu", "GPU".to_owned()).value,
            "AMD 1002:15bf"
        );
    }

    #[test]
    fn shortens_the_vendors_we_know() {
        assert_eq!(short_vendor(0x1002), Some("AMD"));
        assert_eq!(short_vendor(0x10de), Some("NVIDIA"));
        assert_eq!(short_vendor(0x8086), Some("Intel"));
        assert_eq!(short_vendor(0xdead), None);
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Gpu.collect(&Context::for_tests()).unwrap();

        for info in &entries {
            assert_eq!(info.module, "gpu");
            assert!(info.key.starts_with("GPU"), "实际是 {}", info.key);
            assert!(!info.value.is_empty());
        }
    }
}
