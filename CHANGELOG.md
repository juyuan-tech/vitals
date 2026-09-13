# 变更记录

## 未发布

- 文档双语收口：`docs/*.en.md` 六份英文版（`modules`/`configuration`/`cli`/`json`/`faq`/`logo`），
  每份与中文版逐条对照过——代码块去掉注释后逐行相同、`src/...:NN` 引用集合相同、选项与链接
  相同、正文数字相同、反引号里的标识符相同；引用到的程序输出（错误、`--explain`、`--sources`）
  仍是中文原文，不翻译、不改写。`doc/vitals.en.1` 是英文 man page（`groff -man -Tutf8` 渲染无警告）。
  三条新守卫进了 `tests/docs.rs`：中英互指、硬内容一致、英文模块参考覆盖全部 61 个模块；
  版本号守卫现在同时盯着两份 man page。

- 帮助双语：`VITALS_LANG=zh|en`，不设时按 `LC_ALL` / `LC_MESSAGES` / `LANG` 判断，
  **拿不准一律中文**（所以「什么都不设」的行为和以前完全一样）。英文还有 `--module`
  写错时的英文报错与英文取值名。选项结构仍然只有派生那一份，只有帮助文本按语言覆盖；
  运行期文本（`--explain`、`--sources`、错误信息）仍只有中文，README 与 `docs/cli.md` 里写明。
- 新增 `completions/`：bash 与 zsh 补全。两份都真跑过（bash 直接调补全函数、zsh 用 fpath + compinit 装载）；模块名现问 `--list-modules`，选项名与 Logo 名由 `tests/docs.rs` 双向盯着。fish 未提供——本机没有 fish，没验证过的不交付。
- 修掉 `disk_io` 里两个依赖宿主机型的测试（断言了本机 NVMe 的型号、赌虚拟盘叫 zram0），CI 上红过一次（那台机器报 `MSFT NVMe Accelerator v1.0`），上一轮只是碰巧通过。现在自己搭 `/sys/block/<dev>/device/` 的输入，四种情况都钉住。

- 新增 `doc/vitals.1` man page（用 `groff -man` 渲染核对过）与 `presets/` 四份示例配置
  （minimal / desktop / headless / all，每份都实跑过）。
- 新增 `tests/docs.rs`：盯住三条最容易腐烂的连接点——示例配置要能跑、`presets/all.toml`
  要等于注册表里的全部模块、`docs/modules.md` 要覆盖每个模块。加了模块忘改文档就会红。
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
