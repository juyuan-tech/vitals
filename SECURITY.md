# 安全策略

> **English summary.** `vitals` reads local system files and sends no network packets. To report
> a vulnerability, email **gxydream@outlook.com** (please do not open a public issue for
> security-sensitive reports) with the version, your distribution, and reproduction steps.

## 支持的版本

只有最新的发布版本（当前 `0.1.x`）会被修。请先确认问题在最新版上还能复现：

```console
$ vitals --version
```

## 怎么报告

- **安全问题**：发邮件到 **gxydream@outlook.com**。与安全无关的普通 bug、功能请求，
  走 GitHub Issues。
- 请在邮件里带上：
  - `vitals --version` 的输出；
  - 发行版与内核版本；
  - 复现步骤（写了哪个配置、跑了哪条命令）；
  - 能复现的话，附上 `vitals --verbose` 的 stderr 输出（它含最终生效的设置）。
- 这是个人维护的项目，回复时间**尽力而为**，不做时限承诺。修正会出现在
  [`CHANGELOG.md`](CHANGELOG.md) 里；确认过的历史发现都记在 [`AUDIT.md`](AUDIT.md)。

## 什么算安全问题

这个程序的价值在于「输出可信、行为可查」，所以下面这些都在范围内：

- **输出注入**：某个字段里的控制字符（ESC、双向文本控制符）绕过过滤，影响到终端。
- **路径问题**：通过环境变量（`TZ`、`TZDIR`、`XDG_*`、`HOME` 等）把读取带出预期目录，
  或读到不该读的文件。
- **越界 / 崩溃**：畸形输入（`utmp`、EDID、TZif、`/proc` 文件被截断或写坏）导致
  panic、越界或内存问题。
- **行为与承诺不符**：出现了子进程、写文件、发网络包——这三件事项目承诺不做。

## 什么不算

- **显示内容不准确**（数字口径与别的工具不同）：那是取数口径问题，请先看
  [`docs/faq.md`](docs/faq.md)，有疑问欢迎开 issue。
- **需要 root 才能读的模块没有数据**：那是权限，不是漏洞。
- **`local-ip` 建立了一个 UDP socket**：这是设计——它靠内核的路由查找问出本机地址，
  UDP 的 `connect` **不发送任何数据**。审计里记为信息项 F7（见 [`AUDIT.md`](AUDIT.md)）。

## 项目自己的安全底线

改这个仓库的人（包括你自己提 PR）需要维持：

- 零子进程；`when-command-exists` 也只查 `PATH`，不 `exec`。
- 零 `unsafe`（`#![forbid(unsafe_code)]`）。
- 不写文件、不发网络包。
- 单文件读取上限 8 MiB，超限报错而不截断。
- 输出前过滤控制字符（`src/render/sanitize.rs`）。
