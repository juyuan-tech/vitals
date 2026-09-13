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

    Ok(crate::collectors::tzif::offset_in(&bytes, now))
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
        let bytes = synthetic();

        assert_eq!(
            crate::collectors::tzif::offset_in(&bytes, 999),
            Some(8 * 3600),
            "第一次切换之前用类型表首项"
        );
        assert_eq!(
            crate::collectors::tzif::offset_in(&bytes, 1000),
            Some(9 * 3600)
        );
        assert_eq!(
            crate::collectors::tzif::offset_in(&bytes, 1_000_000),
            Some(9 * 3600)
        );
    }

    #[test]
    fn rejects_junk() {
        assert_eq!(crate::collectors::tzif::offset_in(b"", 0), None);
        assert_eq!(crate::collectors::tzif::offset_in(b"NOTZIF", 0), None);
        assert_eq!(
            crate::collectors::tzif::offset_in(&[0; 44], 0),
            None,
            "magic 不对"
        );
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
