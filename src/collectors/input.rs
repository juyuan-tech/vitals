//! `/proc/bus/input/devices` 的解析与设备分类，给 keyboard / mouse / gamepad 共用。
//!
//! 文件是按块来的，每块以 `I:` 开头：
//!
//! ```text
//! I: Bus=0011 Vendor=0001 Product=0001 Version=ab83
//! N: Name="AT Translated Set 2 keyboard"
//! P: Phys=isa0060/serio0/input0
//! S: Sysfs=/devices/platform/i8042/serio0/input/input2
//! U: Uniq=
//! H: Handlers=sysrq kbd leds event2 rfkill
//! B: PROP=0
//! B: EV=120013
//! B: KEY=180000 20000 4000000000000020 0 0 10500f02140007 ff803078f900d401 feffffdfffcfffff fffffffffffffffe
//! B: MSC=10
//! ```
//!
//! 两条从真数据里核对出来的规矩（都踩过）：
//!
//! 1. **`B: KEY=` 是位图，而且最后一个字是低位**。内核的 `%*pb` 从高位往低位打，
//!    所以「有没有 KEY_A（第 30 位）」要翻到**最后**那个字去看。本机键盘的最后一个字是
//!    `fffffffffffffffe`（第 0 位 KEY_RESERVED 为空，其余全 1 ✓），电源键的最后一个字是
//!    `0`——只看 `Handlers` 里有没有 `kbd` 是分不开这两者的（电源键也带 `kbd`）。
//! 2. `B: EV=` 是个十六进制数，不是位图列表（本机键盘 `EV=120013` = EV_SYN|EV_KEY|
//!    EV_MSC|EV_LED|EV_REP ✓）。两者格式不同，别用同一个函数解。

use crate::collectors::read;

/// `Bus=`/`Name=` 里设备的名字。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    /// `H: Handlers=` 里那几个名字（`kbd`、`event2`、`js0`…）。
    pub handlers: Vec<String>,
    /// `B: EV=` 的能力位。
    pub ev: u64,
    /// `B: KEY=` 的按键位图。
    pub keys: Bitmap,
}

impl Device {
    /// 这个设备具备某项 EV 能力。
    pub fn has_ev(&self, bit: u32) -> bool {
        self.ev & (1 << bit) != 0
    }
}

/// 位图。**低位在前**：`bits[0]` 装第 0..63 位，与内核打印的顺序相反，
/// 由 [`Bitmap::parse`] 负责翻过来。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bitmap(Vec<u64>);

impl Bitmap {
    /// 解析内核打印的位图：空格分隔的十六进制字，**高位的字在前**。
    ///
    /// 解析失败的字按 0 算（内核加位宽、打印格式变化都不该让整行失效）。
    pub fn parse(field: &str) -> Self {
        let mut words: Vec<u64> = field
            .split_whitespace()
            .map(|word| u64::from_str_radix(word, 16).unwrap_or(0))
            .collect();
        words.reverse();

        Self(words)
    }

    /// 第 `bit` 位是否为 1。
    pub fn has(&self, bit: u32) -> bool {
        let word = (bit / 64) as usize;
        self.0
            .get(word)
            .is_some_and(|value| value & (1 << (bit % 64)) != 0)
    }

    /// 位图里有没有落在 `from..=to` 之间的位。用来问「有没有 BTN_* 这类按键」。
    pub fn has_any_in(&self, from: u32, to: u32) -> bool {
        (from..=to).any(|bit| self.has(bit))
    }
}

// 用到的位。数值取自 `linux/input-event-codes.h`，不是凭印象。
pub const EV_KEY: u32 = 1;
pub const EV_REL: u32 = 2;
pub const EV_ABS: u32 = 3;

/// 键盘上必然有的键：回车、A、Z、空格、左 Shift。
///
/// **不看 `Handlers` 里的 `kbd`**：本机的电源键也带 `kbd`，拿它当键盘会多报一行。
const KEYBOARD_KEYS: [u32; 5] = [
    28, // KEY_ENTER
    30, // KEY_A
    42, // KEY_LEFTSHIFT
    44, // KEY_Z
    57, // KEY_SPACE
];

/// 指针按键：`BTN_LEFT`..`BTN_TASK`（0x110..0x117）。
const BTN_LEFT: u32 = 0x110;
const BTN_TASK: u32 = 0x117;
/// `BTN_TOUCH`（触摸板/触摸屏），0x14a。
const BTN_TOUCH: u32 = 0x14a;
/// `BTN_TOOL_*`（0x140..0x14f）：笔/手指这些「工具」。
const BTN_TOOL: (u32, u32) = (0x140, 0x14f);

/// 解析整个文件。
pub fn parse(text: &str) -> Vec<Device> {
    let mut devices: Vec<Device> = Vec::new();

    for line in text.lines() {
        let Some((kind, rest)) = line.split_once(':') else {
            continue;
        };
        let rest = rest.trim();

        match kind {
            // `I:` 是新块的开始。
            "I" => devices.push(Device {
                name: String::new(),
                handlers: Vec::new(),
                ev: 0,
                keys: Bitmap::default(),
            }),
            "N" => {
                if let Some(device) = devices.last_mut() {
                    device.name = name_value(rest);
                }
            }
            "H" => {
                if let Some(device) = devices.last_mut() {
                    device.handlers = handlers(rest);
                }
            }
            "B" => {
                let Some(device) = devices.last_mut() else {
                    continue;
                };
                let Some((key, value)) = rest.split_once('=') else {
                    continue;
                };
                let value = value.trim();

                match key.trim() {
                    // 这两个格式不同：EV 是一个数，KEY 是位图。
                    "EV" => device.ev = u64::from_str_radix(value, 16).unwrap_or(0),
                    "KEY" => device.keys = Bitmap::parse(value),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // 名字为空的块（解析不完整）丢掉。
    devices.retain(|device| !device.name.is_empty());
    devices
}

/// `Name="AT Translated Set 2 keyboard"` → 去掉外层引号的名字。
fn name_value(rest: &str) -> String {
    let (_, value) = rest.split_once('=').unwrap_or(("", rest));
    let value = value.trim();

    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
        .to_owned()
}

/// `Handlers=sysrq kbd leds event2` → 那几个名字。
fn handlers(rest: &str) -> Vec<String> {
    let (_, value) = rest.split_once('=').unwrap_or(("", rest));

    value.split_whitespace().map(str::to_owned).collect()
}

/// 是不是一块真正的键盘。
///
/// 只看按键位图里有没有真键盘的键（回车、A、Z、空格、左 Shift），
/// **不看 `Handlers` 里的 `kbd`**：本机的电源键与无线接收器都带 `kbd`，
/// 前者只有 `KEY_POWER`，靠位图才分得开。
///
/// 也**不排除指针设备**：本机的 `G3 Mouse Keyboard` 同时带着 `BTN_0` 与整套键盘键，
/// fastfetch 把它算作键盘。键盘与指针是两个互不排斥的问题，真有设备同时带
/// `KEY_A` 与 `BTN_LEFT`，两个模块都报它也好过谁都不报。
pub fn is_keyboard(device: &Device) -> bool {
    device.has_ev(EV_KEY) && KEYBOARD_KEYS.iter().any(|key| device.keys.has(*key))
}

/// 是不是指针设备（鼠标、触摸板、轨迹球）。
///
/// 判据只看**指针那一带**的按钮：`BTN_LEFT`..`BTN_TASK`、`BTN_TOUCH`、`BTN_TOOL_*`。
/// 踩过的坑：一开始把整个 `BTN_MISC`（0x100 起）都当指针键，结果本机的
/// `G3 Mouse Keyboard` 因为带 `BTN_0`（0x100）被误判成鼠标，键盘少报一块。
/// 不靠名字判（名字是驱动给的，写法不固定）：本机触摸板名字里带 `Touchpad`，
/// 但它真正说明身份的是 `BTN_LEFT` 与 `BTN_TOUCH`。
pub fn is_pointer(device: &Device) -> bool {
    device.keys.has_any_in(BTN_LEFT, BTN_TASK)
        || device.keys.has(BTN_TOUCH)
        || device.keys.has_any_in(BTN_TOOL.0, BTN_TOOL.1)
}

/// 从 `/proc/bus/input/devices` 读全部设备。
pub fn devices() -> Result<Vec<Device>, crate::core::collector::CollectError> {
    Ok(read::text("/proc/bus/input/devices")?
        .map(|text| parse(&text))
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机 `/proc/bus/input/devices` 里原文抄下来的三块（电源键、键盘、鼠标接收器）。
    const REAL: &str = "\
I: Bus=0019 Vendor=0000 Product=0001 Version=0000
N: Name=\"Power Button\"
P: Phys=PNP0C0C/button/input0
S: Sysfs=/devices/platform/PNP0C0C:00/input/input0
U: Uniq=
H: Handlers=kbd event0
B: PROP=0
B: EV=3
B: KEY=8000 10000000000000 0

I: Bus=0011 Vendor=0001 Product=0001 Version=ab83
N: Name=\"AT Translated Set 2 keyboard\"
P: Phys=isa0060/serio0/input0
U: Uniq=
H: Handlers=sysrq kbd leds event2 rfkill
B: PROP=0
B: EV=120013
B: KEY=180000 20000 4000000000000020 0 0 10500f02140007 ff803078f900d401 feffffdfffcfffff fffffffffffffffe
B: MSC=10

I: Bus=0005 Vendor=17EF Product=6167 Version=0034
N: Name=\"G3 Mouse Keyboard\"
P: Phys=28:2e:89:11:9c:11
S: Sysfs=/devices/virtual/misc/uhid/0005:17EF:6167.0034/input/input117
U: Uniq=ff:29:02:10:05:42
H: Handlers=sysrq kbd event13
B: PROP=0
B: EV=10001f
B: KEY=3f00733fff 0 0 483ffff17aff32d bfd4444600000000 1 130ff38b17d007 ffff7bfad9415fff ffbeffdfffefffff fffffffffffffffe
B: REL=1040
B: ABS=100000000
B: MSC=10
";

    #[test]
    fn parses_blocks_into_devices() {
        let devices = parse(REAL);

        assert_eq!(devices.len(), 3);
        assert_eq!(devices[0].name, "Power Button");
        assert_eq!(
            devices[1].handlers,
            ["sysrq", "kbd", "leds", "event2", "rfkill"]
        );
        assert_eq!(devices[1].ev, 0x12_0013);
    }

    #[test]
    fn the_last_bitmap_word_holds_the_low_bits() {
        // 键盘 KEY 的最后一字是 fffffffffffffffe → 第 30 位（KEY_A）必须为真。
        let devices = parse(REAL);

        assert!(devices[1].keys.has(30), "KEY_A 该为真");
        assert!(!devices[1].keys.has(0), "第 0 位是 KEY_RESERVED，该为空");
        assert!(devices[1].keys.has(57), "KEY_SPACE 该为真");
    }

    #[test]
    fn a_power_button_is_not_a_keyboard() {
        let devices = parse(REAL);

        // 电源键也有 `kbd`，但按键位图里只有 KEY_POWER（第 116 位）那一档。
        assert!(!is_keyboard(&devices[0]), "电源键不是键盘");
        assert!(is_keyboard(&devices[1]), "真键盘是键盘");
    }

    #[test]
    fn a_wireless_receiver_counts_as_a_keyboard() {
        let devices = parse(REAL);

        // 真实数据推翻过一条想当然的规则：`G3 Mouse Keyboard` 带着 `BTN_0`（0x100）
        // 与 `EV_REL`，一开始按「BTN_MISC 区间里有位就算指针」把它排除掉了，
        // 结果键盘少报一块（fastfetch 报两块）。它的低位字是 `fffffffffffffffe`，
        // 是真键盘；真正的鼠标按键是 `BTN_LEFT`（0x110），它并没有。
        assert!(is_keyboard(&devices[2]), "无线接收器里那份键盘要算键盘");
        assert!(!is_pointer(&devices[2]), "它没有 BTN_LEFT，不算指针");
    }

    #[test]
    fn junk_does_not_panic() {
        assert!(parse("").is_empty());
        assert!(parse("garbage\n").is_empty());
        assert!(parse("I: Bus=1\n").is_empty(), "只有 I: 没有名字的块丢掉");
        assert!(!Bitmap::parse("zzzz").has(0));
        assert!(!Bitmap::parse("").has(0));
    }

    #[test]
    fn reads_this_machines_devices() {
        // 本机一定有输入设备；没有的机器两种结局都通过。
        for device in devices().unwrap() {
            assert!(!device.name.is_empty());
        }
    }
}
