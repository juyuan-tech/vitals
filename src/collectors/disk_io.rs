//! DiskIO：物理盘的读写速率。
//!
//! ## 为什么要采两次
//!
//! `/sys/block/<盘>/stat` 里的扇区计数是**开机至今的累计值**，读一次只能知道
//! 「一共读了多少扇区」。速率只能靠两次采样作差，和 `net_io` 同一个套路。
//!
//! ## 代价
//!
//! 两次采样之间睡 [`SAMPLE_WINDOW`]（200 ms），顺序执行的调度器会为此整体慢
//! 200 ms。完整理由（upstream 的 500 ms 与它的准备阶段）见 `net_io.rs` 的模块文档。
//!
//! ## 只报「有 `device` 链接」的盘
//!
//! upstream 的 `diskio_linux.c` 一进门就是
//! `openat(dfd, "device", ...)`，失败就 `return "virtual device"` 直接跳过。
//! 本机 `/sys/block` 里的 `zram0` 正没有这个链接，所以 fastfetch 的 DiskIO
//! 只印 Kingston 与 SAMSUNG 两张，**没有 zram0**——这一点和 `physical_disk`
//! 模块（默认把虚拟盘也算进来）**不一样**，两个模块的盘数不同不是 bug。
//!
//! ## 数据从 `/sys/block` 还是 `/proc/diskstats`
//!
//! upstream 用 `/sys/block/<盘>/stat`，于是**只统计整盘、不统计分区**。
//! 本机 `/proc/diskstats` 里还多出 `nvme0n1p1`、`nvme0n1p2`、`nvme0n1p3`、`sda1`
//! 四个分区，而 `/sys/block` 里只有 `nvme0n1`、`sda`、`zram0` 三个整盘
//! ——这正是 fastfetch 的输出里没有分区的原因，我们照它的来源走。
//!
//! ## 盘名从哪来
//!
//! 与 `physical_disk.rs` 同一套规则：`vendor`（非空就加一个空格）+ `model`，
//! 都空则退回设备名。upstream 自己在 `physicaldisk_linux.c` 与 `diskio_linux.c`
//! 里也是各写一遍，所以这里没有抽公共件——但两处将来必须一起改。

use std::time::{Duration, Instant};

use crate::collectors::blockdev;
use crate::collectors::{read, units};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 两次采样之间的固定窗口。见模块文档里的「代价」。
pub const SAMPLE_WINDOW: Duration = Duration::from_millis(200);

/// 块设备目录。
const SYS_BLOCK: &str = "/sys/block";

/// 物理盘读写速率。
pub struct DiskIo;

impl Collector for DiskIo {
    fn name(&self) -> &'static str {
        "disk-io"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(before) = scan()? else {
            // 没有 /sys/block（非 Linux、容器里没挂）：无数据。
            return Ok(Vec::new());
        };
        if before.is_empty() {
            return Ok(Vec::new());
        }

        let started = Instant::now();
        sleep_until(started, SAMPLE_WINDOW);
        // 与 upstream 同一时点：先量间隔，再读第二次。
        let elapsed = started.elapsed();

        let after = scan()?.unwrap_or_default();
        let elapsed_ms = elapsed.as_millis() as u64;
        if elapsed_ms == 0 {
            return Err(CollectError::new(
                "disk-io 的采样间隔是 0 毫秒，算不出每秒速率",
            ));
        }

        // upstream 在这里比对两次的盘数，数量变了就报
        // "Different number of physical disks. Hardware change?"。
        // 我们比它多走一步：把变的是哪一个也说清楚。
        if let Some(changed) = set_difference(&before, &after) {
            return Err(CollectError::new(format!(
                "采样窗口里物理盘集合变了（{changed}），这次算不出真实速率"
            )));
        }

        let mut entries: Vec<(String, Info)> = Vec::with_capacity(before.len());
        for old in &before {
            let Some(new) = after.iter().find(|disk| disk.dev == old.dev) else {
                continue; // 上面已经拦过，这里只是把「找不到」写全
            };

            let Some(read_rate) = rate(old.read_sectors, new.read_sectors, elapsed_ms) else {
                return Err(CollectError::new(format!(
                    "{} 的读扇区数在采样窗口里回退了（盘被重置？），这次算不出真实速率",
                    old.dev
                )));
            };
            let Some(write_rate) = rate(old.write_sectors, new.write_sectors, elapsed_ms) else {
                return Err(CollectError::new(format!(
                    "{} 的写扇区数在采样窗口里回退了（盘被重置？），这次算不出真实速率",
                    old.dev
                )));
            };

            entries.push((
                old.name.clone(),
                describe(
                    self.name(),
                    &old.name,
                    &old.dev,
                    new.read_ios,
                    new.write_ios,
                    read_rate,
                    write_rate,
                ),
            ));
        }

        // upstream 的 `sortDevices` 就是按**盘名**比
        // （`ffStrbufComp(&left->name, &right->name)`），不是按设备名，
        // 所以本机的顺序是 Kingston → SAMSUNG（U 盘排在 NVMe 前面）。
        entries.sort_by(|left, right| left.0.cmp(&right.0));

        Ok(entries.into_iter().map(|(_, info)| info).collect())
    }
}

/// 一张盘在一次采样里的样子。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Disk {
    /// 内核设备名，例如 `nvme0n1`。
    dev: String,
    /// 给人看的名字，例如 `SAMSUNG MZVL21T0HCLR-00BH1`。
    name: String,
    read_ios: u64,
    read_sectors: u64,
    write_ios: u64,
    write_sectors: u64,
}

/// `/sys/block/<盘>/stat` 的字段。
///
/// 内核的字段顺序（`Documentation/block/stat.rst`）：
///
/// ```text
/// 1 read I/Os  2 read merges  3 read sectors  4 read ticks
/// 5 write I/Os 6 write merges 7 write sectors 8 write ticks ...
/// ```
///
/// upstream 的 `sscanf("%lu%*u%lu%*u%lu%*u%lu%*u")` 正好取第 1、3、5、7 个，
/// 跳过的四个是合并次数与耗时（tick）——本模块也一样，不需要它们。
fn parse_stat(text: &str) -> Option<(u64, u64, u64, u64)> {
    let fields: Vec<&str> = text.split_whitespace().collect();
    let pick = |index: usize| fields.get(index)?.parse::<u64>().ok();

    Some((pick(0)?, pick(2)?, pick(4)?, pick(6)?))
}

/// 扫一遍 `/sys/block`。
///
/// - 目录不存在 → `Ok(None)`（无数据）
/// - 目录在但读不了 → `Err`（真失败）
fn scan() -> Result<Option<Vec<Disk>>, CollectError> {
    let entries = match std::fs::read_dir(SYS_BLOCK) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(CollectError::caused_by(
                format!("读取 {SYS_BLOCK} 失败"),
                source,
            ));
        }
    };

    let mut disks = Vec::new();
    for entry in entries.flatten() {
        let dev = entry.file_name().to_string_lossy().into_owned();
        if dev.starts_with('.') {
            continue;
        }

        let dir = entry.path();
        if !blockdev::is_physical(&dir)? {
            continue; // 虚拟盘：upstream 在这里 return "virtual device"
        }

        let Some(text) = read::text(&format!("{SYS_BLOCK}/{dev}/stat"))? else {
            continue; // stat 都没有就不是能测的盘
        };
        let Some((read_ios, read_sectors, write_ios, write_sectors)) = parse_stat(&text) else {
            return Err(CollectError::new(format!(
                "{SYS_BLOCK}/{dev}/stat 的字段不是一个合法的块设备 stat（原文 `{text}`）"
            )));
        };

        disks.push(Disk {
            name: blockdev::name_from_sysfs(&dir, &dev)?,
            dev,
            read_ios,
            read_sectors,
            write_ios,
            write_sectors,
        });
    }

    Ok(Some(disks))
}

/// 找出两次采样之间盘集合的差异，返回一句人话。
///
/// 顺序也要一致：`/sys/block` 的读取顺序理论上稳定，但不依赖它——这里按设备名
/// 配对，只有「多出来」或「少掉了」才算变化。
fn set_difference(before: &[Disk], after: &[Disk]) -> Option<String> {
    let has = |disks: &[Disk], dev: &str| disks.iter().any(|disk| disk.dev == dev);

    if let Some(missing) = before
        .iter()
        .find(|disk| !has(after, &disk.dev))
        .map(|disk| disk.dev.clone())
    {
        return Some(format!("少了 {missing}"));
    }

    after
        .iter()
        .find(|disk| !has(before, &disk.dev))
        .map(|disk| format!("多了 {}", disk.dev))
}

/// `(后 - 前) * 1000 / 间隔毫秒`，扇区数先乘 512 变成字节。
///
/// 与 upstream 的 `sectorRead * 512` 和
/// `(*currValue - *prevValue) * 1000 / (time2 - time1)` 同一套算术（含整数截断）。
fn rate(before: u64, after: u64, elapsed_ms: u64) -> Option<u64> {
    if elapsed_ms == 0 {
        return None;
    }

    let delta = after.checked_sub(before)?;
    Some(delta * 512 * 1000 / elapsed_ms)
}

/// 组装那一条信息。
///
/// upstream 的格式（`modules/diskio/diskio.c` 里 `detectTotal == false` 的那一支）：
/// `"<读>/s (R) - <写>/s (W)"`，键是 `"Disk I/O (<盘名>)"`。
fn describe(
    module: &'static str,
    name: &str,
    dev: &str,
    read_ios: u64,
    write_ios: u64,
    read_rate: u64,
    write_rate: u64,
) -> Info {
    Info::new(
        module,
        format!("Disk I/O ({name})"),
        format!(
            "{}/s (R) - {}/s (W)",
            units::bytes(read_rate),
            units::bytes(write_rate)
        ),
    )
    .with_variable("name", name.to_owned())
    .with_variable("dev-path", format!("/dev/{dev}"))
    .with_variable("bytes-read", read_rate.to_string())
    .with_variable("bytes-written", write_rate.to_string())
    .with_variable("read-count", read_ios.to_string())
    .with_variable("write-count", write_ios.to_string())
}

/// 睡到距 `started` 满 `window` 为止。理由见 `net_io.rs` 里的同名函数。
fn sleep_until(started: Instant, window: Duration) {
    if let Some(rest) = window.checked_sub(started.elapsed()) {
        std::thread::sleep(rest);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use std::path::Path;

    /// 本机 `/sys/block/nvme0n1/stat` 的原文（一次真实读取）。
    const NVME_STAT: &str = "  327968    17681 28335708   175807  3239286    80872 150141953  9344391        0  1403432 10062508   109487        0 675073032   293372   115182   248936";

    /// 本机 `/sys/block/sda/stat` 的原文。
    const SDA_STAT: &str = "     164     1909     9892      318        2        2        4       92        0      376      410        0        0        0        0        0        0";

    /// 本机 `/sys/block/zram0/stat` 的原文。
    const ZRAM_STAT: &str = "  141867        0  1138184      826   355523        0  4651080    12125        0    22650    12951        0        0        0        0        0        0";

    #[test]
    fn takes_fields_one_three_five_seven() {
        // 第 1 个字段是读 IO 数，第 3 个是读扇区数，第 5 个是写 IO 数，
        // 第 7 个是写扇区数；中间的合并次数与 tick 都跳过（和 upstream 的 %*u 一致）。
        assert_eq!(
            parse_stat(NVME_STAT),
            Some((327_968, 28_335_708, 3_239_286, 150_141_953))
        );
        assert_eq!(parse_stat(SDA_STAT), Some((164, 9_892, 2, 4)));
        assert_eq!(
            parse_stat(ZRAM_STAT),
            Some((141_867, 1_138_184, 355_523, 4_651_080))
        );
    }

    #[test]
    fn a_short_stat_file_is_not_parsable() {
        // 少字段就不猜：宁可说不出来，也不把别人的字段当扇区数。
        assert_eq!(parse_stat(""), None);
        assert_eq!(parse_stat("1 2 3"), None);
        assert_eq!(parse_stat("1 2 3 4 5 6"), None);
    }

    #[test]
    fn the_rate_is_sectors_times_512_over_the_window() {
        // 本机实测：zram0 的 size 是 32139264 个 512 字节扇区、disksize 是
        // 16455303168 字节，正好 ×512，所以扇区到字节就是乘 512。
        // 200 ms 里多读 1000 个扇区 → 1000*512*1000/200 = 2_560_000 B/s。
        assert_eq!(rate(0, 1_000, 200), Some(2_560_000));
        // 没动就是 0，不是错误。
        assert_eq!(rate(9, 9, 200), Some(0));
        // 计数回退 / 间隔为 0 → 算不出来。
        assert_eq!(rate(5, 4, 200), None);
        assert_eq!(rate(0, 1, 0), None);
    }

    #[test]
    fn the_value_is_formatted_like_upstream() {
        // 用本机实测过的量级：fastfetch 印过 "728.00 KiB/s (W)"
        // （728 * 1024 = 745472 B/s ≈ 1456 个扇区 / 200 ms）。
        let info = describe(
            "disk-io",
            "Kingston DataTraveler 3.0",
            "sda",
            164,
            2,
            0,
            745_472,
        );

        assert_eq!(info.value, "0 B/s (R) - 728.00 KiB/s (W)");
        assert_eq!(info.key, "Disk I/O (Kingston DataTraveler 3.0)");
        assert_eq!(info.module, "disk-io");
        assert_eq!(info.variable("dev-path"), Some("/dev/sda"));
        assert_eq!(info.variable("read-count"), Some("164"));
    }

    /// 自造一份 `/sys/block/<dev>/device/` 的样子，别去读宿主机的盘——
    /// 型号是别人的，断言固定型号就会在别的机器上红（CI 上红过一次）。
    fn write_field(dir: &std::path::Path, field: &str, text: &str) {
        let device = dir.join("device");
        std::fs::create_dir_all(&device).unwrap();
        std::fs::write(device.join(field), text).unwrap();
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vitals-blockdev-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_nvme_name_falls_back_to_the_model() {
        let root = temp_dir("nvme-name");

        // 只有 model、没有 vendor：用型号（原文可能带尾随空白，要去掉）。
        let only_model = root.join("only-model");
        write_field(&only_model, "model", "SAMSUNG MZVL21T0HCLR-00BH1   \n");
        let name = blockdev::name_from_sysfs(&only_model, "nvme0n1").unwrap();
        assert_eq!(name, "SAMSUNG MZVL21T0HCLR-00BH1");
        assert!(!name.ends_with(' '), "{name:?}");

        // vendor + model：厂商拼在前面。
        let both = root.join("both");
        write_field(&both, "vendor", "ACME\n");
        write_field(&both, "model", "Fast Disk\n");
        assert_eq!(
            blockdev::name_from_sysfs(&both, "nvme0n1").unwrap(),
            "ACME Fast Disk"
        );

        // 只有 vendor。
        let only_vendor = root.join("only-vendor");
        write_field(&only_vendor, "vendor", "ACME\n");
        assert_eq!(
            blockdev::name_from_sysfs(&only_vendor, "nvme0n1").unwrap(),
            "ACME"
        );

        // 两个文件都没有（虚拟盘）：退回设备名。
        let neither = root.join("neither");
        std::fs::create_dir_all(neither.join("device")).unwrap();
        assert_eq!(
            blockdev::name_from_sysfs(&neither, "zram0").unwrap(),
            "zram0"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_virtual_disk_has_no_device_link() {
        // 这条就是 upstream 的 "virtual device" 判据。
        // 本机 /sys/block/zram0/device 不存在（虚拟盘从来不挂 device），
        // /sys/block/nvme0n1/device 是指向 PCI 设备的符号链接。
        // 没有 device 子项 → 假。自造一个，不赌宿主机的虚拟盘叫不叫 zram0。
        let root = temp_dir("is-physical");
        std::fs::create_dir_all(root.join("virtual")).unwrap();
        assert!(!blockdev::is_physical(&root.join("virtual")).unwrap());
        std::fs::remove_dir_all(&root).ok();

        // 别的机器上未必有 nvme0n1，所以先看路径在不在再断言——
        // 这个测试在没有它的机器上也要绿。
        let nvme = Path::new("/sys/block/nvme0n1");
        if nvme.join("device").exists() {
            assert!(blockdev::is_physical(nvme).unwrap());
        }

        // 目录在、但没有 device 子项 → 假。
        assert!(!blockdev::is_physical(Path::new("/sys/block")).unwrap());
    }

    #[test]
    fn a_changed_disk_set_is_reported() {
        let disk = |dev: &str| Disk {
            dev: dev.to_owned(),
            name: dev.to_owned(),
            read_ios: 0,
            read_sectors: 0,
            write_ios: 0,
            write_sectors: 0,
        };

        let before = vec![disk("sda"), disk("nvme0n1")];
        let after = vec![disk("sda")];
        assert_eq!(
            set_difference(&before, &after).as_deref(),
            Some("少了 nvme0n1")
        );

        let after = vec![disk("sda"), disk("nvme0n1"), disk("sdb")];
        assert_eq!(set_difference(&before, &after).as_deref(), Some("多了 sdb"));

        assert_eq!(set_difference(&before, &before), None);
    }

    #[test]
    fn collects_on_this_machine() {
        // 会真的睡 200 ms —— 本模块的既定代价。
        let entries = DiskIo.collect(&Context::for_tests()).unwrap();

        // 没有物理盘（容器里只有 zram/loop）就是空的，也算通过。
        for info in &entries {
            assert_eq!(info.module, "disk-io");
            assert!(
                info.key.starts_with("Disk I/O (") && info.key.ends_with(')'),
                "实际是 {}",
                info.key
            );
            assert!(info.value.contains("/s (R) - "), "{}", info.value);
            assert!(info.value.ends_with("/s (W)"), "{}", info.value);
        }

        // 虚拟盘（zram0）不该出现在这里——它没有 device 链接。
        for info in &entries {
            assert!(!info.key.contains("(zram"), "{}", info.key);
        }
    }
}
