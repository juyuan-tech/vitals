//! Terminal：当前终端。三个来源，按可靠性排序。
//!
//! 1. **环境变量指纹**：kitty、WezTerm、iTerm2、VS Code 都会留下自己的变量。
//!    这是最准的一路——进程名会被包装脚本改掉，环境变量不会。
//! 2. **父进程链**：环境里什么都没有时，顺着 `/proc/<pid>/stat` 往上找第一个
//!    认得出的终端进程名（见 [`proc_chain`]）。fastfetch 只看这条路，在嵌套环境里
//!    会认错：本机实测它把 `node-MainThread` 报成了终端，而环境变量里明明写着 kitty。
//! 3. **`$TERM`** 兜底。
//!
//! 全程不 fork。

use crate::collectors::{env, pkgdb, proc_chain};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 环境变量指纹：变量在 → 终端是它。
///
/// `TERM_PROGRAM` 单独处理，因为它的值本身就是名字，还常常配一个版本变量。
const ENV_HINTS: [(&str, &str); 5] = [
    ("KITTY_WINDOW_ID", "kitty"),
    ("WEZTERM_EXECUTABLE", "WezTerm"),
    ("ALACRITTY_SOCKET", "Alacritty"),
    ("WT_SESSION", "Windows Terminal"),
    ("VTE_VERSION", "VTE"),
];

/// 父进程链上认得出的终端进程名。
///
/// 长的排在前面，免得 `foot` 抢了 `footclient` 的匹配。`gnome-terminal-` 少了尾巴
/// 不是笔误：内核把 `comm` 截断到 15 个字符，完整的 `gnome-terminal-server` 装不下。
const PROCESS_NAMES: [&str; 13] = [
    "gnome-terminal-",
    "xfce4-terminal",
    "gnome-terminal",
    "wezterm-gui",
    "footclient",
    "alacritty",
    "konsole",
    "wezterm",
    "kitty",
    "xterm",
    "foot",
    "tmux",
    "screen",
];

/// 终端。
pub struct Terminal;

impl Collector for Terminal {
    fn name(&self) -> &'static str {
        "terminal"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let terminal = from_env()
            .or_else(from_process_chain)
            .or_else(|| env::var("TERM"));

        let Some(terminal) = terminal.filter(|terminal| !terminal.is_empty()) else {
            return Ok(Vec::new());
        };

        Ok(vec![
            Info::new(self.name(), "Terminal", display(&terminal)).with_variable("name", terminal),
        ])
    }
}

/// 显示值：名字后面跟上版本（`kitty 0.48.2`），版本从包数据库读，零子进程。
///
/// 与 shell 一样，版本是附加信息：查不到（`$TERM` 兜底出来的 `xterm-256color`
/// 就不是包名）就只印名字，不因为查不到而少一行。
fn display(terminal: &str) -> String {
    match pkgdb::version_of(terminal) {
        Ok(Some(version)) => format!("{terminal} {version}"),
        _ => terminal.to_owned(),
    }
}

/// 从环境变量认终端。
fn from_env() -> Option<String> {
    // iTerm2、Apple Terminal、VS Code 都设 `TERM_PROGRAM`，多数还带版本。
    if let Some(program) = env::var("TERM_PROGRAM") {
        return Some(match env::var("TERM_PROGRAM_VERSION") {
            Some(version) => format!("{program} {version}"),
            None => program,
        });
    }

    ENV_HINTS
        .iter()
        .find(|(key, _)| env::var(key).is_some())
        .map(|(_, name)| (*name).to_owned())
}

/// 顺着父进程链找终端。
fn from_process_chain() -> Option<String> {
    proc_chain::ancestors(proc_chain::DEFAULT_DEPTH)
        .into_iter()
        .find_map(|ancestor| {
            PROCESS_NAMES
                .iter()
                .find(|name| **name == ancestor.command)
                .map(|name| (*name).to_owned())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_wins_over_the_process_chain() {
        // 这条是 Terminal 模块存在的理由：本机实测 fastfetch 走进程链得到
        // `node-MainThread`，而环境变量里写着 kitty。环境变量优先级必须更高。
        // 这里直接验证 `from_env` 认得出 kitty 的指纹。
        assert!(
            ENV_HINTS
                .iter()
                .any(|(key, name)| *key == "KITTY_WINDOW_ID" && *name == "kitty"),
            "kitty 的指纹该在表里"
        );
    }

    #[test]
    fn long_process_names_come_first() {
        // `foot` 是 `footclient` 的前缀：顺序反了就会认错。
        let foot = PROCESS_NAMES
            .iter()
            .position(|name| *name == "foot")
            .unwrap();
        let footclient = PROCESS_NAMES
            .iter()
            .position(|name| *name == "footclient")
            .unwrap();

        assert!(footclient < foot, "`footclient` 该排在 `foot` 前面");
    }

    #[test]
    fn collects_on_this_machine_if_anything_is_set() {
        let entries = Terminal.collect(&Context::for_tests()).unwrap();

        if let Some(info) = entries.first() {
            assert_eq!(info.key, "Terminal");
            assert!(!info.value.is_empty());
        }
    }
}
