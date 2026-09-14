# 变更记录

## 0.1.2（2026-09-14）

运行期文案双语、`--gen-config` 分中英两份、补上 fish 补全，英文文档换用英文实跑输出。

### 运行期文案双语

- 帮助之外，**运行期文案**也跟 `VITALS_LANG=zh|en` 走：错误信息、`--explain` 的四种状态、
  `--sources` 的「没有读文件」、`--verbose` 的设置行都按语言给。原先散在采集器、条件判断、
  配置解析里的中文串收进 `src/i18n.rs` 一处；**中文原文逐字未改**，所以不设 `VITALS_LANG`
  时行为与以前完全一样（仍看 locale，拿不准中文）。
- `--gen-config` 分两份模板：`config/default.toml`（中文注释）与
  `config/default.en.toml`（英文注释），结构化内容一致；`tests/config.rs` 把两份都解析回来核对。
- 为什么用进程级语言而不是把语言参数一路传下去：消息**在哪里产生**（采集器深处的解析函数、
  `read.rs` 的自由函数、`thiserror` 属性里的字面量）与**由谁打印**离得很远，为显示给解析函数
  加参数不划算。代价是 `ConfigError` / `RenderError` 的 `Display` 改成手写。
- 字段名（`OS:`、`Memory:`）本来就是英文，两种语言下都一样。

### 修正（发布前全量审查发现）

发布前做了一遍全量审查：静态闸门（`cargo test` / clippy / fmt / MSRV 1.85.0 实编）、
`cargo audit`（46 个依赖 0 告警）、依赖与代码里对零子进程 / 零 `unsafe` / 不写文件 /
不发网络包 / 读取上限 / 控制字符过滤这几条不变式的核查、61 个模块逐个跑 `--json` 并
对着 JSON Schema 核字段与类型、12 类畸形配置 × 中英、超大配置下的线程上限实测
（20000 条模块峰值 17 线程 = 1 主 + 16 worker，正是 `WORKERS`）、以及一遍独立的对抗性
代码审查。查出并修掉下面这些。

- **管道下游提前关闭会 panic**（最严重的一条，0.1.1 就有）：`vitals --list-modules | head -1`
  在 stderr 上打一串 `thread 'main' panicked … failed printing to stdout: Broken pipe`，
  退出码 101。默认视图那条渲染路径早就按 Unix 惯例安静退 0 了，是 `print!` / `println!`
  这几个「打印一段文本」的命令没跟上。现在 `--gen-config`、`--list-modules`、`--explain`、
  `--sources` 都先攒成一段再写，断了就安静退 0；真写不进去（如 `> /dev/full`）仍然是
  一行 `写入输出失败` + 退出码 1。顺带修好 `--json | head -1`：`serde_json` 会把底下的
  `BrokenPipe` 包成 `Other`，导致它也退 1。
- **`--help` 的页脚在说假话**：「运行期文本（`--explain`、`--sources`、错误信息）目前只有中文」
  ——这一条本次改动之后就不成立了，中英两版页脚都改成了「帮助、运行期文案与 `--gen-config`
  模板都跟 `VITALS_LANG` 走」。这是用户打开帮助第一眼就看到的话，比文档里的错更该修。
- 英文的配置版本过高文案不成句：`newer than the 1 this build supports` →
  `newer than version 1, the newest this build supports`；文档里引用它的三处一并更新。
- 清掉两个半成品：`Messages::cannot_read_path`（零调用，连带一条 `use std::path::Path`）与
  `Messages::tagline`（生产代码不用，且它取进程级语言、而 clap 的 `about` 取显式语言参数，
  两条来源有分叉的风险）；`open_failed` 上抄错的文档注释也改了。
- 文档里 19 处 `src/…:NN` 引用因这次改代码而漂移（守卫抓到了，这正是它该干的），已按
  「老文本原样出现在新位置」逐处重指；`docs/configuration.md` 里 4 处所指代码本身改过的，
  连句子一起重写。中文 faq 的内存块数字与英文版差了一台机器的状态（中文 22.75 GiB、
  英文 7.24 GiB），按**同一时刻**的快照把两份重拍成一套数字。
- **配置文件的读取漏了上限**。`--config` 指到 `/dev/zero`、FIFO 这类文件时，
  `fs::read_to_string` 会一直读到内存耗尽；比 8 MiB 大的普通配置也会被整份读进来。
  采集器那边早就有这条上限（`read::MAX_READ`，那里的注释点名的例子正是 `/dev/zero`），
  只是配置文件走的是另一条读路径，漏掉了。现在两条路共用同一个上限：超限报错
  （`超过读取上限` / `over the read limit`），退出码 1。**用内存硬上限做了对照**：
  改动前指到 `/dev/zero` 会 OOM（拿到 `out of memory`），改动后一行错误信息结束。
  注意这是一处有意的行为收紧：12 MiB 的配置以前能读，现在会报错。
- 顺带把不是 UTF-8 的配置文件也换成明确的信息（`<路径> 不是 UTF-8`，此前是
  `读取配置文件 … 失败：stream did not contain valid UTF-8`）。

### fish 补全（更正一条错话）

- 新增 `completions/vitals.fish`，用本机真 fish（4.9.3）装载验证：`--module` 的三种写法
  （`--module os,ho`、`--module=os,ho`、已点过的不再列）都真跑过。
- 此前 README 与 CHANGELOG 里写的「fish 未提供——本机没有 fish」是**错的**：本机一直装着
  fish，是上一轮没查就下了结论。已更正。

### 文档

- 六份英文参考文档里引用的**程序输出**换成 `VITALS_LANG=en` 的真实输出（此前它们写明
  「输出只有中文」，因而照抄了中文原文）。`tests/docs.rs` 的对照守卫随之调整：从「代码块
  逐行相同」改成「代码块数量、语言标记、行数与**命令行逐字**相同」——输出本来就该按语言不同。
- 两份 man page 补上 `VITALS_LANG` 与 `--gen-config` 的说明（中文那份此前没有 `VITALS_LANG`
  条目）。
- 文档双语收口：`docs/*.en.md` 六份英文版（`modules`/`configuration`/`cli`/`json`/`faq`/`logo`），
  每份与中文版逐条对照过——`src/...:NN` 引用集合相同、选项与链接相同、正文数字相同、反引号里的
  标识符相同。`doc/vitals.en.1` 是英文 man page（`groff -man -Tutf8` 渲染无警告）。守卫进了
  `tests/docs.rs`：中英互指、硬内容一致、英文模块参考覆盖全部 61 个模块；版本号守卫同时盯着两份
  man page。
- 帮助双语：不设 `VITALS_LANG` 时按 `LC_ALL` / `LC_MESSAGES` / `LANG` 判断，**拿不准一律中文**。
  选项结构仍然只有派生那一份，只有文案按语言覆盖；`--module` 写错时的报错与取值名也跟着语言。
- 新增 `completions/`：bash、zsh 与 fish 补全（三份都真装载跑过）；模块名现问
  `--list-modules`，选项名与 Logo 名由 `tests/docs.rs` 双向盯着。
- 修掉 `disk_io` 里两个依赖宿主机型的测试（断言了本机 NVMe 的型号、赌虚拟盘叫 zram0），
  CI 上红过一次（那台机器报 `MSFT NVMe Accelerator v1.0`），上一轮只是碰巧通过。现在自己搭
  `/sys/block/<dev>/device/` 的输入，四种情况都钉住。
- 新增 `doc/vitals.1` man page（用 `groff -man` 渲染核对过）与 `presets/` 四份示例配置
  （minimal / desktop / headless / all，每份都实跑过）。
- 新增 `tests/docs.rs`：盯住最容易腐烂的连接点——示例配置要能跑、`presets/all.toml` 要等于
  注册表里的全部模块、`docs/modules.md` 要覆盖每个模块。加了模块忘改文档就会红。
- README 的文档索引补上 man page 与示例配置；配置参考里写明 presets 的用法。

## 0.1.1

两处**行为修正**：

- **`--explain` 与 `--sources` 同时给时，`--sources` 此前被吞掉**。`--help` 里承诺的是
  「两个都给就先打状态、再打依据」，但代码在 `--explain` 分支直接返回，依据报告从来不打印。
  现在两份报告都会输出。
- **`-h` 的首行标语改成中文**「你的系统生命体征，一眼看全。」，与 `--help` 首行一致
  （此前一个是英文、一个是中文）。

以下是本次的文档补全，不涉及其它行为：

- 新增参考文档：`docs/modules.md`（61 个模块逐条，含 `--sources` 实测的读取文件）、
  `docs/configuration.md`（逐键 + 真实运行依据）、`docs/cli.md`、`docs/json.md` 与
  `docs/vitals.schema.json`（JSON Schema）、`docs/faq.md`、`docs/logo.md`。
- 新增 `README.en.md`（英文版）；README 补了安装前置、真实默认视图、文档索引与常见问题入口。
- 新增社区文件：`CONTRIBUTING.md`、`SECURITY.md`、`CODE_OF_CONDUCT.md`、
  `.github/ISSUE_TEMPLATE/`、`.github/pull_request_template.md`。
- **修正 README 里与真实输出不符的示例**：`--explain` 的四态示例（原示例里的 `camera 跳过`
  在本机其实不成立）、`--sources` 的路径顺序、默认视图若干字段的格式；并删掉了配置示例里
  并不存在的模块名 `pci`。

0.x 期间每个小版本都可能带来行为变化；这是第一个版本。

## 0.1.0

61 个系统信息模块。默认视图本机实测 4-6 ms。

**可审计能力**（相对同类工具多出来的东西）

- `--explain`：每个配置项为什么出现、或为什么没出现（显示 / 空 / 跳过 / 失败）
- `--sources`：每个模块**实际**读了哪些文件——运行时观测出来的，不是一张手写对照表
- `--json`：结构化输出，控制字符由 `serde_json` 转义
- `--gen-config`：带注释的配置模板

**性能**

- 采集并行（共享计数器派活，线程数上限 16）：四个采样模块 821 ms → 221 ms
- 默认视图 4-6 ms；同一台机器上 fastfetch 2.68.1 是 20-37 ms

**安全与健壮性**（完整审计见 `AUDIT.md`）

- 零子进程、零 `unsafe`、不写文件、不发网络包
- 输出前去掉终端控制字符（C0/C1/DEL 与双向文本控制符）
- 单文件读取上限 8 MiB，超限报错而不截断；`$TZ` 相对名字拒绝 `..` 成分
- 配置里写大量重复模块不会引发线程爆炸
- `cargo audit`：46 个依赖，0 条告警

**已知未做**

见 README 的「已知未做」：多数是需要链接系统库、发网络请求或调用外部命令的模块。

MSRV 1.85（edition 2024）；CI 里有一个作业真按 1.85 编一遍。
