# Vitals

**中文** | [English](README.en.md)

[![CI](https://github.com/juyuan-tech/vitals/actions/workflows/ci.yml/badge.svg)](https://github.com/juyuan-tech/vitals/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/vitals-rs.svg)](https://crates.io/crates/vitals-rs)
[![docs.rs](https://docs.rs/vitals-rs/badge.svg)](https://docs.rs/vitals-rs)

> 一眼看完这台机器的状态。

`vitals` 是一个系统信息工具（fastfetch / neofetch 这一类）。设计上多守了一条线：
**输出里的每一条都该能被追问「它是从哪来的」**——所以它不靠外部命令取数，而是自己读
`/proc`、`/sys`、`/etc` 与系统调用，并且能自报读了哪些文件。

- 61 个模块（fastfetch 2.68.1 有 76 个，其中 59 个我们也有）
- 默认视图实测 4-6 ms，四个采样模块并行后 221 ms（本机 Ryzen 7 8845H）
- 零子进程、零 `unsafe`、不写文件、不发网络包
- MSRV 1.85（edition 2024）
- 安全审计报告：[`AUDIT.md`](AUDIT.md)

## 安装

```console
$ cargo install vitals-rs     # 装出来的命令叫 vitals
```

从源码装：

```console
$ git clone https://github.com/juyuan-tech/vitals
$ cargo install --path vitals
```

前置条件：**Linux**（数据来自 `/proc` 与 `/sys`；代码里没有任何平台分支，其它系统未验证），
Rust 1.85 或更新。

## 快速开始

```console
$ vitals                      # 默认视图
$ vitals --module os,cpu,memory
$ vitals --list-modules       # 61 个模块名
$ vitals --gen-config > ~/.config/vitals/config.toml

$ vitals --json               # 给脚本用：结构化输出
$ vitals --explain            # 每个模块为什么出现、或为什么没出现
$ vitals --sources            # 每个模块**实际读了哪些文件**
```

默认视图长这样（**示例**：这就是本机真实的默认视图，只把用户名、机型、面板型号、网卡名、电池名换成了占位值，其余原样。`--logo none` 不会去掉这一段——它只去掉左边的艺术字。）：

```console
user@host
──────────────
OS: Arch Linux x86_64
Host: Example Laptop 14
Kernel: Linux 7.2.4-arch1-2
Uptime: 1 day, 7 hours, 16 mins
Packages: 3 (appimage), 3 (flatpak), 1042 (pacman)
Shell: zsh 5.9.2
Display (eDP-1): 2880x1800 in 14", 120 Hz [Built-in]
Window Manager: Niri 26.04 (Wayland)
Cursor: breeze (30px)
Terminal: kitty 0.48.2
Terminal Font: JetBrainsMono Nerd Font 12pt
CPU: AMD Ryzen 7 8845H w/ Radeon(TM) 780M Graphics (8C/16T)
GPU: AMD Radeon 780M [HawkPoint1] (amdgpu)
Memory: 22.13 GiB / 30.65 GiB (72%)
Swap: 7.88 GiB / 47.33 GiB (17%)
Disk: 53.37 GiB / 920.87 GiB (6%)
Local IP (wlan0): 192.168.1.101/24
Battery (BAT0): 100% [AC Connected]
Locale: zh_CN.UTF-8
```

## 每个数字都能追问来源

这三点是这个项目存在的理由，也是它和同类工具最大的不同：

```console
$ vitals --sources --module os,host,wm
os    /etc/os-release
host  /sys/devices/virtual/dmi/id/sys_vendor, /sys/devices/virtual/dmi/id/product_name, /sys/devices/virtual/dmi/id/product_version, /sys/devices/virtual/dmi/id/board_name
wm    没有读文件  （数据来自环境变量或系统调用）
```

- `--sources` 是运行时**观测**出来的，不是一张手写的对照表；
- `--explain` 说**状态**：显示 / 空 / 跳过 / 失败，四种各有理由；
- `--verbose` 说**设置**：最终生效的配置（写到 stderr）。

```console
$ vitals --explain --module os,gamepad
os       显示  1 项
gamepad  空  这台机器上没有可显示的数据
```


另外两态可以自己造出来看（下面两条都在本机跑过）：

```console
$ printf 'config_version = 1\n\n[[modules]]\ntype = "camera"\nwhen-file-exists = "/definitely/not/here"\n' > /tmp/cond.toml
$ vitals --config /tmp/cond.toml --explain --logo none
camera  跳过  路径 /definitely/not/here 不存在

$ TZ=/etc/shadow vitals --explain --module datetime --logo none
vitals: datetime 模块失败：打开 /etc/shadow 失败
datetime  失败  打开 /etc/shadow 失败
```

注意**模块失败时退出码仍是 0**：失败会同时在 stderr 上给一行警告，但不会让一个模块的问题
掩盖掉其它模块的结果。

## 模块

61 个，完整逐条说明见 **[`docs/modules.md`](docs/modules.md)**（含每个模块实际读取的文件）。

**系统**（13）　`os` `host` `kernel` `bios` `board` `chassis` `uptime` `datetime` `version`
`init-system` `bootmgr` `processes` `loadavg`

**硬件**（21）　`cpu` `cpu-cache` `gpu` `memory` `swap` `disk` `physical-disk` `btrfs`
`battery` `power-adapter` `brightness` `lm` `tpm` `sound` `wifi` `camera` `keyboard`
`mouse` `gamepad` `monitor` `display`

**会话与桌面**（16）　`user` `users` `shell` `terminal` `terminal-size` `terminal-font`
`locale` `editor` `de` `wm` `wmtheme` `theme` `icons` `font` `cursor` `colors`

**网络与吞吐**（6）　`local-ip` `dns` `net-io` `disk-io` `cpu-usage` `top`

**软件**（2）　`packages` `rust`

**版式**（3）　`title` `separator` `break`

## 配置

`~/.config/vitals/config.toml`（遵循 `XDG_CONFIG_HOME`），也可以用 `--config` 指到别处。
`--gen-config` 会打印一份带注释的模板。**这个文件整体替换内置默认**：写了 `modules`，
内置列表就不再生效。逐键参考见 **[`docs/configuration.md`](docs/configuration.md)**。

每个模块都能声明条件，条件不满足就跳过它（跳过不是错误）：

```toml
config_version = 1

[[modules]]
type = "os"
when-file-exists = "/etc/os-release"

[[modules]]
type = "camera"
when-file-exists = "/dev/video0"
```

`when-command-exists` 是本项目里唯一看起来像「跑命令」的字段：它只沿 `PATH` 找一个同名文件，
不会 `fork`、不会 `exec`。

## JSON

`vitals --json` 输出 `{"schema_version": 1, "entries": [...], "failures": [...]}`，
字段名是契约。形状、兼容策略、jq 示例见 **[`docs/json.md`](docs/json.md)**，
机器可读的 JSON Schema 在 **[`docs/vitals.schema.json`](docs/vitals.schema.json)**。

## 性能

同一台机器（Ryzen 7 8845H）上的实测：

| 场景 | vitals | fastfetch 2.68.1 |
| --- | --- | --- |
| 默认视图 | 4-6 ms | 20-37 ms |
| `net-io,disk-io,cpu-usage,top` | 221 ms | —— |

四个采样模块各自要等一个 200 ms 的采样窗口。串起来是 821 ms，而这些等待期是空转——现在是
共享计数器派活的并行采集（线程数上限 16），总时长≈一次窗口。

## 安全与隐私

- **零子进程**：不 `fork`/`exec` 任何东西，`when-command-exists` 也只查 `PATH`。
- **零 `unsafe`**：`#![forbid(unsafe_code)]`；系统调用（`uname`、`statvfs`、
  `tcgetwinsize`）走 `rustix` 的安全封装。
- **不写文件**：没有任何 `File::create`/`fs::write`/`remove_*`（测试除外）。
- **不发网络包**：唯一碰 socket 的是 `local-ip`——它建一个 UDP socket 并 `connect` 到
  一个目标地址，只为让内核做一次路由查找好问出本机地址；UDP 的 `connect` **不发包**。
  查不到路由就是「没有出口」＝无数据。
- **输出前去掉终端控制字符**：卷标、`utmp` 里的用户名、EDID 型号、由环境变量拼出来的
  路径都可能带 ESC，原样打到终端就能移动光标、改标题。C0/C1/DEL 与双向文本控制符会被
  去掉，中文与 emoji 保留。
- **单文件读取上限 8 MiB**，超限报错而不截断（`$TZ`/`$TZDIR` 这类环境变量会参与拼路径）。
- **时区名做路径校验**：相对名字里出现 `..` 这类成分一律不接受，不让一个环境变量把读取
  带到 zoneinfo 目录外面。
- **可审计**：`--sources` 告诉你每个模块读了什么；`--json` 由 `serde_json` 转义，
  控制字符不会漏成裸字节。

完整审计（8 条发现、逐条证据与修复）：[`AUDIT.md`](AUDIT.md)。安全问题的报告方式见
[`SECURITY.md`](SECURITY.md)。

## 常见问题

多数问题在 **[`docs/faq.md`](docs/faq.md)** 里有实测回答，例如「为什么内存数字和 `free`
不一样」「为什么 `--json` 的数字不适合拿来计算」「为什么默认视图不显示 BIOS」。

## 已知未做

fastfetch 2.68.1 里我们没有的模块，以及原因：`Bluetooth`、`OpenGL`、`Vulkan`、
`OpenCL`、`Codec`、`PublicIp`、`Weather`、`Player`、`Media`、`Command`、`Wallpaper`、
`TerminalTheme`、`Zpool` 等——多数要么需要链接系统库（与「零 `unsafe`、零外部依赖」冲突），
要么需要发网络请求，要么依赖核心外的外部命令。`Custom` 还没有做：它要的是把任意命令的
输出摆进版式，与本项目「不执行命令」的取向直接冲突，需要先想清楚边界。

其它已知缺口：命令行帮助目前只有中文；没有 shell 补全（补全需要引入 `clap_complete`，
与「不增依赖」冲突，只能手写）。

## 开发

```console
$ cargo test
$ cargo clippy --all-targets -- -D warnings
$ cargo fmt --all -- --check
```

三条都绿才算过（CI 里还有一个作业专门用 1.85 编一遍）。贡献方式与代码结构见
[`CONTRIBUTING.md`](CONTRIBUTING.md)。`PLAN.md` 是设计笔记（不再对外承诺，见其中「过时」的标注）。

## 文档

| 文件 | 内容 |
| --- | --- |
| [`docs/modules.md`](docs/modules.md) | 61 个模块逐条：显示什么、读哪些文件 |
| [`docs/configuration.md`](docs/configuration.md) | 配置逐键参考、条件语法、示例 |
| [`docs/cli.md`](docs/cli.md) | 命令行、退出码、环境变量 |
| [`docs/json.md`](docs/json.md) | `--json` 形状与消费方式 |
| [`docs/vitals.schema.json`](docs/vitals.schema.json) | JSON Schema（draft 2020-12） |
| [`docs/faq.md`](docs/faq.md) | 常见问题（都是实测回答） |
| [`docs/logo.md`](docs/logo.md) | Logo 与配色 |
| [`AUDIT.md`](AUDIT.md) | 安全与质量审计报告 |
| [`CHANGELOG.md`](CHANGELOG.md) | 版本变更 |

## 许可

MIT OR Apache-2.0，任选其一。见 `LICENSE-MIT` 与 `LICENSE-APACHE`。
