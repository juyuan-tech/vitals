//! 阶段 3 验收：参数完整、行为可预期。
//!
//! 采集要到阶段 4、渲染要到阶段 5，所以这里测的是**参数解析与优先级**：
//! 「用户敲了什么」→「最终生效的设置是什么」。可见的输出效果在后面的阶段接上。
//!
//! 用 `Cli::try_parse_from` 而不是真的起进程：它不打印、不退出，失败时把错误返回给我们，
//! 于是连 `--help` 的内容都能断言。

use clap::Parser;

use vitals_rs::VERSION;
use vitals_rs::cli::{Cli, LogoChoice, Settings};
use vitals_rs::config::{Config, ModuleEntry, ModuleType};

/// 像真的命令行那样解析（argv[0] 补上）。
fn parse(args: &[&str]) -> Cli {
    let mut argv = vec!["vitals"];
    argv.extend_from_slice(args);
    Cli::try_parse_from(argv).expect("这些参数应当解析成功")
}

/// 解析失败时的错误文本（`--help` / `--version` 也走这条路）。
fn parse_error(args: &[&str]) -> String {
    let mut argv = vec!["vitals"];
    argv.extend_from_slice(args);
    Cli::try_parse_from(argv)
        .expect_err("这些参数应当解析失败")
        .to_string()
}

fn config_of(modules: &[ModuleType]) -> Config {
    Config {
        config_version: 1,
        modules: modules.iter().copied().map(ModuleEntry::new).collect(),
    }
}

// ---------------------------------------------------------------------------
// 默认状态
// ---------------------------------------------------------------------------

#[test]
fn no_arguments_keeps_everything_at_its_default() {
    let settings = Settings::resolve(&parse(&[]), &Config::default());

    assert_eq!(
        settings.modules,
        Config::default().modules,
        "不带参数时，模块列表就是内置默认"
    );
    assert_eq!(settings.logo, LogoChoice::Auto);
    assert!(
        settings.allow_color,
        "默认允许上色（管道里降级是阶段 5 的事）"
    );
    assert!(!settings.json);
    assert!(!settings.verbose);
}

#[test]
fn default_logo_is_auto() {
    assert_eq!(parse(&[]).logo, LogoChoice::Auto);
}

// ---------------------------------------------------------------------------
// --module 是选择：点谁采谁，顺序跟着你写
// ---------------------------------------------------------------------------

#[test]
fn module_selection_follows_the_order_you_wrote() {
    // 用户写的是 cpu,os，那 CPU 就该排在前面——配置里的顺序不该反过来盖掉它。
    let cli = parse(&["--module", "cpu,os"]);
    let settings = Settings::resolve(&cli, &Config::default());

    assert_eq!(settings.module_names(), ["cpu", "os"]);
}

#[test]
fn module_selection_accepts_repeated_flags_as_well() {
    let cli = parse(&["--module", "os", "--module", "cpu"]);
    let settings = Settings::resolve(&cli, &Config::default());

    assert_eq!(settings.module_names(), ["os", "cpu"]);
}

#[test]
fn a_module_outside_the_config_is_still_selected() {
    // 这条以前会红：`--module` 是从配置视图里过滤，配置里没有 cpu 就什么都不剩，
    // 于是「点了一个不在默认视图里的模块」看起来像「这台机器没有它」。
    let cli = parse(&["--module", "cpu"]);
    let settings = Settings::resolve(&cli, &config_of(&[]));

    assert_eq!(settings.module_names(), ["cpu"]);
}

#[test]
fn module_selection_drops_the_configured_conditions() {
    // 显式点名比配置文件里的条件更硬：点了 battery 就得看到 battery，
    // 不该因为配置里挂着 `when-file-exists` 而被悄悄跳过。
    let mut entry = ModuleEntry::new(ModuleType::Battery);
    entry.when_file_exists = Some(std::path::PathBuf::from("/definitely/not/here"));
    let config = Config {
        config_version: 1,
        modules: vec![entry],
    };

    let cli = parse(&["--module", "battery"]);
    let settings = Settings::resolve(&cli, &config);

    assert_eq!(settings.modules.len(), 1);
    assert!(settings.modules[0].when_file_exists.is_none());
}

#[test]
fn unknown_module_name_is_a_usage_error_listing_the_valid_ones() {
    // 这里**故意用一个永远不会成为模块的名字**：以前写的是 `gpu`，
    // 等 `gpu` 真做出来那天，这条测试就从「不认识的名字该报错」变成了
    // 「认识的名字不该报错」，红得很莫名。
    let text = parse_error(&["--module", "nonsense"]);

    assert!(text.contains("nonsense"), "错误该指出写错的名字：\n{text}");
    // 合法名单从 ModuleType::ALL 现取，所以这里也顺带守着「名单没被抄错」。
    assert!(
        text.contains("os") && text.contains("memory"),
        "错误该列出可用模块：\n{text}"
    );
    // 已经做出来的模块必须出现在「可用」名单里：那份名单是从 `ModuleType::ALL`
    // 现取的，这条守着它没被抄成一份写死的旧表。
    assert!(
        text.contains("gpu") && text.contains("terminal-font"),
        "新做的模块该出现在可用名单里：\n{text}"
    );
}

// ---------------------------------------------------------------------------
// --json 与 --logo、--no-color 的关系
// ---------------------------------------------------------------------------

#[test]
fn json_forces_logo_off_and_color_off() {
    let settings = Settings::resolve(&parse(&["--json"]), &Config::default());

    assert!(settings.json);
    assert_eq!(settings.logo, LogoChoice::None);
    assert!(!settings.allow_color, "JSON 输出里不该混转义码");
}

#[test]
fn json_wins_over_an_explicit_logo() {
    // 计划里写的是「--json 自动关 Logo」，所以这两者不冲突，json 直接覆盖。
    let settings = Settings::resolve(&parse(&["--json", "--logo", "arch"]), &Config::default());

    assert_eq!(settings.logo, LogoChoice::None);
}

#[test]
fn no_color_leaves_the_logo_alone() {
    let settings = Settings::resolve(&parse(&["--no-color"]), &Config::default());

    assert!(!settings.allow_color);
    assert_eq!(settings.logo, LogoChoice::Auto, "关颜色不该顺手关掉 Logo");
}

#[test]
fn logo_none_is_distinct_from_a_logo_actually_named_none() {
    assert_eq!(parse(&["--logo", "none"]).logo, LogoChoice::None);
    assert_eq!(parse(&["--logo", "auto"]).logo, LogoChoice::Auto);
    assert_eq!(
        parse(&["--logo", "arch"]).logo,
        LogoChoice::Named("arch".to_owned())
    );
}

// ---------------------------------------------------------------------------
// 其余参数
// ---------------------------------------------------------------------------

#[test]
fn config_path_is_optional_and_given_as_a_path() {
    assert_eq!(parse(&[]).config, None);
    assert_eq!(
        parse(&["--config", "/etc/x.toml"]).config.as_deref(),
        Some(std::path::Path::new("/etc/x.toml"))
    );
}

#[test]
fn the_three_standalone_flags_are_carried_through() {
    let cli = parse(&["--list-modules", "--gen-config", "--verbose"]);

    assert!(cli.list_modules);
    assert!(cli.gen_config);
    assert!(Settings::resolve(&cli, &Config::default()).verbose);
}

// ---------------------------------------------------------------------------
// 验收：--help 可读、--version 名字对
// ---------------------------------------------------------------------------

#[test]
fn help_documents_every_flag_in_the_plan() {
    let help = parse_error(&["--help"]);

    for flag in [
        "--config",
        "--json",
        "--logo",
        "--module",
        "--no-color",
        "--list-modules",
        "--gen-config",
        "--verbose",
        "--help",
        "--version",
    ] {
        assert!(help.contains(flag), "`--help` 里漏了 {flag}：\n{help}");
    }
}

#[test]
fn help_talks_to_users_not_maintainers() {
    // clap 会把命令结构体的文档注释当作 `--help` 的正文，所以维护者的话
    // （比如「name 必须显式写，否则会打印包名」）一不小心就会漏到用户脸上。
    // 这一条就是守这个坑：那类词一个都不该出现在帮助里。
    let help = parse_error(&["--help"]);

    for leak in ["clap", "包名", "cargo", "vitals-rs"] {
        assert!(
            !help.contains(leak),
            "`--help` 里漏出了维护者的话（{leak}）：\n{help}"
        );
    }
}

#[test]
fn version_uses_the_command_name_not_the_package_name() {
    // 包名是 vitals-rs，命令是 vitals。这条守着那个容易踩的坑。
    assert_eq!(
        parse_error(&["--version"]).trim(),
        format!("vitals {VERSION}")
    );
}

#[test]
fn verbose_diagnostics_are_pure_text_not_io() {
    // describe() 只产出文本，打印由 main 决定——所以这里能直接断言内容。
    let settings = Settings::resolve(
        &parse(&["--module", "os,cpu", "--verbose"]),
        &Config::default(),
    );
    let lines = settings.describe();

    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("模块 2 个"), "实际：{}", lines[0]);
    assert!(lines[0].contains("os, cpu"), "实际：{}", lines[0]);
    assert!(lines[1].contains("logo=auto"), "实际：{}", lines[1]);
}
