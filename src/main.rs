//! CLI 薄壳：解析参数、调用 `vitals_rs`、决定退出码。
//!
//! 这里只干三件事：决定退出码、决定往哪个流写、把逻辑交给 lib。
//! 采集要到阶段 4、渲染要到阶段 5 才接上，那之前 `vitals` 只如实打印版本，
//! 不假装已经渲染了什么。

use std::process::ExitCode;

use clap::Parser;
use vitals_rs::cli::{Cli, Settings};
use vitals_rs::config::{self, ModuleType};
use vitals_rs::{PROGRAM, VERSION};

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

    // 诊断一律走 stderr：`vitals > 文件` 的时候不该被混进结果里。
    if settings.verbose {
        for line in settings.describe() {
            eprintln!("{PROGRAM}: {line}");
        }
    }

    // 阶段 4 在这里接 Dispatcher，阶段 5 接文本渲染器。
    println!("{PROGRAM} {VERSION}");

    ExitCode::SUCCESS
}
