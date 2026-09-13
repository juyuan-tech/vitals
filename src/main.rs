//! CLI 薄壳：解析参数、跑管线、决定退出码。
//!
//! 这里只干三件事：决定退出码、决定往哪个流写、把逻辑交给 lib。
//! 采集、渲染、配置都不在这一层——`main` 里若出现处理业务的循环，说明放错了地方。

use std::io;
use std::process::ExitCode;
use std::time::Duration;

use anstream::{AutoStream, ColorChoice};
use clap::Parser;

use vitals_rs::cli::{Cli, LogoChoice, Settings};
use vitals_rs::collectors::os_release;
use vitals_rs::config::{self, ModuleType};
use vitals_rs::core::dispatch::Failure;
use vitals_rs::core::render::{RenderError, Renderer, Report};
use vitals_rs::render::json::JsonRenderer;
use vitals_rs::render::logo;
use vitals_rs::render::text::TextRenderer;
use vitals_rs::render::theme::Theme;
use vitals_rs::{COLLECTORS, Context, Dispatcher, PROGRAM, Platform};

/// 外部命令的超时预算。
///
/// v0.1 十个模块一个子进程都不开，所以现在没人真的用它；留着是为了 v0.2 的
/// GPU（`nvidia-smi`）这类模块，也让 `Context` 的形状先定下来。
const TIMEOUT: Duration = Duration::from_secs(5);

fn main() -> ExitCode {
    let cli = Cli::parse();

    // 这两个是「问完就走」的参数：不读配置文件、不采集。
    if cli.gen_config {
        print!("{}", config::default_toml());
        return ExitCode::SUCCESS;
    }

    if cli.list_modules {
        for module in ModuleType::ALL {
            println!("{}", module.name());
        }
        return ExitCode::SUCCESS;
    }

    let config = match config::load(cli.config.as_deref()) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{PROGRAM}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let settings = Settings::resolve(&cli, &config);

    // 诊断一律走 stderr：`vitals > 文件` 时不该被混进结果里。
    if settings.verbose {
        for line in settings.describe() {
            eprintln!("{PROGRAM}: {line}");
        }
    }

    render(&settings)
}

/// 采集 + 渲染。
fn render(settings: &Settings) -> ExitCode {
    // 读不到 os-release 不该让整个程序失败：Logo 退回通用那张，OS 模块自己会处理。
    let release = match os_release::read() {
        Ok(release) => release,
        Err(error) => {
            eprintln!("{PROGRAM}: 读取 os-release 失败：{error}");
            None
        }
    };

    // 平台信息由这里解析一次：OS 模块显示它，挑 Logo 也用它，两边读的是同一份数据。
    let context = Context::new(
        Platform {
            os_id: release.as_ref().and_then(|release| release.id.clone()),
            os_id_like: release.as_ref().and_then(|release| release.id_like.clone()),
        },
        TIMEOUT,
    );

    let plan = settings.module_names();
    let outcome = Dispatcher::new(COLLECTORS).run(&plan, &context);

    // 失败的模块照实说，但不中断——采到的那些照常渲染。
    report_failures(&outcome.failures, settings.verbose);

    // Logo：auto 按发行版匹配；none 不画；给了名字就用名字，找不到退回通用那张。
    let entry = match &settings.logo {
        LogoChoice::None => None,
        LogoChoice::Auto => Some(logo::for_release(release.as_ref())),
        LogoChoice::Named(name) => Some(logo::find(name).unwrap_or(&logo::GENERIC)),
    };
    let report = Report::new(
        entry.map(|entry| &entry.logo),
        &outcome.entries,
        &outcome.failures,
    );

    // 两个渲染器平级，谁也不包谁：文本给人看，JSON 给脚本看。
    // `Settings::resolve` 已经替 `--json` 关掉了 Logo 与颜色，这里只管挑一个。
    let renderer: Box<dyn Renderer> = if settings.json {
        Box::new(JsonRenderer)
    } else {
        Box::new(TextRenderer::new(Theme::default()))
    };

    // anstream 负责降级：不是终端（或 `--no-color`）时它会把转义码剥掉。
    let mut out = AutoStream::new(
        io::stdout().lock(),
        if settings.allow_color {
            ColorChoice::Auto
        } else {
            ColorChoice::Never
        },
    );

    match renderer.render(&report, &mut out) {
        Ok(()) => ExitCode::SUCCESS,
        // 下游主动关掉管道（`vitals | head -1`）不算错误，安静收场——
        // 这是 Unix 的惯例，为它印一句错只会盖住真正的失败。
        Err(RenderError::Write(error)) if error.kind() == io::ErrorKind::BrokenPipe => {
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{PROGRAM}: {error}");
            ExitCode::FAILURE
        }
    }
}

/// 把模块失败写到 stderr。
///
/// 默认只说清楚「哪个模块失败了」；`--verbose` 才把完整错误链摊开——
/// `CollectError` 里挂着底层原因，一路 `source()` 走到底。
fn report_failures(failures: &[Failure], verbose: bool) {
    for failure in failures {
        eprintln!("{PROGRAM}: {} 模块失败：{}", failure.module, failure.error);

        if !verbose {
            continue;
        }

        let mut cause = std::error::Error::source(&failure.error);
        while let Some(error) = cause {
            eprintln!("{PROGRAM}:   因为：{error}");
            cause = error.source();
        }
    }
}
