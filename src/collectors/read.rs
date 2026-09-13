//! 读文件的公共部分：把「文件不存在」和「真的读不了」分开。
//!
//! 这条区分是整个错误策略的落点。`PLAN.md` 要求「文件不存在返回无数据、不报错」，
//! 而权限不足、不是 UTF-8 这些**要**报出来。`std::fs` 把两者都塞进 `io::Error`，
//! 所以在这一处按 `ErrorKind` 分开，后面每个模块就都不用再操心了。
//!
//! 为什么不引 `cap-std` 之类的库：这里只需要读几个固定路径，不需要能力抽象。

use std::io::ErrorKind;

use crate::core::collector::CollectError;

/// 读一个文件，并去掉首尾空白。
///
/// - 读到了 → `Ok(Some(内容))`
/// - 文件不存在 → `Ok(None)`，这是**无数据**，不是错误
/// - 其他失败（权限、编码、是目录）→ `Err`，这是**真失败**，主流程会记一条警告
pub fn text(path: &str) -> Result<Option<String>, CollectError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content.trim().to_owned())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(CollectError::caused_by(format!("读取 {path} 失败"), source)),
    }
}

/// 读一个文件的**原始字节**。
///
/// 和 [`text`] 同一套错误规矩：不存在是**无数据**，其他失败是**真失败**。
/// 二进制内容（EDID、DMI 条目）走这条——它们不是 UTF-8，用 [`text`] 读会失败。
pub fn bytes(path: &str) -> Result<Option<Vec<u8>>, CollectError> {
    match std::fs::read(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(CollectError::caused_by(format!("读取 {path} 失败"), source)),
    }
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
