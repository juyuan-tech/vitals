//! 阶段 2 验收：配置能加载、能覆盖、非法输入会明确报错。
//!
//! 不碰用户真实的 `~/.config`：要么解析字符串，要么写进
//! `CARGO_TARGET_TMPDIR`（cargo 给集成测试准备的临时目录）。
//! 也因此不去测 `config::load(None)`——它的行为取决于运行者的 HOME，
//! 测它等于测环境；路径解析本身在 `src/config/path.rs` 里用注入的值做单元测试。

use std::fs;
use std::path::{Path, PathBuf};

use vitals_rs::config::{self, Config, ConfigError, ModuleEntry, ModuleType};

/// 集成测试专属的临时目录，cargo 会把它准备好并清理。
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(dir).unwrap();
    dir.join(name)
}

// ---------------------------------------------------------------------------
// 默认配置
// ---------------------------------------------------------------------------

#[test]
fn default_template_parses_back_to_the_builtin_default() {
    // 这条守着「生成的默认配置」和「代码里的默认」不漂移：
    // 谁改了一边忘了另一边，这里立刻红。
    let parsed = config::from_toml(config::default_toml()).unwrap();
    assert_eq!(parsed, Config::default());
}

#[test]
fn the_default_view_is_the_curated_list() {
    let names: Vec<&str> = Config::default()
        .modules
        .iter()
        .map(|entry| entry.module_type.name())
        .collect();

    assert_eq!(
        names,
        ModuleType::DEFAULT
            .iter()
            .map(|module| module.name())
            .collect::<Vec<_>>(),
        "默认视图就是 `ModuleType::DEFAULT`，顺序一致"
    );
    assert!(
        names.len() < ModuleType::ALL.len(),
        "默认视图是**选出来的**，不是全部模块；全部模块见 --list-modules"
    );
}

#[test]
fn the_default_view_starts_with_a_title_and_a_rule() {
    // fastfetch 默认视图也是这么开头的：`用户@主机名`，然后一条横线。
    let names: Vec<&str> = Config::default()
        .modules
        .iter()
        .map(|entry| entry.module_type.name())
        .collect();

    assert_eq!(names.first(), Some(&"title"));
    assert_eq!(names.get(1), Some(&"separator"));
}

// ---------------------------------------------------------------------------
// 覆盖语义
// ---------------------------------------------------------------------------

#[test]
fn user_modules_replace_the_builtin_default() {
    let parsed = config::from_toml(
        r#"
        [[modules]]
        type = "cpu"

        [[modules]]
        type = "os"
        "#,
    )
    .unwrap();

    // 整体替换，且顺序就是写的顺序——不是追加到默认列表后面，也不是排序。
    assert_eq!(
        parsed.modules,
        vec![
            ModuleEntry::new(ModuleType::Cpu),
            ModuleEntry::new(ModuleType::Os)
        ]
    );
}

#[test]
fn omitting_modules_keeps_the_builtin_default() {
    let parsed = config::from_toml("config_version = 1\n").unwrap();
    assert_eq!(parsed.modules, Config::default().modules);
}

#[test]
fn empty_module_list_means_show_nothing() {
    let parsed = config::from_toml("modules = []\n").unwrap();
    assert!(parsed.modules.is_empty());
}

// ---------------------------------------------------------------------------
// 非法输入必须报错，而且要报得有用
// ---------------------------------------------------------------------------

#[test]
fn unknown_top_level_field_is_rejected() {
    // 故意的拼写错误。计划里明确要求未知字段报错，不许静默通过。
    let error = config::from_toml("config_version = 1\nmoduels = []\n").unwrap_err();
    let text = error.to_string();

    assert!(
        text.contains("moduels"),
        "错误该指出拼错的字段名，实际是：{text}"
    );
}

#[test]
fn unknown_module_type_is_rejected() {
    let error = config::from_toml("[[modules]]\ntype = \"cpuu\"\n").unwrap_err();
    let text = error.to_string();

    assert!(text.contains("cpuu"), "错误该指出写错的值，实际是：{text}");
    // 这是用枚举而不是字符串换来的好处：错误里直接列出所有合法取值。
    assert!(
        text.contains("expected one of"),
        "错误该列出合法取值，实际是：{text}"
    );
}

#[test]
fn missing_type_field_is_rejected() {
    let error = config::from_toml("[[modules]]\n").unwrap_err();
    assert!(error.to_string().contains("type"));
}

#[test]
fn unknown_field_inside_a_module_is_rejected() {
    // 阶段 7 之前的条件字段还没落到 schema 里，写了就得报错，
    // 不能出现「解析通过但什么也没做」的字段。
    let error = config::from_toml("[[modules]]\ntype = \"gpu\"\n").unwrap_err();
    assert!(error.to_string().contains("gpu"));
}

// ---------------------------------------------------------------------------
// 版本
// ---------------------------------------------------------------------------

#[test]
fn future_config_version_is_rejected() {
    let error = config::from_toml("config_version = 2\n").unwrap_err();

    assert!(matches!(
        error,
        ConfigError::UnsupportedVersion {
            found: 2,
            supported: config::CURRENT_CONFIG_VERSION,
        }
    ));
}

#[test]
fn version_defaults_to_current_when_omitted() {
    let parsed = config::from_toml("modules = []\n").unwrap();
    assert_eq!(parsed.config_version, config::CURRENT_CONFIG_VERSION);
}

// ---------------------------------------------------------------------------
// 模块名：枚举、serde 名字、Collector 名字必须是同一套字符串
// ---------------------------------------------------------------------------

#[test]
fn every_module_name_round_trips_through_toml() {
    let mut seen = std::collections::HashSet::new();

    for module in ModuleType::ALL {
        assert!(seen.insert(module.name()), "模块名重复：{}", module.name());

        let text = format!("[[modules]]\ntype = \"{}\"\n", module.name());
        let parsed = config::from_toml(&text).unwrap();

        assert_eq!(
            parsed.modules,
            vec![ModuleEntry::new(module)],
            "`{}` 没能往返回来",
            module.name()
        );
    }
}

// ---------------------------------------------------------------------------
// 文件加载
// ---------------------------------------------------------------------------

#[test]
fn load_file_reads_from_disk() {
    let path = scratch("config-ok.toml");
    fs::write(&path, "[[modules]]\ntype = \"kernel\"\n").unwrap();

    let parsed = config::load_file(&path).unwrap();
    assert_eq!(parsed.modules, vec![ModuleEntry::new(ModuleType::Kernel)]);
}

#[test]
fn parse_error_names_the_file_it_came_from() {
    let path = scratch("config-bad.toml");
    fs::write(&path, "[[modules]]\ntype = \"nope\"\n").unwrap();

    let error = config::load_file(&path).unwrap_err();
    let text = error.to_string();

    assert!(
        text.contains("config-bad.toml"),
        "错误该带上是哪个文件，实际是：{text}"
    );
}

#[test]
fn missing_explicit_file_is_an_error() {
    let path = scratch("never-written.toml");
    let _ = fs::remove_file(&path);

    // 显式指了路径却读不到，就得说。对比 `load(None)`：默认路径下文件不存在
    // 是常见情况（第一次运行），那里安静地用内置默认。
    let error = config::load(Some(&path)).unwrap_err();
    assert!(
        matches!(error, ConfigError::Read { .. }),
        "实际是：{error:?}"
    );
}
