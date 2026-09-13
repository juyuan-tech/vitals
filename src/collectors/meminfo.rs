//! `/proc/meminfo`：内存与 swap 的用量。
//!
//! Memory 和 Swap 两个模块都读它，所以解析放在这里一处。
//! 单位一律是 kB（内核从不改这个），但仍按「取第一个数字字段」来读，
//! 少了单位也不至于读崩。

use crate::collectors::read;
use crate::core::collector::CollectError;

/// 数据源。
const PATH: &str = "/proc/meminfo";

/// `/proc/meminfo` 里我们关心的几行，已经换算成字节。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Meminfo {
    /// 物理内存总量。
    pub total: u64,
    /// 完全空闲的内存。
    pub free: u64,
    /// 内核估计的可用量。
    pub available: Option<u64>,
    /// 块设备缓冲。
    pub buffers: u64,
    /// 页缓存。
    pub cached: u64,
    /// 交换空间总量。
    pub swap_total: u64,
    /// 交换空间空闲量。
    pub swap_free: u64,
}

impl Meminfo {
    /// 读一次。
    ///
    /// 文件不存在（非 Linux）返回全零，由调用方判断算不算「无数据」。
    pub fn read() -> Result<Self, CollectError> {
        Ok(read::text(PATH)?
            .as_deref()
            .map_or_else(Self::default, parse))
    }

    /// 可用内存的估算值。
    ///
    /// `MemAvailable` 是内核估的「还能给新程序用多少」，比 `MemFree` 有意义得多
    /// ——后者不含页缓存，看着总像内存快满了。3.14 之前的内核没有它，
    /// 退回 `free + buffers + cached`，也就是老 `free` 的算法。
    #[must_use]
    pub fn available(&self) -> u64 {
        self.available.unwrap_or_else(|| {
            self.free
                .saturating_add(self.buffers)
                .saturating_add(self.cached)
        })
    }

    /// 「已用」和 `free` 一个口径：`MemTotal - MemAvailable`。
    #[must_use]
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available())
    }

    /// 交换空间已用。
    #[must_use]
    pub fn swap_used(&self) -> u64 {
        self.swap_total.saturating_sub(self.swap_free)
    }
}

/// 解析 `/proc/meminfo`。
///
/// 每行形如 `MemTotal:       32140260 kB`。
fn parse(text: &str) -> Meminfo {
    let mut mem = Meminfo::default();

    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let Some(value) = rest
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        let bytes = value.saturating_mul(1024);

        match key.trim() {
            "MemTotal" => mem.total = bytes,
            "MemFree" => mem.free = bytes,
            "MemAvailable" => mem.available = Some(bytes),
            "Buffers" => mem.buffers = bytes,
            // 注意是精确匹配：SwapCached 不该被算进页缓存。
            "Cached" => mem.cached = bytes,
            "SwapTotal" => mem.swap_total = bytes,
            "SwapFree" => mem.swap_free = bytes,
            _ => {}
        }
    }

    mem
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_lines_we_care_about() {
        let mem = parse(
            "MemTotal:       32140260 kB\n\
             MemFree:         1234567 kB\n\
             MemAvailable:   12345678 kB\n\
             Buffers:          123456 kB\n\
             Cached:          3456789 kB\n",
        );

        assert_eq!(mem.total, 32_140_260 * 1024);
        assert_eq!(mem.available, Some(12_345_678 * 1024));
        assert_eq!(mem.buffers, 123_456 * 1024);
        assert_eq!(mem.cached, 3_456_789 * 1024);
        assert_eq!(mem.used(), (32_140_260 - 12_345_678) * 1024);
    }

    #[test]
    fn parses_swap() {
        let mem = parse("SwapTotal:       8388604 kB\nSwapFree:        7340032 kB\n");

        assert_eq!(mem.swap_total, 8_388_604 * 1024);
        assert_eq!(mem.swap_free, 7_340_032 * 1024);
        assert_eq!(mem.swap_used(), (8_388_604 - 7_340_032) * 1024);
    }

    #[test]
    fn no_swap_is_zero_not_a_failure() {
        let mem = parse("MemTotal: 1000 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n");

        assert_eq!(mem.swap_total, 0);
        assert_eq!(mem.swap_used(), 0);
    }

    #[test]
    fn available_is_what_the_kernel_says() {
        let mem = parse("MemTotal: 1000 kB\nMemFree: 100 kB\nMemAvailable: 800 kB\n");
        assert_eq!(mem.available(), 800 * 1024);
        assert_eq!(mem.used(), 200 * 1024);
    }

    #[test]
    fn swap_cached_is_not_mistaken_for_cached() {
        let mem = parse("Cached: 1000 kB\nSwapCached: 9999 kB\n");
        assert_eq!(mem.cached, 1000 * 1024);
    }

    #[test]
    fn old_kernels_without_mem_available_fall_back() {
        let mem = parse("MemTotal: 1000 kB\nMemFree: 100 kB\nBuffers: 50 kB\nCached: 200 kB\n");

        assert_eq!(mem.available, None);
        assert_eq!(mem.available(), (100 + 50 + 200) * 1024);
        assert_eq!(mem.used(), (1000 - 350) * 1024);
    }

    #[test]
    fn a_broken_file_yields_a_zero_total() {
        let mem = parse("这不是 meminfo\nMemTotal: 不确定 kB\n");
        assert_eq!(mem.total, 0);
    }

    #[test]
    fn reads_this_machine() {
        let mem = Meminfo::read().expect("/proc/meminfo 该读得到");

        assert!(mem.total > 0, "物理内存总量不该是 0");
        assert!(mem.used() <= mem.total, "已用不该超过总量");
    }
}
