//! CLI 薄壳：解析参数、调用 `vitals_rs`、把结果写到 stdout / stderr。
//!
//! 阶段 0–2 只有 `--version`、`--help` 和 `--gen-config`；
//! 完整参数见 `PLAN.md` §4（阶段 3），渲染器在阶段 5 接上。

use clap::Parser;
use vitals_rs::{PROGRAM, VERSION};

/// 这里必须显式写 `name = "vitals"`：clap 默认取包名，也就是 `vitals-rs`，
/// 那样 `--version` 会打印成 `vitals-rs 0.1.0`，和命令名对不上。
#[derive(Debug, Parser)]
#[command(
    name = "vitals",
    version,
    about = "Your system's vital signs, at a glance."
)]
struct Cli {
    /// 把内置默认配置打印到 stdout，作为自己写配置的起点。
    #[arg(long = "gen-config")]
    gen_config: bool,
}

fn main() {
    let cli = Cli::parse();

    if cli.gen_config {
        print!("{}", vitals_rs::config::default_toml());
        return;
    }

    // 阶段 0 的验收标准：`vitals --version` 输出 `vitals 0.1.0`。
    // 无参数时暂时也只报版本——渲染器要到阶段 5 才存在。
    println!("{PROGRAM} {VERSION}");
}
