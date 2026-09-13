//! 当前用户账号：从 uid 查到 `/etc/passwd` 里那一条。
//!
//! 为什么不直接读 `$USER`：`su`、`sudo -u` 之后环境变量可能是陈旧的，
//! 而 uid 是内核给的，骗不了。环境变量只作兜底——精简容器里常常没有
//! 对应的 passwd 条目，那时环境变量是唯一的线索。

use crate::collectors::{env, read};
use crate::core::collector::CollectError;

/// 真实 uid 的来源。
const STATUS: &str = "/proc/self/status";
/// 账号数据库。
const PASSWD: &str = "/etc/passwd";

/// `/etc/passwd` 里的一条。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// 登录名。
    pub name: String,
    /// 用户 id。
    pub uid: u32,
    /// 组 id。
    pub gid: u32,
    /// 备注字段（传统上放全名），常常是空的。
    pub gecos: String,
    /// 家目录。
    pub home: String,
    /// 登录 shell。
    pub shell: String,
}

/// 解析 `/etc/passwd`。
///
/// 每行七段：`name:passwd:uid:gid:gecos:home:shell`。第二段现在是 `x`
/// （真正的口令在 `/etc/shadow`），我们不碰它，也不必管它。
/// 坏行直接跳过——文件里有一条坏行不该让整个模块失败。
fn parse(text: &str) -> Vec<Account> {
    let mut accounts = Vec::new();

    for line in text.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        let [name, _passwd, uid, gid, gecos, home, shell] = fields[..] else {
            continue;
        };
        let (Ok(uid), Ok(gid)) = (uid.parse::<u32>(), gid.parse::<u32>()) else {
            continue;
        };

        accounts.push(Account {
            name: name.to_owned(),
            uid,
            gid,
            gecos: gecos.to_owned(),
            home: home.to_owned(),
            shell: shell.to_owned(),
        });
    }

    accounts
}

/// 从 `/proc/self/status` 取真实 uid。
///
/// 那一行是 `Uid:\t1000\t1000\t1000\t1000`：真实、有效、保存的、文件系统的 uid。
/// 我们只要第一列——真实 uid 才是「我是谁」。
fn parse_uid(text: &str) -> Option<u32> {
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        if key.trim() != "Uid" {
            continue;
        }

        return rest.split_whitespace().next()?.parse().ok();
    }

    None
}

/// 当前账号。
///
/// 三步：拿 uid → 在 `/etc/passwd` 里找它 → 找不到就是无数据。
/// 没有 `/proc` 或没有 `/etc/passwd` 时都返回 `Ok(None)`，不是错误。
pub fn current() -> Result<Option<Account>, CollectError> {
    let Some(status) = read::text(STATUS)? else {
        return Ok(None);
    };
    let Some(uid) = parse_uid(&status) else {
        return Ok(None);
    };
    let Some(passwd) = read::text(PASSWD)? else {
        return Ok(None);
    };

    Ok(parse(&passwd)
        .into_iter()
        .find(|account| account.uid == uid))
}

/// 兜底用的环境变量，顺序即优先级。
pub const ENV: [&str; 2] = ["USER", "LOGNAME"];

/// 当前用户的登录名。
///
/// 顺序：uid 查 `/etc/passwd` → 环境变量。User 与 Title 两个模块都要这个名字，
/// 所以解析放在这里一处，免得两边各写一遍、再各自漂移。
pub fn current_name() -> Result<Option<String>, CollectError> {
    if let Some(account) = current()? {
        return Ok(Some(account.name));
    }

    Ok(env::first(&ENV))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_passwd_line() {
        let accounts = parse("root:x:0:0::/root:/usr/bin/bash\n");

        assert_eq!(accounts.len(), 1);
        assert_eq!(
            accounts[0],
            Account {
                name: "root".to_owned(),
                uid: 0,
                gid: 0,
                gecos: String::new(),
                home: "/root".to_owned(),
                shell: "/usr/bin/bash".to_owned(),
            }
        );
    }

    #[test]
    fn a_gecos_with_commas_and_spaces_survives() {
        // 实测的形状：全名、房间号、电话分段，用逗号隔开。
        let accounts = parse("gxyarch:x:1000:1000:Some One,,,:/home/gxyarch:/usr/bin/zsh\n");
        assert_eq!(accounts[0].name, "gxyarch");
        assert_eq!(accounts[0].uid, 1000);
        assert_eq!(accounts[0].home, "/home/gxyarch");
        assert_eq!(accounts[0].shell, "/usr/bin/zsh");
    }

    #[test]
    fn bad_lines_are_skipped_not_fatal() {
        let accounts = parse(
            "这不是 passwd 行\n\
             missing:x:notanumber:1000::/home/m:/bin/sh\n\
             只有:三段\n\
             good:x:1000:1000::/home/good:/bin/sh\n",
        );

        assert_eq!(accounts.len(), 1, "只有最后一行是好的");
        assert_eq!(accounts[0].name, "good");
    }

    #[test]
    fn reads_the_real_uid_column() {
        // 实测：Uid:\t1000\t1000\t1000\t1000
        assert_eq!(parse_uid("Uid:\t1000\t1000\t1000\t1000\n"), Some(1000));
        assert_eq!(parse_uid("Uid:\t0\t0\t0\t0\n"), Some(0));
        assert_eq!(parse_uid("Gid:\t1000\n"), None, "只看 Uid 那行");
        assert_eq!(parse_uid(""), None);
    }

    #[test]
    fn the_current_account_looks_sane_on_this_machine() {
        let account = current().expect("读 /proc 与 /etc/passwd 不该失败");

        if let Some(account) = account {
            assert!(!account.name.is_empty());
            assert!(account.home.starts_with('/'), "家目录该是绝对路径");
        }
    }
}
