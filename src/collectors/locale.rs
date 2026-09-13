//! Locale：当前区域设置。
//!
//! 顺序照 POSIX：`LC_ALL` > `LC_CTYPE` > `LANG`——前面的非空才轮到后面。
//! 环境变量一个都没有时，退到 `/etc/locale.conf`（systemd 系发行版写的全局设置），
//! 仍然没有就算无数据。

use crate::collectors::{env, read};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 环境变量，按优先级。
const ENV: [&str; 3] = ["LC_ALL", "LC_CTYPE", "LANG"];
/// systemd 发行版的全局设置。
const CONF: &str = "/etc/locale.conf";
/// 配置里的键。
const KEY: &str = "LANG";

/// 区域设置。
pub struct Locale;

impl Collector for Locale {
    fn name(&self) -> &'static str {
        "locale"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let value = match env::first(&ENV) {
            Some(value) => value,
            None => match read::text(CONF)?.as_deref().and_then(parse_conf) {
                Some(value) => value,
                None => return Ok(Vec::new()),
            },
        };

        Ok(vec![
            Info::new(self.name(), "Locale", value.clone()).with_variable("value", value),
        ])
    }
}

/// 从 `/etc/locale.conf` 里取 `LANG=`。
///
/// 文件形如 `LANG=zh_CN.UTF-8`，值可能带引号；注释以 `#` 开头。
fn parse_conf(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != KEY {
            continue;
        }

        let value = value.trim().trim_matches(['"', '\'']);
        if !value.is_empty() {
            return Some(value.to_owned());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_lang_from_the_conf_file() {
        let conf = "# 注释\nLANG=zh_CN.UTF-8\nLC_TIME=en_GB.UTF-8\n";
        assert_eq!(parse_conf(conf).as_deref(), Some("zh_CN.UTF-8"));
    }

    #[test]
    fn quotes_and_spaces_are_tolerated() {
        assert_eq!(
            parse_conf("LANG = \"en_US.UTF-8\"\n").as_deref(),
            Some("en_US.UTF-8")
        );
        assert_eq!(parse_conf("LANG='C'\n").as_deref(), Some("C"));
    }

    #[test]
    fn a_conf_without_lang_is_no_data() {
        assert_eq!(parse_conf("LC_TIME=en_GB.UTF-8\n"), None);
        assert_eq!(parse_conf("LANG=\n"), None, "空值不算数");
        assert_eq!(parse_conf("#LANG=zh_CN.UTF-8\n"), None, "注释掉的也不算");
        assert_eq!(parse_conf(""), None);
    }

    #[test]
    fn collects_on_this_machine_if_the_environment_says_so() {
        let entries = Locale.collect(&Context::for_tests()).unwrap();

        // 容器里可能既没有环境变量也没有 locale.conf，那就是无数据。
        if let Some(info) = entries.first() {
            assert_eq!(info.key, "Locale");
            assert!(!info.value.is_empty());
        }
    }
}
