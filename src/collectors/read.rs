//! 读文件的公共部分：把「文件不存在」和「真的读不了」分开。
//!
//! 这条区分是整个错误策略的落点。`PLAN.md` 要求「文件不存在返回无数据、不报错」，
//! 而权限不足、不是 UTF-8 这些**要**报出来。`std::fs` 把两者都塞进 `io::Error`，
//! 所以在这一处按 `ErrorKind` 分开，后面每个模块就都不用再操心了。
//!
//! 为什么不引 `cap-std` 之类的库：这里只需要读几个固定路径，不需要能力抽象。

use std::fs::File;
use std::io::{ErrorKind, Read};

use crate::core::collector::CollectError;
use crate::core::sources;

/// 单个文件的读取上限。
///
/// 为什么要有：路径不总是我们给的——`$TZ`、`$TZDIR` 这类环境变量会被拼进路径，而
/// `std::fs::read` 会把文件整个读进内存。一个指向 `/dev/zero` 的时区路径足以把进程
/// 吃到 OOM。8 MiB 比我们要读的**任何**文件都大得多（`utmp` 在繁忙机器上几 MB，
/// `/sys`、EDID、`os-release` 都是 KB 级），正常路径完全不受影响，所以超限就报错，
/// 不静默截断——截断过的数据会被下游当成完整的，那比报错更坏。
pub const MAX_READ: u64 = 8 * 1024 * 1024;

/// 有上限地读一个文件的全部字节。
///
/// 与 [`text`] / [`bytes`] 共用同一套错误规矩：不存在是**无数据**，其他失败是**真失败**。
fn read_capped(path: &str) -> Result<Option<Vec<u8>>, CollectError> {
    sources::record(path);

    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(CollectError::caused_by(format!("打开 {path} 失败"), source)),
    };

    let mut content = Vec::new();
    // 多读一个字节：正好等于上限时也能分清「就是这么大」与「还有更多」。
    if let Err(source) = file.take(MAX_READ + 1).read_to_end(&mut content) {
        return Err(CollectError::caused_by(format!("读取 {path} 失败"), source));
    }

    if content.len() as u64 > MAX_READ {
        return Err(CollectError::new(format!(
            "{path} 超过读取上限（{MAX_READ} 字节），拒绝读进内存"
        )));
    }

    Ok(Some(content))
}

/// 读一个文件，并去掉首尾空白。
///
/// - 读到了 → `Ok(Some(内容))`
/// - 文件不存在 → `Ok(None)`，这是**无数据**，不是错误
/// - 其他失败（权限、编码、是目录）→ `Err`，这是**真失败**，主流程会记一条警告
pub fn text(path: &str) -> Result<Option<String>, CollectError> {
    let Some(bytes) = read_capped(path)? else {
        return Ok(None);
    };

    let content = String::from_utf8(bytes)
        .map_err(|error| CollectError::caused_by(format!("{path} 不是 UTF-8"), error))?;

    Ok(Some(content.trim().to_owned()))
}

/// 读一个文件的**原始字节**。
///
/// 和 [`text`] 同一套错误规矩：不存在是**无数据**，其他失败是**真失败**。
/// 二进制内容（EDID、DMI 条目）走这条——它们不是 UTF-8，用 [`text`] 读会失败。
pub fn bytes(path: &str) -> Result<Option<Vec<u8>>, CollectError> {
    read_capped(path)
}

/// 按顺序读第一个**存在**的文件。
///
/// 用来实现规范里的回退链（`/etc/os-release` → `/usr/lib/os-release`）。
///
/// 注意：第一个文件存在但**读不了**时直接报错，不会偷偷跳到下一个——
/// 「文件在那儿但没权限」是真失败，掩盖它只会让人查不出问题。
pub fn first(paths: &[&str]) -> Result<Option<String>, CollectError> {
    for path in paths {
        if let Some(content) = text(path)? {
            return Ok(Some(content));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_not_an_error() {
        // 这是整个错误策略的核心一条：不存在 ≠ 出错。
        assert_eq!(text("/definitely/not/here").unwrap(), None);
    }

    #[test]
    fn bytes_reads_binary_content() {
        // EDID、DMI 条目都是二进制，用 `text` 读会因为不是 UTF-8 而失败。
        let content = bytes("/proc/self/cmdline").unwrap();

        assert!(content.is_some(), "cmdline 该读得到");
        assert_eq!(bytes("/definitely/not/here").unwrap(), None);
    }

    #[test]
    fn existing_file_is_trimmed() {
        let paths = ["/proc/sys/kernel/ostype"];
        let content = first(&paths).unwrap();

        // 这个文件在任何 Linux 上都有，且内容只有 "Linux"。
        let content = content.expect("任何 Linux 都该有 /proc/sys/kernel/ostype");
        assert_eq!(content, content.trim());
        assert!(!content.is_empty());
    }

    #[test]
    fn first_returns_the_first_existing_file() {
        let paths = ["/definitely/not/here", "/proc/sys/kernel/ostype"];
        assert!(first(&paths).unwrap().is_some());
    }

    #[test]
    fn first_with_no_existing_file_is_none() {
        let paths = ["/definitely/not/here", "/also/not/here"];
        assert_eq!(first(&paths).unwrap(), None);
    }
}
