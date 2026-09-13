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

/// `--help` 里出现的所有长选项（自己扫，不引正则库）。
fn flags_in_help() -> Vec<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_vitals"))
        .arg("--help")
        // 中英两份帮助列的是同一批选项，取哪份都一样；但测试不该随外面的 locale 变脸。
        .env("VITALS_LANG", "zh")
        .output()
        .expect("该能跑 --help");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let bytes = text.as_bytes();
    let mut flags = Vec::new();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if &bytes[i..i + 2] == b"--" {
            let start = i;
            let mut j = i + 2;
            while j < bytes.len() && ((bytes[j] as char).is_ascii_lowercase() || bytes[j] == b'-') {
                j += 1;
            }
            if j > start + 2 {
                flags.push(text[start..j].to_owned());
                i = j;
                continue;
            }
        }
        i += 1;
    }
    flags.sort();
    flags.dedup();
    flags
}

#[test]
fn the_completions_know_every_option_and_logo() {
    let bash =
        std::fs::read_to_string(root().join("completions/vitals.bash")).expect("该有 bash 补全");
    let zsh = std::fs::read_to_string(root().join("completions/_vitals")).expect("该有 zsh 补全");

    let flags = flags_in_help();
    assert!(
        flags.len() >= 10,
        "该能从 --help 里扫出选项，实际：{flags:?}"
    );
    for flag in &flags {
        assert!(bash.contains(flag.as_str()), "bash 补全里缺 {flag}");
        assert!(zsh.contains(flag.as_str()), "zsh 补全里缺 {flag}");
    }

    // 内置 Logo 的名字也要在补全里（加了一张图就得补上）。
    let logos: Vec<String> = std::fs::read_dir(root().join("src/render/logos"))
        .expect("该有 logos 目录")
        .map(|entry| entry.expect("目录项").path())
        .filter_map(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .collect();
    assert!(!logos.is_empty());
    for logo in logos {
        assert!(bash.contains(&logo), "bash 补全里缺 logo {logo}");
        assert!(zsh.contains(&logo), "zsh 补全里缺 logo {logo}");
    }
}

#[test]
fn the_version_in_the_cli_reference_is_current() {
    // `docs/cli.md` 里印了 `vitals --version` 的真实输出，升版本时最容易忘了改它。
    let expected = format!("vitals {}", env!("CARGO_PKG_VERSION"));

    for name in ["docs/cli.md", "docs/configuration.md"] {
        let text = std::fs::read_to_string(root().join(name))
            .unwrap_or_else(|error| panic!("读不了 {name}：{error}"));
        assert!(
            text.contains(&expected),
            "{name} 里该有 `{expected}`（改了版本号就更新这几处）"
        );
    }
}
