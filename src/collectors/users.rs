//! Users：当前登录的用户会话（`utmp`）。
//!
//! 数据源是那个二进制文件，不是 `who`（**零子进程**）：记录之间没有分隔符，
//! 就是一条接一条的定长记录，读到一条坏的就跳过这一条。
//!
//! 布局（x86_64 上的 glibc，取自 `<utmp.h>`）：
//!
//! ```text
//! 偏移  字段          长度   说明
//!   0   ut_type        2    ← 这个字段之后有 2 字节对齐填充，因为 ut_pid 要 4 字节对齐
//!   4   ut_pid         4    登录进程的 pid
//!   8   ut_line       32    终端名（`tty1`、`pts/4`）
//!  40   ut_id          4
//!  44   ut_user       32    用户名
//!  76   ut_host      256    远端主机（本地登录是空的）
//! 332   ut_exit        4    { e_termination: i16, e_exit: i16 }
//! 336   ut_session     4
//! 340   ut_tv          8    { tv_sec: i32, tv_usec: i32 }
//! 348   ut_addr_v6    16
//! 364   __unused      20
//! 384   ── 合计
//! ```
//!
//! 这些偏移不是从文档抄来的：同文件里的 `KernelUtmp` 把同一套布局用 `#[repr(C)]`
//! 又写了一遍，单元测试拿 `offset_of!` 逐个比对、`size_of` 再钉住 384。布局哪天变了
//! 会红在这里，而不是悄悄读错字段。（`forbid(unsafe_code)` 下不能把字节切片直接
//! 重解释成结构体，所以真正的读取还是按固定偏移手取。）
//!
//! 只统计 `ut_type == 7`（`USER_PROCESS`）：那才是「有人登进来了」。
//! 开机时间（`BOOT_TIME`）、已死的会话（`DEAD_PROCESS`）都不算——`who`（不带 `-a`）
//! 也是这么滤的。
//!
//! 去重口径跟着 upstream fastfetch（`users_linux.c`）：按**用户名**去重，
//! 同名多次登录只算一个人、报**最近**那次登录；条目顺序是第一次出现的顺序。
//!
//! 登录时间是本地时间：`ut_tv` 里是 Unix 秒，直接印出来是 UTC，
//! 所以要先从时区数据（`/etc/localtime`，或 `$TZ` 指向的 zoneinfo 文件）
//! 查出那一刻的偏移再加。查不到时区就**不印时间**，只报用户名——
//! 印一个差 8 小时的时刻比不印更坏。
//!
//! 没做的事：Debian/Ubuntu 早就不再写 `/var/run/utmp`，upstream 在表为空时会转而读
//! systemd 的 `/run/systemd/users/*`（那要解析它的私有文件格式）。本模块只认 utmp，
//! 在那类系统上就是无数据。

use std::mem::offset_of;
use std::path::{Component, Path};

use crate::collectors::read;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// `utmp` 可能在的位置，按顺序找。
///
/// 新内核里 `/var/run` 就是 `/run`，两个名字指同一个文件；先读前一个是
/// upstream 与老发行版的习惯。
const UTMP: [&str; 2] = ["/var/run/utmp", "/run/utmp"];

/// 一条 `utmp` 记录的字节数。
const RECORD: usize = 384;

/// `ut_type == USER_PROCESS`：一次真实登录会话。
const USER_PROCESS: i16 = 7;

// 用到的字段偏移与长度。名字与内核结构体对齐，改一处就得改另一处——
// 但 `tests` 里的 `offset_of!` 会把两边拴在一起。
const OFF_TYPE: usize = offset_of!(KernelUtmp, ut_type);
const OFF_LINE: usize = offset_of!(KernelUtmp, ut_line);
const OFF_USER: usize = offset_of!(KernelUtmp, ut_user);
const OFF_HOST: usize = offset_of!(KernelUtmp, ut_host);
const OFF_SEC: usize = offset_of!(KernelUtmp, ut_tv) + offset_of!(KernelTimeval, tv_sec);

/// 时区数据可能在的目录，按顺序找。
const ZONEINFO_ROOTS: [&str; 3] = [
    "/usr/share/zoneinfo",
    "/usr/share/lib/zoneinfo",
    "/etc/zoneinfo",
];

/// 用户会话。
pub struct Users;

impl Collector for Users {
    fn name(&self) -> &'static str {
        "users"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let mut raw = None;
        for path in UTMP {
            if let Some(bytes) = read::bytes(path)? {
                raw = Some(bytes);
                break;
            }
        }

        // 两个路径都没有（容器、非 Linux）就是无数据，不是错误。
        let Some(raw) = raw else {
            return Ok(Vec::new());
        };

        let users = dedup(sessions(&raw));
        if users.is_empty() {
            return Ok(Vec::new());
        }

        // 时区只在真有会话时才读：没人登录时连这个文件都不碰。
        let zone = Zone::load()?;

        // 一个用户是 `Users`，多个才编号——与 `gpu` 那边同一个约定。
        let numbered = users.len() > 1;

        Ok(users
            .iter()
            .enumerate()
            .map(|(index, session)| {
                let key = if numbered {
                    format!("Users {}", index + 1)
                } else {
                    "Users".to_owned()
                };

                let login_time = match (session.login, zone.as_ref()) {
                    (login, Some(zone)) if login > 0 => zone.format(login),
                    _ => None,
                };

                let mut info =
                    Info::new(self.name(), key, describe(session, login_time.as_deref()))
                        .with_variable("name", session.user.clone());
                if !session.line.is_empty() {
                    info = info.with_variable("session", session.line.clone());
                }
                if let Some(login_time) = login_time {
                    info = info.with_variable("login_time", login_time);
                }

                info
            })
            .collect())
    }
}

/// 把一条会话拼成显示值：`gxyarch - login time 2026-09-12 10:56:19`。
///
/// 远端登录带 `@主机`（upstream 也这样）；没有主机就不写。
/// 时间那一段拿不到就整段省掉，剩一个用户名也是个正确答案。
fn describe(session: &Session, login_time: Option<&str>) -> String {
    let mut value = session.user.clone();
    if !session.host.is_empty() {
        value.push_str(&format!("@{}", session.host));
    }
    if let Some(login_time) = login_time {
        value.push_str(&format!(" - login time {login_time}"));
    }

    value
}

/// 一条登录会话。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Session {
    /// 用户名。
    user: String,
    /// 登录时刻（Unix 秒）。0 表示记录里没写。
    login: i64,
    /// 终端名。
    line: String,
    /// 远端主机，本地登录是空的。
    host: String,
}

/// 解析整个 `utmp`。
///
/// 文件尾部可能有半个记录（正有进程在写它），`chunks_exact` 会把它丢掉——
/// 拿半个记录去猜字段只会读出垃圾。
fn sessions(bytes: &[u8]) -> Vec<Session> {
    bytes
        .chunks_exact(RECORD)
        .filter_map(parse_record)
        .collect()
}

/// 解析一条记录；不是真实登录会话、或者字段坏掉，就是 `None`。
///
/// `ut_user` 不是合法 UTF-8 时**跳过这一条**，而不是让整个模块失败：
/// `utmp` 是多个进程共同追加的文件，里面有陈旧记录、有断电留下的半个记录，
/// 一条垃圾记录不代表「这台机器上的登录信息读不了」。upstream 也是逐条处理的
/// ——它读的是同一份文件，只是 C 那边不检查编码而已。
fn parse_record(record: &[u8]) -> Option<Session> {
    if record.len() < RECORD {
        return None;
    }
    if i16::from_ne_bytes(record[OFF_TYPE..OFF_TYPE + 2].try_into().ok()?) != USER_PROCESS {
        return None;
    }

    let user = c_string(record, OFF_USER, 32)?;
    // 没有用户名的 USER_PROCESS 记录是坏数据，报出去只是一行空白。
    if user.is_empty() {
        return None;
    }

    Some(Session {
        user,
        login: i64::from(i32::from_ne_bytes(
            record[OFF_SEC..OFF_SEC + 4].try_into().ok()?,
        )),
        line: c_string(record, OFF_LINE, 32).unwrap_or_default(),
        host: c_string(record, OFF_HOST, 256).unwrap_or_default(),
    })
}

/// 取一个 C 字符串字段（到第一个 NUL 为止，去掉首尾空白）。
///
/// 字段里不是合法 UTF-8 就返回 `None`，由调用方决定怎么办：用户名那里是「跳过这条」，
/// 终端名与主机名那里是「当作没有」（它们只是附带信息）。
fn c_string(record: &[u8], offset: usize, len: usize) -> Option<String> {
    // 用检查过的加法：调用方传的偏移都是常量，但不该依赖这一点——溢出在 release 里会
    // 绕回、在 debug 里会 panic，两者都不该取决于传进来的长度。绕回后 start > end，
    // `get` 返回 `None`（不会读到错字段），但 panic 是实打实的。
    let end = offset.checked_add(len)?;
    let field = record.get(offset..end)?;
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());

    Some(std::str::from_utf8(&field[..end]).ok()?.trim().to_owned())
}

/// 按用户名去重：同名多次登录算一个人，保留最近的那次登录。
///
/// 顺序是**第一次出现**的顺序：upstream 找到同名条目时就地更新它，条目位置不动；
/// 照抄这个口径，同一台机器两次运行的输出顺序才稳定。
/// 时间相同时保留先出现的那条（upstream 用的是严格大于）。
fn dedup(sessions: Vec<Session>) -> Vec<Session> {
    let mut users: Vec<Session> = Vec::new();

    for session in sessions {
        match users.iter_mut().find(|user| user.user == session.user) {
            Some(user) if session.login > user.login => *user = session,
            Some(_) => {}
            None => users.push(session),
        }
    }

    users
}

// ---------------------------------------------------------------------------
// 本地时间
// ---------------------------------------------------------------------------

/// 本地时间从哪来。
///
/// 只有两条路：`$TZ` 是空串时按 POSIX 规定用 UTC；否则读一个 TZif 文件。
/// TZif 是 tzdata 的二进制格式（RFC 8536），`/etc/localtime` 与 zoneinfo 里的
/// 每一个时区文件都是它。
enum Zone {
    /// `TZ=""`：UTC。
    Utc,
    /// 一个时区文件的内容。
    File(Vec<u8>),
}

impl Zone {
    /// 读一次时区数据。
    ///
    /// 候选文件的规矩与别的模块一致（[`read::bytes`]）：不存在是「这个候选没有」、
    /// 继续下一个；存在却读不了是**真失败**。
    ///
    /// `$TZ` 里写 POSIX 规则串（`CST-8`、`EST5EDT,M3.2.0,M11.1.0`）时找不到对应文件，
    /// 于是没有时间——解析规则串要一整套 TZ 规则引擎，本模块不做，
    /// 而且**认不出来就不印**，比硬套一个偏移强。
    fn load() -> Result<Option<Self>, CollectError> {
        match std::env::var("TZ") {
            Ok(value) if value.trim().is_empty() => Ok(Some(Self::Utc)),
            Ok(value) => {
                for path in zoneinfo_paths(value.trim()) {
                    if let Some(bytes) = read::bytes(&path)? {
                        return Ok(Some(Self::File(bytes)));
                    }
                }

                Ok(None)
            }
            // 没设 `TZ`：系统时区是 `/etc/localtime`（通常是指向 zoneinfo 的软链接）。
            Err(_) => Ok(read::bytes("/etc/localtime")?.map(Self::File)),
        }
    }

    /// 把 Unix 秒写成那一刻的**本地**时间。
    ///
    /// 时区数据坏掉或认不出来时返回 `None`（调用方就不印时间），不 panic。
    fn format(&self, seconds: i64) -> Option<String> {
        let offset = match self {
            Self::Utc => 0,
            Self::File(bytes) => crate::collectors::tzif::offset_in(bytes, seconds)?,
        };

        Some(civil(seconds + i64::from(offset)))
    }
}

/// `$TZ` 的值 → 要读的候选文件。
///
/// 认三种写法：绝对路径、`Asia/Shanghai` 这样的时区名（按 [`ZONEINFO_ROOTS`] 找）、
/// 以及前面带冒号的等价写法（POSIX 允许 `:Asia/Shanghai`）。
///
/// 名字里出现 `.`、`..`、根目录这类成分时**一律不接受**：它会被拼进路径，
/// 不该让一个环境变量把读取带到 zoneinfo 目录外面去。
fn zoneinfo_paths(value: &str) -> Vec<String> {
    let name = value.strip_prefix(':').unwrap_or(value).trim();
    let path = Path::new(name);

    // 名字里得真的有一个文件成分：`/` 这种根路径、空的、`.` 之类的都读不出东西
    // （读目录会得到 `EISDIR`，那会白白让整个模块失败）。
    if !path
        .components()
        .any(|part| matches!(part, Component::Normal(_)))
    {
        return Vec::new();
    }
    if path.is_absolute() {
        return vec![name.to_owned()];
    }
    // 相对名字里不许有 `..` 这类成分：它会被拼进路径，
    // 不该让一个环境变量把读取带到 zoneinfo 目录外面去。
    if path
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Vec::new();
    }

    ZONEINFO_ROOTS
        .iter()
        .map(|root| format!("{root}/{name}"))
        .collect()
}

/// 把「本地秒」拆成 `2026-09-12 10:56:19`。
///
/// 用 Howard Hinnant 的 `civil_from_days`：纯整数运算、没有查表、
/// 也不像 `struct tm` 那套要担心 1900 或 2038 的边界。负数秒按欧几里得除法
/// 归到「1970 之前」的正确日期上。
fn civil(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let time = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);

    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        time / 3600,
        time % 3600 / 60,
        time % 60
    )
}

/// 从「1970-01-01 起的天数」求年月日。
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    // 把纪元挪到 0000-03-01：这样闰日落在年末，月长规律最简单。
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;

    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    (year, month, day)
}

// ---------------------------------------------------------------------------
// 内核结构体：只用来把上面的偏移钉死
// ---------------------------------------------------------------------------

/// `struct exit_status`。
#[repr(C)]
struct KernelExitStatus {
    e_termination: i16,
    e_exit: i16,
}

/// `struct timeval`（`ut_tv` 用 32 位的那个版本）。
#[repr(C)]
struct KernelTimeval {
    tv_sec: i32,
    tv_usec: i32,
}

/// `struct utmp` 的布局。字段从不被读写，只用来做 `offset_of!`/`size_of` 断言。
#[repr(C)]
#[allow(dead_code)]
struct KernelUtmp {
    ut_type: i16,
    ut_pid: i32,
    ut_line: [u8; 32],
    ut_id: [u8; 4],
    ut_user: [u8; 32],
    ut_host: [u8; 256],
    ut_exit: KernelExitStatus,
    ut_session: i32,
    ut_tv: KernelTimeval,
    ut_addr_v6: [i32; 4],
    _unused: [u8; 20],
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::*;

    // -----------------------------------------------------------------------
    // 布局
    // -----------------------------------------------------------------------

    #[test]
    fn the_kernel_struct_has_the_layout_we_hardcode() {
        // 这条把「手写的偏移」与「C 的布局」拴在一起：偏移写在常量里读起来清楚，
        // 但它必须与结构体一致，否则读出来的就是隔壁字段。
        assert_eq!(size_of::<KernelUtmp>(), RECORD);
        assert_eq!(offset_of!(KernelUtmp, ut_type), 0);
        assert_eq!(offset_of!(KernelUtmp, ut_pid), 4, "type 之后有 2 字节填充");
        assert_eq!(offset_of!(KernelUtmp, ut_line), 8);
        assert_eq!(offset_of!(KernelUtmp, ut_id), 40);
        assert_eq!(offset_of!(KernelUtmp, ut_user), 44);
        assert_eq!(offset_of!(KernelUtmp, ut_host), 76);
        assert_eq!(offset_of!(KernelUtmp, ut_exit), 332);
        assert_eq!(offset_of!(KernelUtmp, ut_session), 336);
        assert_eq!(offset_of!(KernelUtmp, ut_tv), 340);
        assert_eq!(offset_of!(KernelUtmp, ut_addr_v6), 348);
    }

    /// 拼一条记录。写字段用 `offset_of!`（也就是 C 的布局），
    /// 不用被测的那些常量——两边独立，才能互相验证。
    fn record(kind: i16, user: &str, line: &str, host: &str, seconds: i32) -> Vec<u8> {
        let mut record = vec![0u8; RECORD];
        let mut put = |offset: usize, bytes: &[u8]| {
            record[offset..offset + bytes.len()].copy_from_slice(bytes);
        };

        put(offset_of!(KernelUtmp, ut_type), &kind.to_ne_bytes());
        put(offset_of!(KernelUtmp, ut_pid), &4242_i32.to_ne_bytes());
        put(offset_of!(KernelUtmp, ut_line), line.as_bytes());
        put(offset_of!(KernelUtmp, ut_user), user.as_bytes());
        put(offset_of!(KernelUtmp, ut_host), host.as_bytes());
        put(
            offset_of!(KernelUtmp, ut_tv) + offset_of!(KernelTimeval, tv_sec),
            &seconds.to_ne_bytes(),
        );

        record
    }

    // -----------------------------------------------------------------------
    // 解析
    // -----------------------------------------------------------------------

    #[test]
    fn reads_a_user_process_record() {
        // 本机 tty1 那条记录的字段值。
        let bytes = record(USER_PROCESS, "gxyarch", "tty1", "", 1_789_181_779);
        let session = parse_record(&bytes).expect("这是一条登录会话");

        assert_eq!(session.user, "gxyarch");
        assert_eq!(session.line, "tty1");
        assert_eq!(session.host, "");
        assert_eq!(session.login, 1_789_181_779);
    }

    #[test]
    fn only_user_process_records_count() {
        // 本机 utmp 里另外三条：开机（2）与两条已死的会话（8）。
        for kind in [0_i16, 1, 2, 3, 4, 5, 6, 8] {
            assert_eq!(
                parse_record(&record(kind, "gxyarch", "tty1", "", 1)),
                None,
                "ut_type={kind} 不该被当成登录会话"
            );
        }
        assert!(parse_record(&record(USER_PROCESS, "gxyarch", "tty1", "", 1)).is_some());
    }

    #[test]
    fn a_remote_session_keeps_its_host() {
        let bytes = record(USER_PROCESS, "root", "pts/3", "10.0.0.7", 1_700_000_000);
        let session = parse_record(&bytes).unwrap();

        assert_eq!(session.host, "10.0.0.7");
        assert_eq!(
            describe(&session, Some("2023-11-14 22:13:20")),
            "root@10.0.0.7 - login time 2023-11-14 22:13:20"
        );
    }

    #[test]
    fn a_record_with_a_broken_user_is_skipped() {
        let mut bytes = record(USER_PROCESS, "gxyarch", "tty1", "", 5);
        // 用户名中间塞一个非法 UTF-8 字节：这条丢掉。
        bytes[OFF_USER + 2] = 0xff;
        assert_eq!(parse_record(&bytes), None);

        // 终端名坏掉只是少一个附带信息，会话本身还在。
        let mut bytes = record(USER_PROCESS, "gxyarch", "tty1", "", 5);
        bytes[OFF_LINE] = 0xff;
        let session = parse_record(&bytes).expect("用户名还是好的");
        assert_eq!(session.user, "gxyarch");
        assert_eq!(session.line, "");
    }

    #[test]
    fn a_trailing_half_record_is_dropped() {
        let mut bytes = record(USER_PROCESS, "gxyarch", "tty1", "", 5);
        bytes.extend_from_slice(&record(USER_PROCESS, "root", "pts/1", "", 6)[..100]);

        let parsed = sessions(&bytes);
        assert_eq!(parsed.len(), 1, "半个记录既不该被读出，也不该报错");
        assert!(sessions(&[]).is_empty());
        assert!(sessions(&bytes[..RECORD - 1]).is_empty());
    }

    #[test]
    fn the_login_time_is_signed_native_endian() {
        // 记的是 i32 原生字节序；负值（1970 之前）也得读对。
        let session = parse_record(&record(USER_PROCESS, "old", "tty2", "", -1)).unwrap();
        assert_eq!(session.login, -1);
    }

    // -----------------------------------------------------------------------
    // 去重
    // -----------------------------------------------------------------------

    /// 只带用户名与登录时间的一条会话。
    fn session(user: &str, login: i64) -> Session {
        Session {
            user: user.to_owned(),
            login,
            line: String::new(),
            host: String::new(),
        }
    }

    #[test]
    fn one_name_is_one_person_but_the_newest_login_wins() {
        let users = dedup(vec![
            session("gxyarch", 100),
            session("root", 50),
            session("gxyarch", 300),
            session("gxyarch", 200),
        ]);

        assert_eq!(users.len(), 2, "同名多次登录只算一个人");
        assert_eq!(users[0].user, "gxyarch");
        assert_eq!(users[0].login, 300, "报最近那次");
        assert_eq!(users[1].user, "root");
    }

    #[test]
    fn an_earlier_duplicate_does_not_move_the_entry() {
        // 先出现的位置不动：同一台机器两次运行顺序才稳定。
        let users = dedup(vec![session("a", 1), session("b", 1), session("a", 1)]);

        assert_eq!(
            users
                .iter()
                .map(|user| user.user.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
    }

    // -----------------------------------------------------------------------
    // 时间
    // -----------------------------------------------------------------------

    #[test]
    fn local_time_is_the_epoch_plus_the_offset() {
        // 本机 tty1 那条记录的 ut_tv.tv_sec，以及它在 UTC 与 CST(+8) 下的长相。
        // fastfetch 印的是后者（`2026-09-12 10:56:19`），这也是把「Unix 秒直接当
        // 本地时间」这个坑钉住的断言。
        assert_eq!(civil(1_789_181_779), "2026-09-12 02:56:19");
        assert_eq!(civil(1_789_181_779 + 8 * 3600), "2026-09-12 10:56:19");
        assert_eq!(civil(0), "1970-01-01 00:00:00");
        // 闰年与跨年。
        assert_eq!(civil(1_704_067_200), "2024-01-01 00:00:00");
        assert_eq!(civil(1_735_689_600), "2025-01-01 00:00:00");
        assert_eq!(civil(-1), "1969-12-31 23:59:59");
    }

    #[test]
    fn the_utc_zone_prints_utc() {
        let zone = Zone::Utc;
        assert_eq!(zone.format(0).as_deref(), Some("1970-01-01 00:00:00"));
        assert_eq!(
            zone.format(1_789_181_779).as_deref(),
            Some("2026-09-12 02:56:19")
        );
    }

    /// 拼一个 TZif v2 文件：`transitions` 升序，`types` 是每个时间段用的
    /// `(偏移秒数, 是否夏令时)`。
    fn tzif(transitions: &[i64], types: &[(i32, bool)]) -> Vec<u8> {
        /// 一个头部：magic + 版本 + 保留 + 六个大端计数。
        fn header(out: &mut Vec<u8>, timecnt: u32, typecnt: u32) {
            out.extend_from_slice(b"TZif2");
            out.extend_from_slice(&[0u8; 15]);
            for count in [0_u32, 0, 0, timecnt, typecnt, 4] {
                out.extend_from_slice(&count.to_be_bytes());
            }
        }
        /// 一张 ttinfo 表 + 4 字节的名字串。
        fn push_types(out: &mut Vec<u8>, types: &[(i32, bool)]) {
            for (index, (offset, dst)) in types.iter().enumerate() {
                out.extend_from_slice(&offset.to_be_bytes());
                out.push(u8::from(*dst));
                out.push(index as u8);
            }
            out.extend_from_slice(b"UTC\0");
        }

        let mut out = Vec::new();

        // 第一块：32 位兼容副本，这里只放一个类型（真实文件也带一份完整的表）。
        header(&mut out, 0, 1);
        push_types(&mut out, &[(0, false)]);

        // 第二块：真正的数据，64 位转换时刻。
        header(&mut out, transitions.len() as u32, types.len() as u32);
        for transition in transitions {
            out.extend_from_slice(&transition.to_be_bytes());
        }
        for index in 0..transitions.len() {
            out.push(index as u8);
        }
        push_types(&mut out, types);

        out
    }

    #[test]
    fn a_tzif_file_gives_the_offset_for_that_moment() {
        // 两段：第一段 +1h（冬令时），第二段 +2h（夏令时）。
        let bytes = tzif(
            &[-1_000_000_000, 1_000_000_000],
            &[(3600, false), (7200, true)],
        );

        // 第一个转换之前：用第一个非夏令时类型。
        assert_eq!(
            crate::collectors::tzif::offset_in(&bytes, -2_000_000_000),
            Some(3600)
        );
        assert_eq!(crate::collectors::tzif::offset_in(&bytes, 0), Some(3600));
        assert_eq!(
            crate::collectors::tzif::offset_in(&bytes, 1_000_000_000),
            Some(7200),
            "边界算在内"
        );
        assert_eq!(
            crate::collectors::tzif::offset_in(&bytes, 2_000_000_000),
            Some(7200)
        );
    }

    #[test]
    fn the_zone_can_be_read_from_a_real_tzif_file() {
        // 本机 `/etc/localtime` → `Asia/Shanghai`。这条**不假设**本机时区：
        // 读不到文件（没装 tzdata）就跳过，只在读到时断言格式与日期量级。
        let zone = Zone::load().unwrap();
        if let Some(zone) = zone {
            let formatted = zone.format(1_789_181_779).expect("时区数据该能解析");
            assert_eq!(formatted.len(), 19, "实际是 {formatted}");
            assert!(formatted.starts_with("2026-09-1"), "实际是 {formatted}");
        }
    }

    #[test]
    fn a_v1_only_file_still_works() {
        // 老格式的文件（版本字节是 0，没有第二个头部）：只有 32 位转换时刻。
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"TZif");
        bytes.push(0);
        bytes.extend_from_slice(&[0u8; 15]);
        for count in [0_u32, 0, 0, 1, 1, 4] {
            bytes.extend_from_slice(&count.to_be_bytes());
        }
        bytes.extend_from_slice(&0_i32.to_be_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&19800_i32.to_be_bytes());
        bytes.push(0);
        bytes.push(0);
        bytes.extend_from_slice(b"IST\0");

        assert_eq!(crate::collectors::tzif::offset_in(&bytes, 5), Some(19800));
    }

    #[test]
    fn broken_timezone_data_yields_no_time_instead_of_a_wrong_one() {
        assert_eq!(crate::collectors::tzif::offset_in(&[], 0), None);
        assert_eq!(
            crate::collectors::tzif::offset_in(b"not a tzif file at all", 0),
            None
        );

        // 头部说是 TZif，但数据块被截断了：连第一块都读不全。
        let mut truncated = tzif(&[0], &[(3600, false)]);
        truncated.truncate(crate::collectors::tzif::TZIF_HEADER + 9);
        assert_eq!(crate::collectors::tzif::offset_in(&truncated, 0), None);
    }

    #[test]
    fn the_timezone_name_becomes_a_candidate_path() {
        assert_eq!(
            zoneinfo_paths("Asia/Shanghai"),
            vec![
                "/usr/share/zoneinfo/Asia/Shanghai",
                "/usr/share/lib/zoneinfo/Asia/Shanghai",
                "/etc/zoneinfo/Asia/Shanghai"
            ]
        );
        // 前导冒号是 POSIX 的写法；绝对路径直接用。
        assert_eq!(
            zoneinfo_paths(":Asia/Shanghai"),
            zoneinfo_paths("Asia/Shanghai")
        );
        assert_eq!(zoneinfo_paths("/etc/localtime"), vec!["/etc/localtime"]);
        // 规则串（POSIX TZ，`CST-8`、`EST5EDT,M3.2.0,M11.1.0`）会被当成名字去
        // zoneinfo 里找，找不到就没有时间——本模块不解析规则串。
        assert_eq!(zoneinfo_paths("CST-8").len(), ZONEINFO_ROOTS.len());
        // `..` 之类的成分一律拒绝，环境变量不该把读取带出 zoneinfo 目录。
        assert!(zoneinfo_paths("../../etc/shadow").is_empty());
        assert!(zoneinfo_paths("/").is_empty(), "根目录不是时区名");
        assert!(zoneinfo_paths("").is_empty());
    }

    // -----------------------------------------------------------------------
    // 真机 smoke
    // -----------------------------------------------------------------------

    #[test]
    fn collects_on_this_machine() {
        let entries = Users.collect(&Context::for_tests()).unwrap();

        // 没有登录会话（容器、CI）时是空的，这也算通过。
        for info in &entries {
            assert_eq!(info.module, "users");
            assert!(info.key == "Users" || info.key.starts_with("Users "));
            assert!(!info.value.is_empty());
            // 值的第一段一定是用户名。
            assert!(!info.value.starts_with(' '));
        }

        // 多个用户时才编号，而且编号从 1 开始、连续。
        if entries.len() > 1 {
            for (index, info) in entries.iter().enumerate() {
                assert_eq!(info.key, format!("Users {}", index + 1));
            }
        }
    }
}

#[cfg(test)]
mod utmp_fuzz_tests {
    use super::*;

    /// 畸形 utmp 记录不 panic。`utmp` 是多个进程共同追加的文件，里面有陈旧记录、
    /// 有断电留下的半个记录——「输入可以是任意字节」是这条解析器的真实前提。
    #[test]
    fn malformed_utmp_records_never_panic() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        for length in 0..(RECORD + 8) {
            let mut record = vec![0u8; length];
            for byte in record.iter_mut() {
                *byte = (next() & 0xff) as u8;
            }

            let _ = parse_record(&record);

            // `c_string` 的偏移是常量，但把极端值也喂进去：一旦内部用 `offset + len`
            // 而不是检查过的加法，这里就会在 debug 下溢出 panic——那是这次审计要问的问题。
            for offset in [0, 1, 8, RECORD / 2, RECORD - 1, usize::MAX - 30, usize::MAX] {
                let _ = c_string(&record, offset, 32);
            }
        }
    }
}
