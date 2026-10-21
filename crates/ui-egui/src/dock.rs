//! Premiere-style docking: a tree of splits and tab groups. Panels are rounded frames separated by
//! thin gutters; each group has a tab strip (active tab bright, with a panel menu "≡"); the focused
//! panel gets a blue outline. Gutters drag to resize; workspaces are serialized trees.
//!
//! 剪映专业版工作区（`Jianying`）里，含时间轴的组不画这条页签条：换成一条 26 pt 的序列标题行
//! （[`draw_jianying_timeline_header`]），序列切换改由标题行上的 ▾ 列表承担。

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use serde::{Deserialize, Serialize};

use craft_ui::layout::{ResizeHandle, ResizeHandleStyle, SplitAxis, SplitSize as SharedSplitSize, split_rect};

use crate::icons::{self, Icon};
use crate::theme::Tokens;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PanelKind {
    Project,
    MediaBrowser,
    Libraries,
    Info,
    Effects,
    Markers,
    History,
    Source,
    EffectControls,
    AudioClipMixer,
    Metadata,
    Program,
    Timeline,
    Tools,
    AudioMeters,
    AudioTrackMixer,
    LumetriColor,
    LumetriScopes,
    EssentialGraphics,
    EssentialSound,
    TextToSpeech,
    Properties,
    Text,
    Events,
    Progress,
    ReferenceMonitor,
    Timecode,
    ProjectNotes,
}

impl PanelKind {
    pub const ALL: [PanelKind; 28] = [
        PanelKind::Project,
        PanelKind::MediaBrowser,
        PanelKind::Libraries,
        PanelKind::Info,
        PanelKind::Effects,
        PanelKind::Markers,
        PanelKind::History,
        PanelKind::Source,
        PanelKind::EffectControls,
        PanelKind::AudioClipMixer,
        PanelKind::Metadata,
        PanelKind::Program,
        PanelKind::Timeline,
        PanelKind::Tools,
        PanelKind::AudioMeters,
        PanelKind::AudioTrackMixer,
        PanelKind::LumetriColor,
        PanelKind::LumetriScopes,
        PanelKind::EssentialGraphics,
        PanelKind::EssentialSound,
        PanelKind::TextToSpeech,
        PanelKind::Properties,
        PanelKind::Text,
        PanelKind::Events,
        PanelKind::Progress,
        PanelKind::ReferenceMonitor,
        PanelKind::Timecode,
        PanelKind::ProjectNotes,
    ];
    pub fn title(self) -> &'static str {
        match self {
            PanelKind::Project => "Project",
            PanelKind::MediaBrowser => "Media Browser",
            PanelKind::Libraries => "Libraries",
            PanelKind::Info => "Info",
            PanelKind::Effects => "Effects",
            PanelKind::Markers => "Markers",
            PanelKind::History => "History",
            PanelKind::Source => "Source",
            PanelKind::EffectControls => "Effect Controls",
            PanelKind::AudioClipMixer => "Audio Clip Mixer",
            PanelKind::Metadata => "Metadata",
            PanelKind::Program => "Program",
            PanelKind::Timeline => "Timeline",
            PanelKind::Tools => "Tools",
            PanelKind::AudioMeters => "Audio Meters",
            PanelKind::AudioTrackMixer => "Audio Track Mixer",
            PanelKind::LumetriColor => "Lumetri Color",
            PanelKind::LumetriScopes => "Lumetri Scopes",
            PanelKind::EssentialGraphics => "Essential Graphics",
            PanelKind::EssentialSound => "Essential Sound",
            PanelKind::TextToSpeech => "Text to Speech",
            PanelKind::Properties => "Properties",
            PanelKind::Text => "Text",
            PanelKind::Events => "Events",
            PanelKind::Progress => "Progress",
            PanelKind::ReferenceMonitor => "Reference Monitor",
            PanelKind::Timecode => "Timecode",
            PanelKind::ProjectNotes => "Project Notes",
        }
    }
    pub fn id(self) -> String {
        format!("{self:?}")
    }
    pub fn from_name(s: &str) -> Option<PanelKind> {
        let n = s.to_ascii_lowercase().replace([' ', '_', '-'], "");
        Self::ALL.iter().copied().find(|p| format!("{p:?}").to_ascii_lowercase() == n || p.title().to_ascii_lowercase().replace(' ', "") == n)
    }
    /// Window-menu shortcut (Premiere: Shift+1..9).
    pub fn window_shortcut(self) -> Option<&'static str> {
        match self {
            PanelKind::Project => Some("Shift+1"),
            PanelKind::Source => Some("Shift+2"),
            PanelKind::Timeline => Some("Shift+3"),
            PanelKind::Program => Some("Shift+4"),
            PanelKind::EffectControls => Some("Shift+5"),
            PanelKind::AudioClipMixer => Some("Shift+9"),
            PanelKind::Effects => Some("Shift+7"),
            PanelKind::MediaBrowser => Some("Shift+8"),
            PanelKind::AudioTrackMixer => Some("Shift+6"),
            _ => None,
        }
    }
    /// Narrow chrome-less panels (no tab strip; a slim grip instead).
    pub fn compact(self) -> bool {
        matches!(self, PanelKind::Tools | PanelKind::AudioMeters)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum SplitSize {
    Ratio(f32),
    /// First child has a fixed size in points.
    FixedA(f32),
    /// Second child has a fixed size in points.
    FixedB(f32),
}

impl SplitSize {
    fn shared(self) -> SharedSplitSize {
        match self {
            Self::Ratio(value) => SharedSplitSize::Ratio(value),
            Self::FixedA(value) => SharedSplitSize::FixedFirst(value),
            Self::FixedB(value) => SharedSplitSize::FixedSecond(value),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DockNode {
    /// `vertical`: children stacked top/bottom; else side by side.
    Split {
        vertical: bool,
        size: SplitSize,
        a: Box<DockNode>,
        b: Box<DockNode>,
    },
    Tabs {
        panels: Vec<PanelKind>,
        active: usize,
    },
}

fn tabs(p: &[PanelKind], active: usize) -> DockNode {
    DockNode::Tabs { panels: p.to_vec(), active }
}
fn hsplit(size: SplitSize, a: DockNode, b: DockNode) -> DockNode {
    DockNode::Split { vertical: false, size, a: Box::new(a), b: Box::new(b) }
}
fn vsplit(size: SplitSize, a: DockNode, b: DockNode) -> DockNode {
    DockNode::Split { vertical: true, size, a: Box::new(a), b: Box::new(b) }
}

pub const WORKSPACES: [&str; 10] =
    ["Editing", "Jianying", "Assembly", "Color", "Effects", "Audio", "Captions and Graphics", "Learning", "Review", "All Panels"];

/// The user's workspaces file, beside `preferences.json` in the data directory.
pub const WORKSPACES_FILE: &str = "workspaces.json";

/// Window ▸ Workspaces: the user's own workspaces and saved changes to the built-in ones, and the
/// workspace in use (reopened at launch).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkspacePrefs {
    pub saved: Vec<SavedWorkspace>,
    pub current: String,
}

/// A named layout. `layout` is a [`DockNode`] kept as JSON, so one unreadable entry (a panel a
/// newer version added) doesn't lose the others.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SavedWorkspace {
    pub name: String,
    pub layout: serde_json::Value,
    /// Kept raw for the same per-workspace failure isolation as the dock tree.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub floating: serde_json::Value,
}

impl WorkspacePrefs {
    pub fn load(path: &std::path::Path) -> Self {
        use std::io::Read as _;
        // Preferences are untrusted; bound reading before JSON can allocate from it.
        const MAX_BYTES: u64 = 4 * 1024 * 1024;
        let Some(bytes) = std::fs::File::open(path).ok().and_then(|file| {
            let mut bytes = Vec::new();
            file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
            (bytes.len() as u64 <= MAX_BYTES).then_some(bytes)
        }) else {
            return Self::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, path)
    }
}

/// A workspace's command-id key: `Captions and Graphics` → `captionsandgraphics`.
pub fn slug(name: &str) -> String {
    name.to_ascii_lowercase().replace(' ', "")
}

pub fn is_builtin(name: &str) -> bool {
    WORKSPACES.contains(&name)
}

/// Every workspace, built-in and the user's own, in one alphabetical list (Premiere's Window ▸
/// Workspaces).
pub fn names(prefs: &WorkspacePrefs) -> Vec<String> {
    let mut v: Vec<String> = WORKSPACES.iter().map(|s| s.to_string()).collect();
    for s in &prefs.saved {
        if !v.contains(&s.name) {
            v.push(s.name.clone());
        }
    }
    v.sort_by_key(|n| n.to_lowercase());
    v
}

/// The workspace called `query` (any case, spaces optional).
pub fn find(prefs: &WorkspacePrefs, query: &str) -> Option<String> {
    names(prefs).into_iter().find(|n| slug(n) == slug(query))
}

/// The saved layout of a workspace: the user's saved version, else the built-in default.
pub fn saved_layout(prefs: &WorkspacePrefs, name: &str) -> DockNode {
    saved_docking(prefs, name).root
}

pub fn saved_docking(prefs: &WorkspacePrefs, name: &str) -> Layout {
    let saved = prefs.saved.iter().find(|saved| saved.name == name).and_then(|saved| {
        let mut layout = Layout {
            root: serde_json::from_value(saved.layout.clone()).ok()?,
            floating: if saved.floating.is_null() { vec![] } else { serde_json::from_value(saved.floating.clone()).ok()? },
        };
        if layout.floating.is_empty() {
            migrate_legacy_effects(&mut layout.root);
        }
        layout.validate().ok().map(|()| layout)
    });
    saved.unwrap_or_else(|| Layout { root: workspace(name), floating: vec![] })
}

/// Older Effects workspaces stored the same Effect Controls panel twice: selected beside
/// Source, and an inactive duplicate beside Essential Graphics. Remove only that known
/// duplicate shape; unrelated malformed or ambiguous saved layouts still fail validation.
fn migrate_legacy_effects(root: &mut DockNode) {
    let Ok(shared) = crate::dock_shared::encode(root, &[]) else { return };
    if shared.panels().iter().filter(|panel| ***panel == PanelKind::EffectControls).count() != 2 {
        return;
    }
    fn selected_source(node: &DockNode) -> bool {
        match node {
            DockNode::Tabs { panels, active } => {
                panels.contains(&PanelKind::Source) && panels.contains(&PanelKind::AudioClipMixer) && panels.get(*active) == Some(&PanelKind::EffectControls)
            }
            DockNode::Split { a, b, .. } => selected_source(a) || selected_source(b),
        }
    }
    fn remove_inactive(node: &mut DockNode) {
        match node {
            DockNode::Tabs { panels, active } if panels.as_slice() == [PanelKind::EffectControls, PanelKind::EssentialGraphics] && *active == 1 => {
                panels.remove(0);
                *active = 0;
            }
            DockNode::Split { a, b, .. } => {
                remove_inactive(a);
                remove_inactive(b);
            }
            DockNode::Tabs { .. } => {}
        }
    }
    if selected_source(root) {
        remove_inactive(root);
    }
}

/// The default layout of a named workspace.
pub fn workspace(name: &str) -> DockNode {
    use PanelKind::*;
    use SplitSize::*;
    let bottom_editing = || hsplit(FixedA(64.0), tabs(&[Tools], 0), hsplit(FixedB(116.0), tabs(&[Timeline], 0), tabs(&[AudioMeters], 0)));
    match name {
        "Assembly" => vsplit(
            Ratio(0.52),
            hsplit(
                Ratio(0.46),
                tabs(&[Project, MediaBrowser, Libraries, Info, Effects, Markers, History], 0),
                hsplit(Ratio(0.5), tabs(&[Source, EffectControls, AudioClipMixer, Metadata], 0), tabs(&[Program], 0)),
            ),
            bottom_editing(),
        ),
        "Color" => hsplit(
            FixedB(340.0),
            vsplit(
                Ratio(0.58),
                hsplit(Ratio(0.42), tabs(&[LumetriScopes, Source, EffectControls], 0), tabs(&[Program], 0)),
                hsplit(Ratio(0.3), tabs(&[Project, Info, Markers, History], 0), bottom_editing()),
            ),
            tabs(&[LumetriColor, Effects], 0),
        ),
        "Effects" => hsplit(
            FixedB(320.0),
            vsplit(
                Ratio(0.5),
                hsplit(Ratio(0.5), tabs(&[Source, EffectControls, AudioClipMixer], 1), tabs(&[Program], 0)),
                hsplit(Ratio(0.3), tabs(&[Project, MediaBrowser, Info, Markers, History], 0), bottom_editing()),
            ),
            vsplit(Ratio(0.5), tabs(&[Effects], 0), tabs(&[EssentialGraphics], 0)),
        ),
        "Audio" => hsplit(
            FixedB(320.0),
            vsplit(
                Ratio(0.5),
                hsplit(Ratio(0.5), tabs(&[AudioTrackMixer, Source, AudioClipMixer, EffectControls], 0), tabs(&[Program], 0)),
                hsplit(Ratio(0.3), tabs(&[Project, MediaBrowser, Effects, Markers, History], 0), bottom_editing()),
            ),
            tabs(&[EssentialSound, TextToSpeech], 0),
        ),
        "Captions and Graphics" => hsplit(
            FixedB(330.0),
            vsplit(
                Ratio(0.5),
                hsplit(Ratio(0.4), tabs(&[Text, Source, EffectControls], 0), tabs(&[Program], 0)),
                hsplit(Ratio(0.3), tabs(&[Project, MediaBrowser, Libraries], 0), bottom_editing()),
            ),
            tabs(&[EssentialGraphics, Properties], 0),
        ),
        "All Panels" => vsplit(
            Ratio(0.5),
            hsplit(
                Ratio(0.5),
                tabs(&[Source, EffectControls, AudioClipMixer, Metadata, LumetriScopes, Text], 0),
                hsplit(Ratio(0.6), tabs(&[Program], 0), tabs(&[LumetriColor, EssentialGraphics, EssentialSound, TextToSpeech, Properties, AudioTrackMixer], 0)),
            ),
            hsplit(Ratio(0.26), tabs(&[Project, MediaBrowser, Libraries, Info, Effects, Markers, History], 0), bottom_editing()),
        ),
        // JianyingCraft 的剪映式四区：左素材 / 中预览 / 右属性+关键帧 / 底时间轴。
        // 作为可选工作区提供，不改变上游默认布局的行为契约。
        "Jianying" => vsplit(
            Ratio(0.62),
            hsplit(
                FixedA(320.0),
                tabs(&[Project, MediaBrowser, Libraries, Info], 0),
                hsplit(FixedB(340.0), tabs(&[Program], 0), tabs(&[Properties, EffectControls, EssentialGraphics, Text], 0)),
            ),
            tabs(&[Timeline, AudioMeters], 0),
        ),
        // Editing (default), Learning, Review — Premiere 26 factory layout:
        // top: Source group | Program | Properties; bottom: Project group | Tools | Timeline | Meters.
        _ => vsplit(
            Ratio(0.57),
            hsplit(
                FixedB(450.0),
                hsplit(Ratio(0.5), tabs(&[Source, EffectControls, AudioClipMixer, Metadata], 0), tabs(&[Program], 0)),
                tabs(&[Properties, EssentialGraphics, Text], 0),
            ),
            hsplit(
                Ratio(0.29),
                tabs(&[Project, MediaBrowser, Libraries, Info, Effects, Markers, History], 0),
                hsplit(FixedA(64.0), tabs(&[Tools], 0), hsplit(FixedB(116.0), tabs(&[Timeline], 0), tabs(&[AudioMeters], 0))),
            ),
        ),
    }
}

impl DockNode {
    /// Every panel in the tree.
    pub fn panels(&self, out: &mut Vec<PanelKind>) {
        match self {
            DockNode::Split { a, b, .. } => {
                a.panels(out);
                b.panels(out);
            }
            DockNode::Tabs { panels, .. } => out.extend(panels.iter().copied()),
        }
    }
    pub fn contains(&self, p: PanelKind) -> bool {
        let mut v = Vec::new();
        self.panels(&mut v);
        v.contains(&p)
    }
    /// Make `p` the active tab of its group. Returns false if not present.
    pub fn activate(&mut self, p: PanelKind) -> bool {
        crate::dock_shared::apply_tree(self, craft_ui::docking::Action::Activate { panel: p })
    }
    pub fn is_visible(&self, p: PanelKind) -> bool {
        match self {
            DockNode::Split { a, b, .. } => a.is_visible(p) || b.is_visible(p),
            DockNode::Tabs { panels, active } => panels.get(*active) == Some(&p),
        }
    }
    /// Close a panel (remove its tab; empty groups collapse their split).
    pub fn close(&mut self, p: PanelKind) {
        crate::dock_shared::apply_tree(self, craft_ui::docking::Action::Close { panel: p });
    }
    /// Put the Timeline panel back where the workspaces keep it (left of the audio meters) after
    /// it was closed, so opening a sequence always has somewhere to show it.
    pub fn restore_timeline(&mut self) {
        if self.contains(PanelKind::Timeline) {
            return;
        }
        fn beside_meters(n: &mut DockNode) -> bool {
            match n {
                DockNode::Split { a, b, .. } => beside_meters(a) || beside_meters(b),
                DockNode::Tabs { panels, .. } if panels.as_slice() == [PanelKind::AudioMeters] => {
                    *n = hsplit(SplitSize::FixedB(116.0), tabs(&[PanelKind::Timeline], 0), tabs(&[PanelKind::AudioMeters], 0));
                    true
                }
                DockNode::Tabs { .. } => false,
            }
        }
        if !beside_meters(self) {
            self.open_near(PanelKind::Timeline, PanelKind::Project);
        }
    }
    /// Add a panel as a tab next to `near`, reporting a placement failure instead of dropping it.
    ///
    /// 监视器（`Program`）所在的 tab 组不接受别的面板：把它当锚点插 Tab 会把预览盖住（剪映式
    /// 「滤镜 / 调节」Tab 打开 Lumetri Color 时踩过）。此时改放到最近的**相邻组**；Program 独处、
    /// 没有相邻组时在它旁边拆出一组并排，Program 仍留在左半边。
    ///
    /// **锚点必须是已停靠的面板。** `near` 可能正在浮动：浮动组不在这棵树里，把它当锚点去
    /// `Split` 会被共享 docking 模型判成 `InvalidPlacement`（浮动组只接受 `Tab` / `Center`）。
    /// 这里先把「树里真正存在的面板」收集出来再选锚点，浮动面板自然被排除；一棵空树则走
    /// `Open { anchor: None }` 建出第一组。失败原因返回给调用方，不再像 [`Self::apply_tree`]
    /// 的 `bool` 那样被丢弃。
    pub fn try_open_near(&mut self, p: PanelKind, near: PanelKind) -> Result<(), String> {
        if crate::dock_shared::encode(self, &[]).is_err() {
            return Err("the workspace dock tree cannot be encoded".into());
        }
        if self.contains(p) {
            return if self.activate(p) { Ok(()) } else { Err(format!("{} is docked but its tab could not be activated", p.title())) };
        }
        let mut docked = Vec::new();
        self.panels(&mut docked);
        let Some(anchor) = [near, PanelKind::Project, PanelKind::Program].into_iter().find(|panel| docked.contains(panel)).or_else(|| docked.first().copied())
        else {
            // 没有已停靠组：浮动面板不能当锚点，交给 Open 建出第一组。
            return if crate::dock_shared::apply_tree(self, craft_ui::docking::Action::Open { panel: p, anchor: None }) {
                Ok(())
            } else {
                Err(format!("could not open {} in an empty workspace", p.title()))
            };
        };
        let opened = match self.monitor_safe_anchor(anchor) {
            Some(anchor) => crate::dock_shared::apply_tree(self, craft_ui::docking::Action::Open { panel: p, anchor: Some(anchor) }),
            // Program 单独成组：拆成并排的两组，预览留在原来那一半。
            None => {
                let placement = craft_ui::docking::Placement::Split(craft_ui::docking::Zone::Right);
                crate::dock_shared::apply_tree(self, craft_ui::docking::Action::OpenAt { panel: p, anchor, placement })
            }
        };
        if opened { Ok(()) } else { Err(format!("the docking model rejected placing {} beside {}", p.title(), anchor.title())) }
    }

    /// Add a panel as a tab next to `near` (or into the first group).
    ///
    /// 兼容入口：调用 [`Self::try_open_near`]；万一共享模型拒绝，就把面板直接放进第一个已停靠组
    /// （空树则新建一组），而不是像旧代码那样把 `false` 丢掉、让点击「滤镜」毫无反应。
    pub fn open_near(&mut self, p: PanelKind, near: PanelKind) {
        if let Err(error) = self.try_open_near(p, near) {
            log::warn!("{error}; opening {} in the first docked group", p.title());
            self.open_in_first_group(p);
        }
    }

    /// 最后兜底：把 `p` 放进树里的第一个 tab 组；没有组就新建一个。不会失败。
    fn open_in_first_group(&mut self, p: PanelKind) {
        if self.contains(p) {
            self.activate(p);
            return;
        }
        fn first_group(node: &mut DockNode) -> Option<&mut DockNode> {
            match node {
                DockNode::Tabs { .. } => Some(node),
                DockNode::Split { a, b, .. } => first_group(a).or_else(|| first_group(b)),
            }
        }
        match first_group(self) {
            Some(DockNode::Tabs { panels, active }) => {
                panels.push(p);
                *active = panels.len().saturating_sub(1);
            }
            _ => *self = tabs(&[p], 0),
        }
    }

    /// 为 `anchor` 找一个**不会盖住 Program** 的落点：锚点所在组不含 Program 时原样返回；
    /// 含 Program 时沿路径向上找最近的、含其它组的兄弟子树，取其中第一个面板。
    /// `None` 表示 Program 组没有兄弟（整棵树只有它），调用方应改用并排拆分。
    fn monitor_safe_anchor(&self, anchor: PanelKind) -> Option<PanelKind> {
        let path = self.group_path(anchor)?;
        match node_at(self, &path)? {
            DockNode::Tabs { panels, .. } if !panels.contains(&PanelKind::Program) => Some(anchor),
            DockNode::Tabs { .. } => {
                for level in (0..path.len()).rev() {
                    let mut sibling = path[..level].to_vec();
                    sibling.push(!path[level]);
                    let Some(node) = node_at(self, &sibling) else { continue };
                    let mut panels = Vec::new();
                    node.panels(&mut panels);
                    if let Some(first) = panels.first().copied() {
                        return Some(first);
                    }
                }
                None
            }
            _ => Some(anchor),
        }
    }

    /// 从根到含 `panel` 的 tab 组的路径（`true` = 第二个子节点）；深度由工作区模型限制。
    fn group_path(&self, panel: PanelKind) -> Option<Vec<bool>> {
        fn walk(node: &DockNode, panel: PanelKind, out: &mut Vec<bool>) -> bool {
            match node {
                DockNode::Tabs { panels, .. } => panels.contains(&panel),
                DockNode::Split { a, b, .. } => {
                    out.push(false);
                    if walk(a, panel, out) {
                        return true;
                    }
                    out.pop();
                    out.push(true);
                    if walk(b, panel, out) {
                        return true;
                    }
                    out.pop();
                    false
                }
            }
        }
        let mut path = Vec::new();
        walk(self, panel, &mut path).then_some(path)
    }
}

/// A floating group retains the same panel identities as docked tabs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Floating {
    pub panels: Vec<PanelKind>,
    pub active: usize,
    /// [x, y, width, height] in logical points.
    pub rect: [f32; 4],
}

/// App persistence shape; all mutations pass through the shared docking model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub root: DockNode,
    #[serde(default)]
    pub floating: Vec<Floating>,
}

impl Layout {
    pub fn apply(&mut self, action: craft_ui::docking::Action<PanelKind>) -> Result<(), String> {
        crate::dock_shared::apply(self, action)
    }
    pub fn validate(&self) -> Result<(), String> {
        crate::dock_shared::encode(&self.root, &self.floating)?.validate().map_err(|error| error.to_string())
    }
    pub fn contains(&self, panel: PanelKind) -> bool {
        self.root.contains(panel) || self.floating.iter().any(|group| group.panels.contains(&panel))
    }
}

/// One laid-out tab group.
pub struct Group {
    pub path: String,
    pub rect: Rect,
    pub content: Rect,
    pub panels: Vec<PanelKind>,
    pub active: usize,
}

/// Actions produced by interacting with the dock chrome.
#[derive(Debug, Clone, PartialEq)]
pub enum DockAction {
    Activate(PanelKind),
    BeginDrag(PanelKind),
    Focus(PanelKind),
    Close(PanelKind),
    PanelMenu(PanelKind, egui::Pos2),
    /// A sequence tab of the Timeline panel was clicked / closed (project item id).
    OpenSequence(u64),
    CloseSequence(u64),
    /// A sequence tab was dragged past a neighbour: its new place among the sequence tabs.
    MoveSequence(u64, usize),
}

/// Lay out the tree into group rects (with `gap` gutters) and handle gutter dragging.
pub fn layout(ui: &mut egui::Ui, node: &mut DockNode, rect: Rect, t: &Tokens, path: &str, out: &mut Vec<Group>, reg: &mut crate::automation::Registry) {
    let mut actions = Vec::new();
    layout_with_actions(ui, node, rect, t, path, out, reg, &mut actions);
    for action in actions {
        crate::dock_shared::apply_tree(node, action);
    }
}

/// Lay out chrome while returning mutations for the application's UI command route.
pub fn layout_with_actions(
    ui: &mut egui::Ui,
    node: &mut DockNode,
    rect: Rect,
    t: &Tokens,
    path: &str,
    out: &mut Vec<Group>,
    reg: &mut crate::automation::Registry,
    actions: &mut Vec<craft_ui::docking::Action<PanelKind>>,
) {
    layout_inner(ui, node, rect, t, path, &[], out, reg, actions);
}

fn layout_inner(
    ui: &mut egui::Ui,
    node: &mut DockNode,
    rect: Rect,
    t: &Tokens,
    path: &str,
    tree_path: &[bool],
    out: &mut Vec<Group>,
    reg: &mut crate::automation::Registry,
    actions: &mut Vec<craft_ui::docking::Action<PanelKind>>,
) {
    match node {
        DockNode::Tabs { panels, active } => {
            let tab_h = if panels.len() == 1 && panels[0].compact() { 8.0 } else { t.tab_h };
            let content = Rect::from_min_max(pos2(rect.min.x, rect.min.y + tab_h), rect.max);
            out.push(Group { path: path.to_string(), rect, content, panels: panels.clone(), active: *active });
        }
        DockNode::Split { vertical, size, a, b } => {
            let axis = if *vertical { SplitAxis::Vertical } else { SplitAxis::Horizontal };
            let Some(split) = split_rect(rect, axis, size.shared(), t.gap, 20.0) else { return };
            let id = egui::Id::new(("dock-gutter", path.to_string()));
            let handle_style = ResizeHandleStyle { hovered: t.focus.gamma_multiply(0.4), dragged: t.focus.gamma_multiply(0.9), focused: t.focus };
            let label = if *vertical { tl!("Height:") } else { tl!("Width:") };
            let response = ResizeHandle::new(id, axis, split.divider, label, handle_style).value(split.first_size(), 0.0..=split.available()).show(ui);
            reg.add_response(&format!("dock.gutter.{path}"), &response.response, "gutter");
            if response.delta != 0.0
                && let Some(resized) = split.resized(size.shared(), response.delta, 40.0)
            {
                actions.push(craft_ui::docking::Action::ResizeSplit { path: tree_path.to_vec(), size: resized });
            }
            let mut first_path = tree_path.to_vec();
            first_path.push(false);
            let mut second_path = tree_path.to_vec();
            second_path.push(true);
            layout_inner(ui, a, split.first, t, &format!("{path}a"), &first_path, out, reg, actions);
            layout_inner(ui, b, split.second, t, &format!("{path}b"), &second_path, out, reg, actions);
        }
    }
}

/// Draw a group's frame + tab strip. Returns actions (tab clicks, panel menu, focus).
/// The sequences open in the Timeline panel, in tab order: `(project item id, name)`.
#[derive(Debug, Clone, Default)]
pub struct SeqTabs {
    pub open: Vec<(u64, String)>,
    pub active: Option<u64>,
}

pub fn draw_group_chrome(
    ui: &mut egui::Ui,
    g: &Group,
    focused: PanelKind,
    seqs: &SeqTabs,
    t: &Tokens,
    reg: &mut crate::automation::Registry,
) -> Vec<DockAction> {
    let chrome = draw_group_chrome_details(ui, g, seqs, t, reg);
    paint_group_focus(ui, g, focused, t);
    chrome.actions
}

#[derive(Default)]
pub struct Chrome {
    pub actions: Vec<DockAction>,
    pub tabs: Vec<(PanelKind, Rect)>,
}

pub fn draw_group_chrome_details(ui: &mut egui::Ui, g: &Group, seqs: &SeqTabs, t: &Tokens, reg: &mut crate::automation::Registry) -> Chrome {
    draw_group_chrome_details_with(ui, g, seqs, t, reg, false)
}

/// [`draw_group_chrome_details`]，外加「剪映工作区」开关。
///
/// `jianying` 为真且该组含 [`PanelKind::Timeline`] 时不画 Premiere 的页签条，改画一条 26 pt 的
/// 序列标题行（见 [`draw_jianying_timeline_header`]）；其余组、其余工作区一律照旧，页签条原样保留。
pub fn draw_group_chrome_details_with(
    ui: &mut egui::Ui,
    g: &Group,
    seqs: &SeqTabs,
    t: &Tokens,
    reg: &mut crate::automation::Registry,
    jianying: bool,
) -> Chrome {
    let mut actions = Vec::new();
    let mut tab_rects: Vec<(PanelKind, Rect)> = Vec::new();
    let painter = ui.painter().clone();
    painter.rect_filled(g.rect, t.radius, t.panel_bg);
    // 剪映专业版的时间轴组没有页签条：换成轻量标题行，序列切换改由标题行右端的 ▾ 承担。
    if jianying && g.panels.contains(&PanelKind::Timeline) {
        return draw_jianying_timeline_header(ui, g, seqs, t, reg);
    }
    let active_panel = g.panels.get(g.active).copied();
    let compact = g.panels.len() == 1 && g.panels[0].compact();
    // A narrow header grip moves the entire panel, including Timeline panels whose tabs
    // separately reorder open sequences.
    if let Some(panel) = active_panel {
        let grip = Rect::from_min_size(g.rect.min, vec2(if compact { g.rect.width() } else { 10.0 }, if compact { 8.0 } else { t.tab_h }));
        let response = ui.interact(grip, egui::Id::new(("panel-drag", g.path.clone())), Sense::drag());
        reg.add_response(&format!("panel.drag.{}", panel.id()), &response, "Move panel");
        if response.drag_started() {
            actions.push(DockAction::BeginDrag(panel));
        }
    }
    if compact {
        // grip dots
        let c = pos2(g.rect.center().x, g.rect.min.y + 4.0);
        for dx in [-4.0, 0.0, 4.0] {
            painter.circle_filled(c + vec2(dx, 0.0), 1.0, t.text_faint);
        }
    } else {
        let strip = Rect::from_min_size(g.rect.min, vec2(g.rect.width(), t.tab_h));
        // the Timeline panel shows one tab per open sequence (its own tab names the sequence)
        struct Tab {
            panel: PanelKind,
            seq: Option<u64>,
            title: String,
            active: bool,
        }
        let mut tabs: Vec<Tab> = Vec::new();
        for (i, p) in g.panels.iter().enumerate() {
            let active = i == g.active;
            if *p == PanelKind::Timeline && active && !seqs.open.is_empty() {
                tabs.extend(seqs.open.iter().map(|(id, name)| Tab { panel: *p, seq: Some(*id), title: name.clone(), active: seqs.active == Some(*id) }));
            } else if *p == PanelKind::Timeline {
                let name = seqs.open.iter().find(|(id, _)| seqs.active == Some(*id)).map(|(_, n)| n.clone());
                tabs.push(Tab { panel: *p, seq: None, title: name.unwrap_or_else(|| tl!("Timeline: (no sequences)").into()), active });
            } else {
                tabs.push(Tab { panel: *p, seq: None, title: crate::i18n::t(p.title()).to_string(), active });
            }
        }
        let activate = |tab: &Tab, actions: &mut Vec<DockAction>| match tab.seq {
            Some(id) => actions.push(DockAction::OpenSequence(id)),
            None => actions.push(DockAction::Activate(tab.panel)),
        };
        let active_at = tabs.iter().position(|tab| tab.active).unwrap_or(0);
        let text_y = strip.min.y + 16.0;
        // Which tabs fit. The active tab is always shown: tabs before it are left out until it
        // fits, then as many after it as there is room for. Tabs that are left out are reached
        // through the » list at the right end of the strip.
        let widths: Vec<f32> = tabs
            .iter()
            .map(|tab| {
                let label = painter.layout_no_wrap(tab.title.clone(), Tokens::ui(12.0), t.tab_text).size().x;
                label + 16.0 + if tab.active { 20.0 } else { 0.0 } + if tab.active && tab.seq.is_some() { 16.0 } else { 0.0 }
            })
            .collect();
        let span = |from: usize, to: usize| widths.get(from..=to).map_or(0.0, |w| w.iter().sum::<f32>() + 8.0 * w.len() as f32);
        let room = strip.width() - 12.0;
        let all_fit = span(0, tabs.len().saturating_sub(1)) <= room;
        let room = if all_fit { room } else { room - 24.0 };
        let mut first = 0;
        while first < active_at && span(first, active_at) > room {
            first += 1;
        }
        let mut last = active_at.max(first);
        while last + 1 < tabs.len() && span(first, last + 1) <= room {
            last += 1;
        }
        let list_id = egui::Id::new(("tab-list", g.path.clone()));
        let more = (!all_fit).then(|| Rect::from_min_size(pos2(strip.max.x - 22.0, strip.min.y + 6.0), vec2(18.0, 20.0)));
        let mut list_opened = false;
        if let Some(r) = more {
            let resp = ui.interact(r, egui::Id::new(("tab-overflow", g.path.clone())), Sense::click());
            let name = if g.panels.contains(&PanelKind::Timeline) { "timeline.tabs.more".to_string() } else { format!("panel.tabs.more.{}", g.path) };
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, resp.enabled(), "more tabs"));
            reg.add_response(&name, &resp, "more tabs");
            let c = if resp.hovered() { t.tab_text_active } else { t.tab_text };
            icons::paint(&painter, r.shrink(4.0).translate(vec2(-2.0, 0.0)), Icon::ChevronRight, c);
            icons::paint(&painter, r.shrink(4.0).translate(vec2(2.0, 0.0)), Icon::ChevronRight, c);
            paint_widget_focus(&painter, &resp, t);
            if resp.clicked() {
                let open = ui.ctx().data(|d| d.get_temp::<bool>(list_id)).unwrap_or(false);
                ui.ctx().data_mut(|d| d.insert_temp(list_id, !open));
                list_opened = !open;
            }
        }
        // The wheel over the strip steps through the tabs: down = next, up = previous, one tab
        // per wheel notch (trackpad scrolling adds up to a notch first), stopping at the ends.
        let wheel_id = egui::Id::new(("tab-wheel", g.path.clone()));
        if ui.rect_contains_pointer(strip) && !tabs.is_empty() {
            const NOTCH: f32 = 40.0;
            let total = ui.ctx().data(|d| d.get_temp::<f32>(wheel_id)).unwrap_or(0.0) + crate::panels::timeline::wheel_input(ui.ctx()).delta.y;
            let steps = (total / NOTCH).trunc();
            ui.ctx().data_mut(|d| d.insert_temp(wheel_id, total - steps * NOTCH));
            let to = (active_at as i64).saturating_sub(steps as i64).clamp(0, tabs.len() as i64 - 1) as usize;
            if let Some(tab) = tabs.get(to).filter(|_| to != active_at) {
                activate(tab, &mut actions);
                actions.push(DockAction::Focus(tab.panel));
            }
        } else {
            ui.ctx().data_mut(|d| d.remove::<f32>(wheel_id));
        }
        let mut x = strip.min.x + 12.0;
        // sequence tabs as drawn (for dragging one past its neighbours), and the one being dragged
        let mut seq_rects: Vec<(u64, Rect)> = Vec::new();
        let mut dragged: Option<u64> = None;
        for (i, tab) in tabs.iter().enumerate().take(last + 1).skip(first) {
            let p = &tab.panel;
            let is_active = tab.active;
            let galley = painter.layout_no_wrap(tab.title.clone(), Tokens::ui(12.0), if is_active { t.tab_text_active } else { t.tab_text });
            let close_w = if is_active && tab.seq.is_some() { 16.0 } else { 0.0 };
            let w = widths.get(i).copied().unwrap_or(0.0);
            let tab_rect = Rect::from_min_size(pos2(x, strip.min.y), vec2(w, t.tab_h));
            // a sequence tab keeps its identity when the tabs are reordered under the pointer
            let resp = match tab.seq {
                Some(id) => ui.interact(tab_rect, egui::Id::new(("tab-seq", g.path.clone(), id)), Sense::click_and_drag()),
                None => ui.interact(tab_rect, egui::Id::new(("tab", g.path.clone(), i)), Sense::click_and_drag()),
            };
            if tab.seq.is_none() && resp.drag_started() {
                actions.push(DockAction::BeginDrag(*p));
            }
            if let Some((_, rect)) = tab_rects.iter_mut().find(|(panel, _)| panel == p) {
                *rect = rect.union(tab_rect);
            } else {
                tab_rects.push((*p, tab_rect));
            }
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, resp.enabled(), is_active, &tab.title));
            ui.ctx().accesskit_node_builder(resp.id, |node| {
                node.set_role(egui::accesskit::Role::Tab);
                node.set_selected(is_active);
            });
            paint_widget_focus(&painter, &resp, t);
            if let Some(id) = tab.seq {
                seq_rects.push((id, tab_rect));
                if resp.dragged() {
                    dragged = Some(id);
                }
                // Escape puts a dragged tab back where it was (egui ends the drag on Escape, #580)
                let home = egui::Id::new(("tab-seq-home", id));
                if resp.drag_started()
                    && let Some(at) = seqs.open.iter().position(|(s, _)| *s == id)
                {
                    ui.data_mut(|d| d.insert_temp(home, at));
                }
                if resp.drag_stopped()
                    && let Some(at) = ui.data_mut(|d| d.remove_temp::<usize>(home))
                    && ui.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    actions.push(DockAction::MoveSequence(id, at));
                }
            }
            if let Some(id) = tab.seq {
                reg.add_response(&format!("timeline.tab.{id}"), &resp, &tab.title);
            }
            if tab.seq.is_none() || is_active {
                reg.add_response(&format!("panel.tab.{}", p.id()), &resp, p.title());
            }
            let label_x = tab_rect.min.x + 8.0 + close_w;
            let label_w = galley.size().x;
            let col = if is_active || resp.hovered() { t.tab_text_active } else { t.tab_text };
            painter.galley_with_override_text_color(pos2(label_x, text_y - galley.size().y / 2.0), galley, col);
            let mut closed = false;
            if let Some(id) = tab.seq.filter(|_| is_active) {
                // × closes the sequence's tab (the sequence stays in the project)
                let cr = Rect::from_center_size(pos2(tab_rect.min.x + 10.0, text_y), vec2(8.0, 8.0));
                let cresp = ui.interact(cr.expand(4.0), egui::Id::new(("tab-close", g.path.clone(), id)), Sense::click());
                cresp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, cresp.enabled(), "close sequence"));
                reg.add_response(&format!("timeline.tab.{id}.close"), &cresp, "close sequence");
                let cc = if cresp.hovered() { t.tab_text_active } else { t.tab_text };
                painter.line_segment([cr.left_top(), cr.right_bottom()], Stroke::new(1.2, cc));
                painter.line_segment([cr.right_top(), cr.left_bottom()], Stroke::new(1.2, cc));
                paint_widget_focus(&painter, &cresp, t);
                if cresp.clicked() {
                    actions.push(DockAction::CloseSequence(id));
                    closed = true;
                }
            }
            if is_active {
                let mr = Rect::from_center_size(pos2(label_x + label_w + 12.0, text_y), vec2(12.0, 10.0));
                let mresp = ui.interact(mr.expand(3.0), egui::Id::new(("tab-menu", g.path.clone())), Sense::click());
                mresp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, mresp.enabled(), "panel menu"));
                reg.add_response(&format!("panel.menu.{}", p.id()), &mresp, "panel menu");
                let mc = if mresp.hovered() { t.tab_text_active } else { t.tab_text };
                icons::paint(&painter, Rect::from_center_size(mr.center(), vec2(16.0, 16.0)), Icon::Hamburger, mc);
                // 1 pt underline spanning label + ≡, 23 pt below the frame top
                let uy = strip.min.y + 23.0;
                painter.line_segment([pos2(label_x, uy), pos2(mr.max.x, uy)], Stroke::new(1.0, t.tab_text_active));
                paint_widget_focus(&painter, &mresp, t);
                if mresp.clicked() {
                    actions.push(DockAction::PanelMenu(*p, mr.left_bottom()));
                }
            }
            if (resp.clicked() || resp.drag_started()) && !closed {
                activate(tab, &mut actions);
                actions.push(DockAction::Focus(*p));
            }
            // a right-click shows the tab and opens its panel menu (as in Premiere)
            if resp.secondary_clicked() {
                activate(tab, &mut actions);
                actions.push(DockAction::Focus(*p));
                let at = resp.interact_pointer_pos().unwrap_or(tab_rect.left_bottom());
                actions.push(DockAction::PanelMenu(*p, at));
            }
            if resp.middle_clicked() {
                // a sequence tab closes its sequence; the Timeline panel itself stays
                match tab.seq.or(seqs.active.filter(|_| *p == PanelKind::Timeline)) {
                    Some(id) => actions.push(DockAction::CloseSequence(id)),
                    None => actions.push(DockAction::Close(*p)),
                }
            }
            x += w + 8.0;
        }
        // Dragging a sequence tab sideways: it changes places with a neighbour when the pointer
        // passes that neighbour's middle.
        if let Some(id) = dragged
            && let Some(px) = ui.input(|i| i.pointer.latest_pos()).map(|p| p.x)
            && let Some(at) = seq_rects.iter().position(|(s, _)| *s == id)
            && let Some(index) = seqs.open.iter().position(|(s, _)| *s == id)
        {
            let before = at.checked_sub(1).and_then(|k| seq_rects.get(k));
            let after = seq_rects.get(at + 1);
            if before.is_some_and(|(_, r)| px < r.center().x) {
                actions.push(DockAction::MoveSequence(id, index.saturating_sub(1)));
            } else if after.is_some_and(|(_, r)| px > r.center().x) {
                actions.push(DockAction::MoveSequence(id, index + 1));
            }
        }
        // the » list: every tab of the group, the shown one ticked
        let list_open = ui.ctx().data(|d| d.get_temp::<bool>(list_id)).unwrap_or(false);
        if let Some(r) = more.filter(|_| list_open) {
            let mut picked = None;
            let area = egui::Area::new(list_id.with("area")).order(egui::Order::Foreground).pivot(Align2::RIGHT_TOP).fixed_pos(r.right_bottom()).show(
                ui.ctx(),
                |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(160.0);
                        // many open sequences: the list scrolls instead of leaving the window
                        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                            for tab in &tabs {
                                let label = if tab.active { format!("✓ {}", tab.title) } else { tab.title.clone() };
                                let resp = ui.button(label);
                                match tab.seq {
                                    Some(id) => reg.add_response(&format!("timeline.tabs.list.{id}"), &resp, &tab.title),
                                    None => reg.add_response(&format!("panel.tabs.list.{}", tab.panel.id()), &resp, &tab.title),
                                }
                                if resp.clicked() {
                                    picked = Some(tab);
                                }
                            }
                        });
                    });
                },
            );
            if let Some(tab) = picked {
                activate(tab, &mut actions);
                actions.push(DockAction::Focus(tab.panel));
            }
            if picked.is_some() || (!list_opened && area.response.clicked_elsewhere()) {
                ui.ctx().data_mut(|d| d.remove::<bool>(list_id));
            }
        } else if list_open {
            ui.ctx().data_mut(|d| d.remove::<bool>(list_id));
        }
    }
    // Activating presses focus their tab immediately, rather than the group's old active panel.
    // Otherwise entering an inspector can briefly reactivate a stale Project/Timeline selection.
    // Middle-click closes a tab, so it keeps the active panel as the focus target.
    if let Some(p) = active_panel
        && ui.rect_contains_pointer(g.rect)
        && ui.input(|i| i.pointer.any_pressed())
    {
        let panel = ui
            .input(|i| i.pointer.interact_pos().filter(|_| i.pointer.primary_pressed() || i.pointer.secondary_pressed()))
            .and_then(|pos| tab_rects.iter().find(|(_, rect)| rect.contains(pos)).map(|(panel, _)| *panel))
            .unwrap_or(p);
        actions.push(DockAction::Focus(panel));
    }
    Chrome { actions, tabs: tab_rects }
}

/// 剪映专业版时间轴组标题行的高度。
///
/// 对照截图：时间轴上沿没有 Premiere 的 32 pt 页签条，只有一条贴边的轻量头。这里取 26 pt——
/// 比页签条矮 6 pt，视觉上退成「标题」而不是「页签」。
pub const JIANYING_TIMELINE_HEADER_H: f32 = 26.0;

/// 把标题按字符边界收缩到 `max_w` 以内，末尾补省略号。
///
/// 序列名是用户数据（可以任意长）：直接画会溢出到右侧按钮上，按字节切又会切坏 UTF-8，所以按
/// `char` 逐个回退并实测宽度。文本很短，代价可以忽略。
fn fit_title(painter: &egui::Painter, text: &str, font: egui::FontId, max_w: f32) -> String {
    let width = |s: &str| painter.layout_no_wrap(s.to_owned(), font.clone(), Color32::WHITE).size().x;
    if max_w <= 0.0 {
        return String::new();
    }
    if width(text) <= max_w {
        return text.to_owned();
    }
    let mut out = text.to_owned();
    while !out.is_empty() {
        // `pop` 按 `char` 去掉一个完整字符，不会切在 UTF-8 中间。
        out.pop();
        let candidate = format!("{out}…");
        if width(&candidate) <= max_w {
            return candidate;
        }
    }
    String::new()
}

/// 剪映专业版时间轴组的轻量标题行（26 pt），取代 Premiere 的页签条。
///
/// 左边是**序列名**（semibold 12、白色），紧跟一个 ▾；点它弹出「打开中的序列」列表，选一条就走
/// [`DockAction::OpenSequence`]，与原来点序列页签等价。组里还有 Audio Meters 时，右端再放一个
/// 「音频电平表」文字按钮：点它切到电平表，电平表已经显示时点它回到时间轴。
///
/// 页签虽然不画，自动化契约不缩水：`panel.tab.Timeline`、`panel.tab.AudioMeters`、
/// `panel.menu.Timeline`、`panel.menu.AudioMeters`、`panel.drag.<活动面板>` 照旧登记；序列列表的
/// 每一项是 `timeline.seq.item.<id>`，▾ 本身是 `timeline.seq.pick`。
fn draw_jianying_timeline_header(ui: &mut egui::Ui, g: &Group, seqs: &SeqTabs, t: &Tokens, reg: &mut crate::automation::Registry) -> Chrome {
    let mut actions = Vec::new();
    let mut tab_rects: Vec<(PanelKind, Rect)> = Vec::new();
    let painter = ui.painter().clone();
    let Some(active) = g.panels.get(g.active).copied() else {
        return Chrome { actions, tabs: tab_rects };
    };
    // dock 按 `t.tab_h` 预留了页签条的高度；这里只用其中的 26 pt，余下的留作时间轴上方的呼吸位。
    let reserved = (g.content.min.y - g.rect.min.y).max(0.0);
    let row_h = JIANYING_TIMELINE_HEADER_H.min(reserved);
    // 组太矮 / 太窄时不画标题行：宁可少一条装饰，也不把时间轴压没。
    if row_h < 14.0 || g.rect.width() < 64.0 {
        return Chrome { actions, tabs: tab_rects };
    }
    let strip = Rect::from_min_size(g.rect.min, vec2(g.rect.width(), row_h));
    painter.rect_filled(strip, 0.0, t.tl_header_bg);
    painter.line_segment([strip.left_bottom(), strip.right_bottom()], Stroke::new(1.0, t.text_dim.gamma_multiply(0.22)));

    // 左端 10 pt 拖拽手柄：整组移动 / 拖出浮动，行为与原页签条一致。
    let grip = Rect::from_min_size(strip.min, vec2(10.0_f32.min(strip.width()), row_h));
    let grip_response = ui.interact(grip, egui::Id::new(("panel-drag", g.path.clone())), Sense::drag());
    reg.add_response(&format!("panel.drag.{}", active.id()), &grip_response, "Move panel");
    if grip_response.drag_started() {
        actions.push(DockAction::BeginDrag(active));
    }

    // 右端从右往左排：「音频电平表」文字按钮（组里还有电平表时才有）。
    let mut right = strip.max.x - 6.0;
    let mut meters_response: Option<egui::Response> = None;
    if g.panels.contains(&PanelKind::AudioMeters) && row_h >= 20.0 && strip.width() > 150.0 {
        let meters_active = active == PanelKind::AudioMeters;
        let label = crate::i18n::t(PanelKind::AudioMeters.title()).to_string();
        let color = if meters_active { t.text } else { t.text_dim };
        let galley = painter.layout_no_wrap(label.clone(), Tokens::ui(11.5), color);
        let galley_size = galley.size();
        let w = (galley_size.x + 14.0).min(strip.width() * 0.4);
        let rect = Rect::from_min_size(pos2(right - w, strip.center().y - 9.0), vec2(w, 18.0));
        right = rect.min.x - 6.0;
        let response = ui.interact(rect, egui::Id::new(("jy-meters", g.path.clone())), Sense::click());
        response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, meters_active, label.as_str()));
        reg.add_response(&format!("panel.tab.{}", PanelKind::AudioMeters.id()), &response, PanelKind::AudioMeters.title());
        if response.hovered() || meters_active {
            painter.rect_filled(rect, 4.0, t.hover);
        }
        painter.galley(pos2(rect.center().x - galley_size.x / 2.0, rect.center().y - galley_size.y / 2.0), galley, color);
        paint_widget_focus(&painter, &response, t);
        tab_rects.push((PanelKind::AudioMeters, rect));
        if response.clicked() {
            // 已经停在电平表上时，这个按钮就是「回到时间轴」。
            let target = if meters_active { PanelKind::Timeline } else { PanelKind::AudioMeters };
            actions.push(DockAction::Activate(target));
            actions.push(DockAction::Focus(target));
        }
        if response.secondary_clicked() {
            let at = response.interact_pointer_pos().unwrap_or(rect.left_bottom());
            actions.push(DockAction::PanelMenu(PanelKind::AudioMeters, at));
        }
        meters_response = Some(response);
    }

    // 序列名 + ▾：一段连续的点选区域（整块都能点开列表，不必瞄准小箭头）。
    let active_seq = seqs.open.iter().find(|(id, _)| seqs.active == Some(*id));
    let has_list = !seqs.open.is_empty();
    let fallback = crate::i18n::t(PanelKind::Timeline.title()).to_string();
    let title = active_seq.map(|(_, name)| name.clone()).unwrap_or(fallback);
    let title_x = strip.min.x + 12.0;
    let arrow_gap = if has_list { 20.0 } else { 0.0 };
    let font = Tokens::semibold(12.0);
    let title = fit_title(&painter, &title, font.clone(), (right - title_x - arrow_gap).max(0.0));
    let galley = painter.layout_no_wrap(title.clone(), font, Color32::WHITE);
    let (title_w, title_h) = (galley.size().x, galley.size().y);
    let pick_rect = Rect::from_min_size(pos2(title_x - 6.0, strip.min.y), vec2((title_w + arrow_gap + 8.0).clamp(0.0, strip.width()), row_h));
    let pick_response = ui.interact(pick_rect, egui::Id::new(("panel-tab", g.path.clone(), "Timeline")), Sense::click());
    pick_response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, active == PanelKind::Timeline, title.as_str()));
    reg.add_response(&format!("panel.tab.{}", PanelKind::Timeline.id()), &pick_response, PanelKind::Timeline.title());
    tab_rects.push((PanelKind::Timeline, pick_rect));
    if pick_response.hovered() {
        painter.rect_filled(pick_rect, 4.0, t.hover);
    }
    paint_widget_focus(&painter, &pick_response, t);
    painter.galley(pos2(title_x, strip.center().y - title_h / 2.0), galley, Color32::WHITE);
    let arrow = Rect::from_center_size(pos2(title_x + title_w + 10.0, strip.center().y), vec2(14.0, 14.0));
    if has_list {
        icons::paint(&painter, arrow, Icon::ChevronDown, if pick_response.hovered() { t.text } else { t.text_dim });
        reg.add("timeline.seq.pick", arrow, "Open sequences");
    }
    if pick_response.secondary_clicked() {
        let at = pick_response.interact_pointer_pos().unwrap_or(pick_rect.left_bottom());
        actions.push(DockAction::PanelMenu(PanelKind::Timeline, at));
    }
    // 中键关掉当前序列（原页签条上中键点序列页签就是这件事）。
    if pick_response.middle_clicked()
        && let Some((id, _)) = active_seq
    {
        actions.push(DockAction::CloseSequence(*id));
    }

    // ▾ 弹出打开中的序列列表；点一条就切过去，与原页签条上的序列页签等价。
    if has_list {
        let pick_id = egui::Id::new(("jy-seq-list", g.path.clone()));
        let ctx = ui.ctx().clone();
        let open = ctx.data(|d| d.get_temp::<bool>(pick_id)).unwrap_or(false);
        let clicked = pick_response.clicked();
        if clicked {
            ctx.data_mut(|d| d.insert_temp(pick_id, !open));
        }
        let opened = if clicked { !open } else { open };
        if opened {
            let mut picked: Option<u64> = None;
            let anchor = pick_rect.left_bottom();
            let area = egui::Area::new(pick_id.with("area")).order(egui::Order::Foreground).pivot(Align2::LEFT_TOP).fixed_pos(anchor).show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(160.0);
                    // 打开很多序列时列表滚动，而不是把窗口撑出屏幕。
                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        for (id, name) in &seqs.open {
                            let label = if seqs.active == Some(*id) { format!("✓ {name}") } else { name.clone() };
                            let row = ui.button(label);
                            reg.add_response(&format!("timeline.seq.item.{id}"), &row, name);
                            if row.clicked() {
                                picked = Some(*id);
                            }
                        }
                    });
                });
            });
            if let Some(id) = picked {
                actions.push(DockAction::OpenSequence(id));
                actions.push(DockAction::Focus(PanelKind::Timeline));
                ctx.data_mut(|d| d.remove::<bool>(pick_id));
            } else if !clicked && area.response.clicked_elsewhere() {
                // 打开的那一帧里 `clicked_elsewhere` 必然为真（弹层当时还不存在），跳过。
                ctx.data_mut(|d| d.remove::<bool>(pick_id));
            }
        }
    }

    // 电平表的按钮与序列名各自登记自己的面板菜单，切面板不会让菜单消失。
    if let Some(response) = &meters_response {
        reg.add_response(&format!("panel.menu.{}", PanelKind::AudioMeters.id()), response, "panel menu");
    }
    reg.add_response(&format!("panel.menu.{}", PanelKind::Timeline.id()), &pick_response, "panel menu");

    // 指针在组里按下就把焦点交给指针下那个面板（与页签条同一套规则）。
    if ui.rect_contains_pointer(g.rect) && ui.input(|i| i.pointer.any_pressed()) {
        let panel = ui
            .input(|i| i.pointer.interact_pos().filter(|_| i.pointer.primary_pressed() || i.pointer.secondary_pressed()))
            .and_then(|pos| tab_rects.iter().find(|(_, rect)| rect.contains(pos)).map(|(panel, _)| *panel))
            .unwrap_or(active);
        actions.push(DockAction::Focus(panel));
    }
    Chrome { actions, tabs: tab_rects }
}

/// Paint once after the group's contents so child backgrounds cannot hide its focus (#1075).
pub(crate) fn paint_group_focus(ui: &egui::Ui, group: &Group, focused: PanelKind, tokens: &Tokens) {
    if group.panels.get(group.active).copied() == Some(focused) {
        ui.painter().rect_stroke(group.rect, 0.0, Stroke::new(1.0, tokens.focus), StrokeKind::Inside);
    }
}

fn paint_widget_focus(painter: &egui::Painter, response: &egui::Response, tokens: &Tokens) {
    if response.has_focus() {
        painter.rect_stroke(response.rect.shrink(2.0), 2.0, Stroke::new(1.5, tokens.focus), StrokeKind::Inside);
    }
}

/// Placeholder body for panels that are not implemented yet.
pub fn placeholder(ui: &mut egui::Ui, rect: Rect, t: &Tokens, text: &str) {
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, Tokens::ui(12.0), t.text_faint);
}

/// 按根 → 子的路径取节点；空路径就是节点本身。
fn node_at<'a>(node: &'a DockNode, path: &[bool]) -> Option<&'a DockNode> {
    let mut node = node;
    for &second in path {
        let DockNode::Split { a, b, .. } = node else { return None };
        node = if second { b } else { a };
    }
    Some(node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeKind;

    /// 一帧的最小外壳：装好字体，用一个 `Group` 跑一遍页签条，返回本帧的 [`Chrome`]。
    fn frame(ctx: &egui::Context, g: &Group, seqs: &SeqTabs, jianying: bool, events: Vec<egui::Event>, reg: &mut crate::automation::Registry) -> Chrome {
        let t = Tokens::for_kind(ThemeKind::Dark);
        let mut out: Option<Chrome> = None;
        let input = egui::RawInput { events, screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(640.0, 420.0))), ..Default::default() };
        let mut output = ctx.run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                out = Some(draw_group_chrome_details_with(ui, g, seqs, &t, reg, jianying));
            });
        });
        // 测试里没有渲染后端：贴图增量必须显式丢弃，否则 epaint 在 drop 时会 panic。
        output.textures_delta.clear();
        out.unwrap_or_default()
    }

    fn press(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
    }

    /// 本帧登记过的元素矩形（按 id）。
    fn rect_of(reg: &crate::automation::Registry, id: &str) -> Option<Rect> {
        reg.elements.iter().find(|e| e.id == id).map(|e| Rect::from_min_size(pos2(e.rect[0], e.rect[1]), vec2(e.rect[2], e.rect[3])))
    }

    fn has(reg: &crate::automation::Registry, id: &str) -> bool {
        reg.elements.iter().any(|e| e.id == id)
    }

    /// 剪映式时间轴组：序列 + 电平表。
    fn timeline_group(active: usize) -> Group {
        Group {
            path: "t".to_string(),
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 260.0)),
            // dock 按 `t.tab_h` 预留了 32 pt 的页签条高度。
            content: Rect::from_min_size(pos2(0.0, 32.0), vec2(600.0, 228.0)),
            panels: vec![PanelKind::Timeline, PanelKind::AudioMeters],
            active,
        }
    }

    fn two_sequences() -> SeqTabs {
        SeqTabs { open: vec![(7, "Main Edit".to_string()), (9, "Second".to_string())], active: Some(7) }
    }

    /// 剪映工作区的时间轴组要换成 26 pt 轻量头，而不是 32 pt 的页签条；自动化 id 一个都不能少。
    #[test]
    fn jianying_timeline_group_gets_a_light_header_instead_of_the_tab_strip() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, &Tokens::for_kind(ThemeKind::Dark));
        let g = timeline_group(0);
        let seqs = two_sequences();
        let mut reg = crate::automation::Registry::default();
        let chrome = frame(&ctx, &g, &seqs, true, Vec::new(), &mut reg);

        // 轻量头只有 26 pt；页签条是 t.tab_h（32 pt）。序列名占满整条头，电平表按钮落在头里面。
        let (_, header) = chrome.tabs.iter().find(|(panel, _)| *panel == PanelKind::Timeline).expect("the sequence name is a drop target");
        assert!((header.height() - JIANYING_TIMELINE_HEADER_H).abs() < 0.01, "the sequence header is {} pt tall, want 26", header.height());
        let (_, meters) = chrome.tabs.iter().find(|(panel, _)| *panel == PanelKind::AudioMeters).expect("the meters button is a drop target");
        assert!(meters.max.y <= header.max.y + 0.01, "the meters button spills below the 26 pt header: {meters:?} vs {header:?}");
        assert!(meters.min.x > header.min.x + 60.0, "the meters button should sit at the right end of the header: {meters:?}");

        // 页签不画了，但契约在：tab / menu / drag 三个前缀照旧，序列 ▾ 也有自己的 id。
        for id in ["panel.tab.Timeline", "panel.tab.AudioMeters", "panel.menu.Timeline", "panel.menu.AudioMeters", "panel.drag.Timeline", "timeline.seq.pick"] {
            assert!(has(&reg, id), "the light header no longer registers {id}");
        }
    }

    /// 其余工作区照旧：同一个组还是 32 pt 的页签条（回归护栏）。
    #[test]
    fn other_workspaces_keep_the_tab_strip() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, &Tokens::for_kind(ThemeKind::Dark));
        let tab_h = Tokens::for_kind(ThemeKind::Dark).tab_h;
        let g = timeline_group(0);
        let seqs = two_sequences();
        let mut reg = crate::automation::Registry::default();
        let chrome = frame(&ctx, &g, &seqs, false, Vec::new(), &mut reg);
        let height = chrome.tabs.iter().find(|(panel, _)| *panel == PanelKind::Timeline).map(|(_, rect)| rect.height()).unwrap_or(0.0);
        assert!((height - tab_h).abs() < 0.01, "the Premiere tab strip should still be {tab_h} pt, got {height}");
        assert!(has(&reg, "panel.tab.Timeline"), "the tab strip still registers the Timeline tab");
    }

    /// 点序列名弹出打开中的序列列表，选一条就走 `OpenSequence`（与原序列页签等价）。
    #[test]
    fn the_sequence_list_switches_sequences() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, &Tokens::for_kind(ThemeKind::Dark));
        let g = timeline_group(0);
        let seqs = two_sequences();
        let mut reg = crate::automation::Registry::default();
        let _ = frame(&ctx, &g, &seqs, true, Vec::new(), &mut reg);
        let pick = rect_of(&reg, "panel.tab.Timeline").expect("the sequence name is drawn").center();

        reg.begin_frame();
        let _ = frame(&ctx, &g, &seqs, true, vec![egui::Event::PointerMoved(pick)], &mut reg);
        reg.begin_frame();
        let _ = frame(&ctx, &g, &seqs, true, vec![press(pick, true)], &mut reg);
        reg.begin_frame();
        let _ = frame(&ctx, &g, &seqs, true, vec![press(pick, false)], &mut reg);

        reg.begin_frame();
        let _ = frame(&ctx, &g, &seqs, true, vec![egui::Event::PointerMoved(egui::Pos2::new(600.0, 400.0))], &mut reg);
        // 列表列出全部打开中的序列，活动那条打勾。
        let first = rect_of(&reg, "timeline.seq.item.7").expect("the open list should list the active sequence");
        let second = rect_of(&reg, "timeline.seq.item.9").expect("the open list should list every open sequence");
        assert!(first.height() > 0.0 && second.height() > 0.0, "the list rows have no size: {first:?} {second:?}");

        reg.begin_frame();
        let _ = frame(&ctx, &g, &seqs, true, vec![egui::Event::PointerMoved(second.center())], &mut reg);
        reg.begin_frame();
        let _ = frame(&ctx, &g, &seqs, true, vec![press(second.center(), true)], &mut reg);
        reg.begin_frame();
        let chrome = frame(&ctx, &g, &seqs, true, vec![press(second.center(), false)], &mut reg);
        assert!(chrome.actions.contains(&DockAction::OpenSequence(9)), "picking a sequence did not activate it: {:?}", chrome.actions);
    }

    /// 「音频电平表」按钮：点它切到电平表，电平表已经显示时点它回到时间轴。
    #[test]
    fn the_meters_button_toggles_the_audio_meters() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, &Tokens::for_kind(ThemeKind::Dark));
        let seqs = two_sequences();
        let mut reg = crate::automation::Registry::default();

        let g = timeline_group(0);
        let _ = frame(&ctx, &g, &seqs, true, Vec::new(), &mut reg);
        let button = rect_of(&reg, "panel.tab.AudioMeters").expect("the meters button is drawn").center();
        reg.begin_frame();
        let _ = frame(&ctx, &g, &seqs, true, vec![egui::Event::PointerMoved(button)], &mut reg);
        reg.begin_frame();
        let _ = frame(&ctx, &g, &seqs, true, vec![press(button, true)], &mut reg);
        reg.begin_frame();
        let chrome = frame(&ctx, &g, &seqs, true, vec![press(button, false)], &mut reg);
        assert!(chrome.actions.contains(&DockAction::Activate(PanelKind::AudioMeters)), "the meters button did not switch panels: {:?}", chrome.actions);

        // 电平表已经是活动面板：同一个按钮现在负责回到时间轴。
        let g = timeline_group(1);
        let mut reg = crate::automation::Registry::default();
        let _ = frame(&ctx, &g, &seqs, true, Vec::new(), &mut reg);
        let button = rect_of(&reg, "panel.tab.AudioMeters").expect("the meters button is drawn").center();
        reg.begin_frame();
        let _ = frame(&ctx, &g, &seqs, true, vec![egui::Event::PointerMoved(button)], &mut reg);
        reg.begin_frame();
        let _ = frame(&ctx, &g, &seqs, true, vec![press(button, true)], &mut reg);
        reg.begin_frame();
        let chrome = frame(&ctx, &g, &seqs, true, vec![press(button, false)], &mut reg);
        assert!(chrome.actions.contains(&DockAction::Activate(PanelKind::Timeline)), "the meters button did not go back to the timeline: {:?}", chrome.actions);
    }

    #[test]
    fn split_first_never_panics_on_tiny_areas() {
        let split_first = |size: SplitSize, avail: f32| {
            split_rect(Rect::from_min_size(egui::Pos2::ZERO, vec2(avail, 100.0)), SplitAxis::Horizontal, size.shared(), 0.0, 20.0).unwrap().first_size()
        };
        for avail in [0.0, 1.0, 15.0, 20.0, 35.0, 39.9, 40.0, 100.0, 2000.0] {
            for size in [
                SplitSize::Ratio(0.0),
                SplitSize::Ratio(0.5),
                SplitSize::Ratio(1.0),
                SplitSize::FixedA(300.0),
                SplitSize::FixedB(300.0),
                SplitSize::FixedA(0.0),
            ] {
                let f = split_first(size, avail);
                assert!((0.0..=avail).contains(&f), "{size:?} in {avail}: {f}");
            }
        }
        assert_eq!(split_first(SplitSize::FixedA(300.0), 1000.0), 300.0);
        assert_eq!(split_first(SplitSize::FixedB(300.0), 1000.0), 700.0);
        assert_eq!(split_first(SplitSize::Ratio(0.0), 1000.0), 20.0);
    }

    #[test]
    fn workspaces_contain_core_panels() {
        for w in WORKSPACES {
            let d = workspace(w);
            assert!(d.contains(PanelKind::Timeline), "{w}");
            assert!(d.contains(PanelKind::Program), "{w}");
        }
    }

    #[test]
    fn close_and_open() {
        let mut d = workspace("Editing");
        d.close(PanelKind::Metadata);
        assert!(!d.contains(PanelKind::Metadata));
        d.open_near(PanelKind::LumetriColor, PanelKind::Program);
        assert!(d.is_visible(PanelKind::LumetriColor));
        let s = serde_json::to_string(&d).unwrap();
        let back: DockNode = serde_json::from_str(&s).unwrap();
        assert_eq!(back, d);
    }

    /// #284: with Project and Program closed (or every panel closed) a panel still opens.
    #[test]
    fn open_near_falls_back_to_the_first_group() {
        let mut d = workspace("Editing");
        let mut all = Vec::new();
        d.panels(&mut all);
        for p in all {
            d.close(p);
        }
        assert_eq!(d, DockNode::Tabs { panels: vec![], active: 0 });
        d.open_near(PanelKind::Program, PanelKind::Project);
        assert!(d.is_visible(PanelKind::Program));
        d.restore_timeline();
        assert!(d.is_visible(PanelKind::Timeline));
        // a tree without Project or Program: the panel joins the first group
        let mut d = hsplit(SplitSize::Ratio(0.5), tabs(&[PanelKind::Effects], 0), tabs(&[PanelKind::Markers], 0));
        d.open_near(PanelKind::Info, PanelKind::Source);
        assert!(d.is_visible(PanelKind::Info));
        assert_eq!(d, hsplit(SplitSize::Ratio(0.5), tabs(&[PanelKind::Effects, PanelKind::Info], 1), tabs(&[PanelKind::Markers], 0)));
    }

    /// 打开「滤镜 / 调节」（Lumetri Color）这类侧栏面板时，不能作为 Tab 插进 Program 所在组：
    /// 那会把预览盖住。它应当落到最近的相邻组，Program 保持可见（本次修复的已记录缺口）。
    #[test]
    fn open_near_never_tabs_onto_the_program_monitor() {
        for name in ["Editing", "Jianying", "Effects", "Assembly", "Audio", "Captions and Graphics"] {
            let mut d = workspace(name);
            assert!(!d.contains(PanelKind::LumetriColor), "{name}: the fixture already has Lumetri Color");
            d.open_near(PanelKind::LumetriColor, PanelKind::Program);
            assert!(d.contains(PanelKind::LumetriColor), "{name}: the panel did not open");
            assert!(d.is_visible(PanelKind::LumetriColor), "{name}: the new panel is not the active tab");
            assert!(d.is_visible(PanelKind::Program), "{name}: opening a side panel must not hide the Program monitor");
        }
    }

    /// M-3：`near` 正在浮动（因此不在停靠树里）时，不能把它当锚点，也不能静默失败：
    /// 改投最近的已停靠组，并如实返回结果。
    #[test]
    fn try_open_near_falls_back_when_the_near_panel_is_floating() {
        let mut d = workspace("Jianying");
        // Program 浮动后就不在这棵树里了；这里用 Close 表达同一事实。
        d.close(PanelKind::Program);
        assert!(!d.contains(PanelKind::Program));
        d.try_open_near(PanelKind::LumetriColor, PanelKind::Program).expect("a floating anchor must fall back to a docked group");
        assert!(d.contains(PanelKind::LumetriColor), "the panel did not open beside a docked group");
        assert!(!d.contains(PanelKind::Program), "the floating Program must stay out of the dock");
    }

    /// `try_open_near` 把共享模型拒绝落点的原因返回出来（旧 `open_near` 丢掉了这个 `false`）。
    #[test]
    fn try_open_near_reports_a_rejected_placement() {
        let mut d = DockNode::Split {
            vertical: false,
            size: SplitSize::Ratio(f32::NAN),
            a: Box::new(tabs(&[PanelKind::Project], 0)),
            b: Box::new(tabs(&[PanelKind::Markers], 0)),
        };
        let error = d.try_open_near(PanelKind::Info, PanelKind::Project).expect_err("a non-finite split size must be rejected");
        assert!(!error.is_empty(), "the failure reason should be reported");
    }

    /// Program 单独成组、没有相邻组可用时，退化为并排拆分：Program 留在原来那一半。
    #[test]
    fn open_near_splits_beside_a_lone_program_group() {
        let mut d = tabs(&[PanelKind::Program], 0);
        d.open_near(PanelKind::LumetriColor, PanelKind::Program);
        assert!(d.contains(PanelKind::Program) && d.contains(PanelKind::LumetriColor));
        assert!(d.is_visible(PanelKind::Program), "the split must keep Program in its own half");
        assert!(d.is_visible(PanelKind::LumetriColor));
        // 两个面板各自独立成组（并排），不是同一个 Tab 组。
        assert!(matches!(d, DockNode::Split { .. }), "a lone Program group should split, got {d:?}");
    }
}
