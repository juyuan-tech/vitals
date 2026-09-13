//! 单位换算：把原始数字变成人看的字符串。
//!
//! 只在**采集侧**用。`Info::value` 必须是已经能直接显示的文本——
//! 渲染器看到的只有字符串，它不做数值格式化，也不该做。

/// 把字节数写成 `15.60 GiB`。
///
/// 用 1024 进制（GiB 而不是 GB）：这是内核和 `free`/`df -h` 的口径，
/// 系统信息工具跟着它走，免得和用户 `df -h` 看到的数字对不上。
///
/// **两位小数与 fastfetch 一致**。这一条是后来改的：一开始我们印一位（`15.6 GiB`），
/// 理由是更干净；但那样「和参考输出 diff」这条最有力的验收手段就用不上——
/// `Physical Disk`、`Memory`、`BTRFS` 这些行会永远差最后一位。两位小数并不更精确
/// （`/sys/block/*/size` 本身是整数扇区数），它换来的是**可逐字核对**。
#[must_use]
pub fn bytes(bytes: u64) -> String {
    render(bytes, 2)
}

/// 换算与格式化：`{小数位数}` 是唯一可变的那个参数。
///
/// 不足 1024 字节时原样按字节印（和 fastfetch 一样：它连单位换算都不做）。
fn render(bytes: u64, digits: usize) -> String {
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

    format!("{value:.digits$} {}", UNITS[unit])
}

/// 把秒数写成 `3d 4h`、`4h 12m`、`12m 34s`、`34s` 这样的形状。
///
/// 最多给两级单位：给三级让人去数零，给一级又太粗。
#[must_use]
pub fn duration(seconds: u64) -> String {
    let units = [
        (seconds / 86_400, ["day", "days"]),
        (seconds % 86_400 / 3_600, ["hour", "hours"]),
        (seconds % 3_600 / 60, ["min", "mins"]),
        (seconds % 60, ["sec", "secs"]),
    ];

    // fastfetch 的写法：`1 day, 5 hours, 41 mins`——非零的单位依次列出，最多三个
    // （四个都非零时丢掉秒），一个都没有就说 `0 secs`。天与小时写全，分秒用缩写，
    // 单复数跟着数值走。先前是 `1d 5h` 这种紧凑写法，与它对不上。
    let parts: Vec<String> = units
        .iter()
        .filter(|(value, _)| *value > 0)
        .take(3)
        .map(|(value, names)| {
            let name = if *value == 1 { names[0] } else { names[1] };
            format!("{value} {name}")
        })
        .collect();

    if parts.is_empty() {
        return "0 secs".to_owned();
    }

    parts.join(", ")
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
        assert_eq!(bytes(1024), "1.00 KiB");
        assert_eq!(bytes(1536), "1.50 KiB");
        assert_eq!(bytes(1024 * 1024), "1.00 MiB");
        // 实测这台机器的 MemTotal：32140260 kB
        assert_eq!(bytes(32_140_260 * 1024), "30.65 GiB");
    }

    #[test]
    fn the_real_fastfetch_numbers_come_out_byte_for_byte() {
        // Physical Disk 的三个真实数字（扇区数 × 512）：USB 优盘、NVMe、zram。
        // 这三行与 `fastfetch -s physicaldisk` 的 `diff` 是空的。
        assert_eq!(bytes(120_938_496 * 512), "57.67 GiB");
        assert_eq!(bytes(2_000_409_264 * 512), "953.87 GiB");
        assert_eq!(bytes(32_139_264 * 512), "15.33 GiB");
        // 小于 1024 时按字节印，不换算也不补小数（fastfetch 也这样）。
        assert_eq!(bytes(512), "512 B");
    }

    #[test]
    fn durations_read_like_fastfetch() {
        assert_eq!(duration(0), "0 secs");
        assert_eq!(duration(59), "59 secs");
        assert_eq!(duration(60), "1 min", "零的单位不列");
        assert_eq!(duration(3_661), "1 hour, 1 min, 1 sec");
        assert_eq!(duration(86_400), "1 day");
        // 实测这台机器的 /proc/uptime 第一列：101546 秒 = 1 天 4 小时 12 分 26 秒
        assert_eq!(
            duration(101_546),
            "1 day, 4 hours, 12 mins",
            "最多三个单位，秒被丢掉"
        );
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
