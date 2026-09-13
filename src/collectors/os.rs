//! OS：发行版名称。数据来自 `/etc/os-release`。

use rustix::system::uname;

use crate::collectors::os_release;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 操作系统。
pub struct Os;

impl Collector for Os {
    fn name(&self) -> &'static str {
        "os"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(release) = os_release::read()? else {
            return Ok(Vec::new());
        };

        let Some(name) = release.display_name() else {
            return Ok(Vec::new());
        };

        // 变量表放原始字段，给将来的模板用——显示值只是它的一个视图。
        // 值带上机器架构（`Arch Linux x86_64`）。用 `std::env::consts::ARCH` 会把
        // **编译时**的架构当成机器的，那是两回事：交叉编译出来的二进制会撒谎。
        let machine = uname().machine().to_string_lossy().into_owned();
        let mut info = Info::new(self.name(), "OS", format!("{name} {machine}"))
            .with_variable("machine", machine);
        for (key, value) in [
            ("id", &release.id),
            ("id_like", &release.id_like),
            ("name", &release.name),
            ("pretty_name", &release.pretty_name),
            ("version_id", &release.version_id),
        ] {
            if let Some(value) = value {
                info = info.with_variable(key, value.clone());
            }
        }

        Ok(vec![info])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_distribution_on_this_machine() {
        let context = Context::for_tests();
        let entries = Os.collect(&context).expect("读 os-release 不该失败");

        // 容器里也可能没有 os-release；有的话就必须是完整的。
        if let Some(info) = entries.first() {
            assert_eq!(info.module, "os");
            assert_eq!(info.key, "OS");
            assert!(!info.value.is_empty());
            // 这台机器上一定读得到 ID，而且用它挑 Logo，所以必须带在变量里。
            assert!(info.variable("id").is_some(), "变量表里该有 id");
        }
    }

    #[test]
    fn an_empty_file_is_no_data_not_a_failure() {
        let release = os_release::parse("");
        assert_eq!(release.display_name(), None);
    }
}
