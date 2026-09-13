//! 单位换算：把原始数字变成人看的字符串。
//!
//! 只在**采集侧**用。`Info::value` 必须是已经能直接显示的文本——
//! 渲染器看到的只有字符串，它不做数值格式化，也不该做。

/// 把字节数写成 `15.6 GiB`。
///
/// 用 1024 进制（GiB 而不是 GB）：这是内核和 `free`/`df -h` 的口径，
/// 系统信息工具跟着它走，免得和用户 `df -h` 看到的数字对不上。
#[must_use]
pub fn bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    if bytes < 1024 {
        return format!("{bytes} B");
    }

    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }

    format!("{value:.1} {}", UNITS[unit])
}

/// 把秒数写成 `3d 4h`、`4h 12m`、`12m 34s`、`34s` 这样的形状。
///
/// 最多给两级单位：给三级让人去数零，给一级又太粗。
#[must_use]
pub fn duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = seconds % 86_400 / 3_600;
    let minutes = seconds % 3_600 / 60;
    let secs = seconds % 60;

    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

/// `used / total` 的整数百分比，四舍五入。
///
/// 走 f64 而不是整数乘法：以字节计的用量乘 100 会溢出 u64
/// （十几 TB 的盘就是 10^13 量级，乘 100 刚好顶到 u64 的天花板附近）。
#[must_use]
pub fn percent(used: u64, total: u64) -> u64 {
    if total == 0 {
        return 0;
    }

    let percent = (used as f64 * 100.0 / total as f64).round();
    // 夹一下：出现 100 以上只可能说明上游算错了，不该把 300% 摆到用户脸上。
    percent.clamp(0.0, 100.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_byte_counts_stay_in_bytes() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1023), "1023 B");
    }

    #[test]
    fn byte_counts_switch_units_at_1024() {
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(1536), "1.5 KiB");
        assert_eq!(bytes(1024 * 1024), "1.0 MiB");
        // 实测这台机器的 MemTotal：32140260 kB
        assert_eq!(bytes(32_140_260 * 1024), "30.7 GiB");
    }

    #[test]
    fn durations_use_at_most_two_units() {
        assert_eq!(duration(0), "0s");
        assert_eq!(duration(59), "59s");
        assert_eq!(duration(60), "1m 0s");
        assert_eq!(duration(3_661), "1h 1m");
        assert_eq!(duration(86_400), "1d 0h");
        // 实测这台机器的 /proc/uptime 第一列：101546 秒
        assert_eq!(duration(101_546), "1d 4h");
    }

    #[test]
    fn percent_rounds_and_survives_zero() {
        assert_eq!(percent(0, 0), 0, "除以零不该 panic");
        assert_eq!(percent(1, 3), 33);
        assert_eq!(percent(2, 3), 67);
        assert_eq!(percent(0, 100), 0);
        assert_eq!(percent(100, 100), 100);
        assert_eq!(percent(200, 100), 100, "超出的比例夹到 100");
    }
}
