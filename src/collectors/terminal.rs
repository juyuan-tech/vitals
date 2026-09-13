//! Terminal：当前终端。三个来源，按可靠性排序。
//!
//! 1. **环境变量指纹**：kitty、WezTerm、iTerm2、VS Code 都会留下自己的变量。
//!    这是最准的一路——进程名会被包装脚本改掉，环境变量不会。
//! 2. **父进程链**：环境里什么都没有时，顺着 `/proc/<pid>/stat` 往上找第一个
//!    认得出的终端进程名。fastfetch 只看这条路，在嵌套环境里会认错：本机实测它把
//!    `node-MainThread` 报成了终端，而环境变量里明明写着 kitty。
//! 3. **`$TERM`** 兜底。
//!
//! 全程不 fork。

use crate::collectors::{env, read};
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

/// 父进程链最多往上找几层。纯粹是防 `/proc` 数据异常时绕圈。
const MAX_DEPTH: usize = 16;

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
            Info::new(self.name(), "Terminal", terminal.clone()).with_variable("name", terminal),
        ])
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
    let mut pid = std::process::id();

    for _ in 0..MAX_DEPTH {
        let (parent, command) = process_parent(pid)?;

        if let Some(name) = PROCESS_NAMES.iter().find(|name| **name == command) {
            return Some((*name).to_owned());
        }
        // 到 1 号进程就该停了，再往上没有意义。
        if parent <= 1 {
            return None;
        }
        pid = parent;
    }

    None
}

/// 读 `/proc/<pid>/stat` 里的进程名与父进程号。
fn process_parent(pid: u32) -> Option<(u32, String)> {
    let stat = read::text(&format!("/proc/{pid}/stat")).ok().flatten()?;

    // 名字在括号里，且可能含空格与括号，所以从最后一个 ')' 定位。
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let command = stat.get(open + 1..close)?.to_owned();

    // `)` 之后第一个字段是 state，第二个才是 ppid。
    let parent = stat
        .get(close + 1..)?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;

    Some((parent, command))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_parent_and_name_of_a_real_process() {
        // 1 号进程一定在，而且一定有父进程字段（值为 0）。
        let (parent, command) = process_parent(1).expect("1 号进程该读得到");

        assert_eq!(parent, 0, "1 号进程的父进程是 0");
        assert!(!command.is_empty());
    }

    #[test]
    fn parses_the_name_of_the_current_process() {
        let (_, command) = process_parent(std::process::id()).expect("自己总该读得到");

        // cargo test 跑的是测试二进制，名字里带 crate 名。
        assert!(!command.is_empty());
    }

    #[test]
    fn a_missing_process_is_no_data() {
        // 这个 pid 不可能存在。
        assert!(process_parent(u32::MAX).is_none());
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
