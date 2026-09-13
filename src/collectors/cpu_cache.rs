//! Cache：CPU 各级缓存的大小，以及它被几个核共享。
//!
//! 数据在 `/sys/devices/system/cpu/cpu0/cache/index*/`，每个 `indexN` 是一个缓存层级实例：
//!
//! - `level`：1 到 4
//! - `type`：`Data` / `Instruction` / `Unified`（L1 分数据与指令，所以一个 level 可能两条）
//! - `size`：`32K`、`1024K`（内核写的是 K，不写 M）
//! - `shared_cpu_list`：这个实例被哪些逻辑核共享，`0-1` 或 `0,2,4`
//!
//! **报的是「几个实例 × 每个多大」，不是总容量**：fastfetch 也是这么印的
//! （`8x32.00 KiB (D)` = 8 个核各有一份 32 KiB 的数据缓存）。实例数由
//! `逻辑核总数 ÷ 每个实例共享的核数` 得到：L3 常被所有核共享，算出来就是 1，
//! 那时不写 `1x`——写出来只是噪音。
//!
//! 大小格式我们跟自己的 `units` 走（一位小数、`KiB`/`MiB`），fastfetch 是两位
//! （`32.00 KiB`）；这是刻意的取舍，见 `PLAN.md` §5.6。

use crate::collectors::{read, units};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 0 号核的缓存目录。缓存是共享的，看 0 号核就能把各级都列全。
const CACHE: &str = "/sys/devices/system/cpu/cpu0/cache";

/// 在线逻辑核范围，形如 `0-15`。
const ONLINE: &str = "/sys/devices/system/cpu/online";

/// CPU 缓存。
pub struct CpuCache;

impl Collector for CpuCache {
    fn name(&self) -> &'static str {
        "cpu-cache"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let instances = read_instances(CACHE)?;
        if instances.is_empty() {
            return Ok(Vec::new());
        }

        // 逻辑核总数只用来算「几个实例」，读不到就退化成不写倍数——
        // 宁可少一个 `8x`，也不要瞎猜一个核数。
        let cpus = read::text(ONLINE)?
            .as_deref()
            .map(count_cpus)
            .filter(|cpus| *cpus > 0);

        Ok(group(instances, cpus)
            .into_iter()
            .map(|(level, value)| Info::new(self.name(), format!("CPU Cache (L{level})"), value))
            .collect())
    }
}

/// 一个缓存实例的原始信息。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Instance {
    level: u32,
    /// 缓存的种类字母：`D`（数据）、`I`（指令）、`U`（统一）。
    kind: char,
    bytes: u64,
    /// 这个实例被几个逻辑核共享。
    shared_by: u32,
}

/// 读 `cpu0/cache/index*` 下的全部实例。
fn read_instances(dir: &str) -> Result<Vec<Instance>, CollectError> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(Vec::new());
    };

    // 目录顺序不保证稳定，排序后再读，免得同一台机器两次输出换个顺序。
    let mut paths: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("index"))
        })
        .collect();
    paths.sort();

    let mut instances = Vec::new();
    for path in paths {
        let dir = path.to_string_lossy().into_owned();
        let (Some(level), Some(kind), Some(bytes), Some(shared_by)) = (
            read_number(&dir, "level")?,
            read_text(&dir, "type")?.and_then(|name| kind_letter(&name)),
            read_text(&dir, "size")?.and_then(|size| parse_size(&size)),
            read_text(&dir, "shared_cpu_list")?.map(|list| count_cpus(&list)),
        ) else {
            continue;
        };

        instances.push(Instance {
            level,
            kind,
            bytes,
            shared_by,
        });
    }

    Ok(instances)
}

/// 读一个小整数文件。数字解不出来就当「没有」，但**文件读不了仍然是失败**——
/// 与 `read::text` 的口径一致（属性缺失 ≠ 磁盘错误）。
fn read_number(dir: &str, name: &str) -> Result<Option<u32>, CollectError> {
    Ok(read::text(&format!("{dir}/{name}"))?.and_then(|text| text.trim().parse().ok()))
}

/// 读一个文本文件。某一项缺失就跳过这个实例，不该因为一个属性让整个模块失败。
fn read_text(dir: &str, name: &str) -> Result<Option<String>, CollectError> {
    read::text(&format!("{dir}/{name}"))
}

/// `Data` / `Instruction` / `Unified` → `D` / `I` / `U`。
fn kind_letter(kind: &str) -> Option<char> {
    match kind.trim() {
        "Data" => Some('D'),
        "Instruction" => Some('I'),
        "Unified" => Some('U'),
        _ => None,
    }
}

/// `32K` / `1024K` / `2M` → 字节数。
///
/// 内核现在只写 K，但 M 是合法后缀，顺手认掉。
fn parse_size(size: &str) -> Option<u64> {
    let size = size.trim();
    let (digits, multiplier) = match size.chars().last()? {
        'K' | 'k' => (&size[..size.len() - 1], 1024),
        'M' | 'm' => (&size[..size.len() - 1], 1024 * 1024),
        _ => (size, 1),
    };

    digits.trim().parse::<u64>().ok()?.checked_mul(multiplier)
}

/// 数一个 CPU 列表里有几个核：`0-1` → 2、`0,2,4` → 3、`0-3,6` → 5。
fn count_cpus(list: &str) -> u32 {
    let mut total = 0;

    for part in list.trim().split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        match part.split_once('-') {
            Some((first, last)) => {
                let (Ok(first), Ok(last)) =
                    (first.trim().parse::<u32>(), last.trim().parse::<u32>())
                else {
                    continue;
                };
                if last >= first {
                    total += last - first + 1;
                }
            }
            None => {
                if part.parse::<u32>().is_ok() {
                    total += 1;
                }
            }
        }
    }

    total
}

/// 按层级归并成 `L1 → 8x32.00 KiB (D), 8x32.00 KiB (I)` 这样的值。
///
/// 同一层级里可能既有数据缓存又有指令缓存（x86 的 L1），按 `D`→`I`→`U` 的固定顺序排，
/// 不跟着目录顺序走。
fn group(instances: Vec<Instance>, cpus: Option<u32>) -> Vec<(u32, String)> {
    let mut levels: Vec<(u32, Vec<Instance>)> = Vec::new();

    for instance in instances {
        match levels
            .iter_mut()
            .find(|(level, _)| *level == instance.level)
        {
            Some((_, list)) => list.push(instance),
            None => levels.push((instance.level, vec![instance])),
        }
    }

    levels.sort_by_key(|(level, _)| *level);

    levels
        .into_iter()
        .map(|(level, mut list)| {
            list.sort_by_key(|instance| (instance.kind, instance.bytes));

            let parts: Vec<String> = list
                .iter()
                .map(|instance| {
                    // 每个实例被几个核共享 → 一共几个实例。读不到核数就不写倍数。
                    let instances = cpus
                        .map(|cpus| cpus.div_ceil(instance.shared_by.max(1)))
                        .filter(|count| *count > 1);

                    match instances {
                        Some(count) => {
                            format!(
                                "{count}x{} ({})",
                                units::bytes(instance.bytes),
                                instance.kind
                            )
                        }
                        None => format!("{} ({})", units::bytes(instance.bytes), instance.kind),
                    }
                })
                .collect();

            (level, parts.join(", "))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kernel_sizes() {
        assert_eq!(parse_size("32K"), Some(32 * 1024));
        assert_eq!(parse_size("1024K"), Some(1024 * 1024));
        assert_eq!(parse_size("2M"), Some(2 * 1024 * 1024));
        assert_eq!(parse_size("512"), Some(512));
        assert_eq!(parse_size("32K\n"), Some(32 * 1024), "带换行也要认");
        assert_eq!(parse_size(""), None);
        assert_eq!(parse_size("K"), None);
        assert_eq!(parse_size("abc"), None);
    }

    #[test]
    fn counts_cpus_in_a_list() {
        assert_eq!(count_cpus("0-1"), 2);
        assert_eq!(count_cpus("0-15"), 16);
        assert_eq!(count_cpus("3"), 1);
        assert_eq!(count_cpus("0,2,4"), 3);
        assert_eq!(count_cpus("0-3,6"), 5);
        assert_eq!(count_cpus(""), 0);
        assert_eq!(count_cpus(" 0-1 "), 2);
        assert_eq!(count_cpus("0-"), 0, "半截范围不算");
        assert_eq!(count_cpus("5-3"), 0, "反过来的范围不算");
    }

    #[test]
    fn maps_cache_kinds() {
        assert_eq!(kind_letter("Data"), Some('D'));
        assert_eq!(kind_letter("Instruction"), Some('I'));
        assert_eq!(kind_letter("Unified"), Some('U'));
        assert_eq!(kind_letter("Weird"), None);
    }

    /// 本机形状的一套实例：8 核 16 线程，L1/L2 每核独占，L3 全共享。
    fn instance(level: u32, kind: char, bytes: u64, shared_by: u32) -> Instance {
        Instance {
            level,
            kind,
            bytes,
            shared_by,
        }
    }

    #[test]
    fn renders_levels_with_instance_counts() {
        let instances = vec![
            instance(1, 'D', 32 * 1024, 2),
            instance(1, 'I', 32 * 1024, 2),
            instance(2, 'U', 1024 * 1024, 2),
            instance(3, 'U', 16 * 1024 * 1024, 16),
        ];

        assert_eq!(
            group(instances, Some(16)),
            [
                (1, "8x32.00 KiB (D), 8x32.00 KiB (I)".to_owned()),
                (2, "8x1.00 MiB (U)".to_owned()),
                (3, "16.00 MiB (U)".to_owned()),
            ]
        );
    }

    #[test]
    fn a_shared_cache_does_not_get_a_one_x() {
        // L3 被 16 个核共享 → 1 个实例；写 `1x16.00 MiB` 只是噪音。
        assert_eq!(
            group(vec![instance(3, 'U', 16 * 1024 * 1024, 16)], Some(16)),
            [(3, "16.00 MiB (U)".to_owned())]
        );
    }

    #[test]
    fn without_a_cpu_count_there_is_no_multiplier() {
        // 读不到 online 就不写倍数，也不猜。
        assert_eq!(
            group(vec![instance(1, 'D', 32 * 1024, 2)], None),
            [(1, "32.00 KiB (D)".to_owned())]
        );
    }

    #[test]
    fn data_comes_before_instruction_even_if_the_directory_order_differs() {
        let instances = vec![
            instance(1, 'I', 32 * 1024, 2),
            instance(1, 'D', 32 * 1024, 2),
        ];

        assert_eq!(
            group(instances, Some(4)),
            [(1, "2x32.00 KiB (D), 2x32.00 KiB (I)".to_owned())]
        );
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = CpuCache.collect(&Context::for_tests()).unwrap();

        // 本机有缓存，所以这里应当有；但别的机器可能读不到，两种结局都通过。
        for info in &entries {
            assert_eq!(info.module, "cpu-cache");
            assert!(info.key.starts_with("CPU Cache (L"), "实际是 {}", info.key);
            assert!(
                info.value.ends_with(')'),
                "值里该带缓存种类：{}",
                info.value
            );
        }
    }
}
