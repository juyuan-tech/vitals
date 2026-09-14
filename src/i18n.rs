//! 运行期文案：中文与英文两套，一处收齐。
//!
//! 为什么集中在这里：这些字过去散在各个采集器、条件判断、配置解析里，翻译要满仓库
//! 找，还容易漏。现在每条消息只有一个出处，`Messages::of(Lang::En)` 能整份取出来——
//! 测试拿它逐条扫「英文里不许出现汉字」，漏一条就红。
//!
//! 为什么调用点写 [`now`] 而不是把语言一路传下去：消息**在哪里产生**（采集器深处的
//! 解析函数）和**由谁打印**（`main` 的 `--explain`、`--verbose`）离得很远，让解析函数
//! 为了显示背上语言参数不划算。生效语言是进程级的（见 [`crate::lang::current`]），
//! 启动时定一次，之后只读。
//!
//! 中文原文在这里逐字保留，翻译只加不改——既有行为必须一个字节都不变。

use std::fmt::Display;

use crate::lang::{self, Lang};

/// 一套文案。
///
/// `Messages::of(Lang::Zh)` / `of(Lang::En)` 是纯函数，测试直接拿来对照；
/// 运行时用 [`now`] 取当前生效语言的那一套。
#[derive(Debug, Clone, Copy)]
pub struct Messages {
    lang: Lang,
}

/// 当前生效语言的文案。
#[must_use]
pub fn now() -> Messages {
    Messages::of(lang::current())
}

impl Messages {
    /// 指定语言的一套文案。
    #[must_use]
    pub const fn of(lang: Lang) -> Self {
        Self { lang }
    }

    // ---- 主流程写到 stderr 的报告 ----

    /// 读不到 `/etc/os-release`：不致命，Logo 退回通用那张。
    pub fn os_release_failed(&self, error: impl Display) -> String {
        match self.lang {
            Lang::Zh => format!("读取 os-release 失败：{error}"),
            Lang::En => format!("failed to read os-release: {error}"),
        }
    }

    /// `--verbose` 里说明某个模块被什么条件挡下来了。
    pub fn skipped(&self, module: &str, reason: &str) -> String {
        match self.lang {
            Lang::Zh => format!("跳过 {module}：{reason}"),
            Lang::En => format!("skipped {module}: {reason}"),
        }
    }

    /// 某个模块采集失败（其余模块照常出）。
    pub fn module_failed(&self, module: &str, error: impl Display) -> String {
        match self.lang {
            Lang::Zh => format!("{module} 模块失败：{error}"),
            Lang::En => format!("the {module} module failed: {error}"),
        }
    }

    /// `--verbose` 里展开的错误链，一层一行。
    ///
    /// 行首那两个空格是原样保留的：这一行缩在上一行下面，去掉就与改动前不一样了。
    pub fn because(&self, error: impl Display) -> String {
        match self.lang {
            Lang::Zh => format!("  因为：{error}"),
            Lang::En => format!("  because: {error}"),
        }
    }

    // ---- `--explain` 的四种状态 ----

    /// 条件不满足，根本没去采。
    #[must_use]
    pub const fn state_skipped(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "跳过",
            Lang::En => "skipped",
        }
    }

    /// 采了但出错了。
    #[must_use]
    pub const fn state_failed(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "失败",
            Lang::En => "failed",
        }
    }

    /// 采过了，这台机器上确实没数据。
    #[must_use]
    pub const fn state_empty(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "空",
            Lang::En => "empty",
        }
    }

    /// 采到了东西。
    #[must_use]
    pub const fn state_shown(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "显示",
            Lang::En => "shown",
        }
    }

    /// 「空」的理由。
    #[must_use]
    pub const fn no_data(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "这台机器上没有可显示的数据",
            Lang::En => "nothing to show on this machine",
        }
    }

    /// 「显示」的条数。英文里 1 要单数。
    #[must_use]
    pub fn items(&self, count: usize) -> String {
        match self.lang {
            Lang::Zh => format!("{count} 项"),
            Lang::En if count == 1 => format!("{count} item"),
            Lang::En => format!("{count} items"),
        }
    }

    // ---- `--sources` ----

    /// 被条件挡下来的模块，`--sources` 里也给一行。
    pub fn sources_skipped(&self, reason: &str) -> String {
        match self.lang {
            Lang::Zh => format!("跳过  {reason}"),
            Lang::En => format!("skipped  {reason}"),
        }
    }

    /// 一个文件都没读：明说没有，而不是编一个来源出来。
    #[must_use]
    pub const fn sources_no_files(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "没有读文件  （数据来自环境变量或系统调用）",
            Lang::En => "no files read  (data comes from environment variables or system calls)",
        }
    }

    // ---- 命令行 ----

    /// `--verbose` 的头一行：这一趟要采哪些模块。
    pub fn modules_count(&self, count: usize, list: &str) -> String {
        match self.lang {
            Lang::Zh => format!("模块 {count} 个：{list}"),
            Lang::En if count == 1 => format!("{count} module: {list}"),
            Lang::En => format!("{count} modules: {list}"),
        }
    }

    /// `--verbose` 的第二行：生效的设置。
    pub fn settings_line(
        &self,
        logo: impl Display,
        color: bool,
        json: bool,
        verbose: bool,
    ) -> String {
        let color = self.on_off(color);
        let json = self.on_off(json);
        let verbose = self.on_off(verbose);

        match self.lang {
            Lang::Zh => format!("logo={logo} 颜色={color} json={json} verbose={verbose}"),
            Lang::En => format!("logo={logo} color={color} json={json} verbose={verbose}"),
        }
    }

    /// 布尔值的说法。
    #[must_use]
    pub const fn on_off(&self, value: bool) -> &'static str {
        match (self.lang, value) {
            (Lang::Zh, true) => "开",
            (Lang::Zh, false) => "关",
            (Lang::En, true) => "on",
            (Lang::En, false) => "off",
        }
    }

    /// `--module` 写了一个不存在的模块名。
    pub fn unknown_module(&self, name: &str, list: &str) -> String {
        match self.lang {
            Lang::Zh => format!("未知模块 `{name}`；可用：{list}"),
            Lang::En => format!("unknown module `{name}`; available: {list}"),
        }
    }

    // ---- 声明式条件（`--verbose` 与 `--explain` 都会用） ----

    /// 平台不符：说清楚当前是什么平台。
    pub fn platform_is(&self, name: &str) -> String {
        match self.lang {
            Lang::Zh => format!("当前平台是 {name}"),
            Lang::En => format!("the current platform is {name}"),
        }
    }

    /// 平台不在支持的名单里。
    #[must_use]
    pub const fn platform_unsupported(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "当前平台不在支持的名单里",
            Lang::En => "the current platform is not in the supported list",
        }
    }

    /// `when-command-exists`：只在 `PATH` 里找，绝不执行。
    pub fn command_missing(&self, command: &str) -> String {
        match self.lang {
            Lang::Zh => format!("命令 `{command}` 不在 PATH 里"),
            Lang::En => format!("the command `{command}` is not on PATH"),
        }
    }

    /// `when-file-exists`：路径不存在。
    pub fn path_missing(&self, path: impl Display) -> String {
        match self.lang {
            Lang::Zh => format!("路径 {path} 不存在"),
            Lang::En => format!("the path {path} does not exist"),
        }
    }

    // ---- 配置 ----

    /// 配置文件读不出来。
    pub fn config_read_failed(&self, path: impl Display, source: impl Display) -> String {
        match self.lang {
            Lang::Zh => format!("读取配置文件 {path} 失败：{source}"),
            Lang::En => format!("failed to read config file {path}: {source}"),
        }
    }

    /// 配置解析失败。`origin` 是「哪个文件 / 哪段文本」。
    pub fn config_parse_failed(&self, origin: &str, source: impl Display) -> String {
        match self.lang {
            Lang::Zh => format!("{origin} 解析失败\n{source}"),
            Lang::En => format!("failed to parse {origin}\n{source}"),
        }
    }

    /// 配置文件比程序新。
    pub fn config_version_too_new(&self, found: u32, supported: u32) -> String {
        match self.lang {
            Lang::Zh => format!("配置版本 {found} 高于本程序支持的 {supported}，请升级 vitals"),
            Lang::En => format!(
                "config version {found} is newer than version {supported}, the newest this build supports; upgrade vitals"
            ),
        }
    }

    /// 不是从文件来的那段文本，在错误信息里怎么称呼。
    #[must_use]
    pub const fn config_text(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "配置文本",
            Lang::En => "the config text",
        }
    }

    // ---- 调度与渲染 ----

    /// 点名了一个注册表里没有的模块。
    pub fn no_such_module(&self, name: &str) -> String {
        match self.lang {
            Lang::Zh => format!("没有名为 `{name}` 的模块"),
            Lang::En => format!("no module named `{name}`"),
        }
    }

    /// 采集线程 panic 了（`catch_unwind` 兜住的那种）。
    #[must_use]
    pub const fn collector_thread_died(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "采集线程异常结束",
            Lang::En => "a collector thread ended unexpectedly",
        }
    }

    /// 往输出流写失败。
    #[must_use]
    pub const fn write_failed(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "写入输出失败",
            Lang::En => "failed to write output",
        }
    }

    // ---- 读文件（`collectors::read`）----

    /// 打开失败（不存在之外的原因：权限、是目录、设备出错……）。
    pub fn open_failed(&self, path: impl Display) -> String {
        match self.lang {
            Lang::Zh => format!("打开 {path} 失败"),
            Lang::En => format!("failed to open {path}"),
        }
    }

    /// 读文件失败。所有「读取 X 失败」都走这一条，措辞只有一处。
    pub fn cannot_read(&self, path: impl Display) -> String {
        match self.lang {
            Lang::Zh => format!("读取 {path} 失败"),
            Lang::En => format!("failed to read {path}"),
        }
    }

    /// 超过读取上限：不静默截断（截断过的数据会被下游当成完整的）。
    pub fn too_large(&self, path: impl Display, limit: u64) -> String {
        match self.lang {
            Lang::Zh => format!("{path} 超过读取上限（{limit} 字节），拒绝读进内存"),
            Lang::En => format!(
                "{path} is over the read limit of {limit} bytes; not loading it into memory"
            ),
        }
    }

    /// 该是文本的文件不是 UTF-8。
    pub fn not_utf8(&self, path: impl Display) -> String {
        match self.lang {
            Lang::Zh => format!("{path} 不是 UTF-8"),
            Lang::En => format!("{path} is not UTF-8"),
        }
    }

    // ---- 各个采集器自己的失败原因 ----

    /// 块设备：看不了它的 `device/`。
    pub fn view_failed(&self, path: impl Display) -> String {
        match self.lang {
            Lang::Zh => format!("查看 {path} 失败"),
            Lang::En => format!("failed to inspect {path}"),
        }
    }

    /// 盘挂载信息读不出来。
    pub fn mount_read_failed(&self, mount: &str) -> String {
        match self.lang {
            Lang::Zh => format!("读取 {mount} 的文件系统信息失败"),
            Lang::En => format!("failed to read filesystem information from {mount}"),
        }
    }

    /// 时区名读不出来，退回 UTC。
    pub fn timezone_unreadable(&self, name: &str) -> String {
        match self.lang {
            Lang::Zh => format!("{name} (读不了，按 UTC)"),
            Lang::En => format!("{name} (unreadable; assuming UTC)"),
        }
    }

    /// 时区是从 `/etc/localtime` 读出来的。
    #[must_use]
    pub const fn timezone_from_localtime(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "(从 /etc/localtime 读到的时区)",
            Lang::En => "(time zone read from /etc/localtime)",
        }
    }

    /// `disk-io` 的采样间隔是 0。
    #[must_use]
    pub const fn disk_io_zero_interval(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "disk-io 的采样间隔是 0 毫秒，算不出每秒速率",
            Lang::En => {
                "disk-io's sampling interval is 0 ms, so per-second rates cannot be computed"
            }
        }
    }

    /// 采样窗口里物理盘集合变了。
    pub fn physical_disks_changed(&self, changed: impl Display) -> String {
        match self.lang {
            Lang::Zh => {
                format!("采样窗口里物理盘集合变了（{changed}），这次算不出真实速率")
            }
            Lang::En => format!(
                "the set of physical disks changed during the sampling window ({changed}); real rates cannot be computed this time"
            ),
        }
    }

    /// 读扇区数回退了。
    pub fn read_sectors_regressed(&self, disk: impl Display) -> String {
        match self.lang {
            Lang::Zh => {
                format!("{disk} 的读扇区数在采样窗口里回退了（盘被重置？），这次算不出真实速率")
            }
            Lang::En => format!(
                "{disk}'s read sector count went backwards during the sampling window (disk reset?); real rates cannot be computed this time"
            ),
        }
    }

    /// 写扇区数回退了。
    pub fn write_sectors_regressed(&self, disk: impl Display) -> String {
        match self.lang {
            Lang::Zh => {
                format!("{disk} 的写扇区数在采样窗口里回退了（盘被重置？），这次算不出真实速率")
            }
            Lang::En => format!(
                "{disk}'s write sector count went backwards during the sampling window (disk reset?); real rates cannot be computed this time"
            ),
        }
    }

    /// `/sys/block/<dev>/stat` 的字段不是一个合法的 stat。
    pub fn bad_block_stat(&self, path: impl Display, text: &str) -> String {
        match self.lang {
            Lang::Zh => {
                format!("{path} 的字段不是一个合法的块设备 stat（原文 `{text}`）")
            }
            Lang::En => format!(
                "the fields of {path} are not a valid block device stat (raw text `{text}`)"
            ),
        }
    }

    /// 采样前后盘对不上：少了。
    pub fn disks_missing(&self, missing: &str) -> String {
        match self.lang {
            Lang::Zh => format!("少了 {missing}"),
            Lang::En => format!("missing {missing}"),
        }
    }

    /// 采样前后盘对不上：多了。
    pub fn disks_unexpected(&self, disk: &str) -> String {
        match self.lang {
            Lang::Zh => format!("多了 {disk}"),
            Lang::En => format!("unexpected {disk}"),
        }
    }

    /// 网卡在采样窗口里消失了。
    pub fn interface_gone(&self, interface: &str) -> String {
        match self.lang {
            Lang::Zh => format!("采样窗口里 {interface} 消失了，量不到速率"),
            Lang::En => {
                format!("{interface} disappeared during the sampling window; no rate to measure")
            }
        }
    }

    /// `net-io` 的采样间隔是 0。
    #[must_use]
    pub const fn net_io_zero_interval(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "net-io 的采样间隔是 0 毫秒，算不出每秒速率",
            Lang::En => {
                "net-io's sampling interval is 0 ms, so per-second rates cannot be computed"
            }
        }
    }

    /// 收字节数回退了。
    pub fn rx_regressed(&self, interface: &str) -> String {
        match self.lang {
            Lang::Zh => format!(
                "{interface} 的 rx_bytes 在采样窗口里回退了（网卡被重置？），这次算不出真实速率"
            ),
            Lang::En => format!(
                "{interface}'s rx_bytes went backwards during the sampling window (interface reset?); real rates cannot be computed this time"
            ),
        }
    }

    /// 发字节数回退了。
    pub fn tx_regressed(&self, interface: &str) -> String {
        match self.lang {
            Lang::Zh => format!(
                "{interface} 的 tx_bytes 在采样窗口里回退了（网卡被重置？），这次算不出真实速率"
            ),
            Lang::En => format!(
                "{interface}'s tx_bytes went backwards during the sampling window (interface reset?); real rates cannot be computed this time"
            ),
        }
    }

    /// 某个计数器解析不出来。
    pub fn counter_parse_failed(&self, interface: &str, what: &str, text: &str) -> String {
        match self.lang {
            Lang::Zh => format!("解析 {interface} 的 {what} 失败（原文 `{text}`）"),
            Lang::En => format!("failed to parse {what} for {interface} (raw text `{text}`)"),
        }
    }

    /// `/proc/stat` 在采样窗口里没了。
    #[must_use]
    pub const fn stat_gone(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "/proc/stat 在采样窗口里消失了，量不到 CPU 占用率",
            Lang::En => {
                "/proc/stat disappeared during the sampling window; no CPU usage to measure"
            }
        }
    }

    /// `cpu-usage` 的采样间隔是 0。
    #[must_use]
    pub const fn cpu_usage_zero_interval(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "cpu-usage 的采样间隔是 0 毫秒，算不出占用率",
            Lang::En => "cpu-usage's sampling interval is 0 ms, so usage cannot be computed",
        }
    }

    /// 采样窗口里核数变了。
    pub fn core_count_changed(&self, from: impl Display, to: impl Display) -> String {
        match self.lang {
            Lang::Zh => format!(
                "/proc/stat 的核数在采样窗口里从 {from} 变成了 {to}（CPU 热插拔？），这次算不出占用率"
            ),
            Lang::En => format!(
                "the core count in /proc/stat changed from {from} to {to} during the sampling window (CPU hotplug?); usage cannot be computed this time"
            ),
        }
    }

    /// 累计时间没有增长。
    #[must_use]
    pub const fn no_cpu_progress(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "/proc/stat 里有核的累计时间没有增长，这次算不出占用率",
            Lang::En => {
                "cumulative times in /proc/stat did not grow; usage cannot be computed this time"
            }
        }
    }

    /// `top` 的采样间隔是 0。
    #[must_use]
    pub const fn top_zero_interval(&self) -> &'static str {
        match self.lang {
            Lang::Zh => "top 的采样间隔是 0 毫秒，算不出 CPU 占用率",
            Lang::En => "top's sampling interval is 0 ms, so CPU usage cannot be computed",
        }
    }

    /// `rustup` 的 TOML 解析失败。
    pub fn toml_parse_failed(&self, path: &str) -> String {
        match self.lang {
            Lang::Zh => format!("解析 {path} 失败"),
            Lang::En => format!("failed to parse {path}"),
        }
    }

    /// 建 UDP socket 失败（`local-ip` 用它问出本机地址）。
    pub fn udp_socket_failed(&self, bind: impl Display) -> String {
        match self.lang {
            Lang::Zh => format!("创建 UDP socket（bind {bind}）失败"),
            Lang::En => format!("failed to create a UDP socket (bind {bind})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 英文文案里不许出现汉字——漏译一条就红。
    #[test]
    fn the_english_messages_have_no_chinese() {
        let en = Messages::of(Lang::En);
        let messages = vec![
            en.os_release_failed("x"),
            en.skipped("os", "reason"),
            en.module_failed("os", "boom"),
            en.because("source"),
            en.state_skipped().to_owned(),
            en.state_failed().to_owned(),
            en.state_empty().to_owned(),
            en.state_shown().to_owned(),
            en.no_data().to_owned(),
            en.items(1),
            en.items(2),
            en.sources_skipped("reason"),
            en.sources_no_files().to_owned(),
            en.modules_count(3, "os, host"),
            en.settings_line("auto", true, false, true),
            en.on_off(true).to_owned(),
            en.on_off(false).to_owned(),
            en.unknown_module("nope", "os, host"),
            en.platform_is("linux"),
            en.platform_unsupported().to_owned(),
            en.command_missing("bash"),
            en.path_missing("/nope"),
            en.config_read_failed("/nope.toml", "no such file"),
            en.config_parse_failed("the config text", "bad toml"),
            en.config_version_too_new(9, 1),
            en.config_text().to_owned(),
            en.no_such_module("nope"),
            en.collector_thread_died().to_owned(),
            en.write_failed().to_owned(),
            en.open_failed("/nope"),
            en.cannot_read("/nope"),
            en.too_large("/nope", 8),
            en.not_utf8("/nope"),
            en.view_failed("/dev/sda"),
            en.mount_read_failed("/proc/mounts"),
            en.timezone_unreadable("Asia/Shanghai"),
            en.timezone_from_localtime().to_owned(),
            en.disk_io_zero_interval().to_owned(),
            en.physical_disks_changed("sda"),
            en.read_sectors_regressed("sda"),
            en.write_sectors_regressed("sda"),
            en.bad_block_stat("/sys/block/sda/stat", "raw"),
            en.disks_missing("sda"),
            en.disks_unexpected("sdb"),
            en.interface_gone("eth0"),
            en.net_io_zero_interval().to_owned(),
            en.rx_regressed("eth0"),
            en.tx_regressed("eth0"),
            en.counter_parse_failed("eth0", "rx_bytes", "raw"),
            en.stat_gone().to_owned(),
            en.cpu_usage_zero_interval().to_owned(),
            en.core_count_changed(4, 8),
            en.no_cpu_progress().to_owned(),
            en.top_zero_interval().to_owned(),
            en.toml_parse_failed("/nope.toml"),
            en.udp_socket_failed("0.0.0.0:0"),
        ];

        for message in messages {
            assert!(
                !message
                    .chars()
                    .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
                "英文文案里出现了汉字：{message}"
            );
        }
    }

    /// 中文那份就是原来的字，一个字都不能变。
    #[test]
    fn the_chinese_messages_are_unchanged() {
        let zh = Messages::of(Lang::Zh);

        assert_eq!(zh.skipped("os", "理由"), "跳过 os：理由");
        assert_eq!(zh.state_shown(), "显示");
        assert_eq!(zh.state_empty(), "空");
        assert_eq!(zh.no_data(), "这台机器上没有");
        assert_eq!(zh.items(1), "1 项");
        assert_eq!(zh.items(2), "2 项");
        assert_eq!(zh.on_off(true), "开");
        assert_eq!(zh.on_off(false), "关");
        assert_eq!(
            zh.settings_line("auto", true, false, true),
            "logo=auto 颜色=开 json=关 verbose=开"
        );
        assert_eq!(
            zh.unknown_module("nope", "os, host"),
            "未知模块 `nope`；可用：os, host"
        );
        assert_eq!(zh.path_missing("/nope"), "路径 /nope 不存在");
        assert_eq!(zh.command_missing("bash"), "命令 `bash` 不在 PATH 里");
        assert_eq!(
            zh.sources_no_files(),
            "没有读文件  （数据来自环境变量或系统调用）"
        );
        assert_eq!(zh.write_failed(), "写入输出失败");
        // 行首缩进是原样保留的（对照改动前的二进制逐字节核过）。
        assert_eq!(zh.because("boom"), "  因为：boom");
    }

    /// 英文的名词要跟数走。
    #[test]
    fn english_plural_follows_the_count() {
        let en = Messages::of(Lang::En);

        assert_eq!(en.items(0), "0 items");
        assert_eq!(en.items(1), "1 item");
        assert_eq!(en.items(2), "2 items");

        assert_eq!(en.modules_count(1, "os"), "1 module: os");
        assert_eq!(en.modules_count(2, "os, host"), "2 modules: os, host");
    }
}
