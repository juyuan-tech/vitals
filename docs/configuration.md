# 配置参考

[English](configuration.en.md) | **中文**

本文说明 `vitals` 的 TOML 配置文件：放在哪、怎么写、每个键是什么意思。

文中所有结论都来自三个地方：仓库源码（仓库根）、内置默认配置
`src/config/default.toml`、以及对 `target/release/vitals` 的真实运行。每个键都给出
`文件:行号`；每个行为后面都注明验证方式。版本：`vitals 0.1.1`（`vitals --version`）。

配置文件的解析只有一条路径：`src/config.rs` 的 `load`，最终反序列化成
`src/config/schema.rs` 里的 `ConfigFile`。除此以外没有别的配置来源。

---

仓库里还带四份示例配置，可以直接拿来跑（`presets/`）：`minimal`、`desktop`、`headless`、`all`——
`tests/docs.rs` 会保证它们真的跑得通。

## 1. 配置文件在哪

### 1.1 指定路径：`--config`

```sh
vitals --config /path/to/config.toml
```

`--config <路径>` 由 `src/cli.rs:32-34` 定义，`src/main.rs:47-53` 把它交给
`config::load`（`src/config.rs:61-75`）。**显式指定的文件必须能读到**：文件不存在、
无权限或不是 UTF-8 都会报错并以退出码 1 结束（`src/config.rs:46-52`、`100-107`
→ `src/main.rs:49-52`）。

实测：

```console
$ vitals --config /tmp/vitals-docs-tests/nope.toml --logo none
vitals: 读取配置文件 /tmp/vitals-docs-tests/nope.toml 失败：No such file or directory (os error 2)
[exit=1]
```

### 1.2 不指定：`$XDG_CONFIG_HOME/vitals/config.toml`

不写 `--config` 时按下面的顺序算路径（`src/config/path.rs:25-45`）：

1. 环境变量 `XDG_CONFIG_HOME` 存在**且是绝对路径** → `$XDG_CONFIG_HOME/vitals/config.toml`
   （`src/config/path.rs:40-42`；相对路径按 XDG 规范视为无效，不是报错，而是继续往下走）。
2. 否则环境变量 `HOME` 存在 → `$HOME/.config/vitals/config.toml`
   （`src/config/path.rs:44`）。
3. 两者都没有 → 没有默认路径，`config_path()` 返回 `None`
   （`src/config/path.rs:22-23`、`src/config.rs:66-68`）。

相对路径 `vitals/config.toml` 这个常量在 `src/config/path.rs:18`。

找到的路径**不存在时不报错**，安静地用内置默认（`src/config.rs:70-74`）——第一次
运行的人不该看到报错。这是显式路径与默认路径唯一的行为差异（`src/config.rs:54-60`）。

实测（每行一条真实运行）：

| 环境 | 配置文件内容 | 结果 |
| --- | --- | --- |
| `XDG_CONFIG_HOME=/tmp/…/xdghome` | `type = "kernel"` | 只显示 `Kernel: Linux 7.2.4-arch1-2` |
| `XDG_CONFIG_HOME` 未设、`HOME=/tmp/…/homefallback` | `type = "uptime"` | 只显示 `Uptime: …` |
| `XDG_CONFIG_HOME=relative/dir`、`HOME=/tmp/…/homefallback` | 同上 | 相对 XDG 被忽略，仍读 `$HOME/.config/vitals/config.toml` |
| `XDG_CONFIG_HOME=/tmp/…/xdgempty`（目录里没有 `vitals/config.toml`） | — | 内置默认视图，退出码 0 |
| `XDG_CONFIG_HOME`、`HOME` 都不设 | — | 内置默认视图，退出码 0 |

### 1.3 平台范围

`src/config/path.rs:11-12` 写明 Windows / macOS 的分支留到 v1.0，当前只有一个
Unix 实现。因此本文只描述上面这套 Unix 路径规则，不描述 Windows / macOS 的行为。

**注意**：这里只认 `XDG_CONFIG_HOME`，不读 `XDG_CONFIG_DIRS` 一类的其它变量；
`src/config.rs:9` 也说明当前阶段只有「内置默认 + 用户文件」两层，没有系统级配置合并。

---

## 2. 整体语义：整体替换，不是逐项合并

**`modules` 一旦写了，就整体替换内置默认列表，不是往默认列表里追加。**
想少显示几个模块，得把要留的列全。这是 `--gen-config` 输出里的原话
（`src/config/default.toml:3-4`），源码与它一致：`ConfigFile.modules` 是 `Option`，
只有 `Some` 时才覆盖（`src/config/schema.rs:31-36`、`48-50`）；注释里给了理由——
列表的逐项合并没有站得住的语义（顺序怎么排、重复算谁的，`src/config/schema.rs:33-35`）。

`ConfigFile` 的字段全部是 `Option`，是为了区分「没写」与「写了等于默认的值」
（`src/config/schema.rs:5-6`）。所以：

| 你写的 | 结果 |
| --- | --- |
| 完全不写 `modules`（写了别的键、或空文件、或没有配置文件） | 用内置默认视图（24 个模块，见 §3） |
| 写 `[[modules]] ...` | 只显示你列的那些，内置默认列表不再生效 |
| 写 `modules = []` | 0 个模块，没有任何输出，退出码 0 |

`config_version` 与模块列表相互独立：文件里写了就用文件里的值，没写按当前版本算
（`src/config/schema.rs:42-46`）。

实测（只写一半模块）：

```console
$ printf '[[modules]]\ntype = "memory"\n' > t1-replace.toml
$ vitals --config t1-replace.toml --logo none --no-color
Memory: 22.78 GiB / 30.65 GiB (74%)
[exit=0]
```

只写了 `memory`，`OS` / `Kernel` / `Disk` 等全部消失——不是合并。另外三种情形也各
跑过一次：只写 `config_version = 1`、空文件、以及完全不传 `--config`
（`XDG_CONFIG_HOME` 与 `HOME` 都不设），三者列出的模块相同，都是内置默认视图，
退出码 0；`modules = []` 则输出为空、退出码 0。

（具体输出行数**不作为结论**：默认视图里有些模块依赖环境，例如把 `HOME` 也清掉后
实测会少掉 `Packages` 的 appimage 项与 `Terminal Font` 行。这里只保证「模块列表
回到内置默认」。）

### 内置默认视图是什么

内置默认列表是 `ModuleType::DEFAULT`，24 项，顺序即显示顺序
（`src/config/schema.rs:429-455`），与 `src/config/default.toml:41-111` 逐项一致：

```text
title, separator, os, host, kernel, uptime, packages, shell, display, de, wm,
cursor, terminal, terminal-font, cpu, gpu, memory, swap, disk, local-ip,
battery, locale, break, colors
```

`src/config.rs:34-36` 里 `default_toml()` 就是 `include_str!("config/default.toml")`，
`--gen-config` 原样打印它（`src/main.rs:35-38`）。实测 `vitals --gen-config` 的输出与
`/tmp/vitals-facts/gen-config.toml` 逐字节相同（`diff` 无差异）。

---

## 3. 逐键参考

配置文件里**只有 6 个键**：顶层 2 个，`[[modules]]` 每一项里 4 个。这个集合有源码
硬证据：两个结构都带 `deny_unknown_fields`（`src/config/schema.rs:25`、`:90`），
写错键名时 serde 会把合法集合列出来。实测报错原文：

```text
unknown field `moduless`, expected `config_version` or `modules`
unknown field `foo`, expected one of `type`, `platforms`, `when-command-exists`, `when-file-exists`
```

### 3.1 顶层

| 键 | 类型 | 默认值 | 作用 | 位置 |
| --- | --- | --- | --- | --- |
| `config_version` | 整数（u32） | `1`（省略时取 `CURRENT_CONFIG_VERSION`） | 声明这份文件针对哪个配置版本。**高于**程序支持的版本时拒绝启动 | 字段 `src/config/schema.rs:28-29`；默认值来源 `src/config/schema.rs:19-21`、`src/config.rs:26`；校验 `src/config.rs:84-89` |
| `modules` | `[[modules]]` 数组表 | 省略 = 内置默认 24 项 | 要显示的模块，**顺序即显示顺序**；写了就整体替换内置默认 | 字段 `src/config/schema.rs:36`；覆盖逻辑 `src/config/schema.rs:48-50`；默认列表 `src/config/schema.rs:429-455` |

`config_version` 的边界行为（都实测过）：

- 省略 → 按 `1` 处理，正常。
- 等于 `1`（`CURRENT_CONFIG_VERSION`）→ 正常。
- 小于 `1`，例如 `0` → **不报错**，正常渲染。校验只有「高于」这一条
  （`src/config.rs:84`）。
- 大于 `1`，例如 `2` → 退出码 1，原文：`vitals: 配置版本 2 高于本程序支持的 1，请升级 vitals`。
- 类型不是整数，例如 `config_version = "1"` → TOML 解析失败，退出码 1
  （`invalid type: string "1", expected u32`）。

`modules` 的边界行为：顺序 = 你写的顺序；**允许重复**，重复项会渲染两次
（实测连续两个 `type = "kernel"` 输出两行 `Kernel: …`）。没有去重逻辑。

### 3.2 `[[modules]]` 每一项

| 键 | 类型 | 默认值 | 作用 | 位置 |
| --- | --- | --- | --- | --- |
| `type` | 字符串（枚举，kebab-case） | **必填，无默认** | 模块名。取值见 §3.3 | `src/config/schema.rs:93-94`；枚举定义 `src/config/schema.rs:210-345` |
| `platforms` | 字符串数组 | `[]`（空 = 不限平台） | 只在这些平台上采集，否则跳过该模块 | `src/config/schema.rs:97-98`；取值枚举 `src/config/schema.rs:129-150` |
| `when-command-exists` | 字符串 | 未设 = 不检查 | 命令不在 `PATH` 里就跳过该模块；**只查 PATH，不执行命令** | `src/config/schema.rs:100-102`；判定 `src/conditions.rs:113-145` |
| `when-file-exists` | 字符串（路径） | 未设 = 不检查 | 路径不存在就跳过该模块；开头的 `~/` 展开成 `$HOME` | `src/config/schema.rs:104-106`；判定与展开 `src/conditions.rs:95-99`、`172-187` |

`type` 缺失时直接解析失败（实测）：`missing field \`type\``，退出码 1。

**`type` 的拼写规则**：配置文件里的 `type` 走 serde 枚举匹配，必须是我们自己的
kebab-case 名字，**大小写与 `-`/`_` 都不做归一化**（`src/config/schema.rs:541-547`）。
实测 `"LocalIp"`、`"local_ip"`、`"WMTheme"` 都是退出码 1 的解析错误，只有
`"local-ip"` 正常；同样这几个写法在命令行 `--module` 里却都能用
（`src/cli.rs:118-127`、`src/config/schema.rs:530-556`）——两条路故意不同。

两个名字不遵守 kebab-case 的机械转换，由显式 `serde(rename)` 钉死
（`src/config/schema.rs:281-287`、`:317-322`），写成 `wmtheme`、`datetime`
（而不是 `wm-theme`、`date-time`）。

### 3.3 `type` 的合法取值

共 **61** 个（`src/config/schema.rs:349-411` 的 `ModuleType::ALL`，`name()` 在
`:463-527`）。实测 `vitals --list-modules` 输出 61 行，且就是这一串、这个顺序：

```text
os, host, kernel, bios, board, chassis, uptime, loadavg, processes, cpu, memory,
swap, disk, user, shell, terminal, terminal-size, locale, editor, version,
init-system, title, separator, break, rust, battery, power-adapter, brightness,
dns, tpm, packages, display, de, wm, wmtheme, theme, icons, font, cursor, gpu,
terminal-font, local-ip, users, physical-disk, bootmgr, sound, cpu-cache, lm,
btrfs, datetime, wifi, camera, keyboard, mouse, gamepad, net-io, disk-io,
cpu-usage, top, colors, monitor
```

写错一个字母时 serde 会把全部合法取值列在错误里，例如
`unknown variant \`cpuu\`, expected one of \`os\`, \`host\`, …`（实测，退出码 1）。

`type` 里没有「颜色」「布局」之类可配的选项；`title` / `separator` / `break` /
`colors` 是渲染原语，没有额外字段（`src/config/schema.rs:255-260`、`:333-334`）。

### 3.4 `platforms` 的合法取值

9 个（`src/config/schema.rs:129-150`，名字在 `:168-180`）：

```text
linux, macos, windows, freebsd, openbsd, netbsd, android, solaris, illumos
```

写非法值时解析失败，退出码 1（实测 `platforms = ["linx"]`：
``unknown variant `linx`, expected one of `linux`, `macos`, …``）。
`platforms = []` 表示不限平台，模块照常显示（`src/conditions.rs:109-111`，实测通过）。

---

## 4. 条件

条件挂在 `[[modules]]` 每一项上，用上面三个字段里的任意组合。**不满足就跳过该模块，
而且不报错**——「这台机器没有电池」不是错误，只是没什么可显示
（`src/conditions.rs:3-4`）。跳过时退出码仍是 0，输出里那一行直接消失。

三个条件的关系是**与**：任一不满足就跳过（`src/conditions.rs:82-83`）。评估顺序是
`platforms` → `when-command-exists` → `when-file-exists`，**先失败的那个**决定
跳过原因（`src/conditions.rs:84-102`）。

条件在采集之前统一评估，调度器只看到最终名单（`src/conditions.rs:10`、`64-78`；
`src/main.rs:87`）。

### 4.1 三个条件各自怎么判

**`platforms`** — 空数组表示不限；否则当前平台必须在列表里
（`src/conditions.rs:109-111`）。当前平台取自 `std::env::consts::OS`，是编译期常量
（`src/config/schema.rs:191-199`）。若编译目标不在那 9 个名字里，任何非空的
`platforms` 都不满足——这是有意的（`src/config/schema.rs:194-195`、
`src/conditions.rs:104-108`）。

**`when-command-exists`** — 只查 `PATH`，**绝不执行命令**（`src/conditions.rs:5-8`、
`113-119`）。细则：

- 空字符串永远匹配不上（`src/conditions.rs:129-131`）。实测 `when-command-exists = ""`
  会跳过，`--verbose` 显示 ``命令 `` 不在 PATH 里``。
- 名字里含 `/` 就不做 PATH 查找，直接当路径看（`src/conditions.rs:133-136`）。
  实测 `/bin/sh` 通过，`/bin/vitals-不存在` 跳过。
- 必须是一个**有执行位的普通文件**：有同名文件但没有执行位，仍旧算「不存在」
  （`src/conditions.rs:147-164`）。实测把无执行位的文件放进 `PATH`，结果仍是跳过。
- `PATH` 未设置时什么都不匹配（`src/conditions.rs:138-140`）。

「不执行命令」这条做过直接验证：把一个会写标记文件的脚本放进 `PATH`，令
`when-command-exists = "vitals-marker-cmd"`，模块正常显示，而脚本应写的
`/tmp/vitals-docs-tests/EXECUTED-MARKER` **没有出现**。

**`when-file-exists`** — 就是对展开后的路径调 `exists()`（`src/conditions.rs:95-99`）。

- 目录也算存在（`src/conditions.rs:260-268` 的测试语义；实测 `/proc` 通过）。
- 开头的 `~/` 展开成 `$HOME`（`src/conditions.rs:172-187`）。只认开头的 `~/`：
  单独的 `~`、或 `~` 出现在中间都不展开（`src/conditions.rs:332-338`）。
  `HOME` 也没有时原样返回，不猜家目录（`src/conditions.rs:182-186`）。
  实测 `~/.config` 通过、`~/.config/definitely-not-here-vitals` 跳过。
- 注意：`--verbose` 打印的是**配置里原样写的路径**，不展开。实测输出为
  `跳过 memory：路径 ~/.config/definitely-not-here-vitals 不存在`。

### 4.2 跳过时看到什么

默认不出声（`src/main.rs:89-90`）。加 `--verbose` 会往 **stderr** 打一行
（`src/main.rs:91-99`），文案来自 `SkipReason::describe()`（`src/conditions.rs:47-60`）：

| 原因 | 文案 | 实测输出 |
| --- | --- | --- |
| 平台不符 | `当前平台是 <名字>` | `vitals: 跳过 memory：当前平台是 linux` |
| 命令不在 PATH | `` 命令 `<名字>` 不在 PATH 里 `` | ``vitals: 跳过 memory：命令 `vitals-这个命令不存在` 不在 PATH 里`` |
| 路径不存在 | `路径 <路径> 不存在` | `vitals: 跳过 memory：路径 /nonexistent/vitals 不存在` |

想区分「跳过」（条件挡住、根本没采）与「空」（采了但没数据）用 `--explain`
（`src/main.rs:106-112`、`194-228`）。

### 4.3 与 `--module` 的关系

`--module` 是**选择**，不是过滤：点名的模块一定出现，而且会**丢掉配置里的条件**——
显式点名是更强的意愿（`src/cli.rs:130-136`、`159-171`）。实测：配置里给 `memory` 挂了
`platforms = ["windows"]` 和 `when-file-exists = "/nope"`，不带 `--module` 时被跳过；
带 `--module memory` 时正常显示。

---

## 5. 完整示例

下面这份配置真的跑过，输出见其后（`config_version`、`type`、`platforms`、
`when-command-exists`、`when-file-exists`、`modules` 六个键都用到了）：

```toml
config_version = 1

# 标题行 + 一条分隔线
[[modules]]
type = "title"

[[modules]]
type = "separator"

[[modules]]
type = "os"

# 只在 Linux 上采集
[[modules]]
type = "kernel"
platforms = ["linux"]

[[modules]]
type = "uptime"

# sh 不在 PATH 里就跳过（只查 PATH，不执行它）
[[modules]]
type = "memory"
when-command-exists = "sh"

# 路径不存在就跳过；开头的 ~/ 会展开成 $HOME
[[modules]]
type = "disk"
when-file-exists = "~/"

[[modules]]
type = "swap"

# 收尾：空行 + 色块
[[modules]]
type = "break"

[[modules]]
type = "colors"
```

运行命令与结果（`--logo none` 关掉 Logo，`--no-color` 关掉颜色以便粘贴）：

```console
$ vitals --config example-full.toml --logo none --no-color
gxyarch@MyArch
──────────────
OS: Arch Linux x86_64
Kernel: Linux 7.2.4-arch1-2
Uptime: 1 day, 7 hours, 15 mins
Memory: 22.92 GiB / 30.65 GiB (75%)
Disk: 53.37 GiB / 920.87 GiB (6%)
Swap: 7.88 GiB / 47.33 GiB (17%)
[exit=0]
```

（`break` 与 `colors` 会输出空行与色块，它们在纯文本粘贴里不可见。）

同一份配置加 `--verbose`，stderr 上确认 10 个模块全部通过条件、没有任何跳过：

```console
$ vitals --config example-full.toml --logo none --no-color --verbose
vitals: 模块 10 个：title, separator, os, kernel, uptime, memory, disk, swap, break, colors
vitals: logo=none 颜色=关 json=关 verbose=开
```

---

## 6. 验证记录

每个键至少一次真实运行（仓库里的二进制 `target/release/vitals`）：

| 键 | 验证的写法 | 观察到的结果 |
| --- | --- | --- |
| `config_version` | `1` / `0` / 省略 | 正常渲染，退出码 0 |
| `config_version` | `2` | `配置版本 2 高于本程序支持的 1，请升级 vitals`，退出码 1 |
| `config_version` | `"1"` | TOML 解析失败，退出码 1 |
| `modules` | 只写 `[[modules]] type = "memory"` | 只输出 Memory 一行 |
| `modules` | 不写 / 空文件 / 无配置文件 | 内置默认视图，退出码 0 |
| `modules` | `modules = []` | 无输出，退出码 0 |
| `modules` | 连续两个 `type = "kernel"` | 输出两行 Kernel（允许重复） |
| `type` | `"local-ip"` | 正常显示 |
| `type` | `"LocalIp"` / `"local_ip"` / `"WMTheme"` | 解析失败，退出码 1 |
| `type` | 缺失 | `missing field \`type\``，退出码 1 |
| `type` | `"cpuu"` | `unknown variant \`cpuu\``，退出码 1 |
| `platforms` | `["linux"]` / `[]` | 模块显示 |
| `platforms` | `["windows"]` | 跳过，退出码 0，`--verbose` 报平台 |
| `platforms` | `["linx"]` | 解析失败，退出码 1 |
| `when-command-exists` | `"sh"` / `"/bin/sh"` | 模块显示 |
| `when-command-exists` | 不存在的命令 / `""` / 无执行位的文件 | 跳过，退出码 0 |
| `when-command-exists` | 会写标记文件的脚本 | 模块显示，标记文件未生成（未执行） |
| `when-file-exists` | `"/proc"` / `"~/.config"` | 模块显示 |
| `when-file-exists` | `"/nonexistent/vitals"` / 不存在的 `~/` 路径 | 跳过，退出码 0 |
| 三个条件并用 | 全满足 / 第三个失败 / 第一个失败 | 显示 / 报路径 / 报平台 |

## 7. 未覆盖

- Windows / macOS 的路径规则：源码里尚未实现（`src/config/path.rs:11-12`），
  因此本文不描述，也不推测。
- 系统级配置目录（`XDG_CONFIG_DIRS` 等）：当前加载链只有两层
  （`src/config.rs:9` 注明「阶段 2 只做到用户文件覆盖默认值」），没有可写的键。
- 配置文件的其它格式（JSONC 等）：当前只支持 TOML（`src/config.rs:11-13`）。
