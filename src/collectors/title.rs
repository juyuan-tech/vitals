//! Title：`用户@主机名`，fastfetch 默认视图的第一行。
//!
//! 用户名以 uid 为准（同 User 模块：`su`、`sudo` 之后 `$USER` 可能是陈旧的），
//! 主机名走 `uname(2)` 的 nodename。注意这**不是** Host 模块那个「机器型号」：
//! Host 是 DMI 里的产品名，这里是主机名，笔记本上两者完全不同。

use rustix::system::uname;

use crate::collectors::accounts;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 标题。
pub struct Title;

impl Collector for Title {
    fn name(&self) -> &'static str {
        "title"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let value = match (accounts::current_name()?, hostname()) {
            (Some(user), Some(host)) => format!("{user}@{host}"),
            (Some(user), None) => user,
            (None, Some(host)) => host,
            // 既没有用户名也没有主机名，没什么可当标题的。
            (None, None) => return Ok(Vec::new()),
        };

        // 键是空的：这是标题，渲染器见到空键就只印值，不补 `: `。
        Ok(vec![Info::new(self.name(), "", value)])
    }
}

/// 主机名。`uname` 在任何 Unix 上都不会失败。
fn hostname() -> Option<String> {
    let uts = uname();
    let host = uts.nodename().to_string_lossy().into_owned();

    (!host.is_empty()).then_some(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_has_no_key() {
        let entries = Title.collect(&Context::for_tests()).unwrap();

        if let Some(info) = entries.first() {
            assert_eq!(info.key, "", "标题不该印成 `键: 值`");
            assert!(!info.value.is_empty());
        }
    }

    #[test]
    fn uname_gives_a_hostname_on_this_machine() {
        // 容器里 nodename 也可能是空的，那时模块无数据——不算失败。
        if let Some(host) = hostname() {
            assert!(!host.contains(char::is_whitespace), "主机名不该含空白");
        }
    }
}
