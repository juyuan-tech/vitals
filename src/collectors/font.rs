//! Font：界面字体。
//!
//! 两侧的写法完全不同，所以各有各的处理：
//!
//! - GTK 的 `gtk-font-name` 本来就是给人看的（`Cantarell 11`），原样用；
//! - KDE 的 `kdeglobals` `[General] font` 是 Qt 的**机器串**
//!   （`Noto Sans,10,-1,5,50,0,0,0,0,0`），非解析不可——直接印出来没人看得懂，
//!   而它是 KDE 机器上唯一的来源。解析见 [`kde_font_value`]。
//!
//! 两边的输出统一成「名字 + 字号」（`Adwaita Sans 11` / `Noto Sans 10`），
//! 这样从输出上看不出底层是谁写的。字号拿不到（例如字体描述里点大小与像素大小
//! 都是非正数）就只给名字：**宁可少给一个数，也不编一个 0**。
//!
//! 候选顺序和 [`super::theme`] 一致：用户级（GTK → KDE）在发行版默认之前。
//!
//! 读不到就是无数据。

use crate::collectors::ini::{self, Origin, Probe};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// GTK 侧的键。
const GTK_KEY: &str = "gtk-font-name";
/// KDE 侧的 section 与键。KDE 把字体码成一整条 `QFont::toString()`。
const KDE_SECTION: &str = "General";
/// 见 [`KDE_SECTION`]。
const KDE_KEY: &str = "font";

/// 界面字体。
pub struct Font;

impl Font {
    /// 查找线索，按优先级：用户 GTK → 用户 KDE → 系统 GTK → 系统 KDE。
    fn candidates() -> Vec<Probe> {
        let mut candidates = Probe::gtk_user(GTK_KEY);
        candidates.extend(Probe::kconfig_user("kdeglobals", KDE_SECTION, KDE_KEY));
        candidates.extend(Probe::gtk_system(GTK_KEY));
        candidates.extend(Probe::kconfig_system("kdeglobals", KDE_SECTION, KDE_KEY));
        candidates
    }
}

impl Collector for Font {
    fn name(&self) -> &'static str {
        "font"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let Some(hit) = ini::probe(&Self::candidates())? else {
            return Ok(Vec::new());
        };

        let value = match hit.origin {
            // GTK 的 `gtk-font-name` 已经是「名字 字号」，没有可解析的东西。
            Origin::Gtk => hit.value,
            Origin::Kde => {
                // 机器串坏到连名字都取不出来时整条作废：与其印个 `,10,-1,...`，
                // 不如什么都不显示。
                let Some(value) = kde_font_value(&hit.value) else {
                    return Ok(Vec::new());
                };
                value
            }
            // 字体没有 `index.theme` 那一路，候选表里构造不出这个来源。
            Origin::Xcursor => return Ok(Vec::new()),
        };

        Ok(vec![
            Info::new(
                self.name(),
                "Font",
                format!("{} ({})", value, hit.origin.label()),
            )
            .with_variable("name", value)
            .with_variable("origin", hit.origin.label()),
        ])
    }
}

/// KDE 的字体描述串 → 给人看的值。
///
/// Qt 的 `QFont::toString()` 写出来的是
/// `族名,点大小,像素大小,styleHint,weight,italic,underline,strikeOut,fixedPitch`，
/// 除了族名后面全是十进制整数（例：`Noto Sans,10,-1,5,50,0,0,0,0,0`）。
///
/// 切分点取**第一个能当整数读的字段**，它前面整段都是族名：
///
/// - 只按序号取第 0 段，族名里带逗号时（`Noto Sans, Noto Sans CJK`）会把名字切掉一半；
/// - 从右边数固定 9 个字段，遇到 Qt 版本增减字段就会错位。
///
/// 按「第一个数字」切两头都照顾得到，而且族名原样截取，逗号后的空格都不会丢。
///
/// 字号只认有意义的那个：点大小为正就用它，否则退回像素大小（带上 `px`
/// 以便与点大小区分），两个都没有就只给名字。
///
/// 其余字段（weight、italic、下划线……）**刻意丢掉**：它们是给 Qt 还原
/// `QFont` 用的，还原成人类可读的样子需要一张权重码表（`50` 是 Normal、
/// `75` 是 Bold……），而这一列值的用途是「一眼看出用的什么字体」。
/// 取不出族名（例如串以数字开头）时返回 `None`。
fn kde_font_value(raw: &str) -> Option<String> {
    // 每个字段连同它在原串里的起始字节：切族名时要原样保留那段文本
    // （族名里的空格、逗号后的空格都是名字的一部分）。
    let mut fields: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0;
    for field in raw.split(',') {
        fields.push((offset, field));
        offset += field.len() + 1; // +1 是字段之间那个逗号
    }

    let first_number = fields
        .iter()
        .position(|(_, field)| field.trim().parse::<i64>().is_ok());

    // 一个数字字段都没有：整串都是族名（Qt 允许只写族名）。
    let cut = first_number.map_or(raw.len(), |index| fields[index].0);
    let name = raw[..cut].trim().trim_end_matches(',').trim();
    if name.is_empty() {
        return None;
    }

    // 第 0 个数字是点大小、第 1 个是像素大小，都按字段下标取，不重新过滤一遍
    // ——中间混进非数字时也不会错位。
    let number_at = |index: usize| -> Option<i64> {
        let (_, field) = fields.get(first_number?.checked_add(index)?)?;
        field.trim().parse::<i64>().ok()
    };

    let point = number_at(0).unwrap_or(-1);
    let pixel = number_at(1).unwrap_or(-1);

    let size = if point > 0 {
        point.to_string()
    } else if pixel > 0 {
        format!("{pixel}px")
    } else {
        return Some(name.to_owned());
    };

    Some(format!("{name} {size}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_kde_font_string_into_a_name_and_a_size() {
        // 用户给的那条真实例子。
        assert_eq!(
            kde_font_value("Noto Sans,10,-1,5,50,0,0,0,0,0").as_deref(),
            Some("Noto Sans 10")
        );
    }

    #[test]
    fn the_family_is_everything_before_the_first_number() {
        // 族名里带逗号时，逗号后的空格也要原样留着——它是名字的一部分。
        assert_eq!(
            kde_font_value("Noto Sans, Noto Sans CJK,11,-1,5,50,0,0,0,0,0").as_deref(),
            Some("Noto Sans, Noto Sans CJK 11")
        );
    }

    #[test]
    fn a_pixel_size_is_used_when_the_point_size_is_not_positive() {
        // Qt 用 setPixelSize 时点大小是 -1，此时只有像素大小有意义。
        assert_eq!(
            kde_font_value("Cantarell,-1,13,5,50,0,0,0,0,0").as_deref(),
            Some("Cantarell 13px")
        );
    }

    #[test]
    fn a_name_only_description_still_gives_a_name() {
        // 既没点大小也没像素大小：只给名字，不编一个 0。
        assert_eq!(kde_font_value("Noto Sans").as_deref(), Some("Noto Sans"));
        assert_eq!(
            kde_font_value("Noto Sans,0,-1,5,50,0,0,0,0,0").as_deref(),
            Some("Noto Sans")
        );
    }

    #[test]
    fn a_missing_family_is_not_a_font() {
        // 以数字开头的串取不出族名：与其印 `,10,...`，不如什么都不显示。
        assert_eq!(kde_font_value(",10,-1,5,50,0,0,0,0,0"), None);
        assert_eq!(kde_font_value("10,-1,5,50,0,0,0,0,0"), None);
        assert_eq!(kde_font_value(""), None);
        assert_eq!(kde_font_value("   "), None);
    }

    #[test]
    fn whitespace_around_the_fields_does_not_leak_into_the_name() {
        assert_eq!(
            kde_font_value("  Noto Sans , 10 , -1 , 5").as_deref(),
            Some("Noto Sans 10")
        );
    }

    #[test]
    fn a_trailing_comma_does_not_stick_to_the_name() {
        assert_eq!(kde_font_value("Noto Sans,").as_deref(), Some("Noto Sans"));
    }

    #[test]
    fn collects_on_this_machine_if_a_font_is_configured() {
        let entries = Font.collect(&Context::for_tests()).unwrap();

        for info in &entries {
            assert_eq!(info.module, "font");
            assert_eq!(info.key, "Font");
            assert!(!info.value.is_empty());
            assert!(!info.value.contains('\n'));
            assert!(info.variable("origin").is_some());
        }
    }

    #[test]
    fn the_module_name_matches_the_config_vocabulary() {
        assert_eq!(Font.name(), "font");
    }
}
