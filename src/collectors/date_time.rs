//! DateTime：当前本地时间。
//!
//! 看着最简单的一行，但「本地」两个字要自己算：标准库只给 UTC（`SystemTime`），
//! 拿本地时间要么调 libc 的 `localtime`（不许 unsafe），要么自己读时区库。这里是后者。
//!
//! 时区来源按顺序：`$TZ` 指到 `$TZDIR`（默认 `/usr/share/zoneinfo`）下的那个文件，
//! 否则 `/etc/localtime`（通常是指向时区文件的符号链接，也可能是副本）。
//!
//! TZif 是二进制格式（RFC 8536），文件里是：若干次**切换**（unix 秒）+
//! 切换后的类型下标 + 类型表（UTC 偏移、是否夏令时、缩写）。
//! 版本 2 以上文件里有**两份**数据块——第一份是 32 位切换时间（1980 年前后兼容用），
//! 第二份才是 64 位；要读第二份。**尾部的 POSIX TZ 字符串不读**：那是 2038 年之后
//! 没有预置切换表时的规则，而「现在」几乎总在表内。真读不到就退回 UTC 并在 variables 里
//! 说明，绝不瞎猜一个偏移。
//!
//! 日期换算用 Howard Hinnant 的 `civil_from_days`（把「距 1970-01-01 的天数」变成年月日），
//! 全整数运算，没有闰年特判的坑。

use std::time::{SystemTime, UNIX_EPOCH};

use crate::collectors::{env, read};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 时区文件默认目录。
const ZONEINFO: &str = "/usr/share/zoneinfo";

/// 本机时区（通常是符号链接）。
const LOCALTIME: &str = "/etc/localtime";

/// 秒/天。
const DAY: i64 = 86_400;

/// DateTime。
pub struct DateTime;

impl Collector for DateTime {
    fn name(&self) -> &'static str {
        "datetime"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(since) => since.as_secs() as i64,
            // 系统时间早于 1970 年（时钟没设对）。这不是采集失败，是时钟的事，
            // 报出来比装作没事好。
            Err(_) => 0,
        };

        let zone = locate_zone()?;
        let (name, offset) = match zone.as_ref() {
            Some((name, path)) => match offset_at(path, now)? {
                Some(offset) => (name.clone(), offset),
                None => (format!("{name} (读不了，按 UTC)"), 0),
            },
            None => ("UTC".to_owned(), 0),
        };

        let info = Info::new(
            self.name(),
            "Date & Time",
            format_local(now + i64::from(offset)),
        )
        .with_variable("timezone", name)
        .with_variable("offset", describe_offset(offset));

        Ok(vec![info])
    }
}

/// 秒数（unix 秒 + 偏移）格式化成 `2026-09-13 16:10:51`。
fn format_local(seconds: i64) -> String {
    // 负数时间要向下取整，Rust 的 `/` 是向零取整，所以自己算。
    let days = seconds.div_euclid(DAY);
    let rest = seconds.rem_euclid(DAY);

    let (year, month, day) = civil_from_days(days);
    let hour = rest / 3600;
    let minute = rest % 3600 / 60;
    let second = rest % 60;

    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
}

/// 距 1970-01-01 的天数 → `(年, 月, 日)`。
///
/// Howard Hinnant 的算法：先把 3 月当一年的开头（这样闰日落在年末，不用特判），
/// 用 400 年一轮的整除把世纪闰年规则摊平。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;

    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    let year = if month <= 2 { year + 1 } else { year };

    (year, month, day)
}

/// 偏移写成 `+08:00`。
fn describe_offset(offset: i32) -> String {
    let sign = if offset < 0 { '-' } else { '+' };
    let offset = offset.unsigned_abs();

    format!("{sign}{:02}:{:02}", offset / 3600, offset % 3600 / 60)
}

/// 本机的时区：`(名字, 文件路径)`。
///
/// `$TZ` 优先（临时改时区就靠它）。`$TZ` 是 `:Asia/Shanghai` 这种带冒号的写法也认——
/// 那是 POSIX 的历史包袱，冒号可以省略。
fn locate_zone() -> Result<Option<(String, String)>, CollectError> {
    if let Some(tz) = env::var("TZ") {
        let tz = tz.trim().trim_start_matches(':').to_owned();
        // `TZ=UTC0` 这类 POSIX 字符串不是文件路径，认不出就当没有（退回 UTC）。
        if !tz.is_empty() && !tz.starts_with('/') {
            let dir = env::var("TZDIR").unwrap_or_else(|| ZONEINFO.to_owned());
            let path = format!("{dir}/{tz}");
            if std::path::Path::new(&path).exists() {
                return Ok(Some((tz, path)));
            }
        } else if tz.starts_with('/') && std::path::Path::new(&tz).exists() {
            return Ok(Some((tz.clone(), tz)));
        }
    }

    // `/etc/localtime` 是指向时区文件的符号链接时，链接目标就带着时区名字。
    if let Ok(target) = std::fs::read_link(LOCALTIME) {
        let name = target
            .to_string_lossy()
            .split_once("/zoneinfo/")
            .map(|(_, name)| name.to_owned());
        if let Some(name) = name {
            return Ok(Some((name, LOCALTIME.to_owned())));
        }
    }

    if std::path::Path::new(LOCALTIME).exists() {
        return Ok(Some((
            "(从 /etc/localtime 读到的时区)".to_owned(),
            LOCALTIME.to_owned(),
        )));
    }

    Ok(None)
}

/// 读时区文件，算出 `now` 时刻的 UTC 偏移（秒）。
///
/// 读不了返回 `None`，由调用方决定怎么处理——这里不猜。
fn offset_at(path: &str, now: i64) -> Result<Option<i32>, CollectError> {
    let Some(bytes) = read::bytes(path)? else {
        return Ok(None);
    };

    Ok(parse(&bytes).and_then(|zone| zone.offset_at(now)))
}

/// 一个时区文件。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Zone {
    /// 切换时刻（unix 秒），升序。
    transitions: Vec<i64>,
    /// 每次切换后生效的类型下标，与 `transitions` 一一对应。
    types: Vec<u8>,
    /// 类型表。
    local: Vec<Local>,
}

/// 类型表里的一项。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Local {
    /// 相对 UTC 的偏移（秒）。
    offset: i32,
}

impl Zone {
    /// `now` 时刻的偏移。
    fn offset_at(&self, now: i64) -> Option<i32> {
        // 切换表是按时间升序的，找最后一个不晚于 now 的切换。
        let Some(index) = self
            .transitions
            .iter()
            .rposition(|transition| *transition <= now)
        else {
            // 早于第一次切换（或者文件根本没有切换表，比如 UTC）：
            // 用类型表的第一项。这比「当作 UTC」好——`Asia/Kolkata` 这类没有夏令时的
            // 时区，类型表首项就是它唯一的偏移。
            return self.local.first().map(|local| local.offset);
        };

        let kind = *self.types.get(index)?;
        self.local.get(kind as usize).map(|local| local.offset)
    }
}

/// 解析 TZif。
///
/// 只认版本 2/3/4 的第二份数据块；版本 1 的老文件（1980 年代的产物）直接返回 `None`——
/// 现在的发行版不会装那种文件，为它写一套 32 位解析不划算。
fn parse(bytes: &[u8]) -> Option<Zone> {
    if bytes.len() < 44 || &bytes[..4] != b"TZif" {
        return None;
    }

    let version = bytes[4];
    if version == 0 {
        return None;
    }

    // 第一份数据块是 32 位的，按它自己的计数跳过，才能到第二份。
    let counts = Counts::parse(bytes, 0)?;
    let second = counts.size(false);
    let counts = Counts::parse(bytes, second)?;

    let data = &bytes[second..];
    let body = counts.size(true);
    if data.len() < body {
        return None;
    }

    let data = &data[44..];
    let mut transitions = Vec::with_capacity(counts.transitions as usize);
    for index in 0..counts.transitions as usize {
        let start = index * 8;
        let chunk = data.get(start..start + 8)?;
        transitions.push(i64::from_be_bytes(chunk.try_into().ok()?));
    }

    let indices_start = counts.transitions as usize * 8;
    let types: Vec<u8> = data
        .get(indices_start..indices_start + counts.transitions as usize)?
        .to_vec();

    // 类型表：每项 6 字节（i32 偏移 + u8 夏令时标志 + u8 缩写下标）。
    let table_start = indices_start + counts.transitions as usize;
    let mut local = Vec::with_capacity(counts.types as usize);
    for index in 0..counts.types as usize {
        let start = table_start + index * 6;
        let chunk = data.get(start..start + 4)?;
        local.push(Local {
            offset: i32::from_be_bytes(chunk.try_into().ok()?),
        });
    }

    if local.is_empty() {
        return None;
    }

    Some(Zone {
        transitions,
        types,
        local,
    })
}

/// TZif 头部里的各种计数，以及按它算出的数据块长度。
#[derive(Debug, Clone, Copy)]
struct Counts {
    transitions: u32,
    types: u32,
    chars: u32,
    leaps: u32,
    standard: u32,
    ut: u32,
}

impl Counts {
    /// 读一个头部（偏移 44 处是时间数据，头部 44 字节）。
    fn parse(bytes: &[u8], at: usize) -> Option<Self> {
        let header = bytes.get(at..at + 44)?;
        let number = |index: usize| -> Option<u32> {
            let start = 20 + index * 4;
            let chunk = header.get(start..start + 4)?;
            Some(u32::from_be_bytes(chunk.try_into().ok()?))
        };

        Some(Self {
            ut: number(0)?,
            standard: number(1)?,
            leaps: number(2)?,
            transitions: number(3)?,
            types: number(4)?,
            chars: number(5)?,
        })
    }

    /// 数据块的长度（不含 44 字节头部）。`wide` 为真时切换时间是 64 位。
    fn size(&self, wide: bool) -> usize {
        let time_size = if wide { 8 } else { 4 };

        44 + self.transitions as usize * (time_size + 1)
            + self.types as usize * 6
            + self.chars as usize
            + self.leaps as usize * if wide { 12 } else { 8 }
            + self.standard as usize
            + self.ut as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_local_time() {
        // 2026-09-13 16:10:51 UTC
        assert_eq!(format_local(1_789_315_851), "2026-09-13 16:10:51");
    }

    #[test]
    fn formats_the_epoch() {
        assert_eq!(format_local(0), "1970-01-01 00:00:00");
    }

    #[test]
    fn formats_just_before_the_epoch() {
        // 负数时间要向下去整：-1 秒是 1969-12-31 23:59:59，不是同一天的 00:00:00。
        assert_eq!(format_local(-1), "1969-12-31 23:59:59");
    }

    #[test]
    fn handles_leap_years() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(59), (1970, 3, 1), "1970 不是闰年");
        assert_eq!(civil_from_days(11_016), (2000, 2, 29), "2000 是闰年");
        assert_eq!(civil_from_days(-365), (1969, 1, 1));
    }

    #[test]
    fn describes_offsets() {
        assert_eq!(describe_offset(0), "+00:00");
        assert_eq!(describe_offset(8 * 3600), "+08:00");
        assert_eq!(describe_offset(-5 * 3600), "-05:00");
        assert_eq!(describe_offset(5 * 3600 + 1800), "+05:30", "印度那种半时区");
        assert_eq!(describe_offset(-(3 * 3600 + 1800)), "-03:30");
    }

    /// 手搓一个最小的 TZif v2：一次切换、两个类型。
    ///
    /// 数据取自真实文件的结构而不是凭印象：切在 unix 秒 1000，之前用 +08:00，
    /// 之后用 +09:00。
    /// 往缓冲区里追加一个 44 字节的 TZif 头部。
    ///
    /// 版本字节始终写 `'2'`：**第一份数据块用 32 位时间、第二份用 64 位**是版本 2 的
    /// 约定，不靠这个字节区分（踩过：写成 0 会被解析器当成「没有第二份」直接跳过）。
    fn push_header(out: &mut Vec<u8>, transitions: u32, types: u32, chars: u32) {
        out.extend_from_slice(b"TZif");
        out.push(b'2');
        out.extend_from_slice(&[0; 15]);
        for value in [0u32, 0, 0, transitions, types, chars] {
            out.extend_from_slice(&value.to_be_bytes());
        }
    }

    fn synthetic() -> Vec<u8> {
        let mut out = Vec::new();

        // 第一份（32 位）数据块：空的，只有类型表两个。
        push_header(&mut out, 0, 2, 10);
        out.extend_from_slice(&(8 * 3600i32).to_be_bytes());
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(&(9 * 3600i32).to_be_bytes());
        out.extend_from_slice(&[0, 5]);
        out.extend_from_slice(b"UTC\0UTC2\0\0");

        // 第二份（64 位）数据块：一次切换。
        push_header(&mut out, 1, 2, 10);
        out.extend_from_slice(&1000i64.to_be_bytes());
        out.push(1);
        out.extend_from_slice(&(8 * 3600i32).to_be_bytes());
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(&(9 * 3600i32).to_be_bytes());
        out.extend_from_slice(&[0, 5]);
        out.extend_from_slice(b"UTC\0UTC2\0\0");

        out
    }

    #[test]
    fn parses_a_tzif_and_picks_the_offset() {
        let zone = parse(&synthetic()).expect("该解析出来");

        assert_eq!(zone.transitions, [1000]);
        assert_eq!(
            zone.offset_at(999),
            Some(8 * 3600),
            "第一次切换之前用类型表首项"
        );
        assert_eq!(zone.offset_at(1000), Some(9 * 3600));
        assert_eq!(zone.offset_at(1_000_000), Some(9 * 3600));
    }

    #[test]
    fn rejects_junk() {
        assert_eq!(parse(b""), None);
        assert_eq!(parse(b"NOTZIF"), None);
        assert_eq!(parse(&[0; 44]), None, "magic 不对");
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = DateTime.collect(&Context::for_tests()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "Date & Time");
        // 形状：`2026-09-13 16:10:51`
        assert_eq!(entries[0].value.len(), 19, "实际是 {}", entries[0].value);
        assert_eq!(entries[0].value.as_bytes()[4], b'-');
    }
}
