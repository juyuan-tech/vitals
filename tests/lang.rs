//! 语言：帮助文本与运行期文案都看 `VITALS_LANG` / locale。
//!
//! 这里只测「不把二进制跑起来就看不见」的部分：语言怎么选、两种语言列的是不是
//! 同一批选项、报错跟不跟语言走、以及运行期的四种状态与诊断怎么说。
//! 具体的判定表在 `src/lang.rs`，文案本身在 `src/i18n.rs` 的单元测试里。

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

/// 运行期文案（`--explain` 的状态与条数）跟语言走。
///
/// 只挑值一定是 ASCII 的模块：文案是我们的，值是这台机器的，混在一起就分不清谁错。
///
/// **断言里不出现「空」**：那要求这台机器真的没有某样硬件。GitHub 的构建机上有虚拟手柄，
/// `gamepad` 在那里是「显示」而不是「空」，一开始就是这么红的。「空」的措辞由
/// `src/i18n.rs` 的单元测试直接盯着，不靠宿主机凑。
#[test]
fn the_runtime_messages_follow_the_language() {
    let args = ["--explain", "--module", "os,host"];
    let zh = stdout(&vitals(&args, &[("VITALS_LANG", "zh")], &[]));
    let en = stdout(&vitals(&args, &[("VITALS_LANG", "en")], &[]));

    assert!(zh.contains("显示"), "中文 --explain 该有「显示」：\n{zh}");
    assert!(zh.contains("1 项"), "中文该用「项」计数：\n{zh}");
    assert!(en.contains("shown"), "英文 --explain 该有 `shown`：\n{en}");
    assert!(en.contains("1 item"), "英文单数该是 `1 item`：\n{en}");
    assert!(!has_chinese(&en), "英文 --explain 里不该有汉字：\n{en}");
}

/// 跳过与报错的原因也跟语言走：这里用配置里的条件造一次真跳过。
#[test]
fn the_skip_reasons_and_errors_follow_the_language() {
    let directory = std::env::temp_dir().join("vitals-lang-messages");
    std::fs::create_dir_all(&directory).expect("建临时目录");

    let config = directory.join("config.toml");
    std::fs::write(
        &config,
        "config_version = 1\n\n[[modules]]\ntype = \"os\"\n\n[[modules]]\ntype = \"battery\"\nwhen-file-exists = \"/definitely/not/here\"\n",
    )
    .expect("写得进配置");
    let path = config.to_str().expect("临时路径该是 UTF-8");

    let zh = vitals(
        &["--verbose", "--config", path],
        &[("VITALS_LANG", "zh")],
        &[],
    );
    let en = vitals(
        &["--verbose", "--config", path],
        &[("VITALS_LANG", "en")],
        &[],
    );
    assert!(
        stderr(&zh).contains("跳过 battery"),
        "中文该说「跳过 battery」：{}",
        stderr(&zh)
    );
    assert!(
        stderr(&en).contains("skipped battery"),
        "英文该说 `skipped battery`：{}",
        stderr(&en)
    );
    assert!(
        !has_chinese(&stderr(&en)),
        "英文诊断里不该有汉字：{}",
        stderr(&en)
    );

    // 配置版本过高这条错误同样在文案目录里。
    let too_new = directory.join("too-new.toml");
    std::fs::write(&too_new, "config_version = 9\n").expect("写得进配置");
    let path = too_new.to_str().expect("临时路径该是 UTF-8");

    let zh = vitals(&["--config", path], &[("VITALS_LANG", "zh")], &[]);
    let en = vitals(&["--config", path], &[("VITALS_LANG", "en")], &[]);
    assert!(
        stderr(&zh).contains("配置版本 9 高于本程序支持的 1"),
        "中文该报配置版本过高：{}",
        stderr(&zh)
    );
    assert!(
        stderr(&en).contains("config version 9 is newer than version 1"),
        "英文该报配置版本过高：{}",
        stderr(&en)
    );
}

/// `--verbose` 的设置行、`--sources` 的「没有读文件」、`--gen-config` 的注释同样跟语言走。
#[test]
fn the_diagnostics_follow_the_language() {
    let zh = vitals(
        &["--verbose", "--module", "os"],
        &[("VITALS_LANG", "zh")],
        &[],
    );
    let en = vitals(
        &["--verbose", "--module", "os"],
        &[("VITALS_LANG", "en")],
        &[],
    );
    assert!(
        stderr(&zh).contains("颜色=开"),
        "中文设置行该用「颜色=开」：{}",
        stderr(&zh)
    );
    assert!(
        stderr(&en).contains("color=on"),
        "英文设置行该用 `color=on`：{}",
        stderr(&en)
    );

    // `kernel` 一个文件都不读：这里正好说「没有读文件」，不会掺进这台机器的值。
    let zh = stdout(&vitals(
        &["--sources", "--module", "kernel"],
        &[("VITALS_LANG", "zh")],
        &[],
    ));
    let en = stdout(&vitals(
        &["--sources", "--module", "kernel"],
        &[("VITALS_LANG", "en")],
        &[],
    ));
    assert!(zh.contains("没有读文件"), "中文该说「没有读文件」：{zh}");
    assert!(
        en.contains("no files read"),
        "英文该说 `no files read`：{en}"
    );
    assert!(!has_chinese(&en), "英文 --sources 里不该有汉字：{en}");

    let en = stdout(&vitals(&["--gen-config"], &[("VITALS_LANG", "en")], &[]));
    assert!(
        en.starts_with("# Vitals config"),
        "英文 --gen-config 该是英文注释"
    );
    let zh = stdout(&vitals(&["--gen-config"], &[("VITALS_LANG", "zh")], &[]));
    assert!(
        zh.starts_with("# Vitals 配置"),
        "中文 --gen-config 该是中文注释"
    );
}

/// 模块**失败**（不是跳过）时的诊断也跟语言走。
///
/// 让 `datetime` 去读一个读不动的时区文件：建一个**目录**，打开会成功、读会失败，
/// 正好走「模块失败」那条路（退出码仍是 0）。
#[test]
fn a_module_failure_is_reported_in_the_chosen_language() {
    let zone = std::env::temp_dir().join("vitals-lang-zone/tz");
    std::fs::create_dir_all(&zone).expect("建时区目录");
    let zone = zone.to_str().expect("临时路径该是 UTF-8");

    let zh = vitals(
        &["--module", "datetime", "--verbose"],
        &[("VITALS_LANG", "zh"), ("TZ", zone)],
        &[],
    );
    let en = vitals(
        &["--module", "datetime", "--verbose"],
        &[("VITALS_LANG", "en"), ("TZ", zone)],
        &[],
    );

    assert!(
        stderr(&zh).contains("datetime 模块失败"),
        "中文该报模块失败：{}",
        stderr(&zh)
    );
    assert!(
        stderr(&en).contains("the datetime module failed"),
        "英文该报模块失败：{}",
        stderr(&en)
    );
    assert!(
        stderr(&en).contains("because: "),
        "英文 `--verbose` 该展开错误链：{}",
        stderr(&en)
    );
    assert!(
        !has_chinese(&stderr(&en)),
        "英文诊断里不该有汉字：{}",
        stderr(&en)
    );
}
