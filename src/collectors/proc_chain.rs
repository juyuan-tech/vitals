//! 父进程链：从自己往上数几层，看得到祖先进程的名字。
//!
//! 终端、桌面环境、窗口管理器都是我们进程的**祖先**——shell 由终端拉起，终端坐在
//! 会话里，会话由合成器或桌面托管。所以问「我上头是谁」比扫整个 `/proc` 便宜得多：
//! 本机 434 个进程逐个打开一遍要 6.9 ms（`processes` 模块的旧实现量过这个数），
//! 顺着链子走 16 层是 0.3 ms 量级。
//!
//! 这里只如实报告链子上有什么，**不做匹配**：认谁是调用方的事。三个调用方
//! （`terminal`、`de`、`wm`）各有一张自己的表，互不干扰。
//!
//! 全程不 fork。

use crate::collectors::read;

/// 默认最多往上走几层。
///
/// 16 层足够穿过 shell → 终端 → 会话 → 合成器；定个上限纯粹是防 `/proc` 数据
/// 异常时绕圈（比如谁都不认的 pid 反复指向自己）。
pub const DEFAULT_DEPTH: usize = 16;

/// 链子上的一环。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ancestor {
    /// 进程号。
    pub pid: u32,
    /// `comm` 的内容。内核把它截断到 15 个字符，而且**允许空格与括号**。
    pub command: String,
}

/// 从当前进程开始往上，最多 `depth` 层（含 1 号进程）。
///
/// 中途读不到就停下——进程可能正好在这两次读取之间退出了。能拿到的部分仍然有用，
/// 所以**不报错**，返回已经收集到的链子。`depth` 为 0 就是空链子。
#[must_use]
pub fn ancestors(depth: usize) -> Vec<Ancestor> {
    let mut chain = Vec::new();
    let mut pid = std::process::id();

    for _ in 0..depth {
        let Some((parent, command)) = parent_of(pid) else {
            break;
        };

        chain.push(Ancestor { pid, command });

        // 到 1 号（init）就该停了，再往上没有意义。
        if parent <= 1 {
            break;
        }
        pid = parent;
    }

    chain
}

/// 读 `/proc/<pid>/stat` 里的进程名与父进程号。
///
/// 名字在括号里，而且**可能含空格与括号**，所以从最后一个 `')'` 定位，
/// 不能按空白切——`(Web Content)` 那样的名字会把按空白切的实现带沟里。
pub fn parent_of(pid: u32) -> Option<(u32, String)> {
    let stat = read::text(&format!("/proc/{pid}/stat")).ok().flatten()?;

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
        let (parent, command) = parent_of(1).expect("1 号进程该读得到");

        assert_eq!(parent, 0, "1 号进程的父进程是 0");
        assert!(!command.is_empty());
    }

    #[test]
    fn parses_the_name_of_the_current_process() {
        let (_, command) = parent_of(std::process::id()).expect("自己总该读得到");

        // cargo test 跑的是测试二进制，名字里带 crate 名。
        assert!(!command.is_empty());
    }

    #[test]
    fn a_missing_process_is_no_data() {
        // 这个 pid 不可能存在。
        assert!(parent_of(u32::MAX).is_none());
    }

    #[test]
    fn the_chain_starts_at_ourselves() {
        let chain = ancestors(DEFAULT_DEPTH);

        assert_eq!(
            chain.first().map(|ancestor| ancestor.pid),
            Some(std::process::id())
        );
        // 测试进程必然有父进程（cargo 或 shell），链子不该是空的。
        assert!(chain.len() > 1, "链子该不止自己一个");
    }

    #[test]
    fn a_zero_depth_chain_is_empty() {
        assert!(ancestors(0).is_empty());
    }
}
