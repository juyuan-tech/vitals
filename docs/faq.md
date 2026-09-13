# 常见问题

[English](faq.en.md) | **中文**

这里的回答都有实测依据；引用到代码的地方给出 `文件:行号`，引用到命令的地方贴真实输出。

## 为什么内存数字和 `free` 不一样？

**其实是同一个口径**，差别只在单位和取整。

vitals 的算法是 `MemTotal - MemAvailable`（`src/collectors/memory.rs` +
`src/collectors/meminfo.rs:58`），而现代 `free`（procps 4.x）的「已用」也是
`总计 - 可用`，其中「可用」取自同一个 `MemAvailable`。同一时刻实测：

```console
$ free -h | head -2
               总计        已用        空闲        共享   缓冲/缓存        可用
内存：         30Gi        22Gi       494Mi       4.7Gi        10Gi       7.9Gi

$ vitals --json --module memory --logo none | jq -r '.entries[].value'
22.75 GiB / 30.65 GiB (74%)

$ grep -E '^MemTotal|^MemAvailable' /proc/meminfo
MemTotal:       32140260 kB     # 30.65 GiB
MemAvailable:    8281996 kB     #  7.90 GiB
```

32140260 − 8281996 = 23858264 kB = 22.75 GiB——和 vitals 印的完全一致。`free` 印 `22Gi`
是因为它按 GiB 取整。

## `--json` 里的数字能拿来计算吗？

不能。`value` 就是屏幕上那一行，带给人读的单位（`22.75 GiB`、`74%`、`1 day, 7 hours`），
而且 `uptime`、`memory`、`cpu`、`net-io`、`disk-io`、`top` 每次都不同。**这份 JSON 没有原始
数值字段。**

要可计算的数字，用 `vitals --sources` 查出该模块真正读了哪些文件，直接读那些文件：

```console
$ vitals --sources --module memory
memory  /proc/meminfo
```

详见 [`json.md`](json.md)。

## 为什么默认视图没有 BIOS、主板、机箱？

因为默认视图是照着 fastfetch **实际**无参运行的输出对齐的（真机两边并排比过），它不印这些。
想显示就点名或写进配置：

```console
$ vitals --module os,bios,board,chassis
```

`--gen-config` 打印的模板里有一段注释解释了这次取舍，以及早先为什么列了这些模块。

## 为什么 `--sources` 会写「没有读文件」？

因为这个模块的数据不来自文件。`--sources` 记录的是**文件读取**，而像 `kernel`（系统调用）、
`wm`（环境变量）、`terminal`（进程链）这类模块压根不读文件，就照实这么写：

```console
$ vitals --sources --module kernel,wm,terminal
kernel    没有读文件  （数据来自环境变量或系统调用）
wm        没有读文件  （数据来自环境变量或系统调用）
terminal  没有读文件  （数据来自环境变量或系统调用）
```

## 为什么带采样窗口的模块要等 200 ms？

CPU 占用率、网络吞吐、磁盘吞吐、进程榜这些是**差分**，必须隔一段时间取两次样才知道速率。
窗口定义在四个采集器各自的 `SAMPLE_WINDOW`：

- `src/collectors/net_io.rs:59`
- `src/collectors/disk_io.rs:42`
- `src/collectors/cpu_usage.rs:56`
- `src/collectors/top.rs:69`

四个串起来是 821 ms，全是空转。现在改成共享计数器派活的并行采集（线程数上限 16），
总时长≈一次窗口，实测 221 ms。

## 我的卷标 / 主机名里有奇怪字符，为什么被吃掉了？

因为它们在输出前会被过滤。磁盘卷标、`utmp` 里的用户名、EDID 型号、由环境变量拼出来的路径
都可能带 ESC 序列，原样打到终端就能移动光标、改标题、甚至伪造成别的输出。所以
C0/C1/DEL 与双向文本控制符（U+202A–202E、U+2066–2069）会被去掉，中文与 emoji 保留。

实现在 `src/render/sanitize.rs`，在 `Info` 变成输出行的**唯一**一处调用，所以文本版式和
宽度计算用的是同一份文本。审计发现 F1（见 [`../AUDIT.md`](../AUDIT.md)）。

## `local-ip` 会不会偷偷发包？

不会。它建一个 UDP socket 并 `connect` 到一个目标地址，只为让内核做一次路由查找，好问出
本机会用哪个源地址；UDP 的 `connect` 不发送任何数据。查不到路由就是「没有出口」＝无数据。

这是全项目唯一碰 socket 的地方，审计里列为信息项 F7（见 [`../AUDIT.md`](../AUDIT.md)）。

## 支持 macOS / Windows 吗？

代码里没有任何平台分支，数据全部来自 `/proc`、`/sys` 和 Linux 系统调用，所以**只在 Linux 上
验证过**，其它系统预计不可用。这是刻意取舍：为了「不引入需要链接系统库的依赖」，
有些模块干脆不做（见 README「已知未做」）。

## 为什么 `camera` 是「跳过」而不是报错？

因为它带了条件：条件不满足就跳过，跳过不是错误。`--explain` 的四态是
「显示 / 空 / 跳过 / 失败」，只有最后的「失败」会成为 stderr 上的警告：

```console
$ vitals --explain --module os,gamepad
os       显示  1 项
gamepad  空  这台机器上没有可显示的数据
```

## 帮助能换成英文吗？

能，中英两套帮助都在二进制里，不需要语言包：

```
$ VITALS_LANG=en vitals -h      # 就这一条命令要英文
$ export VITALS_LANG=en         # 或者一直用英文
```

不设 `VITALS_LANG` 时看 `LC_ALL` / `LC_MESSAGES` / `LANG`；**拿不准就用中文**，
所以「什么都不设」还是原来的行为。优先级表和判定细则见 [`cli.md`](cli.md#语言)。

只换帮助：`--explain`、`--sources` 和错误信息目前仍是中文。

## 怎么加一个模块？

见 [`../CONTRIBUTING.md`](../CONTRIBUTING.md)。采集器契约只有两个方法
（`name` 与 `collect`），加完记得在注册表里登记——`ModuleType::ALL` 的顺序是不变量，
配置校验和 `--list-modules` 都依赖它。
