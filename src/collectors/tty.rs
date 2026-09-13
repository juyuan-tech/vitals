//! 终端尺寸：问内核要（`tcgetwinsize`）。
//!
//! 走 stdout 而不是 `/dev/tty`：输出被重定向到管道时「终端多大」本来就没有
//! 确定答案，而那正是「不知道」该有的回答——渲染器据此决定不隐藏 Logo。
//!
//! 零 unsafe：`rustix` 把 `ioctl(TIOCGWINSZ)` 包成了安全函数。

/// 终端的（列数, 行数）。
///
/// 拿不到、或者内核报了个 0，都算「不知道」。行列为 0 是 `tcgetwinsize` 在
/// 非终端上的典型返回，不是「0 行高的终端」。
#[must_use]
pub fn size() -> Option<(usize, usize)> {
    let size = rustix::termios::tcgetwinsize(std::io::stdout()).ok()?;
    let columns = usize::from(size.ws_col);
    let rows = usize::from(size.ws_row);

    (columns > 0 && rows > 0).then_some((columns, rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_is_either_unknown_or_plausible() {
        // 这条测试在终端里跑和在 CI（管道）里跑，答案不一样，两种都对。
        match size() {
            None => {}
            Some((columns, rows)) => {
                assert!(columns > 0 && rows > 0, "问到了就该是正数");
                assert!(columns < 100_000 && rows < 100_000, "别是读串了字节序");
            }
        }
    }
}
