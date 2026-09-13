//! User：当前用户名。
//!
//! 「我是谁」以 uid 为准（见 [`accounts`]），环境变量只兜底。

use crate::collectors::{accounts, env};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 用户。
pub struct User;

impl Collector for User {
    fn name(&self) -> &'static str {
        "user"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let account = accounts::current()?;

        let name = match &account {
            Some(account) => account.name.clone(),
            // 容器里常常没有对应的 passwd 条目，这时只能信环境变量。
            None => match env::first(&accounts::ENV) {
                Some(name) => name,
                None => return Ok(Vec::new()),
            },
        };

        let mut info = Info::new(self.name(), "User", name.clone()).with_variable("name", name);
        if let Some(account) = &account {
            info = info
                .with_variable("uid", account.uid.to_string())
                .with_variable("home", account.home.clone());
        }

        Ok(vec![info])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_the_current_user() {
        let entries = User.collect(&Context::for_tests()).unwrap();

        assert_eq!(entries.len(), 1, "这台机器上该能拿到用户");
        assert_eq!(entries[0].module, "user");
        assert_eq!(entries[0].key, "User");
        assert!(!entries[0].value.is_empty());
        assert!(entries[0].variable("uid").is_some(), "该带上 uid");
    }

    #[test]
    fn the_name_matches_the_environment_when_there_is_no_passwd_entry() {
        // 这条只保证「两个来源不会互相矛盾」：本机上 uid 一定能查到，
        // 所以 value 应当等于 passwd 里的名字，也就等于 $USER。
        let entries = User.collect(&Context::for_tests()).unwrap();
        if let Some(from_env) = env::var("USER") {
            assert_eq!(entries[0].value, from_env);
        }
    }
}
