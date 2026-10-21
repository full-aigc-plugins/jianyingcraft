//! 剪映式资源 Tab 的浏览态：顶部搜索、`全部 / 免费 / 会员` chips、6 列密集资源网格。
//!
//! 数据来自插件仓的 `jianyingcraft-plugin/catalogs/*.json`（`name` / `resource_id` / `vip`），
//! **运行时**读取，不 `include_str!`：官方素材（缩略图、音频、字体）不随本仓库分发，目录也可能
//! 被用户放到任意位置。查找顺序：
//!
//! 1. 环境变量 `JIANYINGCRAFT_CATALOGS`（指向 `catalogs` 目录）；
//! 2. 仓库相对路径 `../jianyingcraft-plugin/catalogs`（仓库根是 `CARGO_MANIFEST_DIR` 往上两级，
//!    插件仓与仓库根同级，开发时直接可用）；
//! 3. 当前工作目录下的 `catalogs`（打包分发时与可执行文件同级）。
//!
//! 目录或 JSON 缺失、损坏都只是**可操作的空态**：告诉用户设置 `JIANYINGCRAFT_CATALOGS`，绝不
//! panic。缩略图是按 `resource_id` 哈希生成的渐变占位（首字居中），因为官方缩略图不在仓库里。
//!
//! 交互只做选中：点卡片写 `app.ui.status`（名称 + resource_id + 提示写进 action plan），不执行
//! 任何副作用。双击/回车留给后续任务。
//!
//! 自动化 id：`resource.search` / `resource.chip.<all|free|vip>` / `resource.count` /
//! `resource.grid` / `resource.card.<resource_id>` / `resource.empty` / `resource.note`。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, PoisonError, RwLock};

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::FilmcraftApp;
use crate::dock::PanelKind;
use crate::layout::LayoutClass;
use crate::theme::Tokens;

/// 网格固定 6 列（对照剪映截图），行距 6 px。
const COLUMNS: usize = 6;
const CARD_GAP: f32 = 6.0;
/// 名称 + 编号两行文字区的高度。
const LABEL_HEIGHT: f32 = 27.0;

/// 资源 Tab：剪映一级 Tab 里用资源目录驱动的那些。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResourceTab {
    /// 转场 → `transitions.json`
    Transitions,
    /// 滤镜 → `filters.json`
    Filters,
    /// 特效 → `video_scene_effects.json`
    Effects,
    /// 贴纸 → `video_scene_effects.json`（官方贴纸素材不随包分发，这里复用场景特效目录）
    Sticker,
    /// 文本 → `text_animations_in.json`
    Text,
    /// 字幕 → `fonts.json`（字幕样式以字体目录为主）
    Captions,
    /// 音频 → `audio_scene_effects.json`
    Audio,
}

impl ResourceTab {
    /// 一级 Tab slug → 资源 Tab。
    ///
    /// 宽屏 Tab 行和 Compact 顶栏**共用部分 slug 但含义不同**：`captions` 在宽屏 Tab 行是
    /// 「字幕」（`fonts.json`），在 Compact 顶栏是「文本」（`text_animations_in.json`）。
    /// `shell` 为真表示 slug 来自 Compact/Medium 的顶栏（`SHELL_TABS`）。
    pub(crate) fn from_slug(slug: &str, shell: bool) -> Option<Self> {
        match (slug, shell) {
            ("transitions" | "transition", _) => Some(Self::Transitions),
            ("filters" | "filter", _) => Some(Self::Filters),
            ("effects" | "effect", _) => Some(Self::Effects),
            ("sticker" | "stickers", _) => Some(Self::Sticker),
            ("text" | "texts", _) => Some(Self::Text),
            ("captions", true) => Some(Self::Text),
            ("captions", false) => Some(Self::Captions),
            ("subtitle" | "subtitles", _) => Some(Self::Captions),
            ("audio", _) => Some(Self::Audio),
            _ => None,
        }
    }

    /// 状态栏里怎么称呼这类资源（「已选转场…」）。
    fn label(self) -> &'static str {
        match self {
            Self::Transitions => "转场",
            Self::Filters => "滤镜",
            Self::Effects => "特效",
            Self::Sticker => "贴纸",
            Self::Text => "文本",
            Self::Captions => "字幕",
            Self::Audio => "音频",
        }
    }

    /// 目录里的文件名。
    fn file(self) -> &'static str {
        match self {
            Self::Transitions => "transitions.json",
            Self::Filters => "filters.json",
            Self::Effects | Self::Sticker => "video_scene_effects.json",
            Self::Text => "text_animations_in.json",
            Self::Captions => "fonts.json",
            Self::Audio => "audio_scene_effects.json",
        }
    }

    /// 这个 Tab 在 dock 里落在哪个面板；`None` 表示不接管（音频 Tab 保留素材浏览器）。
    fn panel(self) -> Option<PanelKind> {
        match self {
            Self::Transitions | Self::Effects => Some(PanelKind::Effects),
            Self::Filters => Some(PanelKind::LumetriColor),
            Self::Sticker => Some(PanelKind::EssentialGraphics),
            Self::Text | Self::Captions => Some(PanelKind::Text),
            Self::Audio => None,
        }
    }

    /// 网格下方的补充说明（贴纸复用特效目录，需要说清楚）。
    fn note(self) -> Option<&'static str> {
        match self {
            Self::Sticker => Some("贴纸与场景特效共用目录；官方贴纸素材不随包分发"),
            _ => None,
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Self::Transitions => "transitions",
            Self::Filters => "filters",
            Self::Effects => "effects",
            Self::Sticker => "sticker",
            Self::Text => "text",
            Self::Captions => "captions",
            Self::Audio => "audio",
        }
    }
}

/// `全部 / 免费 / 会员` chips 的当前选择，随 `UiState` 持久化。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceFilter {
    #[default]
    All,
    Free,
    Vip,
}

impl ResourceFilter {
    fn accepts(self, entry: &ResourceEntry) -> bool {
        match self {
            Self::All => true,
            Self::Free => !entry.vip,
            Self::Vip => entry.vip,
        }
    }
}

/// 一条资源目录条目。只保留浏览需要的字段：名称、编号、是否会员。
#[derive(Clone, Debug, PartialEq, Eq)]
struct ResourceEntry {
    name: String,
    resource_id: String,
    vip: bool,
}

/// 缓存：(stamp, 条目)。按**路径**做键，测试换临时目录、用户换 `JIANYINGCRAFT_CATALOGS`
/// 都会自然命中各自的一份；读取失败不缓存，目录稍后出现即可恢复。
/// `stamp` 为 `None` 表示文件已经不存在：此时**不查缓存**，让下一次读取重新报错，目录被删掉
/// 后不会继续画旧列表。
type Stamp = Option<(u64, Option<std::time::SystemTime>)>;
type Cache = RwLock<HashMap<PathBuf, (Stamp, Arc<Vec<ResourceEntry>>)>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// 资源目录的查找顺序：显式环境变量 → 仓库相对路径 → 工作目录下的 `catalogs`。
pub(crate) fn catalogs_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("JIANYINGCRAFT_CATALOGS") {
        let dir = dir.trim();
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    // `CARGO_MANIFEST_DIR` 是 `crates/ui-egui`，往上两级才是仓库根；插件仓与它同级。
    if let Some(root) = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2) {
        let repo = root.join("../jianyingcraft-plugin/catalogs");
        if repo.is_dir() {
            return repo;
        }
    }
    PathBuf::from("catalogs")
}

/// 当前 dock 面板该不该被资源目录接管：Jianying 工作区、当前一级 Tab 属于资源 Tab、且该 Tab
/// 的目标面板正是这个面板。宽屏读 `jianying_tab`（顶部 Tab 行），Compact/Medium 读
/// `jy_shell_slug`（顶栏 Tab）；另一份状态只作回退，避免两套入口互相看不见。
pub(crate) fn active_tab(app: &FilmcraftApp, panel: PanelKind, class: LayoutClass) -> Option<ResourceTab> {
    if app.ui.workspace != "Jianying" {
        return None;
    }
    let wide = class == LayoutClass::Wide;
    let (primary, primary_shell) = if wide { (app.ui.jianying_tab.as_deref(), false) } else { (app.ui.jy_shell_slug.as_deref(), true) };
    let (fallback, fallback_shell) = if wide { (app.ui.jy_shell_slug.as_deref(), true) } else { (app.ui.jianying_tab.as_deref(), false) };
    [primary.and_then(|slug| ResourceTab::from_slug(slug, primary_shell)), fallback.and_then(|slug| ResourceTab::from_slug(slug, fallback_shell))]
        .into_iter()
        .flatten()
        .find(|tab| tab.panel() == Some(panel))
}

/// 把目录里的数组读成条目；字段缺失/类型不对的条目跳过，不让一条坏数据拖垮整个 Tab。
fn parse_entry(value: &Value) -> Option<ResourceEntry> {
    let name = value.get("name").and_then(Value::as_str)?.trim();
    if name.is_empty() {
        return None;
    }
    let resource_id = match value.get("resource_id")? {
        Value::String(id) if !id.trim().is_empty() => id.trim().to_string(),
        Value::Number(id) => id.to_string(),
        _ => return None,
    };
    let vip = value.get("vip").and_then(Value::as_bool).unwrap_or(false);
    Some(ResourceEntry { name: name.to_string(), resource_id, vip })
}

/// 读一个 Tab 的目录（带 mtime 缓存）。任何失败都变成可读的错误字符串，由调用方画成空态。
fn load(tab: ResourceTab) -> Result<Arc<Vec<ResourceEntry>>, String> {
    let path = catalogs_dir().join(tab.file());
    let stamp: Stamp = std::fs::metadata(&path).ok().map(|m| (m.len(), m.modified().ok()));
    {
        let guard = cache().read().unwrap_or_else(PoisonError::into_inner);
        if let Some(current) = stamp
            && let Some((Some(cached), entries)) = guard.get(&path)
            && *cached == current
        {
            return Ok(Arc::clone(entries));
        }
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
    let value: Value = serde_json::from_str(&text).map_err(|e| format!("{} 不是合法 JSON：{e}", path.display()))?;
    let Some(array) = value.as_array() else { return Err(format!("{} 的内容不是资源数组", path.display())) };
    let entries: Vec<ResourceEntry> = array.iter().filter_map(parse_entry).collect();
    if entries.is_empty() {
        return Err(format!("{} 里没有可用条目（需要 name 与 resource_id）", path.display()));
    }
    let entries = Arc::new(entries);
    {
        let mut guard = cache().write().unwrap_or_else(PoisonError::into_inner);
        guard.insert(path, (stamp, Arc::clone(&entries)));
    }
    Ok(entries)
}

/// 画这个资源 Tab：搜索框、chips、密集网格（或可操作的空态）。
pub(crate) fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect, tab: ResourceTab) {
    if rect.width() < 40.0 || rect.height() < 40.0 {
        return;
    }
    let t = app.tokens;
    let pad = 8.0;
    let inner_w = (rect.width() - 2.0 * pad).max(1.0);
    let x0 = rect.min.x + pad;

    // ---- 搜索框（22 px，通栏）
    let search = Rect::from_min_size(pos2(x0, rect.min.y + 6.0), vec2(inner_w, 22.0));
    let mut field = ui.new_child(egui::UiBuilder::new().max_rect(search).id_salt(("resource-search", tab.slug())));
    let mut query = app.ui.resource_search.clone();
    crate::widgets::search_field(&mut field, &mut query, "搜索名称/编号", search.width(), &t);
    app.ui.resource_search = query;
    app.auto.add("resource.search", search, "搜索名称/编号");

    // ---- 数据：加载失败先记下错误，仍然画搜索框让用户能清词/换 Tab 后重试
    let loaded = load(tab);
    let needle = app.ui.resource_search.trim().to_lowercase();
    let filter = app.ui.resource_filter;
    let mut filtered: Vec<&ResourceEntry> = Vec::new();
    let mut total = 0usize;
    if let Ok(entries) = &loaded {
        total = entries.len();
        filtered = entries
            .iter()
            .filter(|entry| filter.accepts(entry))
            .filter(|entry| needle.is_empty() || entry.name.to_lowercase().contains(&needle) || entry.resource_id.contains(&needle))
            .collect();
    }

    // ---- chips：全部 / 免费 / 会员；右侧「N 条」
    let chips_y = search.max.y + 6.0;
    let mut x = x0;
    for (key, label, value) in [("all", "全部", ResourceFilter::All), ("free", "免费", ResourceFilter::Free), ("vip", "会员", ResourceFilter::Vip)] {
        let chip = Rect::from_min_size(pos2(x, chips_y), vec2(46.0, 20.0));
        let on = app.ui.resource_filter == value;
        let resp = ui.interact(chip, egui::Id::new(("resource-chip", key)), Sense::click());
        if on {
            ui.painter().rect_filled(chip, 10.0, t.pill_active_bg);
        } else {
            ui.painter().rect_filled(chip, 10.0, t.panel_bg);
            ui.painter().rect_stroke(chip, 10.0, Stroke::new(1.0, t.separator), StrokeKind::Inside);
        }
        ui.painter().text(chip.center(), Align2::CENTER_CENTER, label, Tokens::ui(11.5), if on { t.text } else { t.text_dim });
        app.auto.add(&format!("resource.chip.{key}"), chip, label);
        if resp.clicked() {
            app.ui.resource_filter = value;
        }
        x += chip.width() + 4.0;
    }
    let count_label = format!("{} 条", filtered.len());
    let count_left = (rect.max.x - pad - 64.0).max(x + 4.0).min(rect.max.x - pad - 4.0);
    let count_rect = Rect::from_min_max(pos2(count_left, chips_y), pos2(rect.max.x - pad, chips_y + 20.0));
    ui.painter().text(pos2(rect.max.x - pad, chips_y + 10.0), Align2::RIGHT_CENTER, &count_label, Tokens::ui(10.5), t.text_dim);
    app.auto.add("resource.count", count_rect, &count_label);
    let mut body_top = chips_y + 24.0;

    // ---- 可选的说明行（贴纸复用特效目录）
    if let Some(note) = tab.note()
        && body_top + 12.0 < rect.max.y
    {
        let note_rect = Rect::from_min_size(pos2(x0, body_top), vec2(inner_w, 12.0));
        ui.painter().text(pos2(x0, body_top + 6.0), Align2::LEFT_CENTER, note, Tokens::ui(9.0), t.text_faint);
        app.auto.add("resource.note", note_rect, note);
        body_top += 14.0;
    }

    if body_top + 8.0 >= rect.max.y {
        return;
    }

    // ---- 空态：目录不可用 / 没有匹配项
    let empty = match &loaded {
        Err(error) => Some(format!("{error}\n请设置环境变量 JIANYINGCRAFT_CATALOGS 指向 jianyingcraft-plugin/catalogs，\n或把 catalogs/ 放到工作目录后重试。")),
        Ok(_) if filtered.is_empty() && total > 0 => Some("没有匹配的资源；换个关键词，或点「全部」看整库。".to_string()),
        Ok(_) => None,
    };
    if let Some(message) = empty {
        let area = Rect::from_min_max(pos2(rect.min.x + pad, body_top), pos2(rect.max.x - pad, rect.max.y));
        app.auto.add("resource.empty", area, &message);
        let galley = ui.painter().layout(message, Tokens::ui(11.0), t.text_dim, area.width());
        let y = (area.center().y - galley.size().y / 2.0).max(area.min.y);
        ui.painter().galley(pos2(area.min.x + 4.0, y), galley, t.text_dim);
        return;
    }

    // ---- 网格（6 列方形卡片，可滚动）
    let body = Rect::from_min_max(pos2(rect.min.x, body_top), rect.max);
    let mut content = ui.new_child(egui::UiBuilder::new().max_rect(body).id_salt(("resource-body", tab.slug())));
    content.set_clip_rect(body.intersect(ui.clip_rect()));
    let selected = app.ui.resource_selection.clone();
    let mut picked: Option<(String, String)> = None;
    egui::ScrollArea::vertical().id_salt(("resource-scroll", tab.slug())).auto_shrink([false, false]).show(&mut content, |ui| {
        let rows = filtered.len().div_ceil(COLUMNS).max(1);
        let card_w = ((ui.available_width() - (COLUMNS - 1) as f32 * CARD_GAP) / COLUMNS as f32).max(16.0);
        let cell_h = card_w + LABEL_HEIGHT + CARD_GAP;
        let (canvas, _) = ui.allocate_exact_size(vec2(ui.available_width(), rows as f32 * cell_h + 2.0), Sense::hover());
        app.auto.add("resource.grid", canvas, tab.label());
        let clip = ui.clip_rect();
        for (index, entry) in filtered.iter().enumerate() {
            let col = index % COLUMNS;
            let row = index / COLUMNS;
            let card = Rect::from_min_size(
                pos2(canvas.min.x + col as f32 * (card_w + CARD_GAP), canvas.min.y + 2.0 + row as f32 * cell_h),
                vec2(card_w, card_w + LABEL_HEIGHT),
            );
            if card.max.y < clip.min.y || card.min.y > clip.max.y {
                continue;
            }
            if draw_card(app, ui, card, entry, selected.as_deref() == Some(entry.resource_id.as_str()), &t) {
                picked = Some((entry.name.clone(), entry.resource_id.clone()));
            }
        }
    });
    if let Some((name, resource_id)) = picked {
        app.ui.resource_selection = Some(resource_id.clone());
        app.ui.status = format!("已选{}「{name}」· {resource_id}；把它写进 action plan 即可", tab.label());
    }
}

/// 一张卡片：渐变占位缩略图 + 首字 + 名称 + 编号后 6 位 + 右上会员角标。返回是否被点击。
fn draw_card(app: &mut FilmcraftApp, ui: &egui::Ui, rect: Rect, entry: &ResourceEntry, selected: bool, t: &Tokens) -> bool {
    let response = ui.interact(rect, egui::Id::new(("resource-card", entry.resource_id.as_str())), Sense::click());
    let thumb = Rect::from_min_size(rect.min, vec2(rect.width(), rect.width()));
    ui.painter().rect_filled(rect, 4.0, if response.hovered() { t.hover } else { t.field_bg });
    paint_gradient(ui, thumb.shrink(1.0), &entry.resource_id);
    if let Some(first) = entry.name.chars().next() {
        ui.painter().text(thumb.center(), Align2::CENTER_CENTER, first.to_string(), Tokens::semibold(16.0), Color32::from_white_alpha(235));
    }
    if entry.vip {
        let badge = Rect::from_min_size(pos2(thumb.max.x - 25.0, thumb.min.y + 3.0), vec2(22.0, 12.0));
        ui.painter().rect_filled(badge, 3.0, Color32::from_rgb(0x8a, 0x62, 0x2a));
        ui.painter().text(badge.center(), Align2::CENTER_CENTER, "会员", Tokens::ui(8.0), Color32::from_rgb(0xff, 0xe2, 0xa8));
    }
    let name_rect = Rect::from_min_size(pos2(rect.min.x + 2.0, thumb.max.y + 2.0), vec2((rect.width() - 4.0).max(1.0), 13.0));
    let name = elide(ui, &entry.name, 11.0, name_rect.width());
    ui.painter().text(pos2(name_rect.min.x, name_rect.center().y), Align2::LEFT_CENTER, &name, Tokens::ui(11.0), t.text);
    let number = last_chars(&entry.resource_id, 6);
    ui.painter().text(pos2(rect.min.x + 2.0, name_rect.max.y + 5.0), Align2::LEFT_CENTER, number, Tokens::ui(9.0), t.text_dim);
    if selected {
        ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, t.accent), StrokeKind::Inside);
    } else if response.hovered() {
        ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, t.separator), StrokeKind::Inside);
    }
    app.auto.add(&format!("resource.card.{}", entry.resource_id), rect, &entry.name);
    response.clicked()
}

/// 按 `resource_id` 的哈希画一层竖向渐变，作为官方缩略图的占位。
fn paint_gradient(ui: &egui::Ui, rect: Rect, resource_id: &str) {
    if rect.width() < 1.0 || rect.height() < 1.0 {
        return;
    }
    const PALETTE: [(u8, u8, u8); 8] = [
        (0x2e, 0x3a, 0x59),
        (0x48, 0x34, 0x54),
        (0x26, 0x46, 0x4a),
        (0x54, 0x3c, 0x2a),
        (0x36, 0x42, 0x3a),
        (0x40, 0x30, 0x48),
        (0x2a, 0x38, 0x4e),
        (0x4a, 0x34, 0x3a),
    ];
    let (r, g, b) = PALETTE[(hash64(resource_id) % PALETTE.len() as u64) as usize];
    let steps = 6usize;
    let painter = ui.painter().with_clip_rect(rect);
    for step in 0..steps {
        let f0 = step as f32 / steps as f32;
        let f1 = (step + 1) as f32 / steps as f32;
        let light = 0.62 + 0.55 * f0;
        let color = Color32::from_rgb(scale(r, light), scale(g, light), scale(b, light));
        let y0 = rect.min.y + rect.height() * f0;
        let y1 = rect.min.y + rect.height() * f1;
        painter.rect_filled(Rect::from_min_max(pos2(rect.min.x, y0), pos2(rect.max.x, y1 + 0.5)), 0.0, color);
    }
}

fn scale(channel: u8, factor: f32) -> u8 {
    (channel as f32 * factor).clamp(0.0, 255.0) as u8
}

/// FNV-1a：稳定的字符串哈希，同一 `resource_id` 每帧得到同一套颜色。
fn hash64(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 编号后 `n` 位（按字符取，避免在 UTF-8 中间切片）。
fn last_chars(text: &str, n: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let start = chars.len().saturating_sub(n);
    chars[start..].iter().collect()
}

/// 单行省略：放不下就逐个字符回退并补 `…`。
fn elide(ui: &egui::Ui, text: &str, size: f32, max_w: f32) -> String {
    let font = Tokens::ui(size);
    if ui.painter().layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE).size().x <= max_w {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().copied().chain(std::iter::once('…')).collect();
        if ui.painter().layout_no_wrap(candidate.clone(), font.clone(), Color32::WHITE).size().x <= max_w {
            return candidate;
        }
    }
    "…".to_string()
}

#[cfg(test)]
mod tests {
    use super::{ResourceTab, hash64, last_chars, parse_entry};
    use serde_json::json;

    #[test]
    fn slugs_map_to_their_catalogs() {
        assert_eq!(ResourceTab::from_slug("transitions", false), Some(ResourceTab::Transitions));
        assert_eq!(ResourceTab::from_slug("subtitle", true), Some(ResourceTab::Captions));
        assert_eq!(ResourceTab::from_slug("filters", false), Some(ResourceTab::Filters));
        assert_eq!(ResourceTab::from_slug("unknown", false), None);
        // `captions` 在宽屏 Tab 行是「字幕」，在 Compact 顶栏是「文本」。
        assert_eq!(ResourceTab::from_slug("captions", false), Some(ResourceTab::Captions));
        assert_eq!(ResourceTab::from_slug("captions", true), Some(ResourceTab::Text));
        assert_eq!(ResourceTab::Effects.file(), "video_scene_effects.json");
        assert_eq!(ResourceTab::Sticker.file(), "video_scene_effects.json");
        assert_eq!(ResourceTab::Text.file(), "text_animations_in.json");
        assert_eq!(ResourceTab::Captions.file(), "fonts.json");
        assert_eq!(ResourceTab::Audio.file(), "audio_scene_effects.json");
        assert_eq!(ResourceTab::Audio.panel(), None, "the audio tab keeps the media browser");
    }

    #[test]
    fn hostile_entries_are_skipped_not_fatal() {
        assert!(parse_entry(&json!({"name": "", "resource_id": "1"})).is_none());
        assert!(parse_entry(&json!({"name": "x"})).is_none());
        assert!(parse_entry(&json!({"name": 7, "resource_id": "1"})).is_none());
        let numeric = parse_entry(&json!({"name": "x", "resource_id": 42, "vip": true})).expect("numeric ids are accepted");
        assert_eq!(numeric.resource_id, "42");
        assert!(numeric.vip);
    }

    #[test]
    fn id_number_and_hash_are_stable() {
        assert_eq!(last_chars("7049979667406656014", 6), "656014");
        assert_eq!(last_chars("42", 6), "42");
        assert_eq!(last_chars("资源编号", 6), "资源编号");
        assert_eq!(hash64("7049979667406656014"), hash64("7049979667406656014"));
        assert_ne!(hash64("a"), hash64("b"));
    }
}
