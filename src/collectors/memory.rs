//! Memory：内存用量。数据来自 `/proc/meminfo`。

use crate::collectors::{read, units};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 数据源。
const PATH: &str = "/proc/meminfo";

/// 内存。
pub struct Memory;

/// `/proc/meminfo` 里我们关心的几行，已经换算成字节。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Meminfo {
    total: u64,
    free: u64,
    available: Option<u64>,
    buffers: u64,
    cached: u64,
}

impl Meminfo {
    /// 可用的估算值。
    ///
    /// `MemAvailable` 是内核估的「还能给新程序用多少」，比 `MemFree` 有意义得多
    /// ——后者不含页缓存，看着总像内存快满了。3.14 之前的内核没有它，
    /// 退回 `free + buffers + cached`，也就是老 `free` 的算法。
    fn available(&self) -> u64 {
        self.available.unwrap_or_else(|| {
            self.free
                .saturating_add(self.buffers)
                .saturating_add(self.cached)
        })
    }

    /// 「已用」和 `free` 一个口径：`MemTotal - MemAvailable`。
    fn used(&self) -> u64 {
        self.total.saturating_sub(self.available())
    }
}

/// 解析 `/proc/meminfo`。
///
/// 每行形如 `MemTotal:       32140260 kB`。单位一律是 kB（内核从不改这个），
/// 但我们仍按「取第一个数字字段」来读，少了单位也不至于读崩。
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
            _ => {}
        }
    }

    mem
}

impl Collector for Memory {
    fn name(&self) -> &'static str {
        "memory"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(text) = read::text(PATH)? else {
            return Ok(Vec::new());
        };

        let mem = parse(&text);
        if mem.total == 0 {
            // 文件在但连总量都读不出来，属于无数据。
            return Ok(Vec::new());
        }

        let used = mem.used();
        let total = mem.total;
        let percent = units::percent(used, total);
        let value = format!(
            "{} / {} ({percent}%)",
            units::bytes(used),
            units::bytes(total)
        );

        Ok(vec![
            Info::new(self.name(), "Memory", value)
                .with_variable("used_bytes", used.to_string())
                .with_variable("total_bytes", total.to_string())
                .with_variable("available_bytes", mem.available().to_string())
                .with_variable("percent", percent.to_string()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 实测本机 `/proc/meminfo` 的开头几行。
    const FIXTURE: &str = "\
MemTotal:       32140260 kB
MemFree:         6326628 kB
MemAvailable:   17917828 kB
Buffers:            3196 kB
Cached:         10791820 kB
SwapCached:            0 kB
SwapTotal:       8388604 kB
";

    #[test]
    fn parses_kilobytes_into_bytes() {
        let mem = parse(FIXTURE);

        assert_eq!(mem.total, 32_140_260 * 1024);
        assert_eq!(mem.available, Some(17_917_828 * 1024));
        assert_eq!(mem.free, 6_326_628 * 1024);
        assert_eq!(mem.buffers, 3_196 * 1024);
        assert_eq!(mem.cached, 10_791_820 * 1024);
    }

    #[test]
    fn used_follows_the_free_convention() {
        let mem = parse(FIXTURE);

        // MemTotal - MemAvailable，不是 MemTotal - MemFree。
        let expected = (32_140_260 - 17_917_828) * 1024;
        assert_eq!(mem.used(), expected);
        assert_ne!(
            mem.used(),
            (32_140_260 - 6_326_628) * 1024,
            "用 MemFree 算会虚高，那不是我们要的口径"
        );
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
    fn collects_on_this_machine() {
        let entries = Memory.collect(&Context::for_tests()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "Memory");
        assert!(
            entries[0].value.contains(" / "),
            "该是「已用 / 总量」的形状"
        );
        assert!(entries[0].value.ends_with("%)"));
    }
}
