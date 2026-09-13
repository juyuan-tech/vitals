//! Shell：用户当前在用的 shell。
//!
//! 只报名字，不报版本——拿版本得把 shell 本身跑起来（`zsh --version`），
//! 而 `PLAN.md` 定的是 v0.1 不开任何子进程。

use crate::collectors::{accounts, env};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 会话里实际在跑的 shell。
const ENV: [&str; 1] = ["SHELL"];

/// Shell。
pub struct Shell;

/// 取路径最后一段：`/usr/bin/zsh` → `zsh`。
fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// 组装一条 shell 信息。
fn entry(path: &str) -> Info {
    let name = basename(path).to_owned();
    Info::new("shell", "Shell", name.clone())
        .with_variable("path", path.to_owned())
        .with_variable("name", name)
}

impl Collector for Shell {
    fn name(&self) -> &'static str {
        "shell"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        // 先看环境变量：`$SHELL` 是用户终端里真正在跑的那个。
        if let Some(path) = env::first(&ENV) {
            return Ok(vec![entry(&path)]);
        }

        // 兜底：`/etc/passwd` 里这个账号的登录 shell。
        let Some(account) = accounts::current()? else {
            return Ok(Vec::new());
        };
        if account.shell.is_empty() {
            return Ok(Vec::new());
        }

        Ok(vec![entry(&account.shell)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basename_handles_the_paths_we_see() {
        assert_eq!(basename("/usr/bin/zsh"), "zsh");
        assert_eq!(basename("/bin/sh"), "sh");
        assert_eq!(basename("zsh"), "zsh", "没有斜杠时原样返回");
        assert_eq!(basename("/usr/bin/"), "", "尾斜杠给出空串，不当成 panic");
    }

    #[test]
    fn the_entry_carries_both_name_and_path() {
        let info = entry("/usr/bin/zsh");

        assert_eq!(info.key, "Shell");
        assert_eq!(info.value, "zsh");
        assert_eq!(info.variable("name"), Some("zsh"));
        assert_eq!(info.variable("path"), Some("/usr/bin/zsh"));
    }

    #[test]
    fn collects_the_shell_of_this_session() {
        let entries = Shell.collect(&Context::for_tests()).unwrap();

        // 环境里没有 SHELL、也读不到 passwd 时会是空的；正常情况有一条。
        if let Some(info) = entries.first() {
            assert_eq!(info.key, "Shell");
            assert!(!info.value.is_empty());
            assert!(!info.value.contains('/'), "显示的是名字，不是整条路径");
        }
    }
}
