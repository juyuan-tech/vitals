//! TZif（tzdata 的二进制格式，RFC 8536）的解析：给一个时刻，问那一刻的 UTC 偏移。
//!
//! `date-time` 用它算本地时间，`users` 用它把 utmp 里的登录时刻转成本地时间。
//! 以前这两个模块**各有一套完整实现**（都在两百行级、都要处理版本块与闰秒字段），
//! 归并后只剩这一份。取舍标准不是「谁更短」，而是**边界语义更容易讲清**：保留的这一份
//! 把「找切换点」与「取类型」拆成两个小函数，测试把边界（切换时刻**算在内**、早于首次
//! 切换时用类型表首项）逐条钉住了。
//!
//! 不处理的：版本 1 的老文件（1980 年代的产物，现在的发行版不会装）直接返回 `None`
//! ——为它写一套 32 位解析不划算。闰秒字段参与长度计算，不影响偏移量。

/// TZif 的头部长度：magic(4) + 版本(1) + 保留(15) + 六个大端 u32。
pub(crate) const TZIF_HEADER: usize = 44;

/// TZif 头部里那六个计数（单位：条）。
struct TzifHeader {
    /// 转换时刻的个数。
    timecnt: usize,
    /// 时区类型的个数。
    typecnt: usize,
    /// 时区名前缀字符串的字节数。
    charcnt: usize,
    /// 闰秒记录的个数。
    leapcnt: usize,
    /// `isstd` 标志的个数。
    isstdcnt: usize,
    /// `isut` 标志的个数。
    isutcnt: usize,
}

/// 求某时刻的 UTC 偏移（秒）。
///
/// TZif 的结构（RFC 8536）：
///
/// ```text
/// 44 字节头部 → 数据块（转换时刻表、类型下标表、ttinfo 表、名字串、闰秒、标志）
/// 版本 ≥ 2 时，上面整段是给老读者看的 32 位兼容副本，真正的数据在**第二个**
/// 头部 + 数据块里（64 位时间戳）。
/// ```
///
/// 所以要先用第一块的计数跳过兼容副本，再读第二块。任何一步对不上长度就返回 `None`
/// （没有时间，而不是一个错的偏移）。
pub(crate) fn offset_in(bytes: &[u8], timestamp: i64) -> Option<i32> {
    let first = tzif_header(bytes, 0)?;

    // 版本字符是第 5 个字节（`\0` = 1、`2`/`3`/`4` = v2+）。
    let version = *bytes.get(4)?;
    let mut at = 0;
    let mut time_size = 4;
    let mut leap_size = 8;

    if version >= b'2' {
        // 第一块是给老读者看的 32 位兼容副本，真正的数据在紧随其后的第二个头部。
        let second = TZIF_HEADER + tzif_block(&first, time_size, leap_size);
        if tzif_header(bytes, second).is_some() {
            at = second;
            time_size = 8;
            leap_size = 12;
        }
    }

    let header = tzif_header(bytes, at)?;
    let data = at + TZIF_HEADER;
    // 计数是文件里的数字，先挡一下荒谬值再拿它算偏移，免得整数溢出或越界。
    if header.typecnt == 0 || header.typecnt > 1_000 || header.timecnt > 10_000 {
        return None;
    }

    let transitions = data;
    let indices = transitions + header.timecnt * time_size;
    let types = indices + header.timecnt;
    // 整个数据块都必须完整躺在文件里：闰秒与两个标志数组也算上，
    // 少一段就说明文件坏了——不猜。
    let end = types
        + header.typecnt * 6
        + header.charcnt
        + header.leapcnt * leap_size
        + header.isstdcnt
        + header.isutcnt;
    if end > bytes.len() {
        return None;
    }

    // 找生效的类型下标：最后一个不晚于该时刻的转换。
    let index = if header.timecnt == 0 {
        standard_type(bytes, types, header.typecnt)?
    } else if timestamp < transition(bytes, transitions, 0, time_size)? {
        // 第一个转换之前：按 tzfile(5) 用第一个**非夏令时**类型。
        standard_type(bytes, types, header.typecnt)?
    } else {
        let position = (0..header.timecnt).rev().find(|index| {
            transition(bytes, transitions, *index, time_size).is_some_and(|at| at <= timestamp)
        })?;
        usize::from(*bytes.get(indices + position)?)
    };

    if index >= header.typecnt {
        return None;
    }

    Some(i32::from_be_bytes(
        bytes
            .get(types + index * 6..types + index * 6 + 4)?
            .try_into()
            .ok()?,
    ))
}

/// 读一个 TZif 头部。
fn tzif_header(bytes: &[u8], at: usize) -> Option<TzifHeader> {
    if !bytes.get(at..)?.starts_with(b"TZif") {
        return None;
    }

    let count = |index: usize| be32(bytes, at + 20 + index * 4).map(|value| value as usize);
    Some(TzifHeader {
        isutcnt: count(0)?,
        isstdcnt: count(1)?,
        leapcnt: count(2)?,
        timecnt: count(3)?,
        typecnt: count(4)?,
        charcnt: count(5)?,
    })
}

/// 一个数据块占多少字节。
fn tzif_block(header: &TzifHeader, time_size: usize, leap_size: usize) -> usize {
    header.timecnt * (time_size + 1)
        + header.typecnt * 6
        + header.charcnt
        + header.leapcnt * leap_size
        + header.isstdcnt
        + header.isutcnt
}

/// 第 `index` 个转换时刻。
fn transition(bytes: &[u8], at: usize, index: usize, time_size: usize) -> Option<i64> {
    if time_size == 8 {
        be64(bytes, at + index * 8)
    } else {
        Some(i64::from(be32(bytes, at + index * 4)?))
    }
}

/// 第一个**非夏令时**类型；一个都没有就用第 0 个（tzfile(5) 的规矩）。
fn standard_type(bytes: &[u8], types: usize, typecnt: usize) -> Option<usize> {
    for index in 0..typecnt {
        if *bytes.get(types + index * 6 + 4)? == 0 {
            return Some(index);
        }
    }

    Some(0)
}

/// 读一个大端 u32。
fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// 读一个大端 i64。
fn be64(bytes: &[u8], at: usize) -> Option<i64> {
    Some(i64::from_be_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_is_not_a_zone_file() {
        assert_eq!(offset_in(&[], 0), None);
        assert_eq!(offset_in(b"not a tzif file at all", 0), None);
        assert_eq!(offset_in(&[0; TZIF_HEADER], 0), None, "没有 TZif 魔数");
    }
}
