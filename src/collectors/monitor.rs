//! Monitor：显示器的物理参数。
//!
//! 值与键的形状（fastfetch 2.68.1 真机实测，逐字）：
//!
//! ```text
//! Monitor (SDC4197): 2880x1800 px @ 120.001 Hz - 300x190 mm (13.98 inches, 242.93 ppi)
//! ```
//!
//! 与 `Display` 是**同一份数据的两种摆法**（见 `display::facts`）：`Display` 印
//! `2880x1800 @ 120Hz`（给人一眼看），`Monitor` 印参数本身——连三位小数的刷新率都要，
//! 因为那是它的口径。所以取数只有一份，谁都不抄第二遍。
//!
//! 两处换算：
//!
//!   - `inches = sqrt(mm_w² + mm_h²) / 25.4`（对角线毫米换英寸）
//!   - `ppi    = sqrt(px_w² + px_h²) / inches`
//!
//! **数据不全就不印**：没有模式、或 EDID 里没有物理尺寸时，这一行只印得出半截，
//! 看着像坏了。宁可没有数据（家族规矩：没有数据不是错误）。

use crate::collectors::display;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 显示器物理参数。
pub struct Monitor;

/// 屏幕对角线英寸数：`sqrt(w² + h²)` 毫米再换英寸。
fn inches(width_mm: u32, height_mm: u32) -> f64 {
    f64::hypot(f64::from(width_mm), f64::from(height_mm)) / 25.4
}

/// 每英寸像素数。`6.98 inches` 这种假长度不算，所以分母用同一个 `inches`。
fn ppi(width: u32, height: u32, width_mm: u32, height_mm: u32) -> f64 {
    f64::hypot(f64::from(width), f64::from(height)) / inches(width_mm, height_mm)
}

impl Collector for Monitor {
    fn name(&self) -> &'static str {
        "monitor"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let mut entries = Vec::new();

        for connector in display::connectors() {
            let Some(facts) = display::facts(&connector)? else {
                continue;
            };

            // 三样缺一不可：没模式就没有 `px`，没物理尺寸就算不出 inches/ppi，
            // 没刷新率那一段就是空的。缺任何一样都跳过这个连接器。
            let (Some((width, height)), Some((width_mm, height_mm)), Some(refresh)) =
                (facts.mode, facts.physical_mm, facts.refresh_hz)
            else {
                continue;
            };

            // 键里的名字用 EDID 的厂商+产品码（`SDC4197`）；EDID 里没有时退回连接器名
            // （`eDP-1`）——`Display` 一直用后者，总比一行没有名字的键好。
            let label = facts
                .edid_name
                .clone()
                .unwrap_or_else(|| facts.connector.clone());
            let value = format!(
                "{width}x{height} px @ {refresh:.3} Hz - {width_mm}x{height_mm} mm ({:.2} inches, {:.2} ppi)",
                inches(width_mm, height_mm),
                ppi(width, height, width_mm, height_mm),
            );

            entries.push(
                Info::new(self.name(), format!("Monitor ({label})"), value)
                    .with_variable("connector", facts.connector)
                    .with_variable("name", label.clone())
                    .with_variable("width", width.to_string())
                    .with_variable("height", height.to_string())
                    .with_variable("refresh", format!("{refresh:.3}"))
                    .with_variable("width_mm", width_mm.to_string())
                    .with_variable("height_mm", height_mm.to_string()),
            );
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机 eDP 面板的真值（EDID 里读出来的 300x190 mm、2880x1800）。
    const REAL_MM: (u32, u32) = (300, 190);
    const REAL_PX: (u32, u32) = (2880, 1800);

    #[test]
    fn the_diagonal_is_measured_from_both_sides() {
        // sqrt(300² + 190²) / 25.4 = 13.9837…，fastfetch 印 13.98。
        assert_eq!(format!("{:.2}", inches(REAL_MM.0, REAL_MM.1)), "13.98");
    }

    #[test]
    fn ppi_divides_by_that_same_diagonal() {
        // sqrt(2880² + 1800²) / 13.9837 = 242.93…，fastfetch 印 242.93。
        let value = ppi(REAL_PX.0, REAL_PX.1, REAL_MM.0, REAL_MM.1);

        assert_eq!(format!("{value:.2}"), "242.93");
    }

    #[test]
    fn the_value_is_shaped_like_fastfetch() {
        let entries = Monitor.collect(&Context::for_tests()).unwrap();

        assert!(!entries.is_empty(), "本机至少有一块已连接且带 EDID 的屏");
        for info in &entries {
            assert_eq!(info.module, "monitor");
            assert!(
                info.key.starts_with("Monitor ("),
                "键该带面板名：{}",
                info.key
            );
            assert!(
                info.value.contains(" px @ ") && info.value.contains(" mm ("),
                "值该是参数行：{}",
                info.value
            );
            assert!(info.value.ends_with(" ppi)"), "以 ppi 收尾：{}", info.value);
        }
    }

    #[test]
    fn the_module_name_matches_the_config_vocabulary() {
        assert_eq!(Monitor.name(), "monitor");
    }
}
