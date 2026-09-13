# 安全与质量审计

2026-09-13，对 `vitals-rs` v0.1.0（提交 `35db727` 起）做的一次完整审计。

## 一句话结论

**没有发现可被远程利用的漏洞。** 发现 4 个本地可达的问题（终端转义注入、`$TZ` 路径穿越、
单文件读取无上限、配置可触发的线程爆炸），全部已修复并补了测试或实测证据；另有 2 项
「声明类」缺口（`rust-version` 从未按 1.85 编译过、CI 不校验 MSRV）已补齐。

## 方法

静态（全量，不只抽查）：

- 非测试代码里的 panic 面：`unwrap` / `expect` / `panic!` / `unreachable!` / `todo!` /
  `unimplemented!`——**0 处**（脚本按 `#[cfg(test)]` 切掉测试段后扫描）。
- 写文件与删除：`File::create` / `OpenOptions` / `fs::write` / `remove_file` /
  `remove_dir` / `set_permissions`——18 处命中**全在 `#[cfg(test)]` 之内**（逐行核对行号）。
- `unsafe`：12 处命中**全是文档注释**，源码里唯一的 `unsafe` 字样是
  `src/lib.rs:13` 的 `#![forbid(unsafe_code)]`。
- 网络：无 `TcpStream`/`TcpListener`；唯一 socket 用法见下文 F7。
- 敏感路径：无 `/etc/shadow`、无 `~/.ssh`、无 `id_rsa`、无 token/凭据文件读取。
  （`users` 读 `utmp`、`accounts` 读 `/etc/passwd`、`rust` 读 `~/.rustup/settings.toml`
  属模块本分。）
- 差分算术：四个采样模块全部用 `checked_sub`，计数器倒退即「无数据」而不是下溢。

动态（实测，不是读代码推测）：

- `HOME`/`TZ` 注入与路径穿越（见 F1/F2）；9 MiB 稀疏文件测读取上限（F3）；
  跑起来边数 `/proc/<pid>/task` 测线程上限（F4）；3000 条重复模块测调度器。
- 供应链：`cargo audit`（RustSec，1243 条公告 / 46 个依赖）**退出码 0，0 条告警**。
- 依赖许可证：36 个 MIT-OR-Apache-2.0、3 个 Apache-OR-MIT、3 个 MIT、
  2 个 Apache-WITH-LLVM-exception-OR-Apache-OR-MIT、1 个 Unlicense-OR-MIT、
  1 个 MIT-AND-Unicode-3.0。**无 GPL/AGPL 类传染性许可**，与 `MIT OR Apache-2.0` 相容。
- 声明验证：`cargo +1.85.0 check --all-targets` 通过（印证 `rust-version = "1.85"`）；
  `cargo package` 与 `cargo publish --dry-run --registry crates-io` 走通到「aborting upload
  due to dry run」。

## 发现

### F1 终端转义注入（中低危，已修复）

**证据（实测）**

```console
$ HOME=$'/tmp/\x1b[31mX' vitals --sources --module rust | cat -v
rust  /tmp/^[[31mX/.rustup/settings.toml
```

原始 ESC 被打印到了终端。同类来源还有挂载点卷标、`utmp` 里的用户名与来源主机、EDID 型号
字符串——这些字符串都不归 vitals 管。一个 ESC 就足以移动光标、改窗口标题、触发 OSC 52
之类的序列：屏幕上写着「系统信息」，画面却由那串字符编排。

**修复**（`61a82aa`）：新增 `render/sanitize.rs`，在**唯一一处** `Info` → 行的转换里清洗
key 与 value（选这个位置是因为宽度也要按清洗后的文本算，否则补空格会错、版式跟着歪），
`--sources`/`--explain` 是 main 自己打印的，另接一遍。去掉 C0/DEL/C1（顺带挡掉换行与制表
符）与双向文本控制符（U+202A–202E、U+2066–2069），保留中文与 emoji；干净字符串走
`Cow::Borrowed`，不给渲染加分配。`--json` 那条路不需要它——`serde_json` 本来就转义。

**验证**：同一命令现在输出 `/tmp/[31mX/.rustup/settings.toml`；新增 5 条单元测试
（ESC、换行制表、双向控制、宽字符保留、`Cow` 借用）。

### F2 `$TZ` 路径穿越（低危，已修复）

**证据（实测）**

```console
$ TZ='../../../../etc/hostname' vitals --sources --module datetime
datetime  /usr/share/zoneinfo/../../../../etc/hostname
```

`$TZ` 被直接拼进 zoneinfo 路径，`..` 一路向上、读到了目录外的任意文件。`users` 那侧一直
有这条检查，`date_time` 漏了。

**影响**：读的是调用者本来就能读的文件，所以不是提权；但它把「读时区文件」变成了「读任意
可读文件」，与 F3 组合即成 OOM（见下），且让 `$TZ` 从「时区名」变成了「路径输入」。

**修复**（`61a82aa`）：规则收进 `collectors/tzif::zone_name_is_safe` 一份，两处共用；
相对名字含 `..`/根目录成分一律不接受。绝对路径仍按 POSIX 照收（`TZ=/etc/localtime` 是
合法用法），那条路由 F3 的读取上限兜住。

**验证**：同一命令现在只读 `/etc/localtime`；新增单元测试覆盖
`Asia/Shanghai`、`UTC`（收）与 `../../../../etc/hostname`、`..`、``、`/`（拒）。

### F3 单文件读取没有上限（低中危，已修复）

**证据（代码 + 实测）**：`read::text`/`bytes` 直接 `fs::read`，而路径可能是环境变量拼的
（`$TZ`/`$TZDIR`）。`TZ=/dev/zero` 会让 `fs::read` 一直读到 OOM。

**修复**（`61a82aa`）：统一走 `read::read_capped`，上限 8 MiB（`MAX_READ`），多读一个字节
以区分「正好这么大」与「还有更多」，超限**报错而不截断**——截断过的数据会被下游当成完整
的，那比报错更坏。

**验证（实测）**：

```console
$ truncate -s 9M /tmp/sparse9m && TZ=/tmp/sparse9m vitals --module datetime
vitals: datetime 模块失败：/tmp/sparse9m 超过读取上限（8388608 字节），拒绝读进内存
```

新增回归测试覆盖边界两侧：正好等于上限要能读、多一个字节要报错。

### F4 配置可触发的线程爆炸与 `spawn` panic（低危，已修复）

**证据**：调度器原本「一条模块一个线程」，而 `plan` 不去重——配置里写 3000 条重复模块就
要 3000 个线程；`scope.spawn` 在起不了线程时会 panic，那时连一句「哪个模块失败了」都报不
出来，整体退出。重复模块不是错误输入（配置里写两遍没有理由被拒），所以修的是调度器。

**修复**（`35db727`）：共享计数器派活，最多 16 个工作线程（`Builder::spawn_scoped`，
起不来就少起几个、剩下的由当前线程接着干——降级，不 panic）；每个线程带回自己的序号，
最后按序号摆回配置顺序。

**验证（实测）**：64 个 `net-io` 跑起来时 task 数 **13**（含主线程），耗时 804 ms
= 4 批 × 200 ms，与上限模型一致；3000 条重复模块仍是 3000 条、28 ms、退出码 0；
默认视图 4-5 ms、四个采样模块 221 ms，（改前 4-6 ms / 221 ms）无回归。新增回归测试
用 3000 条交替 `os`/`host` 同时钉住条数与顺序。

### F5 `rust-version = "1.85"` 从未被验证（已补齐）

声明挂在 `Cargo.toml` 上给用户看，但本机默认连 nightly，谁也没按 1.85 编过。

**补齐**：`cargo +1.85.0 check --all-targets` **通过**（干净完成，无告警）。声明成立，
不需要改。

### F6 CI 不校验 MSRV（已补齐）

`.github/workflows/ci.yml` 只有 fmt/clippy/test 三个步骤，跑在 `stable` 上——第 5 条那种
漂移它抓不住。**补齐**：加一个 `msrv` 作业，用 `dtolnay/rust-toolchain@1.85.0` 跑
`cargo check --all-targets`。

（顺带核对了 `actions/checkout@v7`：该 tag 真实存在，不是笔误。）

### F7 `local-ip` 会创建 UDP socket（信息项，非漏洞）

**证据**：`src/collectors/local_ip.rs:145`，`UdpSocket::bind` + `connect(target)`，
为标准「让内核做一次路由查找好问出本机地址」的写法。

**判定**：UDP 的 `connect` **不发包**，只在内核里设定默认对端并做路由查找；查不到路由返回
`ENETUNREACH`，被当作「这台机器没有出口」＝无数据，不是错误。整个程序**没有任何网络请求**。
但它是这套「零网络」说法里唯一的例外，所以在 README 的「安全与隐私」里明写了。

### F8 `c_string` 的未检查加法（低危/不可达，已修复）

**证据（模糊测试首次运行即命中）**

```
thread 'collectors::users::utmp_fuzz_tests::malformed_utmp_records_never_panic' panicked
at src/collectors/users.rs:218:36:
attempt to add with overflow
```

`c_string` 用 `offset + len` 直接算切片末端。真机调用方传的都是常量（`OFF_USER`、32），
所以**从真实输入不可达**；但 release 下会绕回、debug 下 panic，而两者都不该取决于传进来的
长度。绕回后 `start > end`，`get` 返回 `None`——不会读到错字段，所以危害止于 panic。

**修复**：`offset.checked_add(len)?`。**验证**：上面那条测试现在通过，并且它会一直守着这行。

## 已核验的正面结论

这些不是「读起来没问题」，是查过的：

- 非测试代码 **0 处** `unwrap`/`expect`/`panic!`/`unreachable!`/`todo!`。
- **0 处**子进程（无 `Command::new`/`process::Command`）；`when-command-exists` 只沿
  `PATH` 找同名文件，**不 fork、不 exec**（`conditions::command_exists`）。
- **0 处**非测试写文件/删除；`--gen-config` 只打印到 stdout。
- **0 处** `unsafe` 代码；`#![forbid(unsafe_code)]`，系统调用走 `rustix` 安全封装。
- **0 条**依赖漏洞（`cargo audit`，46 个依赖），依赖许可证全部宽松无传染性。
- 不可信字节解析器都有边界守卫：`utmp` 分块用 `chunks_exact` 且有长度前置检查；
  EDID 用 `get(range)?` 取窗口再做字段索引；`tzif` **完全不做按文件头计数分配**
  （源码里没有 `Vec`/`with_capacity`），只在切片上按偏移读。
- 采样模块的差分全用 `checked_sub`，计数器倒退 → 无数据，不会下溢或说谎。
- `--json` 的控制字符由 `serde_json` 转义，不会漏成裸字节。

## 未覆盖（如实列出）

- **模糊测试是确定性的、不是覆盖率驱动的**：已给两个字节级解析器（TZif、utmp 记录）补上
  畸形输入测试——随机字节跨所有长度、合法文件逐长度截断、逐位翻转、极端时间戳与极端偏移，
  外加一份真机 `/usr/share/zoneinfo/UTC`（拿不到就跳过）。首轮就命中了 F8。但这不是
  libFuzzer/AFL 那种覆盖率引导的 fuzz，EDID、efivar 等其余解析器仍只有手工夹具。
- **没有第三方静态分析**：只用了 `clippy --all-targets -D warnings`，没跑
  MIRI / cargo-deny / coverity 一类。自有一行 `unsafe` 也没有（`forbid`），MIRI 收益有限。
- **F4 里「线程起不来」那条降级分支没有自动化测试**：要可靠造出「`spawn` 失败」得
  注入线程工厂，当前没有这个接缝。它被代码路径与实测的线程上限间接覆盖，但不是直接覆盖。
- **没有做特权/沙箱场景测试**：没在 seccomp、容器或 setuid 场景下跑过。
  （`local-ip` 建 socket 失败会被记为该模块的真失败，按设计如实报告。）
- **没有审计上游依赖的源码**，只查了已知漏洞库与许可证。

## 修复提交

| 提交 | 内容 |
| --- | --- |
| `61a82aa` | F1 转义注入 + F2 `$TZ` 穿越 + F3 读取上限 |
| `35db727` | F4 采集线程数上限 |
| 本提交 | 审计报告 + F6 CI 增加 MSRV 作业 |
