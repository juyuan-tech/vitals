//! 渲染器：把采集结果变成字节。

use std::io::Write;

use crate::core::info::Info;

/// 一个渲染器。
///
/// 拿 `&self` 而不是 `&mut self` 是刻意的：渲染器是 `Report` 到字节的纯函数，
/// 没有要跨次保留的状态。真需要临时缓冲就地分配——它只跑一次，不在热路径上。
///
/// 阶段 5 写文本渲染器（Logo、对齐、颜色），阶段 6 写 JSON 渲染器。
/// 两者是**平级**的兄弟，谁也不包谁：JSON 不是「文本渲染器的一个开关」。
pub trait Renderer {
    /// 把 `report` 写进 `out`。
    ///
    /// `out` 用 `&mut dyn Write` 而不是泛型参数：输出量是几百字节，
    /// 不值得为它做单态化，而且 dyn 能让 `main` 拿一个 `Box<dyn Renderer>` 存两种渲染器。
    fn render(&self, report: &Report<'_>, out: &mut dyn Write) -> Result<(), RenderError>;
}

/// 渲染器唯一的输入。
///
/// 「选哪个 Logo」不在这里决定：那是 CLI/配置的事，渲染器只管画。
#[derive(Debug)]
pub struct Report<'a> {
    /// 已经选好的 Logo。`None` 表示这次不画 Logo（`--logo none` 或 `--json`）。
    pub logo: Option<&'a Logo>,
    /// 采集结果，顺序即显示顺序。
    pub entries: &'a [Info],
}

impl<'a> Report<'a> {
    /// 组装一份渲染输入。
    #[must_use]
    pub const fn new(logo: Option<&'a Logo>, entries: &'a [Info]) -> Self {
        Self { logo, entries }
    }
}

/// 一个发行版 Logo。
///
/// 内容在**编译期**嵌入（阶段 5 用 `include_str!` 填 `lines`），
/// 运行时不读磁盘——`PLAN.md` 第十条明确排除的事。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Logo {
    /// 匹配用的发行版 id，对应 `/etc/os-release` 的 `ID`。
    pub id: &'static str,
    /// ASCII 行，已经按换行切好。
    pub lines: &'static [&'static str],
}

/// 渲染失败。
///
/// 目前只会因为写输出出错。将来真出现别的失败原因再加 variant——
/// 现在多造几个用不上的 variant 只是自欺欺人的「可扩展性」。
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// 往输出流写入失败（管道断了、磁盘满了、终端关了）。
    #[error("写入输出失败")]
    Write(#[from] std::io::Error),
}
