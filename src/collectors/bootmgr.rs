//! Bootmgr：二级启动器（UEFI 启动项，或磁盘上的启动器配置）。
//!
//! 首选 **UEFI 变量**。efivarfs 里每个变量都有一份原文可读，链路是：
//!
//! 1. `BootCurrent` 是一个 u16 序号（本机是 2），拼成 `Boot0002`；
//! 2. `Boot0002` 的内容就是那条启动项（`EFI_LOAD_OPTION`）；
//! 3. 启动项里的**描述**是固件给它起的名字（本机是 `ARCH`），
//!    设备路径里的**文件路径**是加载器（`\EFI\ARCH\grubx64.efi`）。
//!
//! 这两个字段在同一段字节里，中间隔着「描述」，所以要按长度正确跨过去：
//!
//! ```text
//! 相对文件头   字段                 说明
//! 0            Attributes           efivarfs 自己加的 4 字节属性头，不是启动项的一部分
//! 4            Attributes           u32！不是 u16 —— 见下
//! 8            FilePathListLength   u16，设备路径的字节数
//! 10           Description          UTF-16LE，以 NUL 结尾
//! 10+2*(N+1)   FilePathList         设备路径节点的串（Type 0x7F 收尾）
//! ```
//!
//! **最容易写错的就是第 4 字节那个字段是 4 字节**。本机 `Boot0002` 的原文是
//! `07 00 00 00 | 01 00 00 00 | 5e 00 | 41 00 52 00 43 00 00 00 | 04 01 2a 00 …`：
//! 把属性读成 u16 的话，描述会从第 8 字节开始，第一个字符就是长度低字节 `0x5E`（`^`），
//! 于是印出 `^ARCH`——看着像模像样，其实是错的。单元测试把两种读法都钉住了。
//!
//! 拿不到 efivars（BIOS 机器、没挂 efivarfs）时回退到磁盘上的线索：
//! `/boot/loader/loader.conf` 在 → `systemd-boot`；`/boot/grub/grub.cfg` 在 → `GRUB`；
//! 都没有 → 无数据。
//!
//! 明确**不做**的事：
//!
//! - 不解析 `grub.cfg` 猜 GRUB 版本：那个文件由 `grub-mkconfig` 生成，里面没有
//!   可靠的版本号，猜出来的数字比不报更坏。
//! - 不拿 `/boot/efi/EFI/*` 的目录名当启动器名：那是**文件系统布局**，
//!   不是固件的启动项（固件读的是 NVRAM，两者可以完全对不上）。
//! - 不报固件厂商：`/sys/firmware/efi/fw_vendor` 在本机写的是一个地址
//!   （`0x57e17d98`，内核某些固件上拿不到字符串），编一个名字更坏。

use std::path::Path;

use crate::collectors::read;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// EFI 变量目录（efivarfs 的挂载点）。
const EFIVARS: &str = "/sys/firmware/efi/efivars";
/// EFI 全局变量 GUID：`Boot####`、`BootCurrent`、`BootOrder` 都挂在它下面。
const EFI_GLOBAL_GUID: &str = "8be4df61-93ca-11d2-aa0d-00e098032b8c";
/// efivarfs 给每个变量加的属性头长度。
const VARIABLE_ATTRIBUTES: usize = 4;

/// systemd-boot 的配置：文件在就说明机器上装的是它。
const LOADER_CONF: &str = "/boot/loader/loader.conf";
/// GRUB 的配置：只能说明装了 GRUB。
const GRUB_CFG: &str = "/boot/grub/grub.cfg";

/// 启动管理器。
pub struct Bootmgr;

impl Collector for Bootmgr {
    fn name(&self) -> &'static str {
        "bootmgr"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        if let Some(info) = from_uefi(self.name())? {
            return Ok(vec![info]);
        }

        Ok(fallback(self.name())?.into_iter().collect())
    }
}

/// 首选路径：从 UEFI 变量里读当前启动项。
///
/// 任何一步拿不到（没有 efivars、变量不存在、内容短得读不出字段）都返回 `None`
/// 交给调用方回退——**不是错误**：BIOS 机器上这条路本来就不存在。
fn from_uefi(module: &'static str) -> Result<Option<Info>, CollectError> {
    let Some(current) = efivar("BootCurrent")? else {
        return Ok(None);
    };
    let Some(order) = boot_current(&current) else {
        return Ok(None);
    };

    let Some(raw) = efivar(&format!("Boot{order:04X}"))? else {
        return Ok(None);
    };

    Ok(load_option(&raw).and_then(|entry| entry.describe(module)))
}

/// 读一个 EFI 变量。
///
/// 先按标准 GUID 拼出路径直接读；找不到就在目录里扫一遍前缀——`Boot####` 按规范
/// 一定挂在 EFI 全局 GUID 下，但扫一遍只花一次 `read_dir`，能兜住别的挂法。
///
/// 目录不存在（BIOS 机器、没挂 efivarfs）就是 `None`。**目录读不了**（权限）也返回
/// `None` 而不是报错：这台机器上还有文件回退那条路，能给出一个真答案，
/// 为目录权限把整个模块判失败只会让人看不到那个答案。
fn efivar(name: &str) -> Result<Option<Vec<u8>>, CollectError> {
    let exact = format!("{EFIVARS}/{name}-{EFI_GLOBAL_GUID}");
    if let Some(bytes) = read::bytes(&exact)? {
        return Ok(Some(bytes));
    }

    let prefix = format!("{name}-");
    let Ok(entries) = std::fs::read_dir(EFIVARS) else {
        return Ok(None);
    };

    let mut candidates: Vec<String> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .filter(|entry| entry.starts_with(&prefix))
        .collect();
    candidates.sort();

    for candidate in candidates {
        if let Some(bytes) = read::bytes(&format!("{EFIVARS}/{candidate}"))? {
            return Ok(Some(bytes));
        }
    }

    Ok(None)
}

/// `BootCurrent` 的内容 → 启动项序号。
///
/// 本机实测原文（`hexdump -C`）：`06 00 00 00 02 00`——前 4 字节是 efivarfs 的属性，
/// 后面一个小端 u16 就是序号 2。x86 的 UEFI 数据一律小端，所以用 `from_le_bytes`。
fn boot_current(bytes: &[u8]) -> Option<u16> {
    let tail = bytes.get(VARIABLE_ATTRIBUTES..VARIABLE_ATTRIBUTES + 2)?;

    Some(u16::from_le_bytes(tail.try_into().ok()?))
}

/// 一条 EFI 启动项里我们关心的两个字段。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct BootEntry {
    /// 固件给这条启动项起的名字（`ARCH`、`Windows Boot Manager`）。
    description: String,
    /// 加载器的完整设备路径（`\EFI\ARCH\grubx64.efi`）。
    firmware: String,
}

impl BootEntry {
    /// 组装成一行：`ARCH - grubx64.efi`。
    ///
    /// 文件名取路径的最后一段（upstream 也是取最后一个反斜杠之后的部分）。
    /// 描述与路径都可能缺一个：缺路径就只报描述，缺描述就只报文件名，
    /// 两个都没有才算无数据——那是空值，不该印出来。
    fn describe(&self, module: &'static str) -> Option<Info> {
        let file = file_name(&self.firmware);
        let value = match (self.description.is_empty(), file) {
            (false, Some(file)) => format!("{} - {file}", self.description),
            (false, None) => self.description.clone(),
            (true, Some(file)) => file.to_owned(),
            (true, None) => return None,
        };

        let mut info = Info::new(module, "Boot Manager", value);
        if !self.description.is_empty() {
            info = info.with_variable("description", self.description.clone());
        }
        if !self.firmware.is_empty() {
            info = info.with_variable("firmware", self.firmware.clone());
        }

        Some(info)
    }
}

/// 解析一条启动项（`EFI_LOAD_OPTION`，含 efivarfs 的 4 字节属性头）。
///
/// 读不出来的都是 `None`：宁可回退到文件线索，也不印半条猜测的记录。
fn load_option(bytes: &[u8]) -> Option<BootEntry> {
    let length = bytes
        .get(VARIABLE_ATTRIBUTES + 4..VARIABLE_ATTRIBUTES + 6)?
        .try_into()
        .ok()?;
    let path_length = usize::from(u16::from_le_bytes(length));

    // 跳过 u32 属性与 u16 路径长度，从描述开始。
    let rest = bytes.get(VARIABLE_ATTRIBUTES + 6..)?;
    let (description, consumed) = utf16_field(rest);
    let list = rest.get(consumed..)?;

    // `FilePathListLength` 是权威；它写 0 或者超出文件（有的固件就这么干）时，
    // 就按到文件尾走一遍节点串——反正有 `End` 节点收尾。
    let list = if path_length == 0 {
        list
    } else {
        list.get(..path_length).unwrap_or(list)
    };

    Some(BootEntry {
        description,
        firmware: file_path(list).unwrap_or_default(),
    })
}

/// 在设备路径串里找「文件路径」节点（Type 4 / SubType 4）。
///
/// 设备路径是一串节点，每个节点 4 字节头：`Type`、`SubType`、`Length`（u16 小端）。
/// `Type 0x7F` 是结束节点。长度不合法的节点说明这段字节坏了，
/// 直接放弃——按错误的步长往下走只会读出垃圾。
fn file_path(list: &[u8]) -> Option<String> {
    let mut at = 0;

    while let Some(header) = list.get(at..at + 4) {
        let kind = header[0];
        let subkind = header[1];
        let length = usize::from(u16::from_le_bytes([header[2], header[3]]));

        if kind == 0x7F {
            return None;
        }
        if length < 4 || at + length > list.len() {
            return None;
        }
        if kind == 4 && subkind == 4 {
            return Some(utf16_field(&list[at + 4..at + length]).0);
        }

        at += length;
    }

    None
}

/// 解一段 UTF-16LE 到 NUL 为止，返回文本与**含那个 NUL**用掉的字节数。
///
/// 用 `from_utf16_lossy`：固件里的字节是外部输入，落单的代理项替换成 U+FFFD
/// 也比让整个模块失败强——我们只拿它显示。
fn utf16_field(bytes: &[u8]) -> (String, usize) {
    let mut units = Vec::new();

    for unit in bytes.chunks_exact(2) {
        let value = u16::from_le_bytes([unit[0], unit[1]]);
        if value == 0 {
            break;
        }
        units.push(value);
    }

    ((String::from_utf16_lossy(&units)), (units.len() + 1) * 2)
}

/// 取路径的最后一段（`\EFI\ARCH\grubx64.efi` → `grubx64.efi`）。
fn file_name(path: &str) -> Option<&str> {
    path.rsplit(['\\', '/']).find(|part| !part.is_empty())
}

/// 回退路径：磁盘上的启动器配置。
fn fallback(module: &'static str) -> Result<Option<Info>, CollectError> {
    // `loader.conf` 要读内容（里面的 `default` 指向哪个条目），
    // `grub.cfg` 只看在不在——它里面没有我们要的东西，读它反而白读几十 KB。
    let loader_conf = read::text(LOADER_CONF)?;
    let grub_cfg = Path::new(GRUB_CFG).exists();

    let Some((name, config, default)) = hint(loader_conf.as_deref(), grub_cfg) else {
        return Ok(None);
    };

    let mut info = Info::new(module, "Boot Manager", name).with_variable("config", config);
    if let Some(default) = default {
        // 条目名**不是版本**，所以放进变量，不写进值里。
        info = info.with_variable("default", default);
    }

    Ok(Some(info))
}

/// 文件线索的判定。
///
/// 纯函数：BIOS 机器上这一支在真机上走不到（回退条件本机不成立），
/// 靠单元测试把两种情况都钉住。
fn hint(
    loader_conf: Option<&str>,
    grub_cfg: bool,
) -> Option<(&'static str, &'static str, Option<String>)> {
    if let Some(text) = loader_conf {
        return Some(("systemd-boot", LOADER_CONF, loader_default(text)));
    }
    if grub_cfg {
        return Some(("GRUB", GRUB_CFG, None));
    }

    None
}

/// `loader.conf` 里的 `default` 条目名。
///
/// 那一行长这样：`default arch.conf`（没有等号，和 ini 不是一回事）。
/// 只报名字，不当版本。`defaults` 这种别的键不能算。
fn loader_default(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("default")?;
        if !rest.starts_with(char::is_whitespace) {
            return None;
        }

        let value = rest.trim().trim_matches('"');
        (!value.is_empty()).then(|| value.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机 `Boot0002` 的原文（`hexdump -C`，只留十六进制那一段）。
    const BOOT0002: &str = "\
07 00 00 00 01 00 00 00 5e 00 41 00 52 00 43 00
48 00 00 00 04 01 2a 00 01 00 00 00 00 08 00 00
00 00 00 00 00 00 20 00 00 00 00 00 88 ce 9f cf
0a 17 d1 4f 91 dc 53 44 19 f9 b8 8f 02 02 04 04
30 00 5c 00 45 00 46 00 49 00 5c 00 41 00 52 00
43 00 48 00 5c 00 67 00 72 00 75 00 62 00 78 00
36 00 34 00 2e 00 65 00 66 00 69 00 00 00 7f ff
04 00";

    /// 把十六进制串（空白分隔）变成字节。
    fn hex(text: &str) -> Vec<u8> {
        text.split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).expect("测试数据是十六进制"))
            .collect()
    }

    #[test]
    fn boot_current_is_a_little_endian_u16_after_the_variable_header() {
        // 本机原文：`06 00 00 00 02 00` → 序号 2 → 要读的就是 `Boot0002`。
        assert_eq!(boot_current(&[0x06, 0, 0, 0, 0x02, 0x00]), Some(2));
        assert_eq!(boot_current(&[0x07, 0, 0, 0, 0x00, 0x20]), Some(0x2000));
        // 短了就是读不出来，不是读成 0。
        assert_eq!(boot_current(&[]), None);
        assert_eq!(boot_current(&[0x06, 0, 0, 0, 0x02]), None);
    }

    #[test]
    fn the_load_option_attributes_are_four_bytes_not_two() {
        let bytes = hex(BOOT0002);

        // 第 4 字节起是 u32 属性（1 = LOAD_OPTION_ACTIVE），第 8 字节起才是路径长度。
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(bytes[8..10].try_into().unwrap()), 94);
        assert_eq!(bytes[10..12], [0x41, 0x00], "第 10 字节起是描述的 'A'");

        // 把属性当成 u16 读，描述就会从第 8 字节开始，变成 `^ARCH`
        // （`^` 是路径长度低字节 0x5E）。这条就是那个坑本身。
        assert_eq!(utf16_field(&bytes[8..]).0, "^ARCH");
        assert_eq!(utf16_field(&bytes[10..]).0, "ARCH");
    }

    #[test]
    fn reads_the_description_and_the_loader_out_of_a_real_variable() {
        let entry = load_option(&hex(BOOT0002)).expect("本机这条启动项该能解析");

        assert_eq!(entry.description, "ARCH");
        assert_eq!(entry.firmware, "\\EFI\\ARCH\\grubx64.efi");
        assert_eq!(
            entry.describe("bootmgr").unwrap().value,
            "ARCH - grubx64.efi",
            "与本机 fastfetch 的输出逐字一致"
        );
    }

    #[test]
    fn the_device_path_nodes_are_walked_by_their_lengths() {
        // 路径串长 94 字节：HardDrive 节点 42 + FilePath 节点 48 + End 节点 4。
        let file_path_node = "\
04 04 30 00 5c 00 45 00 46 00 49 00 5c 00 41 00 52 00 43 00 48 00 5c 00 67 00 72 00 75 \
00 62 00 78 00 36 00 34 00 2e 00 65 00 66 00 69 00 00 00 7f ff 04 00";
        assert_eq!(
            file_path(&hex(file_path_node)).as_deref(),
            Some("\\EFI\\ARCH\\grubx64.efi")
        );

        // End 节点在前（没有文件路径节点）：没有名字。
        assert_eq!(file_path(&hex("7f ff 04 00")), None);
        // 长度不合法：不往下走，也别 panic。
        assert_eq!(file_path(&hex("04 04 00 00 41 00")), None);
        assert_eq!(
            file_path(&hex("04 04 30 00 41 00")),
            None,
            "长度超出这段字节"
        );
        assert_eq!(file_path(&[]), None);
    }

    #[test]
    fn a_windows_style_entry_reads_the_same_way() {
        // 造一条「描述更长、路径也不一样」的启动项：描述 `Windows Boot Manager`，
        // 路径 `\EFI\Microsoft\Boot\bootmgfw.efi`。
        let description = "Windows Boot Manager";
        let firmware = "\\EFI\\Microsoft\\Boot\\bootmgfw.efi";

        let mut units: Vec<u16> = description.encode_utf16().collect();
        units.push(0);
        let description_bytes: Vec<u8> = units.iter().flat_map(|unit| unit.to_le_bytes()).collect();

        let mut path: Vec<u16> = firmware.encode_utf16().collect();
        path.push(0);
        let mut path_bytes: Vec<u8> = path.iter().flat_map(|unit| unit.to_le_bytes()).collect();
        let node_length = (path_bytes.len() + 4) as u16;
        let mut node = vec![4, 4];
        node.extend_from_slice(&node_length.to_le_bytes());
        node.append(&mut path_bytes);
        let mut list = node;
        list.extend_from_slice(&[0x7f, 0xff, 0x04, 0x00]);

        let mut bytes = vec![0x07, 0, 0, 0]; // efivarfs 属性头
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&(list.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&description_bytes);
        bytes.extend_from_slice(&list);

        let entry = load_option(&bytes).expect("该能解析");
        assert_eq!(entry.description, description);
        assert_eq!(entry.firmware, firmware);
        assert_eq!(
            entry.describe("bootmgr").unwrap().value,
            "Windows Boot Manager - bootmgfw.efi"
        );
    }

    #[test]
    fn broken_variables_yield_nothing_instead_of_half_a_record() {
        // 短得连路径长度都读不出来。
        assert_eq!(load_option(&[0x07, 0, 0, 0, 1, 0, 0, 0, 0x5e]), None);
        // 描述没有终结符（一直读到文件尾）。
        assert_eq!(
            load_option(&hex("07 00 00 00 01 00 00 00 00 00 41 00")),
            None
        );
        // 描述是空的、路径也没有：两个字段都缺，不算一条信息。
        assert_eq!(
            load_option(&hex("07 00 00 00 01 00 00 00 00 00 00 00")).unwrap(),
            BootEntry::default()
        );
        assert_eq!(BootEntry::default().describe("bootmgr"), None);
    }

    #[test]
    fn a_boot_entry_with_only_one_field_still_reports_it() {
        // 有描述没路径（设备路径长度为 0）。
        let only_description = BootEntry {
            description: "ARCH".to_owned(),
            firmware: String::new(),
        };
        assert_eq!(only_description.describe("bootmgr").unwrap().value, "ARCH");

        // 有路径没描述。
        let only_path = BootEntry {
            description: String::new(),
            firmware: "\\EFI\\BOOT\\BOOTX64.EFI".to_owned(),
        };
        let info = only_path.describe("bootmgr").unwrap();
        assert_eq!(info.value, "BOOTX64.EFI");
        assert!(info.variable("description").is_none());
    }

    #[test]
    fn the_file_name_is_the_last_path_component() {
        assert_eq!(file_name("\\EFI\\ARCH\\grubx64.efi"), Some("grubx64.efi"));
        assert_eq!(
            file_name("/boot/efi/EFI/BOOT/BOOTX64.EFI"),
            Some("BOOTX64.EFI")
        );
        assert_eq!(file_name("grubx64.efi"), Some("grubx64.efi"));
        assert_eq!(file_name("\\EFI\\ARCH\\"), Some("ARCH"));
        assert_eq!(file_name(""), None);
    }

    // -----------------------------------------------------------------------
    // 回退（BIOS 机器上真机走不到这一支）
    // -----------------------------------------------------------------------

    #[test]
    fn the_file_hints_are_tried_in_order_of_confidence() {
        // 两个都在时 systemd-boot 说了算：它有一份明确的配置。
        let (name, config, default) = hint(Some("default arch.conf\ntimeout 3\n"), true).unwrap();
        assert_eq!(name, "systemd-boot");
        assert_eq!(config, LOADER_CONF);
        assert_eq!(default.as_deref(), Some("arch.conf"));

        // 只有 grub.cfg：只能说装了 GRUB，说不出条目。
        let (name, config, default) = hint(None, true).unwrap();
        assert_eq!(name, "GRUB");
        assert_eq!(config, GRUB_CFG);
        assert_eq!(default, None);

        // 两个都没有：无数据。
        assert_eq!(hint(None, false), None);
    }

    #[test]
    fn the_default_entry_is_a_name_not_a_version() {
        assert_eq!(
            loader_default("default arch.conf\n").as_deref(),
            Some("arch.conf")
        );
        assert_eq!(
            loader_default("# 注释\ntimeout 4\ndefault  my-entry \n").as_deref(),
            Some("my-entry")
        );
        assert_eq!(
            loader_default("default \"quoted entry\"\n").as_deref(),
            Some("quoted entry")
        );
        // `defaults` 是别的键，不能认成 `default`。
        assert_eq!(loader_default("defaults 2\n"), None);
        assert_eq!(loader_default("default\n"), None);
        assert_eq!(loader_default(""), None);
    }

    // -----------------------------------------------------------------------
    // 真机 smoke
    // -----------------------------------------------------------------------

    #[test]
    fn collects_on_this_machine() {
        let entries = Bootmgr.collect(&Context::for_tests()).unwrap();

        // BIOS 机器、没装 systemd-boot 也没装 GRUB 时是空的，也算通过。
        for info in &entries {
            assert_eq!(info.module, "bootmgr");
            assert_eq!(info.key, "Boot Manager");
            assert!(!info.value.is_empty());
        }
    }
}
