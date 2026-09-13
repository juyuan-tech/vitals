//! 读环境变量：统一「空值等于没设」这条规矩。
//!
//! 环境变量和文件系统一样是**外部输入**，模块不该各自去猜
//! `USER=""` 算不算设了。这里一处定死。

/// 读一个环境变量，去掉首尾空白。
///
/// 空值当作没设——对使用者来说 `USER=""` 和 `USER` 不存在是一回事。
/// 不是 UTF-8 的值也当作没设（`var` 会报错）：系统信息工具没必要为这个报错。
#[must_use]
pub fn var(key: &str) -> Option<String> {
    let value = std::env::var(key).ok()?;
    let value = value.trim();

    if value.is_empty() {
        return None;
    }

    Some(value.to_owned())
}

/// 按顺序取第一个有值的。
#[must_use]
pub fn first(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| var(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_variable_is_none() {
        assert_eq!(var("VITALS_SURELY_NOT_SET_12345"), None);
    }

    #[test]
    fn first_over_unknown_keys_is_none() {
        assert_eq!(
            first(&["VITALS_SURELY_NOT_SET_12345", "VITALS_ALSO_NOT_SET"]),
            None
        );
    }

    #[test]
    fn first_takes_the_first_one_that_is_set() {
        // PATH 在任何正常环境里都有；用它验证「取第一个有值的」这条逻辑。
        let value = first(&["VITALS_SURELY_NOT_SET_12345", "PATH"]);
        assert!(value.is_some(), "PATH 该是有的");
    }
}
