//! 剪映式布局断点：全仓唯一来源。
//!
//! 上游 FilmCraft 的面板可以任意拆分、任意浮动，没有"断点"概念；剪映式单页工作台需要在
//! 三个宽度档位上给出确定行为。为避免各面板各自判断宽度导致三档行为漂移，
//! **所有宽度判断都必须经过本模块的 [`LayoutClass::from_width`]**，其他文件不得出现
//! `available_width < 768` 之类的裸比较（见 `docs/derivation/02-ia-mapping.md` §2）。

/// Compact 档的上界（含）：宽度小于 768 时进入单列布局。
pub const COMPACT_MAX_WIDTH: f32 = 767.0;

/// Medium 档的上界（含）：768–1024 为双栏布局，1025 起进入四区常驻。
pub const MEDIUM_MAX_WIDTH: f32 = 1024.0;

/// Compact 布局下的底部抽屉。
///
/// 抽屉只承载"看/选"类面板，不改变文档；真正改文档仍然走命令面。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Drawer {
    /// 素材：工程/媒体库。
    Media,
    /// 字幕：文本与字幕面板。
    Captions,
    /// 音频：轨道混音器。
    Audio,
    /// 属性：选中片段的属性。
    Inspector,
}

impl Drawer {
    /// 每个一级 Tab 打开哪个抽屉。
    pub fn for_tab(tab: Tab) -> Self {
        match tab {
            Tab::Cut => Self::Media,
            Tab::Captions => Self::Captions,
            Tab::Audio => Self::Audio,
        }
    }
}

/// Compact 顶部的一级 Tab（剪映：剪辑 / 字幕 / 音频）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tab {
    /// 剪辑：素材与粗剪。
    #[default]
    Cut,
    /// 字幕：文本、花字与字幕轨。
    Captions,
    /// 音频：音乐的混音与响度。
    Audio,
}

/// 剪映式的三个布局档位。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutClass {
    /// 手机竖屏：单列，素材与属性走底部抽屉。
    Compact,
    /// 平板：预览在上、时间轴在下，两侧为可收起侧栏。
    Medium,
    /// 桌面：左侧素材、中部预览、右侧属性、底部时间轴四区常驻。
    Wide,
}

impl LayoutClass {
    /// 由可用宽度得到布局档位。
    ///
    /// 宽度来自窗口/面板尺寸，因此按"不可信输入"处理：`NaN`、无穷与非正数一律落到
    /// [`LayoutClass::Compact`]，绝不 panic，也不产生越界布局。
    pub fn from_width(width: f32) -> Self {
        if !width.is_finite() || width <= 0.0 {
            return Self::Compact;
        }
        if width <= COMPACT_MAX_WIDTH {
            Self::Compact
        } else if width <= MEDIUM_MAX_WIDTH {
            Self::Medium
        } else {
            Self::Wide
        }
    }

    /// 是否单列布局。
    pub fn is_compact(self) -> bool {
        matches!(self, Self::Compact)
    }

    /// 是否需要常驻侧栏（Medium 与 Wide 都有侧栏，只是可收起程度不同）。
    pub fn has_sidebars(self) -> bool {
        !self.is_compact()
    }

    /// 稳定的对外名称，用于控制通道的 `ui.inspect`（不要改，测试与脚本按它断言）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Medium => "medium",
            Self::Wide => "wide",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_width_is_compact() {
        assert_eq!(LayoutClass::from_width(390.0), LayoutClass::Compact);
    }

    #[test]
    fn breakpoint_boundaries_are_inclusive() {
        assert_eq!(LayoutClass::from_width(COMPACT_MAX_WIDTH), LayoutClass::Compact);
        assert_eq!(LayoutClass::from_width(768.0), LayoutClass::Medium);
        assert_eq!(LayoutClass::from_width(MEDIUM_MAX_WIDTH), LayoutClass::Medium);
        assert_eq!(LayoutClass::from_width(MEDIUM_MAX_WIDTH + 1.0), LayoutClass::Wide);
    }

    #[test]
    fn tablet_and_desktop_widths_match_the_documented_targets() {
        assert_eq!(LayoutClass::from_width(768.0), LayoutClass::Medium);
        assert_eq!(LayoutClass::from_width(1024.0), LayoutClass::Medium);
        assert_eq!(LayoutClass::from_width(1280.0), LayoutClass::Wide);
    }

    #[test]
    fn hostile_widths_fall_back_to_compact() {
        assert_eq!(LayoutClass::from_width(f32::NAN), LayoutClass::Compact);
        assert_eq!(LayoutClass::from_width(f32::INFINITY), LayoutClass::Compact);
        assert_eq!(LayoutClass::from_width(-1.0), LayoutClass::Compact);
        assert_eq!(LayoutClass::from_width(0.0), LayoutClass::Compact);
    }

    #[test]
    fn sidebars_follow_compactness() {
        assert!(!LayoutClass::Compact.has_sidebars());
        assert!(LayoutClass::Medium.has_sidebars());
        assert!(LayoutClass::Wide.has_sidebars());
    }

    #[test]
    fn names_are_stable_for_the_control_channel() {
        assert_eq!(LayoutClass::from_width(390.0).as_str(), "compact");
        assert_eq!(LayoutClass::from_width(768.0).as_str(), "medium");
        assert_eq!(LayoutClass::from_width(1280.0).as_str(), "wide");
    }
}
