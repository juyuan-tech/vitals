//! 端到端：真的把二进制跑起来。
//!
//! 这里只测「不把整个程序跑起来就看不见」的东西：退出码、stdout 与 stderr 的分工、
//! 以及「管道里没有转义码」这条阶段 5 的验收。

use std::process::{Command, Output};

/// 跑一次 vitals。
///
/// `XDG_CONFIG_HOME` 指向一个不存在的目录，这样读不到用户配置、稳定落回内置默认
/// ——测试不该受本机 `~/.config/vitals/config.toml` 的影响。
/// `COLUMNS` 一律清掉，免得外面环境把「终端多宽」这个变量带进来。
fn vitals(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vitals"))
        .args(args)
        .env("XDG_CONFIG_HOME", "/nonexistent/vitals-for-tests")
        .env_remove("COLUMNS")
        .output()
        .expect("跑不动自己的二进制")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn no_arguments_renders_the_modules() {
    let output = vitals(&[]);

    assert!(output.status.success(), "退出码该是 0");
    let text = stdout(&output);
    for key in ["OS:", "Kernel:", "Uptime:", "CPU:", "Memory:", "Disk:"] {
        assert!(text.contains(key), "少了 {key}：\n{text}");
    }
}

#[test]
fn piped_output_has_no_escape_codes() {
    // 阶段 5 验收：`vitals | cat` 不该出现转义码，也不需要用户加参数。
    let text = stdout(&vitals(&[]));

    assert!(!text.contains('\u{1b}'), "管道里有转义码：{text:?}");
}

#[test]
fn the_logo_is_drawn_when_the_width_is_unknown() {
    // 测试进程的 stdout 是管道，`COLUMNS` 又清掉了，于是「终端多宽」无从得知。
    // 这时宁可多画，也不要因为猜了个 80 就把 Logo 吃掉。
    let text = stdout(&vitals(&[]));

    let release = vitals_rs::collectors::os_release::read().unwrap();
    let entry = vitals_rs::render::logo::for_release(release.as_ref());
    let first_line = entry
        .logo
        .art
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap()
        .trim();

    assert!(
        text.contains(first_line),
        "没有画出 {} 的 Logo：\n{text}",
        entry.logo.id
    );
}

#[test]
fn a_narrow_terminal_hides_the_logo() {
    // 这条把「宽度不足就隐藏 Logo」整条链路都走了一遍：环境变量 → columns() → 版式。
    let output = Command::new(env!("CARGO_BIN_EXE_vitals"))
        .env("XDG_CONFIG_HOME", "/nonexistent/vitals-for-tests")
        .env("COLUMNS", "40")
        .output()
        .expect("跑不动自己的二进制");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();

    assert!(text.contains("OS:"), "信息还在：\n{text}");
    assert!(!text.contains("`ooo/`"), "40 列放不下 Logo：\n{text}");
}

#[test]
fn the_module_filter_narrows_the_output() {
    let text = stdout(&vitals(&["--module", "os"]));

    assert!(text.contains("OS:"));
    assert!(!text.contains("Kernel:"), "只点了 os：\n{text}");
}

#[test]
fn no_color_is_honored() {
    let text = stdout(&vitals(&["--no-color"]));

    assert!(!text.contains('\u{1b}'));
}

#[test]
fn errors_go_to_stderr_and_the_exit_code_is_one() {
    let output = vitals(&["--config", "/nonexistent/vitals.toml"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty(), "出错时不该往 stdout 写东西");
    assert!(
        stderr(&output).contains("读取配置文件"),
        "{}",
        stderr(&output)
    );
    // 报错要说清楚原因，不能只说「失败了」。
    assert!(
        stderr(&output).contains("No such file"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn verbose_diagnostics_go_to_stderr() {
    let output = vitals(&["--verbose"]);

    assert!(output.status.success());
    assert!(stderr(&output).contains("模块"), "{}", stderr(&output));
    // 诊断不该混进结果里。
    assert!(!stdout(&output).contains("模块"));
}

#[test]
fn list_modules_needs_no_system_access() {
    let output = vitals(&["--list-modules"]);
    let text = stdout(&output);

    assert!(output.status.success());
    assert_eq!(text.lines().count(), 10);
    assert!(text.lines().any(|line| line == "memory"));
}

#[test]
fn gen_config_prints_a_parseable_template() {
    let output = vitals(&["--gen-config"]);
    let text = stdout(&output);

    assert!(output.status.success());
    assert!(text.contains("config_version"));
    assert!(text.contains("[[modules]]"));
}

// ---------------------------------------------------------------------------
// `--json`：阶段 6 的验收，`vitals --json | jq` 必须可用
// ---------------------------------------------------------------------------

#[test]
fn json_output_is_valid_json() {
    let output = vitals(&["--json"]);

    assert!(output.status.success());
    let document: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("--json 的输出必须是合法 JSON");

    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["entries"][0]["type"], "os", "顺序跟着配置走");
    assert!(document["failures"].as_array().unwrap().is_empty());
}

#[test]
fn json_drops_the_logo_and_the_colors() {
    // 这一份是给程序看的：没有画面，也没有转义码。
    let text = stdout(&vitals(&["--json"]));

    assert!(!text.contains('\u{1b}'));
    assert!(!text.contains("`ooo/`"), "JSON 里不该出现 Logo：\n{text}");
}

#[test]
fn json_respects_the_module_filter() {
    let output = vitals(&["--json", "--module", "os"]);
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let entries = document["entries"].as_array().unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["type"], "os");
}

#[test]
fn json_still_reports_errors_on_stderr() {
    // 诊断走 stderr、结果走 stdout——两者互不干扰，喂给 jq 的仍是干净的 JSON。
    let output = vitals(&["--json", "--config", "/nonexistent/vitals.toml"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    assert!(stderr(&output).contains("读取配置文件"));
}
