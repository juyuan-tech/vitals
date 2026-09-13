//! CLI 薄壳：解析参数、跑管线、决定退出码。
//!
//! 这里只干三件事：决定退出码、决定往哪个流写、把逻辑交给 lib。
//! 采集、渲染、配置都不在这一层——`main` 里若出现处理业务的循环，说明放错了地方。

use std::io;
use std::process::ExitCode;
use std::time::Duration;

use anstream::{AutoStream, ColorChoice};
use clap::FromArgMatches;

use vitals_rs::cli::{self, Cli, LogoChoice, Settings};
use vitals_rs::collectors::os_release;
use vitals_rs::conditions;
use vitals_rs::config::{self, ModuleEntry, ModuleType};
use vitals_rs::core::dispatch::{Failure, RunOutcome};
use vitals_rs::core::render::{RenderError, Renderer, Report};
use vitals_rs::lang::Lang;
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
    // 语言要在解析之前定下来：它决定帮助文本用哪一套。
    let matches = cli::command(Lang::resolve()).get_matches();
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    };

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

    // 声明式条件在调度前统一评估：不满足的模块直接不进名单，不报错。
    let plan = conditions::plan(&settings.modules);

    // 「跳过」是正常情况（这台机器没有电池），所以默认不出声；
    // `--verbose` 才说清楚谁被什么挡下来了——不然只能靠猜。
    if settings.verbose {
        for skipped in &plan.skipped {
            eprintln!(
                "{PROGRAM}: 跳过 {}：{}",
                skipped.module,
                skipped.reason.describe()
            );
        }
    }

    let outcome = Dispatcher::new(COLLECTORS).run(&plan.names, &context);

    // 失败的模块照实说，但不中断——采到的那些照常渲染。
    report_failures(&outcome.failures, settings.verbose);

    // `--explain` 对着**结果**说话，而不是对着配置：显示 / 空 / 跳过 / 失败，
    // 四种状态各有理由。它天然要真跑一遍采集，否则「空」和「显示」分不出来。
    if settings.explain {
        explain(&settings.modules, &plan, &outcome);
    }

    // `--sources` 说依据：每个模块**实际读了哪些文件**。记录发生在读取那一层
    // （`collectors::read`），所以它说的是实际发生的事，不是一张可能过期的来源表。
    //
    // 两个都给就两份都打（`--help` 里就是这么承诺的：「先打状态、再打依据」）。
    // 早先这里在 explain 之后直接 return，等于把 `--sources` 吞掉了。
    if settings.sources {
        print_sources(&settings.modules, &plan, &outcome);
    }

    if settings.explain || settings.sources {
        return ExitCode::SUCCESS;
    }

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

/// `--explain` 的报告：按配置顺序逐项说明。
///
/// 为什么需要它：这类工具默认只会「少一行」——用户分不清是配置没写、条件不满足、
/// 这台机器真没有数据，还是采集出错了。尤其**「空」与「跳过」是两回事**：
/// 前者是采过了、确实没东西（比如没摄像头），后者是条件挡住、根本没去采。
///
/// 四种状态各自给一句能追下去的理由：跳过给条件（平台/命令/路径），失败给错误原因，
/// 显示给条数，空就说这台机器上没有。
fn explain(modules: &[ModuleEntry], plan: &conditions::Plan, outcome: &RunOutcome) {
    let width = modules
        .iter()
        .map(|entry| entry.module_type.name().len())
        .max()
        .unwrap_or(0);

    for entry in modules {
        let name = entry.module_type.name();

        let (state, detail) =
            if let Some(skipped) = plan.skipped.iter().find(|item| item.module == name) {
                ("跳过", skipped.reason.describe())
            } else if let Some(failure) = outcome.failures.iter().find(|item| item.module == name) {
                ("失败", failure.error.to_string())
            } else {
                let count = outcome
                    .entries
                    .iter()
                    .filter(|info| info.module == name)
                    .count();

                if count == 0 {
                    ("空", "这台机器上没有可显示的数据".to_owned())
                } else {
                    ("显示", format!("{count} 项"))
                }
            };

        // 理由里可能有路径（`跳过` 就是一条路径），同样过一遍清洗再打印。
        let detail = vitals_rs::render::sanitize::sanitize(&detail);

        println!("{name:width$}  {state}  {detail}");
    }
}

/// `--sources` 的报告：每个模块实际读过哪些文件。
///
/// 它是运行时记录下来的（见 `core::sources`），不是每个模块手写的一张表——表会跟代码
/// 漂移，记录不会。一个模块如果什么文件都没碰（数据来自环境变量或系统调用），这里会
/// **明说没有**，而不是编一个来源出来。
fn print_sources(modules: &[ModuleEntry], plan: &conditions::Plan, outcome: &RunOutcome) {
    let width = modules
        .iter()
        .map(|entry| entry.module_type.name().len())
        .max()
        .unwrap_or(0);

    for entry in modules {
        let name = entry.module_type.name();

        if let Some(skipped) = plan.skipped.iter().find(|item| item.module == name) {
            println!("{name:width$}  跳过  {}", skipped.reason.describe());

            continue;
        }

        let paths = outcome
            .sources
            .iter()
            .find(|item| item.module == name)
            .map_or(&[][..], |item| item.paths.as_slice());

        if paths.is_empty() {
            println!("{name:width$}  没有读文件  （数据来自环境变量或系统调用）");
        } else {
            // 路径是拼出来的（`$HOME`、`$TZ`、`$XDG_CONFIG_HOME` 都参与），
            // 所以它跟模块的值一样是外部字符串，打印前必须过一遍清洗。
            let listed: Vec<_> = paths
                .iter()
                .map(|path| vitals_rs::render::sanitize::sanitize(path).into_owned())
                .collect();

            println!("{name:width$}  {}", listed.join(", "));
        }
    }
}
