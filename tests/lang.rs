//! 帮助语言：`VITALS_LANG` 与 locale 谁说了算。
//!
//! 这里只测「不把二进制跑起来就看不见」的部分：语言怎么选、两种语言列的是不是
//! 同一批选项、以及 `--module` 写错时的报错跟不跟帮助语言走。
//! 具体的判定表在 `src/lang.rs` 的单元测试里。

use std::process::{Command, Output};

/// 跑一次 vitals，环境完全由调用方指定（`envs` 设置，`remove` 清掉）。
fn vitals(args: &[&str], envs: &[(&str, &str)], remove: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vitals"));
    command
        .args(args)
        .env("XDG_CONFIG_HOME", "/nonexistent/vitals-for-tests")
        .env_remove("COLUMNS")
        .env_remove("VITALS_LANG")
        .env_remove("LC_ALL")
        .env_remove("LC_MESSAGES")
        .env_remove("LANG");
    for (key, value) in envs {
        command.env(key, value);
    }
    for key in remove {
        command.env_remove(key);
    }
    command.output().expect("跑不动自己的二进制")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// 有没有汉字。帮助只有中英两套，这条就是最直接的判据。
fn has_chinese(text: &str) -> bool {
    text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

/// 从帮助文本里抠出所有长选项（自己扫，不引正则库）。
fn long_options(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut options = Vec::new();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if &bytes[i..i + 2] == b"--" {
            let start = i;
            let mut j = i + 2;
            while j < bytes.len() && ((bytes[j] as char).is_ascii_lowercase() || bytes[j] == b'-') {
                j += 1;
            }
            if j > start + 2 {
                options.push(text[start..j].to_owned());
                i = j;
                continue;
            }
        }
        i += 1;
    }
    options.sort();
    options.dedup();
    options
}

#[test]
fn the_explicit_variable_gives_english() {
    let output = vitals(&["--help"], &[("VITALS_LANG", "en")], &[]);
    let help = stdout(&output);

    assert!(output.status.success());
    assert!(!has_chinese(&help), "英文帮助里不该有汉字：\n{help}");
    assert!(help.contains("--list-modules"), "{help}");
    assert!(help.contains("Usage"), "{help}");
}

#[test]
fn the_explicit_variable_gives_chinese() {
    let help = stdout(&vitals(&["--help"], &[("VITALS_LANG", "zh")], &[]));

    assert!(has_chinese(&help), "中文帮助里该有汉字：\n{help}");
    assert!(help.contains("--list-modules"), "{help}");
}

#[test]
fn the_locale_decides_when_the_variable_is_unset() {
    let english = stdout(&vitals(&["--help"], &[("LANG", "en_US.UTF-8")], &[]));
    let chinese = stdout(&vitals(&["--help"], &[("LANG", "zh_CN.UTF-8")], &[]));

    assert!(!has_chinese(&english), "en_US 该给英文帮助：\n{english}");
    assert!(has_chinese(&chinese), "zh_CN 该给中文帮助：\n{chinese}");
    // 别的语言没有我们的译文，退回英文而不是中文。
    let german = stdout(&vitals(&["--help"], &[("LANG", "de_DE.UTF-8")], &[]));
    assert!(!has_chinese(&german), "de_DE 该退回英文帮助：\n{german}");
}

#[test]
fn the_explicit_variable_beats_the_locale() {
    let english = stdout(&vitals(
        &["--help"],
        &[("LANG", "en_US.UTF-8"), ("VITALS_LANG", "zh")],
        &[],
    ));
    assert!(
        has_chinese(&english),
        "VITALS_LANG=zh 该压过 locale：\n{english}"
    );
}

#[test]
fn nothing_set_at_all_stays_chinese() {
    // 「什么都不设」必须还是今天的行为：locale 一律清掉时给中文。
    let help = stdout(&vitals(&["--help"], &[], &[]));
    assert!(has_chinese(&help), "没设语言时该给中文帮助：\n{help}");
}

#[test]
fn both_languages_list_the_same_options() {
    let chinese = stdout(&vitals(&["--help"], &[("VITALS_LANG", "zh")], &[]));
    let english = stdout(&vitals(&["--help"], &[("VITALS_LANG", "en")], &[]));

    let chinese_options = long_options(&chinese);
    assert!(chinese_options.len() >= 10, "{chinese_options:?}");
    assert_eq!(
        chinese_options,
        long_options(&english),
        "两份帮助必须是同一批选项"
    );
}

#[test]
fn a_bad_module_name_is_reported_in_the_chosen_language() {
    let english = vitals(&["--module", "nope"], &[("VITALS_LANG", "en")], &[]);
    assert_eq!(english.status.code(), Some(2), "参数错该是退出码 2");
    assert!(
        stderr(&english).contains("unknown module `nope`"),
        "{}",
        stderr(&english)
    );

    let chinese = vitals(&["--module", "nope"], &[("VITALS_LANG", "zh")], &[]);
    assert_eq!(chinese.status.code(), Some(2));
    assert!(
        stderr(&chinese).contains("未知模块 `nope`"),
        "{}",
        stderr(&chinese)
    );
}
