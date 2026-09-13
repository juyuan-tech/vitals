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
    assert_eq!(
        text.lines().count(),
        vitals_rs::config::ModuleType::ALL.len(),
        "--list-modules 列的是全部模块"
    );
    assert!(text.lines().any(|line| line == "memory"));
    assert!(
        text.lines().any(|line| line == "terminal-size"),
        "多词的模块名用连字符"
    );
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
    assert_eq!(
        document["entries"][0]["type"], "title",
        "顺序跟着配置走：默认视图的第一条是标题"
    );
    assert_eq!(document["entries"][0]["key"], "", "标题没有键");
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

// ---------------------------------------------------------------------------
// 排版原语：标题、分隔线、空行
// ---------------------------------------------------------------------------

#[test]
fn the_title_has_no_key_and_the_rule_matches_the_widest_line() {
    let text = stdout(&vitals(&[
        "--module",
        "title,separator,os",
        "--logo",
        "none",
    ]));
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(lines.len(), 3, "{text}");
    assert!(!lines[0].contains(": "), "标题不该印成 `键: 值`：{text}");
    // 横线的长度 = 其它行里最宽的那条。
    let widest = lines[0]
        .chars()
        .count()
        .max("OS: Arch Linux".chars().count());
    assert_eq!(lines[1], "─".repeat(widest), "横线该跟着最宽的那行");
    assert_eq!(lines[2], "OS: Arch Linux");
}

#[test]
fn json_leaves_out_the_layout_only_primitives() {
    let document: serde_json::Value =
        serde_json::from_slice(&vitals(&["--json", "--module", "title,separator,break,os"]).stdout)
            .unwrap();
    let types: Vec<&str> = document["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["type"].as_str().unwrap())
        .collect();

    assert_eq!(
        types,
        ["title", "os"],
        "分隔线与空行只在文本版式里有意义，JSON 里不该出现"
    );
}

// ---------------------------------------------------------------------------
// 声明式条件（阶段 7）：精确跳过，而且不产生子进程
// ---------------------------------------------------------------------------

/// 把一份配置写到 `target/` 下的临时目录，返回它的路径。
///
/// 用 `CARGO_TARGET_TMPDIR` 而不是 `/tmp`：它由 cargo 提供、跟着 `target/` 一起被忽略，
/// 也不会在只读的构建环境里失败。
fn config_file(name: &str, text: &str) -> std::path::PathBuf {
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, text).expect("写临时配置");

    path
}

/// 跑一次 vitals，用 `--config` 指到指定配置。
fn vitals_with_config(path: &std::path::Path, extra: &[&str]) -> Output {
    let shown = path.display().to_string();
    let mut args = vec!["--config", shown.as_str()];
    args.extend_from_slice(extra);

    vitals(&args)
}

#[test]
fn a_condition_skips_exactly_the_module_it_names() {
    let path = config_file(
        "conditions.toml",
        r#"
        [[modules]]
        type = "os"

        [[modules]]
        type = "memory"
        when-file-exists = "/nonexistent/vitals-test"

        [[modules]]
        type = "bios"
        platforms = ["windows"]

        [[modules]]
        type = "kernel"
        "#,
    );

    let output = vitals_with_config(&path, &[]);
    let text = stdout(&output);

    assert!(output.status.success());
    assert!(text.contains("OS:"));
    assert!(text.contains("Kernel:"), "没有条件的模块照常采集：{text}");
    assert!(
        !text.contains("Memory:"),
        "文件不存在，memory 该被跳过：{text}"
    );
    assert!(
        !text.contains("BIOS"),
        "平台不是 windows，bios 该被跳过：{text}"
    );
}

#[test]
fn verbose_explains_every_skip_on_stderr() {
    let path = config_file(
        "conditions-verbose.toml",
        r#"
        [[modules]]
        type = "os"

        [[modules]]
        type = "memory"
        when-file-exists = "/nonexistent/vitals-test"
        "#,
    );

    let output = vitals_with_config(&path, &["--verbose"]);
    let errors = stderr(&output);

    assert!(errors.contains("跳过 memory"), "{errors}");
    assert!(errors.contains("/nonexistent/vitals-test"), "{errors}");
    // 「为什么这个模块没出来」是诊断，不该混进结果里。
    assert!(!stdout(&output).contains("跳过"));
}

#[cfg(unix)]
#[test]
fn a_command_condition_looks_at_path_but_never_runs_the_command() {
    use std::os::unix::fs::PermissionsExt;

    // 造一个「一被执行就会留下痕迹」的可执行文件，并让它成为唯一的 PATH。
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("fake-path");
    std::fs::create_dir_all(&dir).unwrap();
    let marker = dir.join("被执行了");
    let script = dir.join("vitals-test-must-not-run");
    std::fs::write(
        &script,
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let config = config_file(
        "command-condition.toml",
        r#"
        [[modules]]
        type = "os"

        [[modules]]
        type = "memory"
        when-command-exists = "vitals-test-must-not-run"
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vitals"))
        .args(["--config", &config.display().to_string()])
        .env("PATH", &dir)
        .env("XDG_CONFIG_HOME", "/nonexistent/vitals-for-tests")
        .env_remove("COLUMNS")
        .output()
        .expect("跑不动自己的二进制");
    let text = stdout(&output);

    assert!(
        text.contains("Memory:"),
        "命令在 PATH 里，memory 该照常采集：{text}"
    );
    assert!(
        !marker.exists(),
        "条件检查把脚本执行了——「只查 PATH，不执行命令」是硬约束"
    );
}
