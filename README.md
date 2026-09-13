# Vitals

> 一眼看完这台机器的状态。

`vitals` 是一个系统信息工具（fastfetch / neofetch 这一类）。设计上多守了一条线：
**输出里的每一条都该能被追问「它是从哪来的」**——所以它不靠外部命令取数，而是自己读
`/proc`、`/sys`、`/etc` 与系统调用，并且能自报读了哪些文件。

- 61 个模块（fastfetch 2.68.1 有 76 个，其中 59 个我们也有）
- 默认视图实测 4-6 ms，四个采样模块并行后 221 ms（本机 Ryzen 7 8845H）
- 零子进程、零 `unsafe`、不写文件、不发网络包
- MSRV 1.85（edition 2024）

## 安装

```console
$ cargo install vitals-rs     # 装出来的命令叫 vitals
```

从源码装：

```console
$ git clone https://github.com/juyuan-tech/vitals
$ cargo install --path vitals
```

## 用法

```console
$ vitals                      # 默认视图
$ vitals --module os,cpu,memory
$ vitals --list-modules
$ vitals --gen-config > ~/.config/vitals/config.toml

$ vitals --json               # 给脚本用：结构化输出
$ vitals --explain            # 每个模块为什么出现、或为什么没出现
$ vitals --sources            # 每个模块**实际读了哪些文件**
```

`--explain` 把每个配置项分成四类之一：

```
os       显示  1 项
gamepad  空  这台机器上没有可显示的数据
camera   跳过  路径 /definitely/not/here 不存在
```

`--sources` 是运行时**观测**出来的，不是一张手写的对照表：

```
os    /etc/os-release
host  /sys/devices/virtual/dmi/id/product_name, /sys/devices/virtual/dmi/id/sys_vendor
wm    没有读文件  （数据来自环境变量或系统调用）
```

## 模块

**系统**　`os` `host` `kernel` `bios` `board` `chassis` `uptime` `datetime` `version`
`init-system` `bootmgr` `processes` `loadavg`

**硬件**　`cpu` `cpu-cache` `gpu` `memory` `swap` `disk` `physical-disk` `btrfs`
`battery` `power-adapter` `brightness` `lm` `tpm` `sound` `wifi` `camera` `keyboard`
`mouse` `gamepad` `monitor` `display`

**会话与桌面**　`user` `users` `shell` `terminal` `terminal-size` `terminal-font`
`locale` `editor` `de` `wm` `wmtheme` `theme` `icons` `font` `cursor` `colors`

**网络与吞吐**　`local-ip` `dns` `net-io` `disk-io` `cpu-usage` `top`

**软件**　`packages` `rust`

**版式**　`title` `separator` `break`

## 配置

`~/.config/vitals/config.toml`（遵循 `XDG_CONFIG_HOME`），也可以用 `--config` 指到别处。
`--gen-config` 会打印一份带注释的模板。每个模块都能声明条件：

```toml
[[modules]]
type = "camera"
when-file-exists = "/dev/video0"

[[modules]]
type = "pci"
when-command-exists = "lspci"   # 只查 PATH，**不执行命令**
```

`when-command-exists` 是本项目里唯一看起来像「跑命令」的字段：它只沿 `PATH` 找一个
同名文件，不会 `fork`、不会 `exec`。

## 性能

同一台机器（Ryzen 7 8845H）上的实测：

| 场景 | vitals | fastfetch 2.68.1 |
| --- | --- | --- |
| 默认视图 | 4-6 ms | 20-37 ms |
| `net-io,disk-io,cpu-usage,top` | 221 ms | —— |

四个采样模块各自要等一个采样窗口。串起来是 821 ms，而这些等待期是空转——现在是共享
计数器派活的并行采集（线程数上限 16），总时长≈一次窗口。

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

## 已知未做

fastfetch 2.68.1 里我们没有的模块，以及原因：`Bluetooth`、`OpenGL`、`Vulkan`、
`OpenCL`、`Codec`、`PublicIp`、`Weather`、`Player`、`Media`、`Command`、`Wallpaper`、
`TerminalTheme`、`Zpool` 等——多数要么需要链接系统库（与「零 `unsafe`、零外部依赖」冲突），
要么需要发网络请求，要么依赖核心外的外部命令。`Custom` 还没有做：它要的是把任意命令的
输出摆进版式，与本项目「不执行命令」的取向直接冲突，需要先想清楚边界。

## 开发

```console
$ cargo test
$ cargo clippy --all-targets -- -D warnings
$ cargo fmt --all -- --check
```

三条都绿才算过。`PLAN.md` 是设计笔记（不再对外承诺，见其中「过时」的标注）。

## 许可

MIT OR Apache-2.0，任选其一。见 `LICENSE-MIT` 与 `LICENSE-APACHE`。
