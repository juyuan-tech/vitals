# 贡献指南

欢迎 issue 和 PR。这个项目对「能查证」的要求比对「功能多」的要求高，所以下面几条不是客套话，
是合并前会逐条看的。

## 项目底线

改代码请先读 [`AUDIT.md`](AUDIT.md) 与 README 的「安全与隐私」一节。这四条是设计的一部分，
不接受为某个功能让步：

- **不新增依赖**（连 `clap_complete` 都不加，所以补全暂时没有）。
- **不加 `unsafe`**（`#![forbid(unsafe_code)]`）。
- **不执行外部命令、不发网络包、不写文件**。
- **单文件读取上限 8 MiB**，超限报错而不截断。

## 本地门禁

```console
$ cargo test
$ cargo clippy --all-targets -- -D warnings
$ cargo fmt --all -- --check
```

三条都必须绿。CI 里除了这三条，还有一个作业专门用 **1.85** 编一遍（MSRV），并跑测试。

## 代码在哪

| 路径 | 作用 |
| --- | --- |
| `src/collectors/` | 每个模块一个采集器，多数还配一个兄弟测试模块 |
| `src/collectors.rs` | **模块登记表**（`&memory::Memory,` 这样一行一个）。表里的顺序就是 `--list-modules`、参数校验报错里的顺序 |
| `src/core/` | 采集调度（`dispatch.rs`）、`Info`、渲染接口、`--sources` 的记录（`sources.rs`） |
| `src/render/` | 文本与 JSON 渲染、Logo、控制字符过滤（`sanitize.rs`） |
| `src/config.rs` + `src/config/` | 配置模型、内置默认配置（`default.toml`）、路径解析（`path.rs`） |
| `src/cli.rs` | 参数解析 |
| `src/conditions.rs` | 条件判断 |

## 加一个模块

1. 新建 `src/collectors/<名字>.rs`，实现 `Collector`：`name()` 与 `collect()` 两个方法。
2. 在 `src/collectors.rs` 的登记表里加一行。**位置有意义**：它决定 `--list-modules` 与报错里的
   模块顺序。
3. 补测试：正常路径、读不到、以及**畸形输入不许 panic**。解析二进制结构的模块请参考
   `src/collectors/users.rs`、`bootmgr.rs`、`display.rs` 里已有的做法。
4. 如果它要出现在默认视图里、或需要默认条件，同步 `src/config/default.toml`——
   注意 `--gen-config` 打印的就是这个文件本身（`include_str!`），改它就等于改模板。
5. 同步文档：`docs/modules.md` 对应条目、以及 `--sources` 能观测到的新路径。
6. 把新模块加进 `presets/all.toml`（它必须等于 `--list-modules` 的全部模块）。

   `tests/docs.rs` 会替你检查第 5、6 步：示例配置能不能跑、`all.toml` 是否等于注册表、
   `docs/modules.md` 是否覆盖了每个模块。忘了改就会红。

## 测试与文档的要求

- **测试不许依赖宿主状态。** CI 是 headless 的：别假定有屏幕、有独立显卡、有 `/dev/video0`、
  有已连接 EDID 的显示器。真实样本要**用 guard**——样本不存在就跳过，而不是 `assert`。
  （历史上 CI 因此红过两次，见 `CHANGELOG.md`。）
- **文档里的每条命令都要真跑过。** 数字、路径、输出片段都要能追到真实运行；拿不准就不写。
- **模糊测试用仓库里已有的手写 PRNG**，不要为此引入新依赖。
- 改了行为就同步改 `docs/` 与 README；改了用户可见的东西就补 `CHANGELOG.md`。

## 提交信息

中文，陈述句，说清「改了什么、为什么」。一次提交一件事。参考现有历史：

```console
$ git log --oneline -3
```

## 许可

提交即表示你同意以 MIT OR Apache-2.0 双许可发布你的贡献。
