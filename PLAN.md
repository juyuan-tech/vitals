# Vitals 开发计划（修订版）

> 本文件在原始计划基础上修订。所有「替换/新增」都标注了依据；**实测**字样表示我在本机验证过，
> 不是印象。原文十章的结构保留，章内做了增删。

---

## §0 变更记录：被替换掉的过时做法

| # | 原计划写法 | 问题 | 改为 | 依据 |
|---|---|---|---|---|
| 1 | 「他人可 `cargo install vitals`」 | crate 名已定 `vitals-rs`，命令本身对不上 | `cargo install vitals-rs`（装出来仍是 `vitals`） | 实测：crates.io 上 `vitals` 被 robopoker 的 telemetry 库占用（1.2.0、106 下载、09-06 更新），`vitals-rs` 空闲 |
| 2 | 阶段 10 手写 README/Release/二进制 | 手搓 GitHub Actions 是 2022 年的做法 | `cargo-dist` 0.32.0：一条命令生成跨平台产物 + shell/pwsh 安装器 + Homebrew/MSI + binstall 元数据 | crates.io：cargo-dist 0.32.0 |
| 3 | 阶段 9「快照测试」未指定工具 | 手写 expect 文件难维护 | `insta` 1.48.0（`cargo insta review` 工作流） | crates.io：insta 1.48.0 |
| 4 | 配置路径写死 `~/.config/vitals/` | 无视 XDG 规范 | 读 `$XDG_CONFIG_HOME`（相对路径按规范忽略），回退 `~/.config`；**不用 `etcetera`，自己写十几行** | 实测：库把 env 读取藏在内部，而 edition 2024 的 `set_var` 是 `unsafe`（本 crate `forbid(unsafe_code)`）→ 用库就没法给路径解析写测试；自写则纯逻辑可注入、可测，Windows 分支留 v1.0 |
| 5 | `--no-color` + 手写 ANSI | 管道里仍会吐转义码，需用户手动干预 | `anstyle` 1.0.14 + `anstream` 1.0.0：非 TTY 自动降级，并遵循 `NO_COLOR` / `CLICOLOR_FORCE` | crates.io：anstyle 1.0.14 / anstream 1.0.0 |
| 6 | 「对齐必须用 Unicode 显示宽度」但未指定 | 教程多为 0.1 写法，0.2 是破坏性变更 | `unicode-width` 0.2.2；`char::width()` 现返回 `Option<usize>` | 实测：`"a\u{0301}b"` → bytes=4 / chars=3 / **display=2**；`'a'.width()` = `Some(1)` |
| 7 | 阶段 8「模块并行采集」未指定手段 | 容易顺手引 rayon | `std::thread::scope`（1.63 起稳定），一次性 CLI 无需线程池 | std 稳定特性 |
| 8 | 「外部命令带超时」未指定实现 | **std 没有这个 API** | 自写 `try_wait` + deadline 轮询（零依赖，推荐）；备选 `wait-timeout` 0.2.1 | 实测：stable 1.98.1 编译 `child.wait_timeout(..)` → `error[E0599] no method named wait_timeout` |
| 9 | 「禁止 unsafe」+ 需要 `statvfs`/`uname` | 二者直接冲突，会卡住 Disk/Kernel 模块 | 自有代码零 unsafe，系统调用一律经 `rustix` 1.1.4 | 实测：`rustix::fs::statvfs("/")` 与 `rustix::system::uname()` 编译运行通过，用户代码无 unsafe |
| 10 | 「能用系统信息库就用库」 | 与阶段 8 的 <20ms 冷启动冲突；`sysinfo` 重且 CPU 占用率必须两次采样 | 反转默认：**优先直读 `/proc`、`/sys`、`/etc`**；系统调用走 rustix；确需才引 `sysinfo` | 见 §3、§5 |
| 11 | 里程碑表与开发阶段表 | **两套切片互不兼容**：里程碑把 JSON/Logo 放 v0.3、条件放 v0.2；开发顺序却在第 7、8 步就做 Logo/JSON，第 9 步才做条件，且 v0.1 直接吞掉了全部内容 | 以**阶段**为唯一真源，里程碑由阶段边界重新推导 | 见 §7、§8 |
| 12 | 阶段 2 验收「`vitals --gen-config` 可输出」 | 验收依赖阶段 3 才有的 CLI | 阶段 2 验收改为 `cargo test` + 最小 `cargo run -- --gen-config` | 依赖倒置 |
| 13 | 只有 `[[bin]]`，没有 lib | 集成测试、快照测试无法复用内部结构 | `src/lib.rs` + 薄 `src/main.rs` 双 target | 见 §2.6 |
| 14 | 默认「用户配置覆盖默认值」但没定工具链 | 本机 rustup 默认是 nightly | 写 `rust-version`，加 `rust-toolchain.toml` 锁 stable，日常 `cargo +stable` 验证 | 实测：本机默认 nightly 1.100.0；stable 1.98.1；另有 1.97.1 |
| 15 | 公网 IP / 网络模块未定依赖形态 | 会拉进 TLS 栈 + 联网，与体积、冷启动、默认隐私冲突 | 做成 Cargo feature（`net`，默认关），模块也默认关 | 阻塞式 `ureq` 3.4.1 可用，但体积代价必须可选 |
| 16 | 包管理器计数写 `pacman / dpkg / rpm`（暗示开子进程） | 违反自己「能读文件就不开子进程」的原则 | 数 `/var/lib/pacman/local` 目录项、解析 `/var/lib/dpkg/status`；rpm 库格式复杂，退回 `rpm -qa` | 见 §5.2 |
| 17 | `when-command-exists` 未定实现 | 若真去执行命令，既慢又有副作用 | 只做 PATH 查找，**绝不执行** | 见 §2.5 |
| 18 | JSONC 兼容被当作「fastfetch 迁移预留」 | 格式兼容 ≠ schema 兼容，fastfetch 的 `modules` 结构与本计划不同 | 明确 JSONC 只是「带注释的另一种写法」；fastfetch schema 适配列为 v0.6 独立条目，且要求模块类型名刻意对齐；**并且不单独落地 JSONC 解析器**——阶段 2 只做 TOML，JSONC 与适配层一起排到 v0.6 | 见 §2.2 |
| 19 | 文件布局未规定，目录模块会顺手写成 `foo/mod.rs` | `mod.rs` 是 Rust 2018 之前的旧写法；同仓库多个 `mod.rs` 在标签页/搜索/diff 里无法区分 | 一律自名文件 + 同名目录（`collectors.rs` + `collectors/`），禁止 `mod.rs`；并用 `#![warn(clippy::mod_module_files)]` 在 CI 里钉死 | 实测：clippy 0.1.100 报 `` `mod.rs` files are not allowed `` + `` move `src/foo/mod.rs` to `src/foo.rs` ``；该 lint 属 restriction 组、默认关闭 |

---

## 一、项目定位

- 项目名：**Vitals**
- 命令名：`vitals`
- crate 名：`vitals-rs`（`vitals` 已被占用，见 §0-1），二进制名仍为 `vitals`
- 标语：*Your system's vital signs, at a glance.*
- 定位：系统信息采集与展示工具，类 fastfetch / neofetch
- 语言：Rust，edition 2024，MSRV 1.85，`rust-version = "1.85"`
- 平台：Linux 优先，架构预留跨平台（见 §2.7）
- 配置路径：`$XDG_CONFIG_HOME/vitals/config.toml`，未设置时回退 `~/.config/vitals/config.toml`
- 配置格式：TOML（v0.1 实际落地的唯一格式）；JSONC 兼容与 fastfetch schema 适配层绑定，一起排到 v0.6；JSON 仅作输出
- 动态配置：明确排除，不做脚本引擎
- 输出：终端彩色 + JSON
- 许可证：MIT OR Apache-2.0（Rust 生态惯例）

**工具链纪律**：本机 rustup 默认是 nightly，而本 crate 要发给 stable 用户。因此仓库加
`rust-toolchain.toml`（`channel = "stable"`），CI 与本地都以 stable 为准；nightly 只在需要
`cargo +nightly fmt` 之类时显式调用。

---

## 二、技术决策

### 2.1 依赖清单（按阶段引入，不一次性定死）

| 依赖 | 版本 | 引入阶段 | 用途 |
|---|---|---|---|
| `clap` | 4.6（derive） | 3 | CLI 解析 |
| `serde` + `serde_json` | 1 / 1 | 2、6 | 配置与 JSON 输出 |
| `toml` | 1.1 | 2 | 主配置解析/生成（注意：生态里大量教程仍是 0.8 写法） |
| `jsonc-parser` | 0.33 | v0.6（feature `jsonc`） | JSONC 兼容，与 fastfetch 适配层同期落地 |
| `anstyle` + `anstream` | 1.0 / 1.0 | 5 | 颜色与非 TTY 自动降级 |
| `unicode-width` | 0.2.2 | 5 | 显示宽度对齐 |
| `rustix` | 1.1（features `fs`, `system`, `termios`） | 4、5 | `statvfs`、`uname`、`tcgetwinsize` 等系统调用的安全封装 |
| `insta` | 1.48（dev） | 11 | 渲染快照测试 |
| `ureq` | 3.4（feature `net`，默认关） | 8 | 唯一的联网模块（公网 IP） |

**明确不引**：`sysinfo`（除非将来确需进程列表）、`rayon` / `crossbeam`、任何 async runtime、
`inventory` / `linkme`（模块注册用静态数组即可，不要编译期魔法）、
`etcetera`（XDG 路径自己写十几行，用库会让路径解析无法测试，见 §0-4）。

### 2.2 配置语言

- **主配置 TOML**：Rust 生态最好、支持注释、手写友好、无 YAML 隐式类型坑、无 JSON 无注释问题。
  `toml = "1"`（不是 0.8）。
- **兼容 JSONC（推迟到 v0.6）**：仅指**语法**上的「JSON + 注释 + 尾逗号」，用 `jsonc-parser` 解析后映射到同一套内部结构。阶段 2 只做 TOML——在适配层存在之前，独立的 JSONC 解析器服务不了任何人（见下方 ⚠️）。
- **输出 JSON**：仅用于 `--json`，不作为手写配置。建议同时带一个输出 schema 版本号。
- **排除**：YAML、RON、KDL、动态脚本。

> ⚠️ 澄清一个容易自我欺骗的点：**JSONC 兼容 ≠ fastfetch 迁移**。fastfetch 的配置是
> `modules` 里混放字符串与 `{"type": "cpu"}` 对象，字段名自成一套。要真正「为迁移预留」，
> 模块类型名与字段名就得刻意对齐它的命名；否则 JSONC 只是「允许注释的另一种写法」。
> 因此把 **fastfetch schema 适配层**单列为 v0.6 条目，不塞进 JSONC 里假装已经兼容。

### 2.3 配置加载顺序

内置默认 → 用户配置文件（`--config` 或 XDG 默认路径）→ CLI 参数覆盖。

第一版只做「用户配置覆盖默认值」，不做多层系统级合并（`/etc/vitals/` 之类留到 v1.0 再议）。

阶段 2 落地的细则（每条都有测试守着）：

- **显式路径读不到就报错，默认路径读不到就静默用默认**：`--config x.toml` 指了个不存在的文件必须说；
  而 `~/.config/vitals/config.toml` 不存在是第一次运行的常态，不该报错。
- **`modules` 一旦出现就整体替换**，不是追加：列表逐项合并说不清顺序与去重。
- **`config_version` 省略按当前版本算；高于当前版本直接报错**，低于的暂时接受（还没有迁移要跑）。
- **连 `HOME` 都没有时也用内置默认**，不报错。

### 2.4 配置结构原则

- 用数组表保证模块顺序
- 模块用类型标签区分，且类型标签是**枚举**而非自由字符串——写错一个字母 serde 就报
  ``unknown variant `cpuu`, expected one of ...``，把合法取值一并列出来
- **未知字段报错**（`#[serde(deny_unknown_fields)]`），避免拼写错误静默通过
- 带版本字段，为将来迁移留口（叫 `config_version`，不叫 `version`，免得和应用版本混淆）
- 每个模块可带声明式条件字段——**阶段 7 才进 schema**：在那之前写了就是未知字段报错，
  好过出现「解析通过但什么也没做」的字段

### 2.5 声明式条件

模块可声明：

- `platforms`：限定平台，如只跑 `linux`
- `when-command-exists`：命令不存在则跳过
- `when-file-exists`：路径不存在则跳过

**实现要点**：`when-command-exists` 只做 **PATH 查找**，绝不真的执行命令——否则「判断有没有
nvidia-smi」本身就要起一个进程。主流程在调度前统一评估，任一不满足则跳过该模块，不报错。

### 2.6 格式化与错误处理

- **格式化**：极简模板替换，不引入脚本引擎。模块采集时填充变量，渲染时按模板替换。
  语法限定为 `{var}` 一种；缺失变量的行为由配置决定（`keep` 保留原样 / `empty` 替换为空），
  默认 `keep`，便于发现拼写错误。
- **错误类型**：模块内部用 `thiserror` 定义具体错误；主流程用统一错误类型汇聚。
- **单模块失败不影响整体**：打印警告到 stderr 后继续；`--verbose` 显示原因。
- **外部命令**：一律带超时；非零退出码按失败处理。
- **文件不存在**：返回「无数据」，不报错。
- **禁止 unsafe**：`#![forbid(unsafe_code)]` 放在 `src/lib.rs` 顶部。系统调用（`statvfs`、
  `uname`、`sysconf`）一律经 `rustix` 的安全封装——**已实测可行**。
  注意 `forbid(unsafe_code)` 只覆盖本 crate 自有代码，依赖内部（含 rustix）不可避免有 unsafe，
  这点要在 README 里说清楚，别承诺做不到的事。
- **外部命令超时实现**：std **没有** `Child::wait_timeout`（实测 stable 1.98.1 报 E0599）。
  采用零依赖方案：`thread::scope` 内 `spawn` 子进程 + `try_wait` 轮询到 deadline，
  超时则 `kill()` 并回收。备选 `wait-timeout` 0.2.1（API 极简但更新停滞）。

### 2.7 双 target 结构（新增）

```
src/
  lib.rs              # #![forbid(unsafe_code)] + #![warn(clippy::mod_module_files)] + 模块树
  main.rs             # 薄壳：解析 CLI → 调 lib
  collectors.rs       # 模块声明（自名文件，不是 collectors/mod.rs）
  collectors/         # 每个采集器一个文件：os.rs、kernel.rs、host.rs、cpu.rs …
  config.rs
  config/             # 需要展开时才建（如 loader.rs、defaults.rs）
  render.rs
  render/             # text.rs、json.rs
tests/
  cli.rs              # 集成测试（跑二进制）
  snapshot.rs         # insta 快照
```

**模块布局规则：一律「自名文件 + 同名目录」，禁止 `mod.rs`。**

- `collectors.rs` 与 `collectors/` 同级；`collectors/os.rs` 由 `collectors.rs` 里的
  `pub mod os;` 引入，而不是 `collectors/mod.rs`。
- Rust 2018 起 rustc **两种布局都接受**，但 `mod.rs` 是旧写法：同一仓库里若干个 `mod.rs`
  在编辑器标签页、全局搜索、`git diff` 路径里都无法区分，只能靠上一级目录名辨认。
- 该规则不是编译器强制的，所以用 lint 钉死：`src/lib.rs` 顶部加
  `#![warn(clippy::mod_module_files)]`。它属 restriction 组、**默认关闭**，不显式开启就永远不生效；
  一旦开启，CI 的 `cargo clippy -- -D warnings` 会把任何 `mod.rs` 拦成错误。
- 实测依据：clippy 0.1.100 对该布局报
  ``warning: `mod.rs` files are not allowed, found `src/foo/mod.rs` `` +
  ``help: move `src/foo/mod.rs` to `src/foo.rs` ``。

必须有 `lib.rs`：否则阶段 9 的集成测试与快照测试只能黑盒测二进制，无法复用内部结构。
`cargo install vitals-rs` 仍然正常（bin target 保留）。

### 2.8 跨平台预留的落地方式

不是「写一堆 `#[cfg(windows)]`」，而是：

1. 采集器接口与渲染器接口完全平台无关；
2. 平台差异收敛到少数几个函数（路径常量、系统调用包装、命令存在性检查）；
3. Linux 实现放 `platform/linux/`，其他平台留空但接口已定；
4. 优先用 `rustix` 的 `unix` 抽象而非直接 `libc`，Windows 侧将来对应 `windows-sys`。

---

## 三、核心抽象

四件事先定清楚，后面所有模块都围绕它们：

1. **采集器 `Collector`**
   - 一个模块一个实现，统一接口
   - 输入：`&Context`（配置片段、路径解析器、平台信息、超时预算）
   - 输出：`Result<Info, CollectError>`
   - 约束：`Send + Sync`（阶段 10 要并行采集）；**只返回数据，绝不打印**
2. **信息 `Info`**
   - 显示键、显示值、供模板使用的变量表
   - JSON 输出直接用它（含类型、键、值）
3. **模块调度**
   - 配置里的模块声明 → 映射到采集器 → 评估声明式条件 → 执行 → 收集结果
   - 条件不满足则跳过（不报错）；采集失败则记录警告并继续
   - 注册表用静态数组：`const COLLECTORS: &[&dyn Collector]`，用名字线性查找（模块数量级 < 50，无需哈希表）
4. **渲染器 `Renderer`**
   - 输入：`Option<&Logo>` + `&[Info]`，输出到指定流
   - 实现两个：文本渲染器、JSON 渲染器

采集与渲染分离，模块与核心分离。

---

## 四、CLI 设计

| 参数 | 作用 |
|---|---|
| 无参数 | 默认渲染 |
| `--config <path>` | 指定配置文件 |
| `--json` | JSON 输出，自动关颜色与 Logo |
| `--logo <auto\|none\|名称>` | Logo 控制 |
| `--module <列表>` | **过滤**模块，逗号分隔（不改变配置中的顺序） |
| `--no-color` | 关闭颜色（与 `NO_COLOR` 环境变量等效） |
| `--list-modules` | 列出可用模块及其平台/条件 |
| `--gen-config` | 生成默认配置到 stdout |
| `--verbose` | 显示模块失败原因（stderr） |
| `--version` / `--help` | 版本 / 帮助 |

- 优先级：CLI 参数 > 配置文件 > 内置默认。
- `--module` 是**过滤**，不是覆盖：被过滤掉的模块不采集，但配置顺序不变。
- 颜色由 `anstream` 决定：stdout 不是 TTY 时自动无色，无需用户加参数；`--no-color` 只是显式再确认一次。

---

## 五、模块清单

### 5.1 v0.1 基础模块（十个）

OS、Host、Kernel、Uptime、Shell、User、CPU、Memory、Disk、Rust。

**数据来源（修订后）**：

| 模块 | 来源 | 说明 |
|---|---|---|
| OS | `/etc/os-release` | 解析 `ID` / `ID_LIKE` / `PRETTY_NAME` |
| Host | `/sys/devices/virtual/dmi/id/` | 读不到时回退 `/proc/device-tree/model` |
| Kernel | `rustix::system::uname()` | 实测可用，零 unsafe |
| Uptime | `/proc/uptime` | 直读，微秒级 |
| Shell | `$SHELL` → `/etc/passwd` 的登录 shell | 「此刻在跑哪个」由会话决定，所以环境变量优先；不 fork |
| User | uid（`/proc/self/status`）→ `/etc/passwd` | 「我是谁」以 uid 为准：`$USER` 在 `su`/`sudo` 之后可能是陈旧的，只作兜底 |
| CPU | `/proc/cpuinfo` + `/sys/devices/system/cpu/` | 只做型号与核心数（必要时加频率）；**不做占用率**，理由见 §5.3 |
| Memory | `/proc/meminfo` | 直读 |
| Disk | `rustix::fs::statvfs()` + `/proc/mounts` | 实测可用；`statvfs` 才能拿到用量 |
| Rust | `$RUSTUP_TOOLCHAIN` → `$RUSTUP_HOME/settings.toml` 的 `default_toolchain` | 见下 |

> **Rust 模块已拍板：只读文件，不开子进程。** `rustc --version` 更准，但要付进程开销，
> 而这里的取舍是「能读文件就不开子进程」。因此该项显示的是**工具链名**
> （`stable-x86_64-unknown-linux-gnu`），不是编译器版本号——文案上必须诚实，不能标成 `rustc`。
>
> 优先级是**环境变量 > `settings.toml`**：`RUSTUP_TOOLCHAIN` 说的是「此刻生效的是哪个」。
> 实测：本项目用 `rust-toolchain.toml` 钉了 stable，于是在仓库里跑出来是 stable，
> 在仓库外跑出来是 rustup 的默认 nightly——两者都对，因为「此刻生效的」本来就不同。
>
> 代价：只装了系统 `rustc`（没有 rustup）的机器上该模块无数据。可以接受——无数据不报错、不留警告。
>
> **至此 v0.1 的十个模块一个子进程都不开**，`Context::timeout` 因此暂时无人使用，
> 留给 v0.2 的 GPU（`nvidia-smi`）这类模块。

### 5.2 v0.2 扩展模块（按优先级）

1. **GPU**：`nvidia-smi` 或 `lspci`；声明式条件跳过无 GPU 机器
2. **网络**：默认路由网卡的 IP
3. **电池**：`/sys/class/power_supply`，条件跳过台式机
4. **桌面环境 / WM**：`XDG_CURRENT_DESKTOP`
5. **终端**：`TERM_PROGRAM` 或 `TERM`
6. **包管理器计数**：直接数 `/var/lib/pacman/local` 目录项、解析 `/var/lib/dpkg/status` 的
   `Package:` 行；rpm 的库格式复杂，退回 `rpm -qa | wc -l`（这是唯一例外，需在模块文档里写明理由）
7. **温度**：`/sys/class/thermal` + `/sys/class/hwmon`
8. **多挂载点磁盘**
9. **本地 IP**；公网 IP **默认关闭**（需 `net` feature）
10. **主题 / 图标 / 字体**：GNOME / KDE

每个模块都走同一套：采集器接口 + 条件评估 + 注册到调度。

### 5.3 不做 CPU 占用率（范围纪律）

**决策：v0.1–v1.0 都不做 CPU 占用率。**

- 占用率是**监视器**（htop / btop / top）的职责，不是 fetch 工具的。fetch 工具给的是
  「身份与容量快照」，不是实时指标。
- 算术上的硬约束：`/proc/stat` 是**自开机累计计数器**，单次读取只能得出「开机至今平均占用」；
  要瞬时值必须两次采样加间隔，这与阶段 10 的「冷启动 < 20ms」直接冲突。
- 事实核查：fastfetch 确实有独立的 `cpuusage` 模块（`src/modules/cpuusage/`，并被
  `presets/all.jsonc` 引用），但它的 Linux 实现是**单次读 `/proc/stat`、不 sleep**
  （`src/detection/cpuusage/cpuusage_linux.c`），所以它报的正是「开机至今平均占用」而非瞬时值。
  它便宜，是因为它压根没做采样——这种数字对用户没什么价值。
- 将来若真要做，作为**可选模块**，并在文案上明确标注语义为「开机至今平均」。

CPU 模块只负责：型号、物理/逻辑核心数（必要时加频率）。

> **新基准（用户指定）**：目标是**全面对标 fastfetch 并做得更好**，不再满足于十个模块。
> 于是 `cpuusage` 归入「可选模块、默认不开、文案标注真实语义」，见 §5.4。

### 5.4 对标 fastfetch 的完整工作清单

基准：**fastfetch 2.68.1，76 个模块，默认视图 26 项**。它的默认结构是

```text
Title:Separator:OS:Host:Kernel:Uptime:Packages:Shell:Display:DE:WM:WMTheme:Theme:Icons:Font:
Cursor:Terminal:TerminalFont:CPU:GPU:Memory:Swap:Disk:LocalIp:Battery:PowerAdapter:Locale:Break:Colors
```

下表是工作清单。**判据只有一条：能不能不开子进程拿到**——能读文件/系统调用的优先，
需要外部命令的排在后面，且必须挂 `when-command-exists` 条件。

| 批次 | 模块 | 数据来源 / 备注 | 状态 |
|---|---|---|---|
| **核心**（§5.1） | OS、Host、Kernel、Bios、Board、Chassis、Uptime、Loadavg、Processes、Cpu、Memory、Swap、Disk、User、Shell、Terminal、TerminalSize、Locale、Editor、Version、InitSystem、Rust | 全是 `/proc`、`/sys`、环境变量 | ✅ 22 |
| **渲染原语** | Title、Separator、Break | 内容由渲染器决定，不是采集器 | ✅ 3 |
| **第一批**（纯读取） | Packages、Battery、PowerAdapter、Brightness、Dns、Tpm | 包数据库目录计数、`/sys/class/power_supply`、`/sys/class/backlight`、`/etc/resolv.conf`（全是 stub 时转 `/run/systemd/resolve/resolv.conf`）、`/sys/class/tpm` | ✅ 6 |
| **第二批** | Display、De、Wm | DRM sysfs + EDID 自己算刷新率；`XDG_CURRENT_DESKTOP` 整串→拆冒号→父进程链，版本查包数据库 | ✅ 3 |
| **第三批**（进行中） | WMTheme、Theme、Icons、Font、Cursor | GTK `settings.ini`、KDE `kdeglobals`/`kwinrc`、`/usr/share/icons/default/index.theme` | ⏳ |
| **第四批** | DateTime、Colors | TZif 自己解析（`localtime_r` 是 unsafe，禁止）；Colors 由渲染器算 | ⏳ |
| **第五批** | Gpu、LocalIp、Wifi、Users、PhysicalMemory、PhysicalDisk | Gpu：`/sys/class/drm` + `pci.ids`（没有就报原始 ID）；网络要开 `rustix` 的 `net` feature；Users 读 utmp；PhysicalMemory 读 DMI type 17 | ⏳ |
| **第六批** | OpenGL、Vulkan、OpenCL、Codec、Sound、Media、Player、Wallpaper、Camera、Gamepad、Mouse、Keyboard、Bluetooth | 多数要 ioctl / D-Bus / 设备树；**要开子进程的一律挂 `when-command-exists`**，且先问「这个值值不值得为它 fork」 | ⏳ |
| **第七批** | NetIO、DiskIO、CPUUsage、Top、Btrfs、Zpool | 前四个是**差值采样**（两次读取加间隔，`CPUUsage` 的 Linux 实现若只读一次 `/proc/stat` 就只是「开机至今平均」，必须写明口径）；后两个读 `/sys/fs/btrfs`、`/proc/spl` | ⏳ |
| **第八批** | PublicIp、Weather | 要 `net` feature（`ureq`） | ⏳ |
| **不属于模块** | Logo（查询内置 Logo，给 JSON 用）、Separator/Break（渲染原语，见 §6.1） | | |

### 5.5 fastfetch 2.68.1 的模块名（`fastfetch --list-modules` 实测）

本机装的 fastfetch 自己列出的 76 项，是这份清单的**权威来源**。已经做掉的 41 个不重复列，
剩下 36 个（按它给的顺序）：

`Bluetooth`、`BluetoothRadio`、`Bootmgr`、`Btrfs`、`CPUCache`、`CPUUsage`、`Camera`、`Codec`、
`Colors`、`Command`、`Custom`、`DateTime`、`DiskIO`、`Gamepad`、`Keyboard`、`Mouse`、`LM`、
`LocalIp`、`Media`、`Player`、`Monitor`、`NetIO`、`OpenCL`、`OpenGL`、`PhysicalDisk`、
`PhysicalMemory`、`PublicIp`、`Sound`、`TerminalTheme`、`Top`、`Users`、`Vulkan`、`Wallpaper`、
`Weather`、`Wifi`、`Zpool`

三条从这份清单里读出来的事实：

1. **fastfetch 的模块名都是单个词**：`TerminalFont`、`LocalIp`、`PhysicalDisk`、`CPUUsage`、
   `WMTheme`、`InitSystem`、`TerminalSize`、`PowerAdapter`、`NetIO`、`DiskIO`。
   我们的规范是 kebab-case（`terminal-font`、`init-system`…），两边在 4 个已做模块上不一致。
   决定：**kebab-case 保持为我们的规范**，到 CLI 阶段给每个模块加 fastfetch 拼写的
   **别名**（配置与 `--structure` 都认），`--list-modules` 仍印我们自己的名字。
2. `Monitor` 是 `Display` 的别名（「Same as Display module, but with a different default output
   format」）、`Logo` 是给 JSON 用的内置 Logo 查询——这三个都不是新采集器。
3. `Command`（跑自定义脚本）与 `Custom`（自定义字符串）是**用户内容**不是系统信息；
   `Command` 要开子进程，与「零子进程」原则冲突，单独评估。

### 5.6 fastfetch 的输出格式参考（本机实测，写新模块前先看这里）

`fastfetch -s sound:datetime:users:physicaldisk:physicalmemory:localip:bootmgr:terminaltheme:cpucache:lm --pipe`
在本机（AMD 笔记本 + Arch）的原样输出。**这些是权威格式，别自己发明键名与值的样子**：

```text
Sound: Ryzen HD Audio Controller Speaker (70%)
Date & Time: 2026-09-13 16:07:54
Users: gxyarch - login time 2026-09-12 10:56:19
Physical Disk (Kingston DataTraveler 3.0): 57.67 GiB [HDD, Removable]
Physical Disk (SAMSUNG MZVL21T0HCLR-00BH1): 953.87 GiB [SSD, Fixed]
Physical Disk (zram0): 15.33 GiB [Virtual, Fixed]
Local IP (enp5s0f4u1u3c2): 192.168.1.101/24
Boot Manager: ARCH - grubx64.efi
CPU Cache (L1): 8x32.00 KiB (D), 8x32.00 KiB (I)
CPU Cache (L2): 8x1.00 MiB (U)
CPU Cache (L3): 16.00 MiB (U)
Login Manager: login
```

读出来的几条约定：型号 / 网卡名 / 缓存层级都进**键**的括号；容量两位小数；`Physical Disk`
的值带 `[介质, 固定性]` 且 **`zram0` 也算一张盘**（`Virtual`），但 `loop`/`dm` 不显示；
`Local IP` 的值是 CIDR（带前缀长度）；`Users` 带登录时间；`Boot Manager` 是
`<EFI 启动项描述> - <加载器文件名>`。

**一处我们做不到的**：`Sound` 的值里那个设备名与音量百分比来自 PipeWire/PulseAudio 的协议
（守护进程里），零子进程拿不到；我们报服务本身 + 版本 + 声卡名（放 `variables`），
理由写在 `sound.rs` 顶部。

**一处待修的**：包数据库给的版本带 pacman 的 epoch（`PipeWire 1:1.6.8-1` 里的 `1:`）。
项目自己的版本是 `1.6.8`，epoch 只是打包排序用的。`pkgdb::version_of` 应该把 epoch 前缀去掉
（`-pkgrel` 保留，与本机已有的 `Niri 26.04-1` 一致）。

第二批参考（`fastfetch -s wifi:bluetooth:bluetoothradio:btrfs:zpool:netio:diskio:camera:gamepad:keyboard:mouse:media:player:wallpaper:physicalmemory:lm:datetime:terminaltheme:codec --pipe`）：

```text
Wi-Fi: down
Bluetooth 1: G3 Mouse (71%)
Bluetooth Radio (MyArch): Bluetooth 5.3 (Unknown)
BTRFS (myArch): 52.42 GiB / 920.87 GiB (6%, 12% allocated)
Network I/O (enp5s0f4u1u3c2): 326.97 KiB/s (IN) - 23.45 KiB/s (OUT)
Disk I/O (SAMSUNG MZVL21T0HCLR-00BH1): 0 B/s (R) - 0 B/s (W)
Camera 1: HP Wide Vision 5MP Camera: HP W - sRGB (2592x1944 px)
Keyboard 1: AT Translated Set 2 keyboard
Mouse 3: ELAN07D0:00 04F3:321A Touchpad
Media: 抖音-记录美好生活 [Playing]
Media Player: Douyin (Mozilla firefox)
Login Manager: login
Date & Time: 2026-09-13 16:10:51
Codec (Encoder): H.264, HEVC / H.265, AV1
Codec (Decoder): MJPEG, H.264, HEVC / H.265, VP9, AV1
```

几条读出来的：`Keyboard`/`Mouse`/`Camera`/`Bluetooth` 都按序号编号（`Keyboard 2:`），
名字来自设备自己报的字符串；`Disk I/O` 按**物理盘**（键里带型号）而不是按分区；
`Network I/O` 按网卡；`BTRFS (标签): 已用 / 总量 (百分比, 分配百分比)`。
本机**没有**输出的：`Zpool`（没装 ZFS）、`Gamepad`、`Wallpaper`、`TerminalTheme`、
`PhysicalMemory`（本机 DMI type 17 那条路没给东西，实现时要先查清是权限还是真没有）。
`Media`/`Media Player` 走的是 MPRIS（D-Bus），`Codec` 是问 GPU 的能力——这两个要单独评估。

**做得比 fastfetch 好的地方**（这是目标，不是口号）：

1. **核心三十多个模块零子进程**：fastfetch 为拿终端名、字体、主题会起不少进程。
   我们的原则是「能读文件就不 fork」——冷启动更快，也没有 shell 转义与超时的连带风险。
2. **配置校验严格**：未知字段、未知模块名、未知平台名一律报错并列出合法取值；
   fastfetch 的 JSONC 宽容得多，拼错一个键会静默失效。
3. **`--gen-config` 打到 stdout**，不写用户的 `~/.config`。
4. **JSON 带 schema 版本**，键名是契约；失败模块单独成 `failures`，不混进 `entries`。
5. **「为什么这个模块没出来」说得清**：条件不满足时 `--verbose` 会说明是被哪个条件挡下的。
6. **显示宽度对齐**按 Unicode 显示宽度算（CJK、组合字符不会歪）。
7. **终端认得更准**：环境变量指纹先于父进程链。本机实测 fastfetch 报 `node-MainThread`，
   我们报 `kitty`——嵌套环境里父进程链会被包装脚本带偏。
8. **刷新率自己从 EDID 算**（像素时钟 ÷ 行总数 × 场总数），而且只在算出的分辨率与
   `modes` 首选模式一致时才报；fastfetch 走 libdrm，且不设这道保险。
9. **桌面/窗口管理器的版本查包数据库**：`gnome-shell`、`kwin`、`niri` 都是包，
   pacman 与 dpkg 两条路都通；fastfetch 这条路只在 Debian 系走得通。
10. **进程/线程数取内核自己的账**：读一次 `/proc/loadavg` 的总任务数，
    与逐个打开 434 个 `/proc/<pid>/stat` 累加**完全相等**（2008 = 2008），耗时 6.9 ms → 0.3 ms。

**明确不追平的**：只有 Sixel/Kitty/Chafa 这类图像协议（要动终端图形栈，收益与风险
不成比例）。`--watch` 动态刷新与天气/公网 IP 都在清单里，不算不追平。

---

## 六、渲染设计

### 6.1 文本渲染

- Logo 在左，信息在右，垂直居中（信息行比画面高时，画面上下各留一半空位）
- 键右对齐、值左对齐；键与值之间固定 `": "`
- 终端宽度自适应；宽度不足时**隐藏 Logo**——不换小图、也不截断值（截断会丢信息）
- **宽度从哪来**：`rustix::termios::tcgetwinsize(stdout)` → `$COLUMNS` → **不知道**。
  问不出来时**不隐藏 Logo**：输出是管道时「多少列」本就没有确定答案，
  宁可多画一张，也不要因为猜了个 80 就把用户的 Logo 悄悄吃掉。
- 颜色：**键 = 加粗青、值 = 不上色**（即终端默认前景色）、Logo = 发行版配色。
  值刻意不写死白色：白字在浅色背景的终端上等于看不见，而「键有色、值没色」
  已经足够区分两者。Logo 的颜色来自发行版表而不是 `Theme`。
- 样式由 `anstyle` 描述、`anstream` 降级：**渲染器永远带颜色**，写出去的那一刻才按
  「是不是终端」决定去留，并遵守 `NO_COLOR` / `CLICOLOR_FORCE`。所以
  `vitals | cat` 是干净的，用户不需要为此加参数；`--no-color` 只是再确认一次。
- **对齐必须用显示宽度**，不能用字节长度也不能用 `chars().count()`。实测示例：
  `"a\u{0301}b"` 的 `len()` = 4、`chars().count()` = 3、`unicode_width` 宽度 = **2**。
  三个数各不相同，只有第三个是对的。
- `unicode-width` 用 **0.2.2**：`str::width() -> usize`，但 `char::width() -> Option<usize>`
  （0.1 返回 `usize`），照旧教程写会编译不过。
- 它不认转义码，这没关系：版式永远在**还没上色**的文本上算。

### 6.2 Logo

- 内置常见发行版 ASCII Logo，`include_str!` **编译期嵌入**，不读磁盘
- `logo = "auto"` 时匹配链：`/etc/os-release` 的 `ID` → `ID_LIKE` → 通用 Linux Logo
  （**必须带 `ID_LIKE` 回退**，否则 cachyos、endeavouros 这类衍生版全掉到通用 Logo）
- 找不到时用通用 Linux Logo
- **来源**：fastfetch（MIT；Copyright (c) 2021-2023 Linus Dierheimer，2022-2026 Carter Li）
  的 `src/logo/ascii/`。取来时去掉它自己的 `$1`/`$2` 颜色占位符，改成
  **一个发行版一种颜色**——`AnsiColor` 只有八基础色加八亮色，所以配色是「最接近标识主色」
  而不是精确复刻，也省得实现一套分段着色。
- 已内置 11 张 + 通用 Tux：arch、manjaro、debian、ubuntu、fedora、centos、alpine、
  gentoo、void、nixos、opensuse，加 linux。衍生版靠 `ID_LIKE` 接住，
  两条都接不住的才落到通用 Logo——十来张图覆盖绝大多数机器。
- 画面存成**一整块文本**（`Logo { id, art }`）而不是切好的行数组：`include_str!` 拿到的
  本就是一整块，硬要在编译期切成数组就得把每张图写成 Rust 字符串字面量，反斜杠逐个转义。
- v0.2 可考虑 `_small` 变体：80 列终端上「大图放不下」目前是隐藏 Logo，换小图会更像
  fastfetch。v0.1 不做，取舍见 §6.1。

### 6.3 JSON 渲染

- 输出结构化信息列表，含类型、键、值
- 带输出 schema 版本号
- 自动关闭颜色与 Logo；`vitals --json | jq` 必须可用
- 形状固定成这样，键名不随版本漂移：

  ```json
  {
    "schema_version": 1,
    "entries": [{ "type": "os", "key": "OS", "value": "Arch Linux" }],
    "failures": [{ "type": "disk", "error": "statvfs 失败" }]
  }
  ```

- `type` 用模块自己的名字（`ModuleType::name()`），不是 `Display` 的散文
- 只输出**采到的**条目；失败的模块进 `failures`，不混进 `entries`
  ——脚本不该把一条错误当成一条信息去读
- 没有采集结果时是空数组，不是 `null`（`jq '.entries[]'` 不该炸）
- 带缩进、末尾有换行：人也会扫一眼（`vitals --json | head`），`jq` 两种都吃
- `variables`（模板变量）暂不进 JSON。那是 v0.3 模板特性的形状，
  等到那时再定；现在冻结它只会锁死一个还没设计过的格式
- 形状不在 `Info` 上 derive `Serialize`，而是渲染器自己定义一个 `Entry`：
  JSON 的键名与字段取舍是**渲染器的事**，核心接口不该为了输出的方便而依赖 serde

---

## 七、开发阶段

> **以本节为唯一真源**，§8 的里程碑由阶段边界推导。

### 阶段 0：项目初始化
- **目标**：能运行的空壳
- **任务**：`cargo new`；lib + bin 双 target；`rust-toolchain.toml` + `rust-version`；许可证与
  `Cargo.toml` 元数据（含 `[[bin]] name = "vitals"`）；CI 骨架（fmt / clippy / nextest 三件套）
- **验收**：`cargo run -- --version` 输出 `vitals 0.1.0`；`cargo +stable build` 通过；
  `cargo fmt --check` 与 `cargo clippy -- -D warnings` 全绿

### 阶段 1：核心抽象
- **目标**：定接口，不写实现
- **任务**：确定采集器接口、信息结构、模块调度流程、渲染器接口；只定义，编译通过
- **验收**：用一个假采集器跑通「采集 → 渲染」

### 阶段 2：配置系统
- **目标**：TOML 能加载、能合并、能生成默认
- **任务**：定义配置结构；实现加载顺序与 XDG 查找；实现生成默认配置；未知字段报错；版本字段
- **验收**：`cargo test` 覆盖合法/非法配置；最小 `cargo run -- --gen-config` 可输出
  （不依赖阶段 3 的完整 CLI）

### 阶段 3：CLI
- **目标**：参数完整、行为可预期
- **任务**：按 §4 实现全部参数；落实优先级
- **验收**：所有参数生效，`--help` 可读

### 阶段 4：基础模块
- **目标**：v0.1 十个模块可用
- **任务**：逐模块实现，独立文件、独立测试；失败不 panic；外部命令带超时
- **验收**：干净 Linux 上输出合理，无 panic

### 阶段 5：渲染（文本）
- **目标**：好看
- **任务**：文本渲染、显示宽度对齐、Logo、颜色（anstyle + anstream）
- **验收**：`vitals` 好看；管道输出无转义码

### 阶段 6：JSON 输出
- **任务**：JSON 渲染器 + 输出 schema 版本
- **验收**：`vitals --json | jq` 可用

### 阶段 7：声明式条件
- **任务**：`platforms` / `when-command-exists`（仅 PATH 查找）/ `when-file-exists`；调度前统一评估
- **验收**：伪条件可精确跳过指定模块，且不产生任何子进程

### 阶段 8：扩展模块
- **任务**：按 §5.2 优先级逐个实现，全部走声明式条件；`net` feature 隔离公网 IP
- **验收**：无 GPU 机器不显示 GPU；台式机不显示电池

### 阶段 9：健壮性
- **目标**：任何异常不崩溃
- **任务**：统一错误类型；单模块失败继续；`--verbose` 显示原因；所有外部命令超时；
  所有文件读取缺失返回空
- **验收**：故意破坏配置、删除文件、断命令，程序不 panic

### 阶段 10：性能
- **目标**：冷启动低于 20ms（不含 Logo 渲染）
- **任务**：`thread::scope` 并行采集；能读 `/proc` 就不开子进程；外部命令按需调用；
  用 `hyperfine --warmup 3` 做基准并记录
- **验收**：`vitals --json` 与 fastfetch 对比不落后

### 阶段 11：测试与 CI
- **任务**：单元测试用 fixture 测解析；集成测试测 CLI；`insta` 快照测渲染；
  配置测试覆盖合法与非法
- **CI 内容**：`cargo fmt --check`、`cargo clippy -D warnings`、`cargo nextest run`、
  `cargo deny check`（license + advisory + 重复依赖）；v1.0 冻结接口时再加 `cargo semver-checks`
- **验收**：CI 全绿

### 阶段 12：发布
- **任务**：README、LICENSE、CHANGELOG、配置文档、模块文档；`cargo-dist` 初始化并发布到 crates.io；
  GitHub Release 附二进制与安装器；加 `[package.metadata.binstall]` 支持 `cargo binstall`；
  后期考虑 AUR、Homebrew
- **验收**：他人 `cargo install vitals-rs`（或 `cargo binstall vitals-rs`）后可直接使用命令 `vitals`

---

## 八、里程碑（由阶段边界推导，替换原表）

| 版本 | 内容 | 对应阶段 | 可发布 |
|---|---|---|---|
| v0.1 | 十个基础模块 + TOML 配置 + CLI + 文本渲染 + JSON 输出 | 0–6 | 是 |
| v0.2 | 声明式条件 + 扩展模块 | 7–8 | 是 |
| v0.3 | 健壮性（统一错误、无 panic） | 9 | 是 |
| v0.4 | 并行采集 + 性能达标 | 10 | 是 |
| v0.5 | 测试 + CI + 打包发布 | 11–12 | 是 |
| v0.6 | fastfetch schema 适配层（可选） | — | 否 |
| v1.0 | 接口冻结 + 跨平台预留 | — | 是 |

---

## 九、开发顺序

1. 建项目（lib + bin 双 target、工具链锁定、CI 骨架）
2. **定义四个核心抽象，只定义不实现** ← 关键，接口定错返工大
3. 实现 OS、Kernel、Host 三个模块，跑通文本渲染
4. 加 TOML 配置加载与生成默认
5. 加 CLI 参数
6. 补齐十个基础模块
7. 加 Logo、对齐、颜色
8. 加 JSON 输出
9. 加声明式条件
10. 加扩展模块
11. 健壮性与性能
12. 测试与 CI
13. 发布 v0.1

---

## 十、明确排除的事项

- 不做动态配置脚本（Rhai、Lua 等）
- 不用 YAML 作主配置
- 不引入异步运行时
- 不在模块内直接打印，只返回数据
- 不因单模块失败中断整体
- 不用字节长度做对齐（也不用 `chars().count()`，用显示宽度）
- 不读磁盘加载 Logo
- 不写 unsafe（自有代码；系统调用一律经 `rustix`）
- 不引 `sysinfo`（除非将来确需进程列表）、不引 `rayon` / `inventory` / `linkme`
- 不硬编码 `~/.config`（走 XDG）
- 不手搓 release workflow（用 `cargo-dist`）
- 不在「判断命令是否存在」时真的执行命令
- 不用 `mod.rs`（一律自名文件 + 同名目录，由 `clippy::mod_module_files` 在 CI 里强制）

---

## 附：已经拍板的争议点

1. **v0.1 包含 JSON 输出**（原计划放在 v0.2 的阶段 6）。理由很具体：阶段 3 已经把
   `--json` 接进了 CLI，不实现它，那个参数就是个**会撒谎的开关**——用户加上它，
   得到的却是文本。宁可把阶段 6 提前，也不要留一个按了没反应的按钮。

已经拍板的（留在这里，免得又被翻出来）：

- **CPU 占用率**：不做。理由见 §5.3。
- **Rust 模块**：保留，只读文件、不开子进程。阶段 4 已落地，理由见 §5.1。
- **`mod.rs`**：不用。阶段 1 起就由 `clippy::mod_module_files` 在 CI 里强制。
- **Logo 放不下时隐藏**，不换小图、不截断值。理由见 §6.1。
