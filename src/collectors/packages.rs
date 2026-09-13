//! Packages：数一数这台机器上装了多少个包。
//!
//! 一个子进程都不开（`PLAN.md` §0-16 就是为这条把「跑 `pacman -Q`」换掉的）：
//! pacman / flatpak 数目录项，snap 数 `*.snap` 文件，appimage 数 `~/AppImages` 下
//! `*.appimage` 文件，Debian 系数 `/var/lib/dpkg/status` 里的记录。数法都在
//! [`crate::collectors::pkgdb`] 里，与「查某个包的版本」共用同一批路径常量。
//!
//! 不做的：rpm / nix / xbps / apk 的库要么是 sqlite、要么是自定义格式，
//! 不开子进程就得写一个解析器，那是另一个工作量。不做「先写着，读不到就报 0」
//! 的假条目——报 0 比不报更糟，用户会以为这台机器一个包都没装。
//!
//! 值是**一行**：`3 (appimage), 3 (flatpak), 1042 (pacman)`，每项 `<数量> (<名字>)`，
//! 按名字字母序排（不按发现顺序：那取决于哪个数据库先被读到，
//! 让文件系统的遍历顺序影响输出是没道理的）。
//! 数量为 0 的包管理器不出现在结果里；一个都数不到 → 无数据。
//!
//! **flatpak 只数应用、不数 runtime**：本机 app 目录 3 个、runtime 目录 7 个（其中 2 个
//! 是 `*.Locale` 扩展），fastfetch 报 8（3 + 5，看着就是「应用 + 非 Locale 的 runtime」）。
//! 我们保留 3——用户问「装了几个 flatpak 包」问的是应用，把 runtime 算进去只是让数字变大。
//! 这是刻意偏差，记在 `PLAN.md` §5.8。

use crate::collectors::pkgdb;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 包。
pub struct Packages;

impl Collector for Packages {
    fn name(&self) -> &'static str {
        "packages"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let counts = pkgdb::counts()?;
        if counts.is_empty() {
            return Ok(Vec::new());
        }

        let value = counts
            .iter()
            .map(|count| format!("{} ({})", count.count, count.name))
            .collect::<Vec<_>>()
            .join(", ");

        let mut info = Info::new(self.name(), "Packages", value);
        // 每个包管理器一个变量：模板里想要单独一项时不用去切字符串。
        for count in &counts {
            info = info.with_variable(count.name, count.count.to_string());
        }

        Ok(vec![info])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_on_this_machine_if_any_package_manager_is_known() {
        let entries = Packages.collect(&Context::for_tests()).unwrap();

        // 极小容器里可能一个数据库都没有，那就是无数据。
        match entries.first() {
            None => {}
            Some(info) => {
                assert_eq!(info.key, "Packages");
                assert_eq!(info.module, "packages");
                assert!(info.value.ends_with(')'), "每项都以 (名字) 结尾");
                assert!(!info.value.contains('\n'), "值必须是一行");
                // 数量与名字是配对的：`1042 (pacman)`。
                for item in info.value.split(", ") {
                    let (count, name) = item.split_once(" (").expect("形如 <数量> (<名字>)");
                    assert!(count.parse::<u64>().is_ok(), "数量该是数字：{count}");
                    assert!(name.ends_with(')') && name.len() > 2, "名字该有括号");
                }
            }
        }
    }

    #[test]
    fn the_order_is_alphabetical_and_independent_of_discovery() {
        let entries = Packages.collect(&Context::for_tests()).unwrap();
        let Some(info) = entries.first() else { return };

        let names: Vec<&str> = info
            .value
            .split(", ")
            .map(|item| item.split_once(" (").unwrap().1.trim_end_matches(')'))
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();

        assert_eq!(names, sorted, "输出顺序按名字字母序");
    }

    #[test]
    fn every_variable_matches_the_value() {
        let entries = Packages.collect(&Context::for_tests()).unwrap();
        let Some(info) = entries.first() else { return };

        // 变量表与显示值出自同一次统计，不许各算各的。
        for (name, count) in &info.variables {
            let item = format!("{count} ({name})");
            assert!(
                info.value.split(", ").any(|part| part == item),
                "变量 {name}={count} 在值里找不到对应项：{}",
                info.value
            );
        }
    }
}
