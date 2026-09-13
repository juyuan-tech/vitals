//! PhysicalDisk：物理盘。
//!
//! 数据全在 sysfs 的 `/sys/block/<盘>/` 下，判据是**有没有 `device` 这个符号链接**：
//! 有它的是真设备（`nvme0n1`、`sda`），没有的是内核造出来的虚拟块设备
//! （`zram0` 的内存盘、`loop0` 的镜像文件、`dm-0` 的映射层）。upstream fastfetch
//! 用的就是这一条（`physicaldisk_linux.c`：`openat(dfd, "device", O_DIRECTORY)` 失败
//! 就是 `VIRTUAL`）。
//!
//! 读的字段：
//!
//! - `size`：容量，**单位固定是 512 字节的扇区**，不是 `queue/logical_block_size`。
//!   `/sys/block/<盘>/size` 的 ABI 就写死了 512（内核文档 sysfs-block：
//!   "The size of the device in 512-byte sectors"），`partitions` 那套才用 1024。
//!   本机 `zram0` 可以直接验证：`size` = 32139264，而它的 `disksize`（字节）
//!   = 16455303168 = 32139264 × **512**；乘 4096（它的 logical_block_size）会得到
//!   122.6 GiB，是它真实容量 15.33 GiB 的八倍。fastfetch 也是硬写 512 的。
//! - `device/model`、`device/vendor`：型号与厂商。有的盘（NVMe）没有 vendor，
//!   有的型号在内核里被截断成 16 字符——**照它给的原样报**，不改写、不补齐。
//! - `queue/rotational`：0 是固态、1 是机械。
//! - `removable`：1 是可移动（U 盘、读卡器）。
//!
//! 值的样子与 upstream 逐字对齐：`953.87 GiB [SSD, Fixed]`，键是
//! `Physical Disk (<名字>)`——名字进键，多张盘自然分得开，不必编号。
//! 名字是 `厂商 型号`（厂商已经出现在型号里时见 [`display_name`]），都读不到就退回设备名。
//!
//! 排除 `loop*` 与 `dm-*`：它们一个是镜像文件、一个是设备映射层，
//! **不是盘**，列出来只会让人以为机器上真多了一堆磁盘。upstream 会把它们
//! 当成 `Virtual` 一起列出来（源码里的判据只有 `device` 链接），这一条是我们
//! 自己的取舍——本机没有这两种设备，无法用实机输出对照。

use std::path::Path;

use crate::collectors::{read, units};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 块设备目录。
const SYS_BLOCK: &str = "/sys/block";

/// `/sys/block/*/size` 的单位：512 字节。见模块文档。
const SECTOR: u64 = 512;

/// 一张盘。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Disk {
    /// 显示名（`Kingston DataTraveler 3.0`、`zram0`）。
    name: String,
    /// 设备名（`sda`），也是 `/sys/block` 下的目录名。
    device: String,
    /// 容量（字节）。
    size: u64,
    /// `SSD` / `HDD` / `Virtual`。分类不出来就是 `None`。
    class: Option<&'static str>,
    /// 0 = 固定，1 = 可移动。读不到就是 `None`。
    removable: Option<bool>,
    /// 总线（`NVMe`、`USB`、`ATA`、`SCSI`、`Virtual`…）。
    interconnect: Option<String>,
}

/// 物理盘。
pub struct PhysicalDisk;

impl Collector for PhysicalDisk {
    fn name(&self) -> &'static str {
        "physical-disk"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Ok(entries) = std::fs::read_dir(SYS_BLOCK) else {
            // 没有 sysfs（非 Linux、极简容器）就是无数据，不是错误。
            return Ok(Vec::new());
        };

        // 目录顺序不保证稳定，先排一遍；最后再按显示名排（见下）。
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
            .filter(|name| !name.starts_with('.') && !is_layer(name))
            .collect();
        names.sort();

        let mut disks = Vec::new();
        for name in names {
            if let Some(disk) = read_disk(&name)? {
                disks.push(disk);
            }
        }

        // 按**显示名**排序，和 upstream 一样（`physicaldisk.c` 里 sortDevices 比的就是
        // 这个名字）。实际效果：`Kingston …` < `SAMSUNG …` < `zram0`，
        // 设备名顺序恰好相反——所以这个顺序是照抄来的，不是随手定的。
        disks.sort_by(|left, right| left.name.cmp(&right.name));

        Ok(disks
            .iter()
            .map(|disk| disk.describe(self.name()))
            .collect())
    }
}

/// 这张是块设备层叠（loop / device-mapper），不是盘。
///
/// `loop0` 背后是一个镜像文件（snap、ISO），`dm-0` 背后是 LVM/LUKS 的映射，
/// 它们都没有自己的物理介质。upstream 会按 `Virtual` 列出来，我们选择不列——
/// 见模块文档里的说明。
fn is_layer(name: &str) -> bool {
    name.starts_with("loop") || name.starts_with("dm-")
}

impl Disk {
    /// 组装成一行。
    ///
    /// `953.87 GiB [SSD, Fixed]`。分类与固定性都可能读不到，那时对应那一段省掉，
    /// 只剩容量——容量是唯一必须有的东西。
    fn describe(&self, module: &'static str) -> Info {
        let mut value = units::bytes_precise(self.size);

        let class = self.class;
        let fixedness = self
            .removable
            .map(|removable| if removable { "Removable" } else { "Fixed" });

        if class.is_some() || fixedness.is_some() {
            let mut parts: Vec<&str> = Vec::new();
            parts.extend(class);
            parts.extend(fixedness);
            value.push_str(&format!(" [{}]", parts.join(", ")));
        }

        let mut info = Info::new(module, format!("Physical Disk ({})", self.name), value)
            .with_variable("dev_path", format!("/dev/{}", self.device))
            .with_variable("size_bytes", self.size.to_string());

        if let Some(class) = class {
            info = info.with_variable("type", class);
        }
        if let Some(interconnect) = &self.interconnect {
            info = info.with_variable("interconnect", interconnect.clone());
        }

        info
    }
}

/// 读一张盘；没有容量或容量为 0 就不是盘（`zram` 初始化前、空槽位）。
fn read_disk(device: &str) -> Result<Option<Disk>, CollectError> {
    let dir = format!("{SYS_BLOCK}/{device}");

    let Some(size) = read::text(&format!("{dir}/size"))? else {
        return Ok(None);
    };
    let Ok(sectors) = size.trim().parse::<u64>() else {
        return Ok(None);
    };
    if sectors == 0 {
        // upstream 默认把这类（`UNUSED`）藏掉。
        return Ok(None);
    }

    let physical = Path::new(&dir).join("device").is_dir();
    let removable = read::text(&format!("{dir}/removable"))?
        .as_deref()
        .and_then(parse_flag);

    let (name, class, interconnect) = if physical {
        // 型号与厂商都在 `device/` 下面；NVMe 通常没有 `vendor`。
        let vendor = read::text(&format!("{dir}/device/vendor"))?;
        let model = read::text(&format!("{dir}/device/model"))?;
        let rotational = read::text(&format!("{dir}/queue/rotational"))?;

        (
            display_name(device, vendor.as_deref(), model.as_deref()),
            rotational.as_deref().and_then(classify),
            interconnect(device, &dir)?,
        )
    } else {
        (
            device.to_owned(),
            Some("Virtual"),
            Some("Virtual".to_owned()),
        )
    };

    Ok(Some(Disk {
        name,
        device: device.to_owned(),
        size: sectors.saturating_mul(SECTOR),
        class,
        removable,
        interconnect,
    }))
}

/// 显示名：`厂商 型号`，都读不到就是设备名。
///
/// 显示名：`厂商 型号`，都读不到就是设备名。
///
/// **与 upstream 逐字一致**（`physicaldisk_linux.c`）：`vendor` 非空就拼上它加一个空格，
/// 再拼 `model`，然后去掉尾部空白；两边都空才退回设备名。这里**没有**做「厂商已经在
/// 型号里就不重复」这种聪明处理——本机那块 NVMe 盘**根本没有** `vendor` 文件
/// （`SAMSUNG` 是型号 `SAMSUNG MZVL21T0HCLR-00BH1` 自带的前缀），所以那条差别
/// 在这台机器上看不出来；看不出来就照抄 upstream，不多想。
fn display_name(device: &str, vendor: Option<&str>, model: Option<&str>) -> String {
    let vendor = vendor.map(str::trim).filter(|value| !value.is_empty());
    let model = model.map(str::trim).filter(|value| !value.is_empty());

    let name = match (vendor, model) {
        (Some(vendor), Some(model)) => format!("{vendor} {model}"),
        (Some(vendor), None) => vendor.to_owned(),
        (None, Some(model)) => model.to_owned(),
        (None, None) => device.to_owned(),
    };

    if name.is_empty() {
        device.to_owned()
    } else {
        name
    }
}

/// `queue/rotational` → 固态还是机械。
///
/// 0 是固态、1 是机械。upstream 的口径一样。**没有把它当成「SSD 就是快」**：
/// U 盘与读卡器常常报 1（本机那块 DataTraveler 就是），所以它只是
/// 「内核认为这是旋转介质」。
fn classify(rotational: &str) -> Option<&'static str> {
    match rotational.trim() {
        "0" => Some("SSD"),
        "1" => Some("HDD"),
        _ => None,
    }
}

/// `removable` → 布尔。
fn parse_flag(text: &str) -> Option<bool> {
    match text.trim() {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

/// 总线类型。
///
/// 只有设备目录里那一行 `subsystem` 还不够：USB 与 SATA 盘在 `/sys/bus/scsi` 下
/// 长得一样。upstream 走的是 `realpath` 之后看路径里有没有 `/usb`、`/ata`、`/scsi`、
/// `/nvme`、`/virtio`，本机那块 U 盘正是这样认出 `USB` 的
/// （`/sys/devices/…/usb2/2-1/2-1:1.0/host0/target0:0:0/0:0:0:0`）。
/// 认不出来就退回 `device/transport`（NVMe 写的是 `pcie`），再不行就不报。
fn interconnect(device: &str, dir: &str) -> Result<Option<String>, CollectError> {
    if let Some(bus) = bus_from_name(device) {
        return Ok(Some(bus.to_owned()));
    }

    if let Ok(path) = std::fs::canonicalize(format!("{dir}/device")) {
        let path = path.to_string_lossy();
        for (marker, bus) in [
            ("/usb", "USB"),
            ("/ata", "ATA"),
            ("/scsi", "SCSI"),
            ("/nvme", "NVMe"),
            ("/virtio", "VirtIO"),
        ] {
            if path.contains(marker) {
                return Ok(Some(bus.to_owned()));
            }
        }
    }

    Ok(read::text(&format!("{dir}/device/transport"))?.filter(|transport| !transport.is_empty()))
}

/// 从设备名就能认出来的总线。upstream 只认这两个前缀；
/// 虚拟盘（`zram`、`loop`）走的是「没有 `device` 链接 → `Virtual`」那条路，
/// 不靠名字，所以这里也不写它们的名字规则。
fn bus_from_name(device: &str) -> Option<&'static str> {
    if device.starts_with("nvme") {
        Some("NVMe")
    } else if device.starts_with("mmcblk") {
        Some("MMC")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_disk_the_way_upstream_does() {
        // 本机三张盘的字段原文（`vendor`/`model` 里的尾随空白已经由 read::text 去掉）。
        assert_eq!(
            display_name("sda", Some("Kingston"), Some("DataTraveler 3.0")),
            "Kingston DataTraveler 3.0"
        );
        // NVMe 没有 vendor 文件，型号里自带厂商名。
        assert_eq!(
            display_name("nvme0n1", None, Some("SAMSUNG MZVL21T0HCLR-00BH1")),
            "SAMSUNG MZVL21T0HCLR-00BH1"
        );
        // 虚拟盘没有型号，退回设备名。
        assert_eq!(display_name("zram0", None, None), "zram0");
        assert_eq!(display_name("zram0", Some("  "), Some("")), "zram0");
    }

    #[test]
    fn the_vendor_is_concatenated_exactly_like_upstream() {
        // upstream 就是无条件拼：`vendor` 非空 → `vendor + ' ' + model`。
        // 所以型号里已经带着厂商时会出现两遍——这看着蠢，但它与 fastfetch 一致，
        // 而本机的 NVMe 盘没有 vendor 文件，压根走不到这一支。
        assert_eq!(
            display_name("nvme0n1", Some("SAMSUNG"), Some("SAMSUNG MZVL21T0HCLR")),
            "SAMSUNG SAMSUNG MZVL21T0HCLR"
        );
        assert_eq!(
            display_name("sdb", Some("WDC"), Some("WD20EZBX-00A")),
            "WDC WD20EZBX-00A"
        );
        // 只有厂商、没有型号。
        assert_eq!(display_name("sdc", Some("WDC"), None), "WDC");
    }

    #[test]
    fn the_size_is_sectors_times_512() {
        // 本机实测：zram0 的 size 是 32139264，disksize 是 16455303168 字节，
        // 正好是 ×512；乘它的逻辑块大小 4096 会得到八倍大的假容量。
        assert_eq!(32_139_264_u64 * SECTOR, 16_455_303_168);
        assert_ne!(32_139_264_u64 * 4096, 16_455_303_168);

        assert_eq!(units::bytes_precise(2_000_409_264 * SECTOR), "953.87 GiB");
    }

    #[test]
    fn rotational_tells_ssd_from_hdd() {
        assert_eq!(classify("0"), Some("SSD"));
        assert_eq!(classify("1"), Some("HDD"));
        assert_eq!(classify("1\n"), Some("HDD"));
        assert_eq!(classify(""), None);
        assert_eq!(classify("yes"), None);
    }

    #[test]
    fn removable_flags_parse() {
        assert_eq!(parse_flag("0"), Some(false));
        assert_eq!(parse_flag("1\n"), Some(true));
        assert_eq!(parse_flag(""), None);
    }

    #[test]
    fn block_layers_are_not_disks() {
        assert!(is_layer("loop0"));
        assert!(is_layer("loop15"));
        assert!(is_layer("dm-0"));
        // zram、md 是内核的虚拟盘，但它们不是「层叠」——zram 还要报出来。
        assert!(!is_layer("zram0"));
        assert!(!is_layer("md0"));
        assert!(!is_layer("nvme0n1"));
        assert!(!is_layer("sda"));
    }

    #[test]
    fn busses_are_recognized_from_the_name_first() {
        assert_eq!(bus_from_name("nvme0n1"), Some("NVMe"));
        assert_eq!(bus_from_name("mmcblk0"), Some("MMC"));
        // `zram` 这种虚拟盘不靠名字判总线：它是「没有 device 链接」那一支
        // （upstream 也一样），名字规则只管 NVMe 与 MMC。
        assert_eq!(bus_from_name("zram0"), None);
        assert_eq!(bus_from_name("sda"), None);
    }

    /// 造一张盘，只为测拼装。
    fn disk(class: Option<&'static str>, removable: Option<bool>) -> Disk {
        Disk {
            name: "Test Disk".to_owned(),
            device: "sdtest".to_owned(),
            size: 1_024_209_543_168,
            class,
            removable,
            interconnect: None,
        }
    }

    #[test]
    fn the_value_lists_class_and_fixedness() {
        assert_eq!(
            disk(Some("SSD"), Some(false))
                .describe("physical-disk")
                .value,
            "953.87 GiB [SSD, Fixed]"
        );
        assert_eq!(
            disk(Some("HDD"), Some(true))
                .describe("physical-disk")
                .value,
            "953.87 GiB [HDD, Removable]"
        );
        assert_eq!(
            disk(Some("Virtual"), Some(false))
                .describe("physical-disk")
                .value,
            "953.87 GiB [Virtual, Fixed]"
        );
    }

    #[test]
    fn a_disk_without_a_class_still_reports_its_size() {
        // 容量是唯一必须有的东西：别的字段读不到也要有一条信息。
        assert_eq!(
            disk(None, None).describe("physical-disk").value,
            "953.87 GiB"
        );
        assert_eq!(
            disk(None, Some(true)).describe("physical-disk").value,
            "953.87 GiB [Removable]"
        );
    }

    #[test]
    fn the_key_carries_the_name() {
        let info = disk(Some("SSD"), Some(false)).describe("physical-disk");

        assert_eq!(info.key, "Physical Disk (Test Disk)");
        assert_eq!(info.module, "physical-disk");
        assert_eq!(info.variable("dev_path"), Some("/dev/sdtest"));
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = PhysicalDisk.collect(&Context::for_tests()).unwrap();

        // 容器里没有 /sys/block（或只有 zram/loop）就是空的，也算通过。
        for info in &entries {
            assert_eq!(info.module, "physical-disk");
            assert!(
                info.key.starts_with("Physical Disk ("),
                "实际是 {}",
                info.key
            );
            assert!(
                info.key.ends_with(')') && info.key.len() > "Physical Disk ()".len(),
                "键里该有盘的名字：{}",
                info.key
            );
            assert!(info.value.contains("GiB") || info.value.contains("MiB"));
        }

        // 真机上不该出现 loop/dm——那是层叠块设备，不是盘。
        for info in &entries {
            assert!(!info.key.contains("(loop"), "{}", info.key);
            assert!(!info.key.contains("(dm-"), "{}", info.key);
        }
    }
}
