//! Editor：默认编辑器。
//!
//! 顺序照 POSIX：`$VISUAL` 给全屏编辑器用，`$EDITOR` 是通用的，前者更具体。
//! 值是**名字**不是路径（同 Shell 模块的口径）：`/usr/bin/vim` 印成 `vim`，
//! 要看完整路径的去 JSON 的变量里拿。

use std::path::Path;

use crate::collectors::env;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 环境变量，按优先级。
const ENV: [&str; 2] = ["VISUAL", "EDITOR"];

/// 编辑器。
pub struct Editor;

impl Collector for Editor {
    fn name(&self) -> &'static str {
        "editor"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(path) = env::first(&ENV) else {
            return Ok(Vec::new());
        };

        let name = basename(&path);
        if name.is_empty() {
            return Ok(Vec::new());
        }

        Ok(vec![
            Info::new(self.name(), "Editor", name.to_owned())
                .with_variable("name", name.to_owned())
                .with_variable("path", path),
        ])
    }
}

/// 路径的最后一段：`/usr/bin/vim` → `vim`。
///
/// 带参数的写法（`EDITOR="code --wait"`）不特殊处理：那不是一个可执行路径，
/// 原样显示反而让人一眼看出环境变量写得不对。
fn basename(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basename_takes_the_last_segment() {
        assert_eq!(basename("/usr/bin/vim"), "vim");
        assert_eq!(basename("/usr/bin/nvim"), "nvim");
        assert_eq!(basename("vim"), "vim");
    }

    #[test]
    fn the_trailing_slash_and_empty_cases_do_not_panic() {
        // `Path` 会把尾斜杠规范化掉，所以 `/usr/bin/` 取出来还是 `bin`——这正是
        // 想要的。（Shell 模块那个手写 basename 在这里会给空串，是它的短板。）
        assert_eq!(basename("/usr/bin/"), "bin");
        assert_eq!(basename(""), "");
    }

    #[test]
    fn a_command_with_arguments_is_left_alone() {
        // 不是路径，Path 也取不出 file_name，于是原样带出来。
        assert_eq!(basename("code --wait"), "code --wait");
    }

    #[test]
    fn collects_on_this_machine_if_the_environment_says_so() {
        let entries = Editor.collect(&Context::for_tests()).unwrap();

        if let Some(info) = entries.first() {
            assert_eq!(info.key, "Editor");
            assert!(!info.value.is_empty());
        }
    }
}
