//! 记录这一趟**真的读了哪些文件**。
//!
//! 为什么在运行时记录，而不是让每个模块手写一张「数据来源」表：**手写的表会跟代码漂移**。
//! 改了读取逻辑、忘了改表，没有任何东西会发现这两者已经不一致了；而记在读取的那一层
//! （`collectors::read::text` / `bytes`）描述的就永远是实际发生的事——说读了
//! `/proc/mounts`，就是真读了。
//!
//! 代价是每次读取多一次 `Vec::push`。比起「来源表可能是假的」，这个代价值得。
//!
//! 用 `thread_local` + `RefCell`（都在安全 Rust 里）：读取点在 `read::` 那一层，拿不到
//! `Context`，而本程序的采集是单线程跑一趟，不存在并发争用。记录的是**尝试**而不是成功
//! ——「看过但文件不存在」本身就是要告诉用户的信息。

use std::cell::RefCell;

thread_local! {
    /// 当前这个模块碰过的路径，按首次碰到的顺序，已去重。
    static READ: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// 记一条：真的碰了这个路径。
///
/// 同一个路径在一个模块里读两次只记一次——报告要回答的是「依据是什么」，
/// 不是「读了几次」。
pub(crate) fn record(path: &str) {
    READ.with_borrow_mut(|paths| {
        if !paths.iter().any(|seen| seen == path) {
            paths.push(path.to_owned());
        }
    });
}

/// 清空记录。
///
/// 与 [`take`] 分开是因为两种意图不同：调度器在**每个模块开始前**清空（扔掉上一个模块的
/// 残留，不看内容），跑完再 `take` 取走（要看内容）。用一个 `take` 兼两职会写出
/// `let _ = take()` 那种看不出意图的代码。
pub fn clear() {
    READ.with_borrow_mut(Vec::clear);
}

/// 取走当前模块的记录并清空。
///
/// 调度器在**每个模块开始前**先清空、跑完再取走，这样来源永远归属到正确的模块，
/// 不会把上一个模块读的文件算到下一个头上。
#[must_use]
pub fn take() -> Vec<String> {
    READ.with_borrow_mut(std::mem::take)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_in_order_and_dedupes() {
        clear();

        record("/proc/mounts");
        record("/etc/os-release");
        record("/proc/mounts");

        assert_eq!(take(), ["/proc/mounts", "/etc/os-release"]);
    }

    #[test]
    fn taking_clears() {
        record("/sys/class/dmi/id/product_name");

        assert_eq!(take().len(), 1);
        assert!(take().is_empty(), "取走之后该是空的");
    }
}
