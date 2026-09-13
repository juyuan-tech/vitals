//! 具体模块。
//!
//! 每个模块一个文件、一个单元结构体、一个 [`Collector`] 实现。
//! （`x.rs` + 同名目录的规矩见 `PLAN.md` §2.2，`mod.rs` 一律不用。）
//!
//! 依赖方向是单向的：模块依赖 `crate::core`，核心不认识任何模块。
//! 模块之间不互相调用，只共用这里几个无状态工具：
//!
//! - [`read`] / [`env`]：读文件与环境变量，并把「没有」和「失败」分开
//! - [`units`]：字节数与时长的格式化
//! - [`proc_chain`]：顺着父进程链认祖先（Terminal、DE、WM 共用）
//! - [`ini`]：带 section 的配置文件的解析与查找链
//!   （WMTheme、Theme、Icons、Font、Cursor 共用）
//! - [`input`]：`/proc/bus/input/devices` 的解析与设备分类
//!   （Keyboard、以及将来的 Mouse/Gamepad 共用）
//! - [`os_release`] / [`accounts`] / [`dmi`] / [`meminfo`] / [`pkgdb`] / [`power_supply`] / [`tty`]：
//!   被多个模块共用的系统数据源
//!
//! 目标是对标 fastfetch 的模块面（`PLAN.md` §5.4）。加一个模块要动三处：
//! 这里注册、`config::ModuleType` 加变体、`config/default.toml` 视情况收录，
//! `tests/config.rs` 的往返测试会把漏掉的那处指出来。

pub mod accounts;
pub mod battery;
pub mod bios;
pub mod blockdev;
pub mod board;
pub mod bootmgr;
pub mod brightness;
pub mod btrfs;
pub mod camera;
pub mod chassis;
pub mod colors;
pub mod cpu;
pub mod cpu_cache;
pub mod cpu_usage;
pub mod cursor;
pub mod date_time;
pub mod de;
pub mod disk;
pub mod disk_io;
pub mod display;
pub mod dmi;
pub mod dns;
pub mod editor;
pub mod env;
pub mod font;
pub mod gamepad;
pub mod gpu;
pub mod host;
pub mod icons;
pub mod ini;
pub mod init_system;
pub mod input;
pub mod kernel;
pub mod keyboard;
pub mod line_break;
pub mod lm;
pub mod loadavg;
pub mod local_ip;
pub mod locale;
pub mod meminfo;
pub mod memory;
pub mod monitor;
pub mod mouse;
pub mod net_io;
pub mod os;
pub mod os_release;
pub mod packages;
pub mod physical_disk;
pub mod pkgdb;
pub mod power_adapter;
pub mod power_supply;
pub mod proc_chain;
pub mod processes;
pub mod read;
pub mod routing;
pub mod rust;
pub mod separator;
pub mod session;
pub mod shell;
pub mod sound;
pub mod swap;
pub mod terminal;
pub mod terminal_font;
pub mod terminal_size;
pub mod theme;
pub mod title;
pub mod top;
pub mod tpm;
pub mod tty;
pub mod tzif;
pub mod units;
pub mod uptime;
pub mod user;
pub mod users;
pub mod version;
pub mod wifi;
pub mod wm;
pub mod wmtheme;

use crate::core::collector::Collector;

/// 模块注册表。
///
/// 顺序无关紧要：**显示顺序由配置决定**（`Config::modules`），
/// 这里只是「有哪些模块」的名单，调度时按名字线性查找。
/// 模块数量是几十个的量级，建哈希表纯属自找复杂度（`PLAN.md` §3）。
///
/// 全部是单元结构体，没有状态：模块自己的配置块（`format` 之类）进来时，
/// 若真需要再给它们加字段——给结构体加字段不会破坏已有的 `Collector` 实现。
pub const COLLECTORS: &[&dyn Collector] = &[
    &os::Os,
    &host::Host,
    &kernel::Kernel,
    &bios::Bios,
    &board::Board,
    &chassis::Chassis,
    &uptime::Uptime,
    &loadavg::Loadavg,
    &processes::Processes,
    &cpu::Cpu,
    &memory::Memory,
    &swap::Swap,
    &disk::Disk,
    &user::User,
    &shell::Shell,
    &terminal::Terminal,
    &terminal_size::TerminalSize,
    &locale::Locale,
    &editor::Editor,
    &version::Version,
    &init_system::InitSystem,
    &title::Title,
    &separator::Separator,
    &line_break::Break,
    &rust::Rust,
    &battery::Battery,
    &power_adapter::PowerAdapter,
    &brightness::Brightness,
    &dns::Dns,
    &tpm::Tpm,
    &packages::Packages,
    &display::Display,
    &de::Desktop,
    &wm::WindowManager,
    &wmtheme::WmTheme,
    &theme::Theme,
    &icons::Icons,
    &font::Font,
    &cursor::Cursor,
    &gpu::Gpu,
    &terminal_font::TerminalFont,
    &local_ip::LocalIp,
    &users::Users,
    &physical_disk::PhysicalDisk,
    &bootmgr::Bootmgr,
    &sound::Sound,
    &cpu_cache::CpuCache,
    &lm::Lm,
    &btrfs::Btrfs,
    &date_time::DateTime,
    &wifi::Wifi,
    &camera::Camera,
    &keyboard::Keyboard,
    &mouse::Mouse,
    &gamepad::Gamepad,
    &net_io::NetIo,
    &disk_io::DiskIo,
    &cpu_usage::CpuUsage,
    &top::Top,
    &colors::Colors,
    &monitor::Monitor,
];
