# 命令行参考

本文描述 `vitals`（版本 0.1.0）的命令行行为。命令行只有选项，没有位置参数；多给一个位置参数会以退出码 2 结束。

文中每个命令都在本机真实执行过，输出片段为原样截取（过长处截断，未改写数值）。

```
Usage: vitals [OPTIONS]
```

`-h` 打印摘要（首行是英文标语 `Your system's vital signs, at a glance.`），`--help` 打印完整说明（首行是中文 `你的系统生命体征，一眼看全。`）。两者列出的选项集合相同。

## 选项一览

| 选项 | 作用 |
| --- | --- |
| `--config <路径>` | 指定配置文件；不写就找 `$XDG_CONFIG_HOME/vitals/config.toml` |
| `--json` | 以 JSON 输出（自动关掉颜色与 Logo） |
| `--logo <auto\|none\|名称>` | Logo：`auto` 按发行版自动选、`none` 不显示、或直接给名称（默认 `auto`） |
| `--module <列表>` | 只显示这些模块，逗号分隔。顺序就是你写的顺序；配置里没有的也能点 |
| `--no-color` | 关闭颜色（等同设置 `NO_COLOR`） |
| `--list-modules` | 列出全部可用模块 |
| `--gen-config` | 把内置默认配置打印到 stdout |
| `--verbose` | 把诊断信息（含最终生效的设置）写到 stderr |
| `--explain` | 逐项说明每个模块为什么出现、或为什么没有出现 |
| `--sources` | 每个模块**实际读了哪些文件**（运行时记录，不是手写的来源表） |
| `-h, --help` | 打印帮助 |
| `-V, --version` | 打印版本 |

---

## `--config <路径>`

指定配置文件。不写时按 `$XDG_CONFIG_HOME/vitals/config.toml` 查找。

显式指定路径时，文件读不到是**错误**：写一行 `vitals: ...` 到 stderr，退出码 1，不渲染任何内容。

```
$ vitals --config /tmp/vxdg/vitals/config.toml --logo none
OS: Arch Linux x86_64
Memory: 22.81 GiB / 30.65 GiB (74%)
```

配置文件不存在：

```
$ vitals --config /tmp/definitely-missing-vitals.toml ; echo $?
vitals: 读取配置文件 /tmp/definitely-missing-vitals.toml 失败：No such file or directory (os error 2)
1
```

配置文件存在但解析失败（TOML 语法错、未知模块类型、版本高于本程序支持的 `1`）同样是退出码 1：

```
$ vitals --config /tmp/vbad.toml ; echo $?
vitals: /tmp/vbad.toml 解析失败
TOML parse error at line 4, column 8
  |
4 | type = "not-a-module"
  |        ^^^^^^^^^^^^^^
unknown variant `not-a-module`, expected one of `os`, `host`, ...（截断）
1
```

```
$ vitals --config /tmp/vver.toml ; echo $?
vitals: 配置版本 99 高于本程序支持的 1，请升级 vitals
1
```

空的配置文件（只有零字节）不算错误，等价于内置默认：

```
$ vitals --config /tmp/vempty.toml --logo none | head -4
gxyarch@MyArch
──────────────
OS: Arch Linux x86_64
Host: HP Pavilion Plus Laptop 14-ey1xxx
```

## `--json`

以 JSON 输出；自动关掉颜色与 Logo。输出是一个对象，含 `schema_version`、`entries`、`failures`：

```
$ vitals --module memory --json
{
  "schema_version": 1,
  "entries": [
    {
      "type": "memory",
      "key": "Memory",
      "value": "22.17 GiB / 30.65 GiB (72%)"
    }
  ],
  "failures": []
}
```

`--json` 与 `--explain`、`--sources` 同给时，JSON 不生效：`--explain` 与 `--sources` 在渲染之前就结束进程。

```
$ vitals --module memory --explain --json
memory  显示  1 项
```

## `--logo <auto|none|名称>`

默认 `auto`：按发行版自动选 Logo，匹配不到时用通用那张。`none` 不显示 Logo。给名称就直接用那个名称。

名称**不存在也不会报错**：`--logo` 的取值解析器对任何字符串都成功，匹配不到只是在渲染时回退到通用 Logo。下面这台机器是 Arch，指定 `ubuntu` 仍会画出 Ubuntu 的图：

```
$ vitals --module os --logo ubuntu
                             ....            OS: Arch Linux x86_64
              .',:clooo:  .:looooo:.
           .;looooooooc  .oooooooooo'
        .;looooool:,''.  :ooooooooooc
（截断）
```

不存在的名称同样以退出码 0 结束：

```
$ vitals --module os --logo definitelynotalogo --json >/dev/null ; echo $?
0
```

## `--module <列表>`

只显示点名的模块，逗号分隔，**顺序就是你写的顺序**。点名的模块一定出现，与配置里有没有它无关（内置默认里没有 `bios`，一样可以点）：

```
$ vitals --module gpu,os,memory --logo none
GPU: AMD Radeon 780M [HawkPoint1] (amdgpu)
OS: Arch Linux x86_64
Memory: 22.31 GiB / 30.65 GiB (73%)

$ vitals --module bios --logo none
BIOS (UEFI): Insyde F.09 (12/05/2025)
```

模块名比较时忽略大小写，并忽略 `-` 与 `_`；下面四种写法都指向同一个模块：

```
$ for n in localip Local-IP LOCAL_IP local-ip; do vitals --module "$n" --json --logo none | grep -o '"type": "[^"]*"' | head -1; done
"type": "local-ip"
"type": "local-ip"
"type": "local-ip"
"type": "local-ip"
```

未知模块在参数解析阶段就被拒绝，退出码 2，stderr 打印全部可用模块；stdout 为空：

```
$ vitals --module nope ; echo $?
error: invalid value 'nope' for '--module <列表>': 未知模块 `nope`；可用：os, host, kernel, bios, ...（截断）
2
```

`--module` 是**选择**，不是过滤：它整体替换配置文件里的模块列表（不是从里面挑），并且点名的模块会丢掉配置里挂的条件——配置里 `battery` 被 `when-file-exists` 挡住，`--module battery` 仍会采集它：

```
$ vitals --config /tmp/vcond.toml --module battery --logo none --explain
battery  显示  1 项

$ vitals --config /tmp/vcond.toml --logo none --explain
os       显示  1 项
battery  跳过  路径 /definitely/not/here 不存在
camera   显示  2 项
```

## `--no-color`

关闭颜色（等同设置 `NO_COLOR`）。在真实终端里，键默认带 ANSI 序列；加该选项后不带：

```
# 终端（pty）下，不带 --no-color；其中 \x1b 表示 ESC 字节
\x1b[1m\x1b[36mOS\x1b[0m: Arch Linux x86_64

# 加 --no-color，或 NO_COLOR=1 后
OS: Arch Linux x86_64
```

输出是管道时本来就没有颜色，此时该选项无可见差别。

## `--list-modules`

把全部可用模块按固定顺序逐行打印到 stdout，共 61 个：

```
$ vitals --list-modules | head -6
os
host
kernel
bios
board
chassis
```

它是「问完就走」的选项：不读配置文件、不采集。即使 `--config` 给了一个不存在的路径也照样成功：

```
$ vitals --list-modules --config /tmp/nope-vitals.toml >/dev/null ; echo $?
0
```

（但参数本身仍要先通过解析：`--list-modules --module nope` 依然是退出码 2。）

## `--gen-config`

把内置默认配置打印到 stdout（2422 字节，以换行结束），不读配置文件、不采集；与其它选项同给也照常成功：

```
$ vitals --gen-config | head -6
# Vitals 配置
#
# 由 `vitals --gen-config` 打印。这个文件**整体替换**内置默认：写了 modules，
# 内置列表就不再生效，所以想少显示几个模块，得把要留的列全。
#
# 下面这份就是内置默认视图，与 fastfetch 2.68.1 **无参运行**时打印的逐项对得上
```

```
$ vitals --gen-config --config /tmp/nope-vitals.toml >/dev/null ; echo $?
0
```

## `--verbose`

把诊断信息写到 **stderr**，每行带 `vitals: ` 前缀。至少包含最终生效的设置，以及被条件跳过的模块：

```
$ vitals --module os --verbose --logo none 2>&1 >/dev/null
vitals: 模块 1 个：os
vitals: logo=none 颜色=开 json=关 verbose=开
```

带条件跳过时追加：

```
vitals: 跳过 battery：路径 /definitely/not/here 不存在
```

模块采集失败时打印失败行；只有 `--verbose` 才继续摊开底层原因链：

```
vitals: datetime 模块失败：打开 /tmp/nozone/tz 失败
vitals:   因为：Permission denied (os error 13)
```

`--verbose` 不影响退出码，也不改变 stdout 的内容。

## `--explain`

逐项说明每个模块**为什么出现、或为什么没有出现**。对着结果说话，四种状态各有理由；要分辨「空」与「显示」必须真跑一遍采集，所以它会花一次采集的时间。

报告写 stdout，退出码 0——即使某项是「失败」。四种状态的真实样子：

```
$ vitals --module memory --explain
memory  显示  1 项

$ vitals --config /tmp/vcond.toml --explain
os       显示  1 项
battery  跳过  路径 /definitely/not/here 不存在
camera   显示  2 项

$ vitals --module gamepad,battery --explain
gamepad  空  这台机器上没有可显示的数据
battery  显示  1 项

$ TZDIR=/tmp/nozone TZ=tz vitals --module datetime --explain
vitals: datetime 模块失败：打开 /tmp/nozone/tz 失败        # stderr
datetime  失败  打开 /tmp/nozone/tz 失败                  # stdout
```

「空」与「跳过」是两回事：前者采过了、这台机器上确实没有数据，后者是条件不满足、根本没去采。

`--explain` 与 `--json` 同给时只打印这份报告，不输出 JSON。

## `--sources`

每个模块**实际读了哪些文件**（运行时记录，不是手写的来源表）。报告写 stdout，退出码 0。跳过的模块显示跳过理由；什么都没读的模块会明说没有，而不是编一个来源：

```
$ vitals --module memory,gpu --sources
memory  /proc/meminfo
gpu     /sys/class/drm/card1/device/vendor, /sys/class/drm/card1/device/device, /sys/class/drm/card1/device/uevent, ...（截断）
```

```
$ vitals --config /tmp/vcond.toml --sources
os       /etc/os-release
battery  跳过  路径 /definitely/not/here 不存在
camera   /sys/class/video4linux/video0/name, /sys/class/video4linux/video1/name, /sys/class/video4linux/video2/name, /sys/class/video4linux/video3/name
```

什么都没读的模块：

```
$ vitals --module wm --sources
wm  没有读文件  （数据来自环境变量或系统调用）
```

> 注意：0.1.0 里 `--explain` 分支会直接返回，`--sources` 不会执行——与帮助原文「两个都给就
> 先打状态、再打依据」不符。**0.1.1 起已修正**，两个都给时两份报告都会打印：
>
> ```console
> $ vitals --module memory --explain --sources
> memory  显示  1 项
> memory  /proc/meminfo
> ```

只给 `--sources` 时它正常工作。

## `-h, --help` 与 `-V, --version`

两者都以退出码 0 结束，内容写 stdout。

```
$ vitals --version
vitals 0.1.0
```

## 输出流约定

| 流 | 内容 |
| --- | --- |
| stdout | 默认渲染结果（文本或 `--json` 的 JSON）；`--list-modules` 的模块名；`--gen-config` 的 TOML；`--explain` 报告；`--sources` 报告；`--help` / `--version` |
| stderr | 所有 `vitals: ` 前缀的诊断：配置读取/解析/版本错误、模块采集失败、`--verbose` 的最终设置与跳过理由、渲染写出错误。以及 clap 的参数错误（`error: ...`） |

诊断走 stderr 是刻意的，`vitals > 文件` 时结果文件不会被污染。

退出码（三种，均已实测）：

| 退出码 | 何时出现 |
| --- | --- |
| `0` | 正常结束：渲染成功、`--help`、`--version`、`--list-modules`、`--gen-config`、`--explain`、`--sources`。**某个模块采集失败但渲染成功仍是 0**（失败只写 stderr）。下游提前关闭管道（如 `vitals \| head -1`）也返回 0。 |
| `1` | 运行期失败：配置文件读取/解析/版本错误；写出输出失败（如 `vitals > /dev/full`，stderr 为 `vitals: 写入输出失败`）。 |
| `2` | 参数错误：未知选项、未知模块、多给位置参数。由 clap 产生。 |

```
$ vitals --module os --logo none >/dev/full ; echo $?
vitals: 写入输出失败
1

$ vitals 2>/dev/null | head -1 >/dev/null ; echo $?   # 管道提前关闭
0

$ vitals --nope ; echo $?
error: unexpected argument '--nope' found
2
```

## 环境变量

以下变量均在 `src/` 代码里确认读取位置（`文件:行号`）；其中 `NO_COLOR`、`CLICOLOR_FORCE` 由依赖 anstream 读取，本仓库没有直接调用，行为已实测确认。

### 影响程序本身（配置、颜色、布局）

| 变量 | 作用 | 读取位置 |
| --- | --- | --- |
| `XDG_CONFIG_HOME` | 默认配置目录；必须是绝对路径，相对路径会被忽略并回退到 `$HOME/.config` | `src/config/path.rs:27`；另见 `src/collectors/ini.rs:357`、`src/collectors/terminal_font.rs:126` |
| `HOME` | `XDG_CONFIG_HOME` 未设（或不是绝对路径）时的回退；`when-file-exists` 里 `~/` 的展开；若干模块的候选路径 | `src/config/path.rs:28`、`src/conditions.rs:182`、`src/collectors/ini.rs:358`、`src/collectors/ini.rs:491`、`src/collectors/terminal_font.rs:130`、`src/collectors/pkgdb.rs:240`、`src/collectors/pkgdb.rs:259`、`src/collectors/rust.rs:37` |
| `PATH` | `when-command-exists` 条件的查找范围（只查 PATH，不执行命令） | `src/conditions.rs:118` |
| `NO_COLOR` | 设为任意值即禁用颜色（等同 `--no-color`） | anstream 读取；`src/main.rs:143-150` 构造输出流，说明见 `src/render/theme.rs:4` |
| `CLICOLOR_FORCE` | 非空时即使输出是管道也强制保留颜色 | 同上；实测 `CLICOLOR_FORCE=1 vitals --module os --logo none \| cat -v` 会打印 `^[[1m^[[36mOS^[[0m: ...` |
| `COLUMNS` | 拿不到 tty 尺寸时用它当终端列数；列数未知就不隐藏 Logo（`COLUMNS=40 vitals` 会把 Logo 收掉） | `src/render/text.rs:320` |

### 影响模块采集

| 变量 | 作用 | 读取位置 |
| --- | --- | --- |
| `LC_ALL` / `LC_CTYPE` / `LANG` | `locale` 模块的区域设置，按此优先级取第一个非空值；都为空时回退读 `/etc/locale.conf` 的 `LANG=` | `src/collectors/locale.rs:12`（声明）、`src/collectors/locale.rs:27`（读取）；`src/collectors/locale.rs:14,16` |
| `TZ` | `datetime` 模块的时区（指向 `$TZDIR` 下的 zoneinfo 文件；没设则读 `/etc/localtime`）；`users` 模块也用它 | `src/collectors/date_time.rs:123`、`src/collectors/users.rs:276` |
| `TZDIR` | `$TZ` 的查找根目录，默认 `/usr/share/zoneinfo` | `src/collectors/date_time.rs:130` |
| `SHELL` | `shell` 模块优先用它；没设时回退 `/etc/passwd` 的登录 shell | `src/collectors/shell.rs:11`、`src/collectors/shell.rs:47` |
| `USER` / `LOGNAME` | `title` 与 `user` 模块的用户名兜底（先查 `/etc/passwd`） | `src/collectors/accounts.rs:102`、`src/collectors/accounts.rs:113`、`src/collectors/user.rs:23` |
| `VISUAL` / `EDITOR` | `editor` 模块 | `src/collectors/editor.rs:14`、`src/collectors/editor.rs:25` |
| `XCURSOR_THEME` / `XCURSOR_SIZE` | `cursor` 模块当前会话的主题与尺寸 | `src/collectors/cursor.rs:42`、`src/collectors/cursor.rs:52`；`src/collectors/cursor.rs:44`、`src/collectors/cursor.rs:69` |
| `XDG_SESSION_TYPE` | `wm` 模块判断会话类型 | `src/collectors/wm.rs:35` |
| `XDG_CURRENT_DESKTOP` / `XDG_SESSION_DESKTOP` / `DESKTOP_SESSION` | `de` 模块识别桌面环境 | `src/collectors/session.rs:52-56`、`src/collectors/session.rs:88` |
| `TERM` | `terminal` 模块与 `terminal-font` 模块识别终端 | `src/collectors/terminal.rs:58`、`src/collectors/terminal_font.rs:81` |
| `TERM_PROGRAM` / `TERM_PROGRAM_VERSION` | `terminal` 模块的终端名与版本；`terminal-font` 也用它认 ghostty / Alacritty | `src/collectors/terminal.rs:84-85`、`src/collectors/terminal_font.rs:94,105` |
| `KITTY_WINDOW_ID` / `KITTY_PID` | 认 kitty | `src/collectors/terminal.rs:20`、`src/collectors/terminal_font.rs:83-84` |
| `WEZTERM_EXECUTABLE` | 认 WezTerm | `src/collectors/terminal.rs:21` |
| `ALACRITTY_SOCKET` / `ALACRITTY_LOG` | 认 Alacritty | `src/collectors/terminal.rs:22`、`src/collectors/terminal_font.rs:103-104` |
| `WT_SESSION` | 认 Windows Terminal | `src/collectors/terminal.rs:23` |
| `VTE_VERSION` | 认 VTE | `src/collectors/terminal.rs:24` |
| `GHOSTTY_RESOURCES_DIR` | 认 ghostty | `src/collectors/terminal_font.rs:93` |
| `XDG_CONFIG_HOME` / `HOME` | `terminal-font` 找 kitty 配置；`theme` / `icons` / `font` / `wmtheme` 找 GTK、KDE 配置 | `src/collectors/terminal_font.rs:126,130`、`src/collectors/ini.rs:357,358` |
| `XDG_CONFIG_DIRS` / `XDG_DATA_DIRS` | GTK / 图标 / 光标主题的系统级候选目录 | `src/collectors/ini.rs:390`、`src/collectors/ini.rs:399` |
| `XDG_RUNTIME_DIR` / `PULSE_SERVER` | `sound` 模块判断音频是否在位 | `src/collectors/sound.rs:94`、`src/collectors/sound.rs:85` |
| `RUSTUP_TOOLCHAIN` / `RUSTUP_HOME` / `HOME` | `rust` 模块（读 rustup 的 `settings.toml`） | `src/collectors/rust.rs:32`、`src/collectors/rust.rs:36`、`src/collectors/rust.rs:37` |

实测三例：

```
$ XCURSOR_THEME=FooBar XCURSOR_SIZE=48 vitals --module cursor --json --logo none
      "value": "FooBar (48px)"

$ TERM_PROGRAM=ghostty TERM_PROGRAM_VERSION=1.2 vitals --module terminal --json --logo none
      "value": "ghostty 1.2"

$ env -u LC_ALL -u LC_CTYPE LANG=fr_FR.UTF-8 vitals --module locale --json --logo none
      "value": "fr_FR.UTF-8"
```

未发现其它会影响行为的变量。`PROGRAM`、`TAGLINE` 等是编译期常量，不是环境变量。

## 常见组合

**1. 只看几个模块**（顺序即你写的顺序）

```
$ vitals --module os,kernel,memory --logo none
OS: Arch Linux x86_64
Kernel: Linux 7.2.4-arch1-2
Memory: 22.17 GiB / 30.65 GiB (72%)
```

**2. JSON 输出给脚本**

```
$ vitals --module memory --json
{
  "schema_version": 1,
  "entries": [
    {
      "type": "memory",
      "key": "Memory",
      "value": "22.17 GiB / 30.65 GiB (72%)"
    }
  ],
  "failures": []
}
```

**3. 生成一份配置再改**

```
$ vitals --gen-config | head -6
# Vitals 配置
#
# 由 `vitals --gen-config` 打印。这个文件**整体替换**内置默认：写了 modules，
# 内置列表就不再生效，所以想少显示几个模块，得把要留的列全。
#
# 下面这份就是内置默认视图，与 fastfetch 2.68.1 **无参运行**时打印的逐项对得上
```

**4. 查某个模块的数据来源**

```
$ vitals --module memory,gpu --sources
memory  /proc/meminfo
gpu     /sys/class/drm/card1/device/vendor, /sys/class/drm/card1/device/device, /sys/class/drm/card1/device/uevent, ...（截断）
```

**5. 查某个模块为什么一行都没有**

```
$ vitals --module gamepad,battery --explain
gamepad  空  这台机器上没有可显示的数据
battery  显示  1 项
```

## 与配置文件的关系

**不带参数时按配置文件渲染**；用 `--module` 只留其中几个。

叠加顺序是 内置默认 → 用户配置文件 → 命令行参数。命令行只在前两层的结果上做选择（`--module` 整体替换模块列表，`--json` / `--no-color` 收紧输出，`--logo` 覆盖 Logo）。

配置文件的定位：

- 不给 `--config` 时找 `$XDG_CONFIG_HOME/vitals/config.toml`；`XDG_CONFIG_HOME` 没设或不是绝对路径时回退 `$HOME/.config/vitals/config.toml`；连 `HOME` 都没有则直接用内置默认，不报错。
- 这个路径下的文件**不存在不算错误**，安静地用内置默认——第一次运行的人不该看到报错。
- `--config` 显式给的路径读不到才是错误（退出码 1）。

关于 `modules` 列表，`--gen-config` 打印出来的文件里写得很明确：

> 由 `vitals --gen-config` 打印。这个文件**整体替换**内置默认：写了 modules，内置列表就不再生效，所以想少显示几个模块，得把要留的列全。

也就是说：

- 配置文件里**不写** `[[modules]]`：用内置默认列表（24 个模块，依次为 `title`、`separator`、`os`、`host`、`kernel`、`uptime`、`packages`、`shell`、`display`、`de`、`wm`、`cursor`、`terminal`、`terminal-font`、`cpu`、`gpu`、`memory`、`swap`、`disk`、`local-ip`、`battery`、`locale`、`break`、`colors`）。
- 配置文件里**写了** `[[modules]]`：内置列表不再生效，只显示你列的那些——想少显示几个模块，得把要留的列全。
- 每个模块都可以挂条件（`platforms`、`when-command-exists`、`when-file-exists`）；条件不满足就跳过，跳过不是错误。但被 `--module` 点名的模块会丢掉这些条件。
- `config_version` 必须不高于本程序支持的 `1`，否则报错退出 1。
