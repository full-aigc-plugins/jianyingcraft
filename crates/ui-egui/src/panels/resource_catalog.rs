//! 剪映式资源 Tab 的浏览态：左侧分类列、顶部搜索、`全部 / 免费 / 会员` chips、6 列密集资源网格。
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
//! **分类列是关键词归类，只作浏览辅助，不是剪映官方分类。** 官方目录 JSON 没有分类字段；左侧
//! 列按资源名称是否包含规则表里的关键词归类（规则见 [`ResourceTab::category_rules`]），因此名称
//! 不在规则内的条目会落到「其他」。分类与数量在目录加载时计算一次，随目录缓存；数量为 0 的分类
//! 不画出来。
//!
//! 交互只做选中、过滤与「复制 action」：点分类只筛当前 Tab 的那一类，和搜索、免费/会员 chips
//! 叠加；点卡片写 `app.ui.status`（名称 + resource_id + 提示写进 action plan），不执行任何副作用。
//! 面板底部固定 44 px 详情条（网格区相应缩短）显示选中资源的名称、完整编号、会员/免费与时长，右侧
//! 「复制 action」按钮把 `{"action":…,"resource_id":…,"resource_name":…}` 写进剪贴板，给智能体
//! 直接粘进 action plan；复制本身仍然不改工程。双击/回车应用资源留给后续任务。
//!
//! 自动化 id：`resource.search` / `resource.category.<slug>` / `resource.chip.<all|free|vip>` /
//! `resource.count` / `resource.grid` / `resource.card.<resource_id>` / `resource.empty` /
//! `resource.note` / `resource.detail` / `resource.detail.name` / `resource.detail.id` /
//! `resource.detail.meta` / `resource.copyAction`。

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
/// 面板底部固定详情条的高度；网格区（与左侧分类列）相应缩短。
const DETAIL_HEIGHT: f32 = 44.0;
/// 详情条右侧「复制 action」按钮的尺寸（对照剪映详情条右侧按钮）。
const COPY_WIDTH: f32 = 68.0;
const COPY_HEIGHT: f32 = 26.0;
/// 没有选中资源时详情条的提示。
const DETAIL_HINT: &str = "选一条资源，查看编号与 action 片段";
/// 左侧分类列宽度（对照剪映截图）与行高。
const CATEGORY_WIDTH: f32 = 96.0;
const CATEGORY_ROW_HEIGHT: f32 = 26.0;
/// 分类列与右侧搜索/网格之间的空隙。
const CATEGORY_GAP: f32 = 8.0;
/// 「全部」不是关键词规则，单独占一行。
const ALL_CATEGORY_SLUG: &str = "all";
const ALL_CATEGORY_LABEL: &str = "全部";

/// 资源 Tab：剪映一级 Tab 里用资源目录驱动的那些。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

/// 一条关键词归类规则。`keywords` 按名称做**大小写不敏感**的子串匹配，命中顺序就是规则表顺序；
/// 最后一条规则的关键词为空，充当「其他」兜底。
#[derive(Clone, Copy, Debug)]
struct CategoryRule {
    /// 自动化 id 与过滤状态使用的 ASCII slug。
    slug: &'static str,
    /// 分类列显示的中文名。
    label: &'static str,
    /// 小写关键词（含中文）。
    keywords: &'static [&'static str],
}

const TRANSITION_CATEGORY_RULES: &[CategoryRule] = &[
    CategoryRule { slug: "dissolve", label: "叠化", keywords: &["叠化", "溶解", "渐隐"] },
    CategoryRule { slug: "slide", label: "幻灯片", keywords: &["翻页", "幻灯片", "百叶窗"] },
    CategoryRule { slug: "motion", label: "运镜", keywords: &["推", "拉", "旋转", "缩放", "移", "摇", "晃动"] },
    CategoryRule { slug: "blur", label: "模糊", keywords: &["模糊", "虚化"] },
    CategoryRule { slug: "glitch", label: "故障", keywords: &["故障", "雪花", "闪屏", "噪点"] },
    CategoryRule { slug: "light", label: "光效", keywords: &["光", "闪白", "曝光"] },
    CategoryRule { slug: "space", label: "空间", keywords: &["3d", "空间", "立体"] },
    CategoryRule { slug: "split", label: "分割", keywords: &["分割", "切割", "切开", "分屏"] },
    CategoryRule { slug: "shake", label: "抖动", keywords: &["抖动", "震动", "摇晃", "晃动"] },
    CategoryRule { slug: "other", label: "其他", keywords: &[] },
];

const FILTER_CATEGORY_RULES: &[CategoryRule] = &[
    CategoryRule { slug: "portrait", label: "人像", keywords: &["人像", "美颜", "肤色", "冷白"] },
    CategoryRule { slug: "landscape", label: "风景", keywords: &["风景", "天空", "海", "山", "日落"] },
    CategoryRule { slug: "food", label: "美食", keywords: &["美食", "食物", "甜品"] },
    CategoryRule { slug: "film", label: "胶片", keywords: &["胶片", "复古", "电影"] },
    CategoryRule { slug: "mono", label: "黑白", keywords: &["黑白", "去色"] },
    CategoryRule { slug: "night", label: "夜景", keywords: &["夜", "霓虹", "暗"] },
    CategoryRule { slug: "other", label: "其他", keywords: &[] },
];

const EFFECT_CATEGORY_RULES: &[CategoryRule] = &[
    CategoryRule { slug: "border", label: "边框", keywords: &["边框", "画框"] },
    CategoryRule { slug: "glitch", label: "故障", keywords: &["故障", "闪", "噪"] },
    CategoryRule { slug: "light", label: "光效", keywords: &["光", "发光", "星河"] },
    CategoryRule { slug: "comic", label: "漫画", keywords: &["漫画", "动漫", "手绘"] },
    CategoryRule { slug: "particle", label: "粒子", keywords: &["粒子", "烟花", "花瓣"] },
    CategoryRule { slug: "other", label: "其他", keywords: &[] },
];

const STICKER_CATEGORY_RULES: &[CategoryRule] = &[
    CategoryRule { slug: "emoji", label: "表情", keywords: &["表情", "emoji", "笑脸"] },
    CategoryRule { slug: "arrow", label: "箭头", keywords: &["箭头", "指向"] },
    CategoryRule { slug: "shape", label: "形状", keywords: &["圆", "方", "线", "框"] },
    CategoryRule { slug: "festival", label: "节日", keywords: &["节日", "新年", "圣诞"] },
    CategoryRule { slug: "other", label: "其他", keywords: &[] },
];

const TEXT_CATEGORY_RULES: &[CategoryRule] = &[
    CategoryRule { slug: "enter", label: "入场", keywords: &["入场", "进入"] },
    CategoryRule { slug: "exit", label: "出场", keywords: &["出场", "退出"] },
    CategoryRule { slug: "loop", label: "循环", keywords: &["循环", "强调"] },
    CategoryRule { slug: "other", label: "其他", keywords: &[] },
];

const AUDIO_CATEGORY_RULES: &[CategoryRule] = &[
    CategoryRule { slug: "ambience", label: "环境", keywords: &["雨", "风", "海", "城市"] },
    CategoryRule { slug: "animal", label: "动物", keywords: &["鸟", "猫", "狗"] },
    CategoryRule { slug: "voice", label: "人声", keywords: &["人", "笑", "喊", "呼吸"] },
    CategoryRule { slug: "music", label: "音乐", keywords: &["乐", "鼓", "吉它", "钢琴"] },
    CategoryRule { slug: "other", label: "其他", keywords: &[] },
];

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

    /// 「复制 action」片段里的 action 名，按 Tab 映射到智能体 action plan 的动词：
    /// 转场 `add_transition`、滤镜 `apply_filter`、特效 `add_effect`、贴纸 `add_sticker`、
    /// 文本 `add_text_animation`、音频 `add_audio`。
    ///
    /// 字幕 Tab 读的是 `fonts.json`（字体/字幕样式），不在上面六项映射里；这里用
    /// `apply_text_style` 表示「套用字幕样式」，而不是误写成文本动画。
    fn action(self) -> &'static str {
        match self {
            Self::Transitions => "add_transition",
            Self::Filters => "apply_filter",
            Self::Effects => "add_effect",
            Self::Sticker => "add_sticker",
            Self::Text => "add_text_animation",
            Self::Captions => "apply_text_style",
            Self::Audio => "add_audio",
        }
    }

    /// 这个资源类型的分类规则表。**关键词归类只是浏览辅助，不是剪映官方分类**：官方目录 JSON
    /// 没有分类字段，规则只按名称子串命中，命中顺序从左到右，都未命中落到最后的「其他」。
    fn category_rules(self) -> &'static [CategoryRule] {
        match self {
            Self::Transitions => TRANSITION_CATEGORY_RULES,
            Self::Filters => FILTER_CATEGORY_RULES,
            Self::Effects => EFFECT_CATEGORY_RULES,
            Self::Sticker => STICKER_CATEGORY_RULES,
            Self::Text | Self::Captions => TEXT_CATEGORY_RULES,
            Self::Audio => AUDIO_CATEGORY_RULES,
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

/// 一条资源目录条目。只保留浏览需要的字段：名称、编号、是否会员、可选时长、关键词归类 slug。
#[derive(Clone, Debug, PartialEq, Eq)]
struct ResourceEntry {
    name: String,
    resource_id: String,
    vip: bool,
    /// 目录里的 `duration_us`（微秒）；缺失、非数字或 0 时按「没有时长」处理。
    duration_us: Option<u64>,
    /// 加载时由 [`classify`] 写入的分类 slug；匹配不到规则时为 `other`。
    category: &'static str,
}

/// 一个分类的缓存计数。数量 0 的分类不进入界面。
#[derive(Clone, Debug, PartialEq, Eq)]
struct CategoryCount {
    slug: &'static str,
    label: &'static str,
    count: usize,
}

/// 一次加载的结果：条目、每个分类的计数、总条数。分类与数量只在加载时算一次，随后随目录缓存。
#[derive(Clone, Debug)]
struct Catalog {
    entries: Vec<ResourceEntry>,
    categories: Vec<CategoryCount>,
    total: usize,
}

/// 缓存：(stamp, catalog)。按**路径 + Tab** 做键：`Effects` 与 `Sticker` 共用同一个
/// `video_scene_effects.json`，但两张规则表不同，不能共用归类结果。测试换临时目录、用户换
/// `JIANYINGCRAFT_CATALOGS` 都会自然命中各自的一份；读取失败不缓存，目录稍后出现即可恢复。
/// `stamp` 为 `None` 表示文件已经不存在：此时**不查缓存**，让下一次读取重新报错，目录被删掉
/// 后不会继续画旧列表。
type Stamp = Option<(u64, Option<std::time::SystemTime>)>;
type Cache = RwLock<HashMap<(PathBuf, ResourceTab), (Stamp, Arc<Catalog>)>>;

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
    // 时长是可选的浏览信息：数字或数字字符串都接受，其余（缺失、0、坏字符串）当作没有。
    let duration_us = match value.get("duration_us") {
        Some(Value::Number(us)) => us.as_u64(),
        Some(Value::String(us)) => us.trim().parse::<u64>().ok(),
        _ => None,
    }
    .filter(|us| *us > 0);
    // 分类在 [`build_catalog`] 里统一写入；这里先留空，避免解析和归类两件事纠缠。
    Some(ResourceEntry { name: name.to_string(), resource_id, vip, duration_us, category: "" })
}

/// 按规则表给名称归类：**大小写不敏感**，按规则顺序从左到右取第一个命中，都未命中落到
/// 最后一条「其他」。返回分类 slug。
fn classify(rules: &[CategoryRule], name: &str) -> &'static str {
    let lower = name.to_lowercase();
    for rule in rules {
        if rule.keywords.iter().any(|keyword| lower.contains(keyword)) {
            return rule.slug;
        }
    }
    // 规则表最后一条是无关键词的「其他」；即使表为空也返回一个稳定 slug，绝不索引越界。
    rules.last().map_or("other", |rule| rule.slug)
}

/// 把解析出的条目按 Tab 的规则归类，并一次性算出每个分类的计数。
fn build_catalog(tab: ResourceTab, mut entries: Vec<ResourceEntry>) -> Catalog {
    let rules = tab.category_rules();
    let mut counts = vec![0usize; rules.len()];
    for entry in &mut entries {
        entry.category = classify(rules, &entry.name);
        if let Some(index) = rules.iter().position(|rule| rule.slug == entry.category)
            && let Some(count) = counts.get_mut(index)
        {
            *count = count.saturating_add(1);
        }
    }
    let categories = rules.iter().zip(counts).map(|(rule, count)| CategoryCount { slug: rule.slug, label: rule.label, count }).collect();
    let total = entries.len();
    Catalog { entries, categories, total }
}

/// 读一个 Tab 的目录（带 mtime 缓存）。任何失败都变成可读的错误字符串，由调用方画成空态。
fn load(tab: ResourceTab) -> Result<Arc<Catalog>, String> {
    let path = catalogs_dir().join(tab.file());
    let stamp: Stamp = std::fs::metadata(&path).ok().map(|m| (m.len(), m.modified().ok()));
    let key = (path.clone(), tab);
    {
        let guard = cache().read().unwrap_or_else(PoisonError::into_inner);
        if let Some(current) = stamp
            && let Some((Some(cached), catalog)) = guard.get(&key)
            && *cached == current
        {
            return Ok(Arc::clone(catalog));
        }
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
    let value: Value = serde_json::from_str(&text).map_err(|e| format!("{} 不是合法 JSON：{e}", path.display()))?;
    let Some(array) = value.as_array() else { return Err(format!("{} 的内容不是资源数组", path.display())) };
    let entries: Vec<ResourceEntry> = array.iter().filter_map(parse_entry).collect();
    if entries.is_empty() {
        return Err(format!("{} 里没有可用条目（需要 name 与 resource_id）", path.display()));
    }
    let catalog = Arc::new(build_catalog(tab, entries));
    {
        let mut guard = cache().write().unwrap_or_else(PoisonError::into_inner);
        guard.insert(key, (stamp, Arc::clone(&catalog)));
    }
    Ok(catalog)
}

/// 画这个资源 Tab：左侧分类列、搜索框、chips、密集网格（或可操作的空态）。
pub(crate) fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect, tab: ResourceTab) {
    if rect.width() < 40.0 || rect.height() < 40.0 {
        return;
    }
    let t = app.tokens;
    let pad = 8.0;

    // ---- 底部固定 44 px 详情条：网格区与左侧分类列都到这里为止（详情条盖在面板底部）。
    let detail_height = DETAIL_HEIGHT.min(rect.height());
    let detail = Rect::from_min_max(pos2(rect.min.x, rect.max.y - detail_height), rect.max);
    let detail_top = detail.min.y;

    // ---- 切换 Tab 时分类回到「全部」：记录上次画的是哪个 Tab，只在新 Tab 首次出现时重置。
    if app.ui.resource_category_tab.as_deref() != Some(tab.slug()) {
        app.ui.resource_category_tab = Some(tab.slug().to_string());
        app.ui.resource_category = None;
    }

    // ---- 布局：[分类列 96 px][右侧搜索/chips/网格]
    let cat_x = rect.min.x + pad;
    let content_x = cat_x + CATEGORY_WIDTH + CATEGORY_GAP;
    let content_right = rect.max.x - pad;
    let content_w = (content_right - content_x).max(1.0);

    let loaded = load(tab);
    let catalog = loaded.as_ref().ok().map(|catalog| catalog.as_ref());

    // 目录换掉后，原先选中的分类可能已经空了/不存在：回到「全部」，避免卡在看不见的分类里。
    if let Some(selected) = app.ui.resource_category.as_deref() {
        let still_available = catalog.is_some_and(|catalog| catalog.categories.iter().any(|category| category.slug == selected && category.count > 0));
        if !still_available {
            app.ui.resource_category = None;
        }
    }

    // ---- 左侧分类列：「全部」+ 数量大于 0 的关键词分类（空分类不列）
    let category_rows: Vec<(&'static str, &'static str, usize)> = match catalog {
        Some(catalog) => std::iter::once((ALL_CATEGORY_SLUG, ALL_CATEGORY_LABEL, catalog.total))
            .chain(catalog.categories.iter().filter(|category| category.count > 0).map(|category| (category.slug, category.label, category.count)))
            .collect(),
        None => vec![(ALL_CATEGORY_SLUG, ALL_CATEGORY_LABEL, 0)],
    };
    let cat_bottom = detail_top - 4.0;
    // 分类列在详情条上方独立滚动：Compact 抽屉只有 ~200 px 高，7 个分类放不下时不能丢分类。
    let cat_area = Rect::from_min_max(pos2(cat_x, rect.min.y + 6.0), pos2(cat_x + CATEGORY_WIDTH, cat_bottom));
    if cat_area.height() >= CATEGORY_ROW_HEIGHT {
        let mut cat_ui = ui.new_child(egui::UiBuilder::new().max_rect(cat_area).id_salt(("resource-categories", tab.slug())));
        cat_ui.set_clip_rect(cat_area.intersect(ui.clip_rect()));
        egui::ScrollArea::vertical().id_salt(("resource-category-scroll", tab.slug())).auto_shrink([false, false]).show(&mut cat_ui, |ui| {
            // 行高 26 px 是版式约定（对照截图）；不叠加 egui 的默认行距。
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            for (slug, label, count) in &category_rows {
                let (row, response) = ui.allocate_exact_size(vec2(CATEGORY_WIDTH, CATEGORY_ROW_HEIGHT), Sense::click());
                let selected = if *slug == ALL_CATEGORY_SLUG { app.ui.resource_category.is_none() } else { app.ui.resource_category.as_deref() == Some(*slug) };
                if selected {
                    ui.painter().rect_filled(row, 0.0, t.row_selected);
                    ui.painter().rect_filled(Rect::from_min_size(row.min, vec2(2.0, row.height())), 0.0, t.accent);
                } else if response.hovered() {
                    ui.painter().rect_filled(row, 0.0, t.hover);
                }
                let text_color = if selected { t.text } else { t.text_dim };
                ui.painter().text(pos2(row.min.x + 10.0, row.center().y), Align2::LEFT_CENTER, *label, Tokens::ui(11.0), text_color);
                if *count > 0 {
                    // 右侧留出滚动条的位置，计数不被竖条盖住。
                    ui.painter().text(pos2(row.max.x - 14.0, row.center().y), Align2::RIGHT_CENTER, count.to_string(), Tokens::ui(9.0), t.text_faint);
                }
                app.auto.add(&format!("resource.category.{slug}"), row, label);
                if response.clicked() {
                    app.ui.resource_category = if *slug == ALL_CATEGORY_SLUG { None } else { Some((*slug).to_string()) };
                }
            }
        });
    }

    // ---- 搜索框（22 px，右侧通栏）
    let search = Rect::from_min_size(pos2(content_x, rect.min.y + 6.0), vec2(content_w, 22.0));
    let mut field = ui.new_child(egui::UiBuilder::new().max_rect(search).id_salt(("resource-search", tab.slug())));
    let mut query = app.ui.resource_search.clone();
    crate::widgets::search_field(&mut field, &mut query, "搜索名称/编号", search.width(), &t);
    app.ui.resource_search = query;
    app.auto.add("resource.search", search, "搜索名称/编号");

    // ---- 数据：加载失败先记下错误，仍然画搜索框与分类列让用户能清词/换 Tab 后重试
    let needle = app.ui.resource_search.trim().to_lowercase();
    let filter = app.ui.resource_filter;
    let selected_category = app.ui.resource_category.clone();
    let mut filtered: Vec<&ResourceEntry> = Vec::new();
    let mut total = 0usize;
    if let Some(catalog) = catalog {
        total = catalog.total;
        filtered = catalog
            .entries
            .iter()
            .filter(|entry| selected_category.as_deref().is_none_or(|slug| entry.category == slug))
            .filter(|entry| filter.accepts(entry))
            .filter(|entry| needle.is_empty() || entry.name.to_lowercase().contains(&needle) || entry.resource_id.contains(&needle))
            .collect();
    }

    // ---- chips：全部 / 免费 / 会员；右侧「N 条」= 分类 + 搜索 + chips 叠加后的条数
    let chips_y = search.max.y + 6.0;
    let mut x = content_x;
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
    let count_left = (content_right - 64.0).max(x + 4.0).min(content_right - 4.0);
    let count_rect = Rect::from_min_max(pos2(count_left, chips_y), pos2(content_right, chips_y + 20.0));
    ui.painter().text(pos2(content_right, chips_y + 10.0), Align2::RIGHT_CENTER, &count_label, Tokens::ui(10.5), t.text_dim);
    app.auto.add("resource.count", count_rect, &count_label);
    let mut body_top = chips_y + 24.0;

    // ---- 可选的说明行（贴纸复用特效目录）
    if let Some(note) = tab.note()
        && body_top + 12.0 < detail_top
    {
        let note_rect = Rect::from_min_size(pos2(content_x, body_top), vec2(content_w, 12.0));
        ui.painter().text(pos2(content_x, body_top + 6.0), Align2::LEFT_CENTER, note, Tokens::ui(9.0), t.text_faint);
        app.auto.add("resource.note", note_rect, note);
        body_top += 14.0;
    }

    if body_top + 8.0 >= detail_top {
        show_detail(app, ui, detail, tab, catalog, &t);
        return;
    }

    // ---- 空态：目录不可用 / 没有匹配项
    let empty = match &loaded {
        Err(error) => Some(format!("{error}\n请设置环境变量 JIANYINGCRAFT_CATALOGS 指向 jianyingcraft-plugin/catalogs，\n或把 catalogs/ 放到工作目录后重试。")),
        Ok(_) if filtered.is_empty() && total > 0 => Some("没有匹配的资源；换个关键词或分类，或点「全部」看整库。".to_string()),
        Ok(_) => None,
    };
    if let Some(message) = empty {
        let area = Rect::from_min_max(pos2(content_x, body_top), pos2(content_right, detail_top));
        app.auto.add("resource.empty", area, &message);
        let galley = ui.painter().layout(message, Tokens::ui(11.0), t.text_dim, area.width());
        let y = (area.center().y - galley.size().y / 2.0).max(area.min.y);
        ui.painter().galley(pos2(area.min.x + 4.0, y), galley, t.text_dim);
        show_detail(app, ui, detail, tab, catalog, &t);
        return;
    }

    // ---- 网格（6 列方形卡片，可滚动；只占右侧内容区，底部让给详情条）
    let body = Rect::from_min_max(pos2(content_x, body_top), pos2(content_right, detail_top));
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
    show_detail(app, ui, detail, tab, catalog, &t);
}

/// 面板底部 44 px 详情条：选中资源的名称（11 px 白）、完整 `resource_id`（10 px 等宽灰）、
/// `会员`/`免费` 标签与可选时长，右侧 68×26 的「复制 action」按钮（accent 底）。
///
/// 点按钮把 `{"action":…,"resource_id":…,"resource_name":…}` 写进剪贴板，并在状态栏回显
/// `已复制 action 片段：<action> <resource_id>`；不执行任何编辑。没有选中、或选中的编号不在
/// 当前目录里（例如切了 Tab）时只显示提示——详情条不猜名称。
fn show_detail(app: &mut FilmcraftApp, ui: &egui::Ui, detail: Rect, tab: ResourceTab, catalog: Option<&Catalog>, t: &Tokens) {
    let pad = 8.0;
    ui.painter().rect_filled(detail, 0.0, t.app_bg);
    ui.painter().hline(detail.x_range(), detail.min.y, Stroke::new(1.0, t.separator));

    // 右侧按钮位置固定；左边文字区的可用宽度到它（或会员标签）为止。
    let button = Rect::from_min_size(pos2(detail.max.x - pad - COPY_WIDTH, detail.center().y - COPY_HEIGHT / 2.0), vec2(COPY_WIDTH, COPY_HEIGHT));
    let selection = app.ui.resource_selection.clone();
    let entry = selection.as_deref().and_then(|selected| catalog.and_then(|catalog| catalog.entries.iter().find(|entry| entry.resource_id == selected)));

    let response = ui.interact(button, egui::Id::new(("resource-copy-action", tab.slug())), Sense::click());
    let armed = entry.is_some();
    let button_bg = match (armed, response.hovered()) {
        (true, true) => t.accent_hover,
        (true, false) => t.accent,
        (false, _) => t.hover,
    };
    ui.painter().rect_filled(button, 6.0, button_bg);
    ui.painter().text(button.center(), Align2::CENTER_CENTER, "复制 action", Tokens::ui(11.0), if armed { Color32::WHITE } else { t.text_faint });
    app.auto.add("resource.copyAction", button, "复制 action");

    let Some(entry) = entry else {
        ui.painter().text(pos2(detail.min.x + pad, detail.center().y), Align2::LEFT_CENTER, DETAIL_HINT, Tokens::ui(11.0), t.text_dim);
        app.auto.add("resource.detail", detail, DETAIL_HINT);
        return;
    };

    // 会员/免费标签；有 duration_us 时并排显示秒数（如 1.5s）。
    let mut meta = if entry.vip { "会员".to_string() } else { "免费".to_string() };
    if let Some(duration_us) = entry.duration_us {
        meta.push_str(" · ");
        meta.push_str(&seconds_label(duration_us));
    }
    let meta_w = (ui.painter().layout_no_wrap(meta.clone(), Tokens::ui(9.5), t.text_dim).size().x + 12.0).max(28.0);
    let meta_rect = Rect::from_min_size(pos2(button.min.x - pad - meta_w, detail.min.y + 13.0 - 8.0), vec2(meta_w, 16.0));
    if entry.vip {
        ui.painter().rect_filled(meta_rect, 8.0, Color32::from_rgb(0x4a, 0x36, 0x1c));
        ui.painter().text(meta_rect.center(), Align2::CENTER_CENTER, &meta, Tokens::ui(9.5), Color32::from_rgb(0xf2, 0xd0, 0x8c));
    } else {
        ui.painter().rect_filled(meta_rect, 8.0, t.hover);
        ui.painter().rect_stroke(meta_rect, 8.0, Stroke::new(1.0, t.separator), StrokeKind::Inside);
        ui.painter().text(meta_rect.center(), Align2::CENTER_CENTER, &meta, Tokens::ui(9.5), t.text_dim);
    }
    app.auto.add("resource.detail.meta", meta_rect, &meta);

    // 名称 11 px 白字（放不下就省略）、完整编号 10 px 等宽灰字。
    let text_left = detail.min.x + pad;
    let name_right = (meta_rect.min.x - 6.0).max(text_left + 1.0);
    let name = elide(ui, &entry.name, 11.0, name_right - text_left);
    let name_rect = Rect::from_min_max(pos2(text_left, detail.min.y + 5.0), pos2(name_right, detail.min.y + 21.0));
    ui.painter().text(pos2(text_left, detail.min.y + 13.0), Align2::LEFT_CENTER, &name, Tokens::ui(11.0), Color32::WHITE);
    app.auto.add("resource.detail.name", name_rect, &entry.name);

    let id_right = (button.min.x - pad).max(text_left + 1.0);
    let id_rect = Rect::from_min_max(pos2(text_left, detail.min.y + 22.0), pos2(id_right, detail.max.y - 1.0));
    ui.painter().text(pos2(text_left, detail.min.y + 30.0), Align2::LEFT_CENTER, &entry.resource_id, Tokens::mono(10.0), t.text_dim);
    app.auto.add("resource.detail.id", id_rect, &entry.resource_id);
    app.auto.add("resource.detail", detail, &entry.name);

    if response.clicked() {
        // 只写剪贴板与状态栏：复制 action 片段给智能体，不改工程。
        ui.ctx().copy_text(action_fragment(tab, entry));
        app.ui.status = format!("已复制 action 片段：{} {}", tab.action(), entry.resource_id);
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

/// 时长（微秒）→ 秒数文本，保留一位小数（`1_500_000` → `1.5s`）。用整数十分位计算，
/// 不经过浮点，四舍五入行为稳定。
fn seconds_label(duration_us: u64) -> String {
    let tenths = duration_us.saturating_add(50_000) / 100_000;
    format!("{}.{}s", tenths / 10, tenths % 10)
}

/// 「复制 action」写进剪贴板的 JSON 片段，形如
/// `{"action":"add_transition","resource_id":"7049667406656014","resource_name":"3D空间"}`。
fn action_fragment(tab: ResourceTab, entry: &ResourceEntry) -> String {
    serde_json::json!({
        "action": tab.action(),
        "resource_id": &entry.resource_id,
        "resource_name": &entry.name,
    })
    .to_string()
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
    use super::{ResourceTab, action_fragment, build_catalog, classify, hash64, last_chars, parse_entry, seconds_label};
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
    fn durations_are_optional_and_render_as_seconds() {
        assert_eq!(seconds_label(1_500_000), "1.5s");
        assert_eq!(seconds_label(2_000_000), "2.0s");
        assert_eq!(seconds_label(0), "0.0s");
        let with_duration = parse_entry(&json!({"name": "x", "resource_id": 1, "duration_us": 1_500_000})).expect("numeric duration");
        assert_eq!(with_duration.duration_us, Some(1_500_000));
        let string_duration = parse_entry(&json!({"name": "x", "resource_id": 1, "duration_us": "2500000"})).expect("string duration");
        assert_eq!(string_duration.duration_us, Some(2_500_000));
        // 缺失、0、坏字符串都只是「没有时长」，不是错误。
        for value in [
            json!({"name": "x", "resource_id": 1}),
            json!({"name": "x", "resource_id": 1, "duration_us": 0}),
            json!({"name": "x", "resource_id": 1, "duration_us": "oops"}),
        ] {
            assert!(parse_entry(&value).expect("entry without duration").duration_us.is_none(), "{value}");
        }
    }

    #[test]
    fn action_fragments_follow_the_tab_and_carry_name_and_id() {
        let entry = parse_entry(&json!({"name": "3D空间", "resource_id": "7049667406656014"})).expect("fixture entry");
        assert_eq!(ResourceTab::Transitions.action(), "add_transition");
        assert_eq!(ResourceTab::Filters.action(), "apply_filter");
        assert_eq!(ResourceTab::Effects.action(), "add_effect");
        assert_eq!(ResourceTab::Sticker.action(), "add_sticker");
        assert_eq!(ResourceTab::Text.action(), "add_text_animation");
        assert_eq!(ResourceTab::Audio.action(), "add_audio");
        assert_eq!(
            action_fragment(ResourceTab::Transitions, &entry),
            r#"{"action":"add_transition","resource_id":"7049667406656014","resource_name":"3D空间"}"#
        );
        assert_eq!(action_fragment(ResourceTab::Effects, &entry), r#"{"action":"add_effect","resource_id":"7049667406656014","resource_name":"3D空间"}"#);
    }

    #[test]
    fn keyword_classification_follows_the_rule_tables() {
        let transitions = ResourceTab::Transitions.category_rules();
        assert_eq!(classify(transitions, "叠化"), "dissolve");
        assert_eq!(classify(transitions, "上下翻页"), "slide");
        assert_eq!(classify(transitions, "推近"), "motion");
        assert_eq!(classify(transitions, "模糊"), "blur");
        assert_eq!(classify(transitions, "故障闪屏"), "glitch");
        assert_eq!(classify(transitions, "星光"), "light");
        assert_eq!(classify(transitions, "找不到规则的名字"), "other");

        let filters = ResourceTab::Filters.category_rules();
        assert_eq!(classify(filters, "冷白人像"), "portrait");
        assert_eq!(classify(filters, "海边日落"), "landscape");
        assert_eq!(classify(filters, "美食甜品"), "food");
        assert_eq!(classify(filters, "复古胶片"), "film");
        assert_eq!(classify(filters, "黑白去色"), "mono");
        assert_eq!(classify(filters, "夜景霓虹"), "night");

        let effects = ResourceTab::Effects.category_rules();
        assert_eq!(classify(effects, "金色边框"), "border");
        assert_eq!(classify(effects, "故障闪屏"), "glitch");
        assert_eq!(classify(effects, "发光星河"), "light");
        assert_eq!(classify(effects, "漫画手绘"), "comic");
        assert_eq!(classify(effects, "烟花粒子"), "particle");

        // 英文关键词与大小写：贴纸的 `emoji`。
        let stickers = ResourceTab::Sticker.category_rules();
        assert_eq!(classify(stickers, "Emoji 笑脸"), "emoji");
        assert_eq!(classify(stickers, "节日圣诞"), "festival");

        let text = ResourceTab::Text.category_rules();
        assert_eq!(classify(text, "入场进入"), "enter");
        assert_eq!(classify(text, "出场退出"), "exit");
        assert_eq!(classify(text, "循环强调"), "loop");

        let audio = ResourceTab::Audio.category_rules();
        assert_eq!(classify(audio, "城市雨声"), "ambience");
        assert_eq!(classify(audio, "猫狗鸟"), "animal");
        assert_eq!(classify(audio, "笑声呼吸"), "voice");
        assert_eq!(classify(audio, "吉它"), "music");
        assert_eq!(classify(audio, "音乐鼓点"), "music");
    }

    #[test]
    fn catalog_counts_categories_once_and_hides_empty_ones() {
        let entries = ["叠化", "上下翻页", "推近", "模糊", "故障闪屏", "星光"]
            .iter()
            .enumerate()
            .filter_map(|(index, name)| parse_entry(&json!({"name": name, "resource_id": index + 1})))
            .collect();
        let catalog = build_catalog(ResourceTab::Transitions, entries);
        assert_eq!(catalog.total, 6);
        let nonempty: Vec<_> = catalog.categories.iter().filter(|category| category.count > 0).map(|category| (category.slug, category.count)).collect();
        assert_eq!(
            nonempty,
            vec![("dissolve", 1), ("slide", 1), ("motion", 1), ("blur", 1), ("glitch", 1), ("light", 1)],
            "the six fixtures hit six different transition rules; `other` stays empty"
        );
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
