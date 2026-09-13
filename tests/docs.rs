//! 文档与示例配置不能和程序走散。
//!
//! 这三条测试盯的是「文档说的事」和「程序实际做的事」之间最容易腐烂的连接点：
//! 示例配置能不能真的跑、`presets/all.toml` 是不是等于注册表里的全部模块、
//! 模块参考是不是覆盖了每一个模块。加了模块忘了改这三处，测试就会红。
//!
//! 刻意不引入正则库：解析只用字符串操作（这个项目不新增依赖）。

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// 注册表里的模块名，顺序就是 `--list-modules` 的顺序。
fn module_list() -> Vec<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_vitals"))
        .arg("--list-modules")
        .output()
        .expect("该能跑 --list-modules");

    assert!(output.status.success(), "--list-modules 失败了");

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// 从一份配置里取出 `type = "..."` 列出的模块名（按出现顺序）。
fn config_modules(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("读不了 {}：{error}", path.display()))
        .lines()
        .filter_map(|line| line.trim().strip_prefix("type = \""))
        .filter_map(|rest| rest.strip_suffix('"'))
        .map(str::to_owned)
        .collect()
}

fn sorted_unique(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names.dedup();
    names
}

#[test]
fn every_preset_is_a_config_that_runs() {
    let dir = root().join("presets");
    let mut presets: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("读不了 {}：{error}", dir.display()))
        .map(|entry| entry.expect("目录项").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    presets.sort();

    assert!(!presets.is_empty(), "presets/ 里该有示例配置");

    for preset in presets {
        let output = Command::new(env!("CARGO_BIN_EXE_vitals"))
            .arg("--config")
            .arg(&preset)
            .args(["--logo", "none"])
            .output()
            .expect("该能跑预设");

        assert!(
            output.status.success(),
            "{} 跑不通：{}",
            preset.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !output.stdout.is_empty(),
            "{} 什么都没输出",
            preset.display()
        );
    }
}

#[test]
fn the_all_preset_lists_exactly_the_registry() {
    let listed = config_modules(&root().join("presets/all.toml"));

    assert_eq!(
        listed.len(),
        sorted_unique(listed.clone()).len(),
        "presets/all.toml 里有重复的模块"
    );
    assert_eq!(
        sorted_unique(listed),
        sorted_unique(module_list()),
        "presets/all.toml 与 --list-modules 对不上（加了模块就要重排这一份）"
    );
}

#[test]
fn the_module_reference_documents_every_module() {
    let text =
        std::fs::read_to_string(root().join("docs/modules.md")).expect("该有 docs/modules.md");

    // 条目形如 `- **os** — …`
    let documented: Vec<String> = text
        .lines()
        .filter_map(|line| line.strip_prefix("- **"))
        .filter_map(|rest| rest.split("**").next())
        .map(str::to_owned)
        .collect();

    assert_eq!(
        documented.len(),
        sorted_unique(documented.clone()).len(),
        "docs/modules.md 里有重复条目"
    );
    assert_eq!(
        sorted_unique(documented),
        sorted_unique(module_list()),
        "docs/modules.md 与 --list-modules 对不上（加了模块就要补一条）"
    );
}
