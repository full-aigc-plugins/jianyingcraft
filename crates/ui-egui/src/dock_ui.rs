//! JianyingCraft's panel chrome and document tabs over the shared docking transactions.

use craft_ui::docking::{Action, DropTarget, Placement, Zone, resolve_drop};
use egui::{Color32, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use serde_json::{Value, json};

use crate::dock::{self, DockAction, DockNode, Group, PanelKind};
use crate::icons::{self, Icon};
use crate::layout::LayoutClass;
use crate::menus::MenuItem;
use crate::theme::Tokens;
use crate::{FilmcraftApp, panels};

/// Single source for rendering and hit testing: saved offscreen positions remain
/// reachable, and every panel has enough room for its chrome.
fn floating_rect(group: &dock::Floating, screen: Rect) -> Rect {
    let rect = Rect::from_min_size(pos2(group.rect[0], group.rect[1]), vec2(group.rect[2].max(160.0), group.rect[3].max(100.0)));
    rect.translate(vec2(
        (screen.min.x - rect.min.x).max(0.0) + (screen.max.x - 40.0 - rect.min.x).min(0.0),
        (screen.min.y - rect.min.y).max(0.0) + (screen.max.y - 30.0 - rect.min.y).min(0.0),
    ))
}

fn drag_id() -> egui::Id {
    egui::Id::new("dock-panel-drag")
}

/// 弹层的竖直安放：先量锚点上下两侧的真实可用空间。
///
/// 下方放得下期望高度就挂在锚点下；放不下、并且上方比下方更宽裕时翻到锚点上方
/// （`anchor.min.y - gap - height`）。高度一律取 `min(期望, 可用)`，所以返回值保证落在
/// `bounds` 的竖直范围内——矮窗口里是"变矮"，而不是"压出 body"。
fn popover_top_and_height(anchor: Rect, desired_height: f32, bounds: Rect, gap: f32) -> (f32, f32) {
    let below = (bounds.max.y - (anchor.max.y + gap)).max(0.0);
    let above = (anchor.min.y - gap - bounds.min.y).max(0.0);
    let desired = desired_height.max(0.0);
    // 上方没有更多空间时宁可在下方变矮：翻上去只会更挤。
    if desired <= below || below >= above {
        (anchor.max.y + gap, desired.min(below))
    } else {
        let height = desired.min(above);
        (anchor.min.y - gap - height, height)
    }
}

pub(crate) fn route(app: &mut FilmcraftApp, ctx: &egui::Context, id: &str, params: &Value) -> Option<Result<Value, String>> {
    // 一级 Tab 的唯一命令入口：顶栏直排的 3 个和「更多」里的 10 个都走这里，
    // 因此 `engine.execute` / `ui.menu.invoke` / 顶栏点击是同一条路径。
    if id == "window.shellTab" {
        return Some((|| {
            let slug = params.get("tab").and_then(Value::as_str).ok_or("need `tab`")?;
            app.shell_tab_activate(slug)?;
            Ok(json!({"tab": slug, "panel": app.ui.jy_shell_panel.map(|panel| panel.id())}))
        })());
    }
    if id == "window.dockAction" {
        return Some((|| {
            let value = params.get("action").cloned().ok_or("missing docking action")?;
            let action = serde_json::from_value(value).map_err(|error| format!("invalid docking action: {error}"))?;
            app.apply_dock(action)?;
            Ok(Value::Null)
        })());
    }
    if !matches!(id, "window.dockPanel" | "window.floatPanel" | "window.closePanel") {
        return None;
    }
    Some((|| {
        let panel = match params.get("panel") {
            None => app.ui.focused,
            Some(value) => value.as_str().and_then(PanelKind::from_name).ok_or("unknown panel")?,
        };
        let action = match id {
            "window.closePanel" => Action::Close { panel },
            "window.floatPanel" => {
                let rect = match params.get("rect") {
                    None => default_float_rect(ctx.content_rect()),
                    Some(value) => serde_json::from_value::<[f32; 4]>(value.clone()).map_err(|_| "rect must contain four finite numbers")?,
                };
                Action::Float { panel, rect }
            }
            _ => {
                let zone_name = match params.get("zone") {
                    None => "center",
                    Some(value) => value.as_str().ok_or("zone must be a string")?,
                };
                let zone = match zone_name.to_ascii_lowercase().as_str() {
                    "left" => Zone::Left,
                    "right" => Zone::Right,
                    "top" => Zone::Top,
                    "bottom" => Zone::Bottom,
                    "center" | "tab" => Zone::Center,
                    _ => return Err("unknown docking zone".into()),
                };
                let Some(anchor) = params.get("anchor") else {
                    if params.get("before").is_some() {
                        return Err("before needs an anchor panel".into());
                    }
                    app.dock_to_edge(panel, zone)?;
                    return Ok(json!({"panel": panel.id()}));
                };
                let anchor = anchor.as_str().and_then(PanelKind::from_name).ok_or("a known anchor panel is required")?;
                let placement = match params.get("before") {
                    Some(value) => Placement::Tab { before: Some(value.as_str().and_then(PanelKind::from_name).ok_or("unknown before panel")?) },
                    None => Placement::Split(zone),
                };
                Action::Move { panel, anchor, placement }
            }
        };
        app.apply_dock(action)?;
        Ok(json!({"panel": panel.id()}))
    })())
}

pub(crate) fn default_float_rect(screen: Rect) -> [f32; 4] {
    let center = screen.center();
    [center.x - 210.0, center.y - 160.0, 420.0, 320.0]
}

/// 顶栏一条 Tab 的静态描述：(标签, slug, 自动化 id, 目标面板)。
/// `None` 表示本仓库没有对应模块（灰显、点了不打开任何面板）。
pub(crate) type ShellTab = (&'static str, &'static str, &'static str, Option<PanelKind>);

/// 剪映式一级 Tab 的完整登记表（13 条）。
///
/// 顶栏只横排前 [`SHELL_PRIMARY_COUNT`] 个主 Tab（剪辑/素材、字幕/文本、音频），其余 10 个收进
/// 「更多」溢出菜单——它们仍然可点、可自动化，只是不再全部挤在顶栏里
/// （`docs/derivation/02-ia-mapping.md` §7.7：13 Tab 收敛为 3 个主 Tab + "更多"溢出菜单）。
pub(crate) static SHELL_TABS: [ShellTab; 13] = [
    ("素材", "cut", "jy.tab.cut", Some(PanelKind::Project)),
    ("文本", "captions", "jy.tab.captions", Some(PanelKind::Text)),
    ("音频", "audio", "jy.tab.audio", Some(PanelKind::AudioTrackMixer)),
    ("贴纸", "sticker", "jy.tab.sticker", Some(PanelKind::EssentialGraphics)),
    ("特效", "effects", "jy.tab.effects", Some(PanelKind::Effects)),
    ("转场", "transitions", "jy.tab.transitions", Some(PanelKind::Effects)),
    ("字幕", "subtitle", "jy.tab.subtitle", Some(PanelKind::Text)),
    ("智能包装", "smart", "jy.tab.smart", None),
    ("滤镜", "filters", "jy.tab.filters", Some(PanelKind::LumetriColor)),
    ("调节", "adjust", "jy.tab.adjust", Some(PanelKind::LumetriColor)),
    ("模板", "templates", "jy.tab.templates", None),
    ("数字人", "avatar", "jy.tab.avatar", None),
    ("插件", "plugins", "jy.tab.plugins", None),
];

/// 顶栏直排的主 Tab 数量：前 3 个（剪辑/素材、字幕/文本、音频）。
pub(crate) const SHELL_PRIMARY_COUNT: usize = 3;

/// 顶栏直排的主 Tab（剪辑 / 字幕 / 音频），顺序即绘制顺序。
pub(crate) fn shell_primary_tabs() -> &'static [ShellTab] {
    &SHELL_TABS[..SHELL_PRIMARY_COUNT]
}

/// 「更多」溢出菜单里的 10 个 Tab（贴纸 / 特效 / 转场 / 字幕 / 智能包装 / 滤镜 / 调节 /
/// 模板 / 数字人 / 插件），顺序即菜单顺序。
pub(crate) fn shell_overflow_tabs() -> &'static [ShellTab] {
    &SHELL_TABS[SHELL_PRIMARY_COUNT..]
}

/// 该 slug 是否画在顶栏上（否则在「更多」里）。未知 slug 也算溢出（调用方会按未实现处理）。
pub(crate) fn shell_tab_is_primary(slug: &str) -> bool {
    shell_primary_tabs().iter().any(|tab| tab.1 == slug)
}

/// 顶栏 Tab 的 slug → 面板；`None` 表示未实现（调用方应保持原状）。
pub fn shell_panel(slug: &str) -> Option<PanelKind> {
    SHELL_TABS.iter().find(|tab| tab.1 == slug).and_then(|tab| tab.3)
}

/// 顶栏 Tab 的左右循环：跳过未实现的 Tab，首尾相接。`step` 是 +1 / -1。
///
/// 循环范围是**整张登记表**（主 Tab + 「更多」里的溢出 Tab），否则 Alt+←/→ 只能摸到前三个，
/// 溢出里的入口就只剩鼠标一条路。
pub fn shell_tab_step(current: Option<&str>, step: i32) -> Option<(&'static str, PanelKind)> {
    let enabled: Vec<&'static ShellTab> = SHELL_TABS.iter().filter(|tab| tab.3.is_some()).collect();
    if enabled.is_empty() {
        return None;
    }
    let index = current.and_then(|slug| enabled.iter().position(|tab| tab.1 == slug));
    let next = match (index, step.signum()) {
        (Some(index), step) => (index as i32 + step).rem_euclid(enabled.len() as i32) as usize,
        (None, step) if step < 0 => enabled.len() - 1,
        (None, _) => 0,
    };
    let tab = enabled[next];
    Some((tab.1, tab.3?))
}

impl FilmcraftApp {
    /// 选中顶栏的一级 Tab：记录 slug 与目标面板，并把抽屉切到素材区（沿用旧契约）。
    ///
    /// 未知 slug 和没有实现的模块（目标为 `None`）都返回错误——灰显的入口不能被程序打开。
    /// 顶栏点击、`window.shellTab` 命令和控制通道的 `ui.shell.tab` 共用这一处状态迁移，
    /// 所以三者不会各写一套。
    pub fn shell_tab_activate(&mut self, slug: &str) -> Result<(), String> {
        let Some(panel) = shell_panel(slug) else { return Err("unknown or unimplemented `tab`".into()) };
        // 前三个主 Tab 沿用旧的 jy_tab 取值（其它 Tab 只记 shell panel）。
        self.ui.jy_tab = match slug {
            "cut" => crate::layout::Tab::Cut,
            "captions" => crate::layout::Tab::Captions,
            "audio" => crate::layout::Tab::Audio,
            _ => self.ui.jy_tab,
        };
        self.ui.jy_shell_panel = Some(panel);
        self.ui.jy_shell_slug = Some(slug.to_string());
        // 三个入口（`ui.shell.tab` / `ui.shell.tab.step` / Alt+←→）共用一个状态出口：
        // 切到溢出里的 Tab 时自动展开「更多」，否则选中的入口在屏幕上点不到。
        self.ui.jy_tab_more = !shell_tab_is_primary(slug);
        self.ui.jy_drawer = Some(crate::layout::Drawer::Media);
        // 剪映的"音频"Tab 只列音频素材，"素材"Tab 恢复全部。
        self.ui.jy_media_filter = (slug == "audio").then(|| "audio".to_string());
        Ok(())
    }

    fn dock_command(&mut self, ctx: &egui::Context, action: Action<PanelKind>) -> Result<(), String> {
        let value = serde_json::to_value(action).map_err(|error| error.to_string())?;
        crate::menus::invoke(self, ctx, "window.dockAction", json!({"action": value})).map(|_| ())
    }

    pub fn apply_dock(&mut self, action: Action<PanelKind>) -> Result<(), String> {
        let relocating = matches!(&action, Action::Move { .. } | Action::Float { .. });
        let reveal = match &action {
            Action::Activate { panel } => Some(*panel),
            _ => None,
        };
        self.commit_dock(|shared| {
            crate::dock_shared::check_last_docked(shared, &action)?;
            shared.apply(action).map_err(|error| error.to_string())
        })?;
        if let Some(panel) = reveal {
            self.pending_panel_raise = Some(panel);
        }
        if relocating
            || self.ui.keys.maximized.is_some_and(|panel| !self.ui.dock.contains(panel) && !self.ui.floating.iter().any(|group| group.panels.contains(&panel)))
        {
            self.ui.keys.maximized = None;
        }
        Ok(())
    }

    /// Dock a panel along an edge of the whole workspace rather than beside one group.
    pub fn dock_to_edge(&mut self, panel: PanelKind, zone: Zone) -> Result<(), String> {
        self.commit_dock(|shared| crate::dock_shared::dock_at_edge(shared, panel, zone))?;
        self.ui.keys.maximized = None;
        Ok(())
    }

    fn commit_dock(&mut self, change: impl FnOnce(&mut craft_ui::docking::Layout<PanelKind>) -> Result<(), String>) -> Result<(), String> {
        let mut shared = crate::dock_shared::encode(&self.ui.dock, &self.ui.floating)?;
        shared.validate().map_err(|error| error.to_string())?;
        change(&mut shared)?;
        let layout = crate::dock_shared::decode(shared)?;
        crate::dock_shared::encode(&layout.root, &layout.floating)?;
        self.ui.dock = layout.root;
        self.ui.floating = layout.floating;
        Ok(())
    }

    fn panel_body(&mut self, ui: &mut egui::Ui, panel: PanelKind, content: Rect, class: LayoutClass) {
        let t = self.tokens;
        let mut body = content;
        // 剪映专业版里工具条贴在时间轴上方；这里只在 `Jianying` 工作区给 Timeline 让出 38 px。
        // Compact/Medium 的工具条由 `shell_area` 画在时间轴上方；这里只负责 Wide，
        // 否则同一帧会画两次（重复的 egui id 还会让界面叠出红色告警）。
        // 档位用本帧统一算出的 `class`，不再按面板内容区宽度重算（body 比窗口窄 2 px）。
        let wide = class == LayoutClass::Wide;
        if wide && panel == PanelKind::Timeline && self.ui.workspace == "Jianying" && content.height() > 240.0 {
            let toolbar_height = 38.0_f32.min(content.height() * 0.12);
            let toolbar = Rect::from_min_size(content.min, egui::vec2(content.width(), toolbar_height));
            self.timeline_toolbar(ui, toolbar, &t);
            body = Rect::from_min_max(egui::pos2(content.min.x, toolbar.max.y), content.max);
        }
        self.auto.add(&format!("panel.{}", panel.id()), body, panel.title());
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body).id_salt(("panel", panel.id())));
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        panels::show(self, &mut child, panel, body);
    }

    /// OS drag positions come from the host cursor hook, so egui pointer capture alone
    /// cannot decide whether a Timeline is covered. Use the same geometry and layer
    /// ordering as our renderer before any panel can dispatch an edit.
    pub(crate) fn timeline_drop_target(&self, ctx: &egui::Context, pos: egui::Pos2) -> bool {
        if !ctx.content_rect().contains(pos) {
            return false;
        }
        if let Some(panel) =
            self.ui.keys.maximized.filter(|panel| self.ui.dock.contains(*panel) || self.ui.floating.iter().any(|group| group.panels.contains(panel)))
        {
            return panel == PanelKind::Timeline;
        }
        for index in self.floating_order(ctx).into_iter().rev() {
            if let Some(group) = self.ui.floating.get(index)
                && floating_rect(group, ctx.content_rect()).contains(pos)
            {
                return group.panels.get(group.active) == Some(&PanelKind::Timeline);
            }
        }
        true
    }

    fn floating_order(&self, ctx: &egui::Context) -> Vec<usize> {
        let layers = ctx.memory(|memory| memory.layer_ids().collect::<Vec<_>>());
        let mut order: Vec<_> = (0..self.ui.floating.len()).collect();
        order.sort_by_key(|index| {
            self.ui
                .floating
                .get(*index)
                .and_then(|group| group.panels.first())
                .and_then(|panel| {
                    let id = egui::Id::new(("floating-panel", panel.id()));
                    layers.iter().position(|layer| layer.id == id)
                })
                .unwrap_or(usize::MAX)
        });
        order
    }

    /// 剪映式单页布局（Compact / Medium）：顶部条 + 预览 +（可选）抽屉 + 底部时间轴。
    ///
    /// 宽屏（Wide）仍走 dock 树；本函数是 Compact/Medium 下唯一的主区绘制入口。
    /// 时间轴下限 220 px（Medium 240 px），上限为可用高度的 55%；抽屉只压缩预览区。
    fn shell_area(&mut self, ui: &mut egui::Ui, body: Rect, class: LayoutClass) {
        let t = self.tokens;
        let medium = class == LayoutClass::Medium;
        let header_height = 44.0_f32.min(body.height());
        let header = Rect::from_min_size(body.min, egui::vec2(body.width(), header_height));
        let available = (body.height() - header_height).max(0.0);
        let min_timeline = if medium { 240.0 } else { 220.0 };
        let timeline_height = (available * 0.38).max(min_timeline).min(available * 0.55);
        let timeline = Rect::from_min_max(egui::pos2(body.min.x, body.max.y - timeline_height), body.max);
        // 剪映式底部工具条：撤销/重做/分割/删除/播放。只占 Compact，Medium 用侧栏不挤竖向空间。
        // 剪映平板/手机都在时间轴上方放工具条；Medium 的侧栏占横向，竖向仍留得下 36 px。
        let toolbar_height = if medium { 36.0_f32.min(available * 0.1) } else { 40.0_f32.min(available * 0.1) };
        let toolbar = (toolbar_height > 24.0)
            .then(|| Rect::from_min_max(egui::pos2(body.min.x, timeline.min.y - toolbar_height), egui::pos2(body.max.x, timeline.min.y)));
        let content_bottom = toolbar.map_or(timeline.min.y, |rect| rect.min.y);
        let content = Rect::from_min_max(egui::pos2(header.min.x, header.max.y), egui::pos2(body.max.x, content_bottom));
        // 顶部条：一级 Tab（剪辑/字幕/音频）在左，属性开关在右。
        ui.painter().rect_filled(header, 0.0, t.tl_header_bg);
        self.auto.add("jy.topbar", header, crate::brand::PRODUCT_NAME);
        // ☰：全局菜单的唯一入口（Compact/Medium 不再画第二条顶栏）。
        let menu_rect = Rect::from_min_size(egui::pos2(header.min.x + 8.0, header.center().y - 14.0), egui::vec2(30.0, 28.0));
        let menu_response = ui.interact(menu_rect, egui::Id::new("jy.menu"), Sense::click());
        ui.painter().rect_filled(menu_rect, 6.0, if self.ui.jy_menu_open || menu_response.hovered() { t.accent_hover } else { t.hover });
        for line in 0..3 {
            let y = menu_rect.center().y - 5.0 + line as f32 * 5.0;
            ui.painter().line_segment([egui::pos2(menu_rect.min.x + 9.0, y), egui::pos2(menu_rect.max.x - 9.0, y)], egui::Stroke::new(1.5, t.text));
        }
        self.auto.add("jy.menu", menu_rect, "Menu");
        if menu_response.clicked() {
            self.ui.jy_menu_open = !self.ui.jy_menu_open;
            if self.ui.jy_menu_open {
                // 每次打开都从菜单根开始，不继承上次展开的分组；两个弹层不叠加。
                self.ui.jy_menu_group = None;
                self.ui.jy_tab_more = false;
            }
        }
        // 剪映手机端的顶栏只排 3 个主 Tab（剪辑/素材、字幕/文本、音频），其余 10 个收进「更多」：
        // 390 px 下 13 个横排会被裁掉大半（`docs/derivation/02-ia-mapping.md` §7.7）。
        // 前三个保持旧 id（jy.tab.cut/captions/audio）以维持既有状态与测试契约。
        // 右侧要同时容纳「属性」和「导出」，Tab 条必须让开两者（96 + 52 + 间距）。
        let inspector_width = 160.0_f32;
        let strip_rect = Rect::from_min_max(
            egui::pos2(menu_rect.max.x + 6.0, header.min.y),
            egui::pos2((header.max.x - 8.0 - inspector_width - 6.0).max(menu_rect.max.x + 40.0), header.max.y),
        );
        let entries = shell_primary_tabs();
        let mut chosen: Option<(String, bool)> = None;
        // 控制通道要求"滚到某个 Tab"时，本帧把它滚进可视区。
        let pending_scroll = self.ui.jy_tab_scroll.clone();
        // Alt+←/→ 切一级 Tab（与剪映桌面一致）；跳过未实现的 Tab，并把目标滚进可视区。
        let step = ui.input(|input| {
            if !input.modifiers.alt {
                return 0;
            }
            if input.key_pressed(egui::Key::ArrowRight) {
                1
            } else if input.key_pressed(egui::Key::ArrowLeft) {
                -1
            } else {
                0
            }
        });
        if step != 0
            && let Some((slug, _panel)) = shell_tab_step(self.ui.jy_shell_slug.as_deref(), step)
        {
            let _ = self.shell_tab_activate(slug);
            self.ui.jy_tab_scroll = Some(slug.to_string());
        }
        let mut more_anchor: Option<Rect> = None;
        let mut strip = ui.new_child(egui::UiBuilder::new().max_rect(strip_rect).id_salt("jy-compact-tabs"));
        strip.set_clip_rect(strip_rect);
        egui::ScrollArea::horizontal().id_salt("jy-compact-tabs-scroll").auto_shrink([false, true]).show(&mut strip, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for &(label, slug, id, panel) in entries {
                    let enabled = panel.is_some();
                    // 按 slug 判定高亮：溢出里的「字幕」与主 Tab 的「文本」共用 Text 面板，
                    // 只看面板会让两个入口同时亮。
                    let active = self.ui.jy_shell_slug.as_deref() == Some(slug) && self.ui.jy_drawer.is_some();
                    let color = if active {
                        Color32::WHITE
                    } else if enabled {
                        t.text
                    } else {
                        t.text_dim
                    };
                    let galley = ui.painter().layout_no_wrap(label.to_string(), Tokens::ui(12.0), color);
                    // 12 px（而不是旧 18 px）内边距：390 px 下 3 个主 Tab + 「更多」必须整条放得下，
                    // 被 ScrollArea 裁掉的按钮既点不到、`ui.click` 也命中不了。
                    let width = galley.size().x + 12.0;
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 28.0), if enabled { Sense::click() } else { Sense::hover() });
                    if pending_scroll.as_deref() == Some(slug) {
                        ui.scroll_to_rect(rect, Some(egui::Align::Center));
                    }
                    if active {
                        ui.painter().rect_filled(rect, 6.0, t.accent);
                    } else if enabled && response.hovered() {
                        ui.painter().rect_filled(rect, 6.0, t.accent_hover);
                    } else if enabled {
                        ui.painter().rect_filled(rect, 6.0, t.hover);
                    }
                    ui.painter().galley(egui::pos2(rect.center().x - galley.size().x / 2.0, rect.center().y - galley.size().y / 2.0), galley, color);
                    self.auto.add(id, rect, label);
                    if enabled && response.clicked() {
                        chosen = Some((slug.to_string(), active));
                    }
                }
                // 「更多」：其余 10 个 Tab 的溢出入口。选中溢出里的 Tab 时它也跟着高亮，
                // 否则顶栏会一条都不亮、看不出当前在哪。
                let more_active = self.ui.jy_shell_slug.as_deref().is_some_and(|slug| !shell_tab_is_primary(slug));
                let color = if more_active || self.ui.jy_tab_more { Color32::WHITE } else { t.text };
                let galley = ui.painter().layout_no_wrap("更多".to_string(), Tokens::ui(12.0), color);
                let width = galley.size().x + 12.0;
                let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 28.0), Sense::click());
                if pending_scroll.as_deref().is_some_and(|slug| !shell_tab_is_primary(slug)) {
                    ui.scroll_to_rect(rect, Some(egui::Align::Center));
                }
                if more_active || self.ui.jy_tab_more {
                    ui.painter().rect_filled(rect, 6.0, t.accent);
                } else if response.hovered() {
                    ui.painter().rect_filled(rect, 6.0, t.accent_hover);
                } else {
                    ui.painter().rect_filled(rect, 6.0, t.hover);
                }
                ui.painter().galley(egui::pos2(rect.center().x - galley.size().x / 2.0, rect.center().y - galley.size().y / 2.0), galley, color);
                self.auto.add("jy.tab.more", rect, "更多");
                if response.clicked() {
                    self.ui.jy_tab_more = !self.ui.jy_tab_more;
                    if self.ui.jy_tab_more {
                        // 两个弹层不叠在一起：打开一个就收起另一个。
                        self.ui.jy_menu_open = false;
                    }
                }
                more_anchor = Some(rect);
            });
        });
        if let Some((slug, active)) = chosen {
            // 顶栏点击和控制通道走同一条 `window.shellTab` 命令路径。
            let ctx = ui.ctx().clone();
            if crate::menus::invoke(self, &ctx, "window.shellTab", json!({ "tab": slug })).is_ok() {
                // 再点当前 Tab 收起抽屉。
                if active {
                    self.ui.jy_drawer = None;
                }
                self.ui.jy_tab_more = false;
            }
        }
        if pending_scroll.is_some() {
            self.ui.jy_tab_scroll = None;
        }
        // 手机端右上角的「导出」，和剪映一样永远在顶栏最右。
        let export_width = 52.0_f32;
        let export_rect = Rect::from_min_size(egui::pos2(header.max.x - 8.0 - export_width, header.center().y - 14.0), egui::vec2(export_width, 28.0));
        let export_response = ui.interact(export_rect, egui::Id::new("jy.export"), Sense::click());
        ui.painter().rect_filled(export_rect, 6.0, if export_response.hovered() { t.accent_hover } else { t.accent });
        let export_galley = ui.painter().layout_no_wrap("导出".to_string(), Tokens::semibold(12.0), Color32::WHITE);
        ui.painter().galley(
            egui::pos2(export_rect.center().x - export_galley.size().x / 2.0, export_rect.center().y - export_galley.size().y / 2.0),
            export_galley,
            Color32::WHITE,
        );
        self.auto.add("jy.export", export_rect, "导出");
        if export_response.clicked() {
            let ctx = ui.ctx().clone();
            let _ = crate::menus::invoke(self, &ctx, "file.exportMedia", json!({}));
        }
        // 属性开关：固定在「导出」左侧，与当前 Tab 无关。
        {
            let label = crate::i18n::t("Properties");
            let galley = ui.painter().layout_no_wrap(label.to_string(), Tokens::ui(12.0), t.text);
            let width = (galley.size().x + 20.0).min(104.0);
            let rect = Rect::from_min_size(egui::pos2(export_rect.min.x - 6.0 - width, header.center().y - 14.0), egui::vec2(width, 28.0));
            let response = ui.interact(rect, egui::Id::new("jy.tab.inspector"), Sense::click());
            let active = self.ui.jy_drawer == Some(crate::layout::Drawer::Inspector);
            ui.painter().rect_filled(
                rect,
                6.0,
                if active {
                    t.accent
                } else if response.hovered() {
                    t.accent_hover
                } else {
                    t.hover
                },
            );
            ui.painter().galley(egui::pos2(rect.min.x + 10.0, rect.center().y - galley.size().y / 2.0), galley, if active { Color32::WHITE } else { t.text });
            self.auto.add("jy.tab.inspector", rect, label);
            if response.clicked() {
                self.ui.jy_drawer = if active { None } else { Some(crate::layout::Drawer::Inspector) };
            }
        }
        // 抽屉：Compact 从底部切（压缩预览），Medium 从右侧切（240 px 侧栏）；时间轴不受影响。
        let drawer_width = 240.0_f32;
        let (preview, drawer) = match self.ui.jy_drawer {
            Some(drawer) if medium && content.width() >= 560.0 => {
                let split = content.max.x - drawer_width;
                (
                    Rect::from_min_max(content.min, egui::pos2(split, content.max.y)),
                    Some((drawer, Rect::from_min_max(egui::pos2(split, content.min.y), content.max))),
                )
            }
            Some(drawer) if content.height() >= 320.0 => {
                let drawer_height = (content.height() * 0.45).max(160.0).min((content.height() - 140.0).max(0.0));
                let split = content.max.y - drawer_height;
                (
                    Rect::from_min_max(content.min, egui::pos2(content.max.x, split)),
                    Some((drawer, Rect::from_min_max(egui::pos2(content.min.x, split), content.max))),
                )
            }
            _ => (content, None),
        };
        // 预览与时间轴之间的分隔线。
        ui.painter().line_segment(
            [egui::pos2(body.min.x, timeline.min.y), egui::pos2(body.max.x, timeline.min.y)],
            egui::Stroke::new(1.0, t.text_dim.gamma_multiply(0.35)),
        );
        if let Some(toolbar) = toolbar {
            self.timeline_toolbar(ui, toolbar, &t);
        }
        self.auto.add("jy.preview", preview, PanelKind::Program.title());
        // 浮动出去的面板只由 `draw_floating_panels` 画一次：否则同一帧同一面板画两遍、
        // 自动化 id 重复（egui 还会叠出红色告警）。Program / 抽屉面板 / 时间轴都适用。
        if !self.is_floating(PanelKind::Program) {
            self.panel_body(ui, PanelKind::Program, preview, class);
        }
        if let Some((drawer, rect)) = drawer {
            // 顶栏选的 Tab 决定抽屉画哪个面板；属性开关始终是 Properties；没选过就退回旧映射。
            let panel = if drawer == crate::layout::Drawer::Inspector {
                PanelKind::Properties
            } else {
                self.ui.jy_shell_panel.unwrap_or(match drawer {
                    crate::layout::Drawer::Media => PanelKind::Project,
                    crate::layout::Drawer::Captions => PanelKind::Text,
                    crate::layout::Drawer::Audio => PanelKind::AudioTrackMixer,
                    crate::layout::Drawer::Inspector => PanelKind::Properties,
                })
            };
            self.auto.add("jy.drawer", rect, panel.title());
            if !self.is_floating(panel) {
                self.panel_body(ui, panel, rect, class);
            }
        }
        self.auto.add("jy.timeline", timeline, PanelKind::Timeline.title());
        if !self.is_floating(PanelKind::Timeline) {
            self.panel_body(ui, PanelKind::Timeline, timeline, class);
        }
        // 菜单最后画，保证盖在面板之上。
        if self.ui.jy_menu_open {
            self.shell_menu(ui, header, body, &t);
        }
        if self.ui.jy_tab_more
            && let Some(anchor) = more_anchor
        {
            self.shell_tab_overflow(ui, anchor, body, &t);
        }
    }

    /// 顶栏「更多」溢出菜单（Compact / Medium）：其余 10 个一级 Tab 都在这里。
    ///
    /// 外壳、行高、自动化 id 约定都与 ☰ `shell_menu` 一致，只是行数固定（10 条）。没有实现的
    /// 模块仍然列出来、灰显、不可点——不能因为收进菜单就"悄悄消失"。每行复用原来的
    /// `jy.tab.<slug>` id，测试与代理的点击路径不变；点击走 `window.shellTab` 命令。
    ///
    /// 弹层先量可用空间再定尺寸：下方放得下就挂在「更多」下面，放不下才向上翻；
    /// 高度取 `min(期望, 可用)`，最后与 `body.shrink(8.0)` 求交——任何窗口尺寸下都不会越过
    /// body 的右 / 下边界（矮窗口不会再把 10 行整条压出内容区）。
    fn shell_tab_overflow(&mut self, ui: &mut egui::Ui, anchor: Rect, body: Rect, t: &Tokens) {
        let rows = shell_overflow_tabs();
        let row_height = 26.0;
        let bounds = body.shrink(8.0);
        let desired = vec2(148.0, (rows.len() as f32 * row_height + 8.0).max(120.0));
        let width = desired.x.min(bounds.width()).max(0.0);
        let (top, height) = popover_top_and_height(anchor, desired.y, bounds, 4.0);
        // 菜单右边缘不出 body：靠右的锚点在窄窗口里会向左让位。
        let x = anchor.min.x.min(bounds.max.x - width).max(bounds.min.x);
        let popup = Rect::from_min_size(egui::pos2(x, top), egui::vec2(width, height)).intersect(bounds);
        // 弹层吃掉落在自己身上的点击，否则会漏到下面的预览/时间轴上。
        let _ = ui.interact(popup, egui::Id::new("jy.tab.more.background"), Sense::click());
        ui.painter().rect_filled(popup, 8.0, t.header_bg);
        ui.painter().rect_stroke(popup, 8.0, egui::Stroke::new(1.0, t.text_dim.gamma_multiply(0.4)), StrokeKind::Inside);
        self.auto.add("jy.tab.more.panel", popup, "更多");
        let inner = popup.shrink(4.0);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).id_salt("jy-tab-more-list"));
        child.set_clip_rect(popup);
        let mut chosen: Option<String> = None;
        egui::ScrollArea::vertical().id_salt("jy-tab-more-scroll").auto_shrink([false, false]).show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for &(label, slug, id, panel) in rows {
                let enabled = panel.is_some();
                let active = self.ui.jy_shell_slug.as_deref() == Some(slug);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), Sense::hover());
                let response = ui.interact(rect, egui::Id::new(("jy-tab-more", slug)), if enabled { Sense::click() } else { Sense::hover() });
                if active && enabled {
                    ui.painter().rect_filled(rect, 4.0, t.accent);
                } else if enabled && response.hovered() {
                    ui.painter().rect_filled(rect, 4.0, t.hover);
                }
                let color = if active && enabled {
                    Color32::WHITE
                } else if enabled {
                    t.text
                } else {
                    t.text_dim
                };
                let galley = ui.painter().layout_no_wrap(label.to_string(), Tokens::ui(12.5), color);
                ui.painter().galley(egui::pos2(rect.min.x + 10.0, rect.center().y - galley.size().y / 2.0), galley, color);
                self.auto.add(id, rect, label);
                if enabled && response.clicked() {
                    chosen = Some(slug.to_string());
                }
            }
        });
        if let Some(slug) = chosen {
            let ctx = ui.ctx().clone();
            if crate::menus::invoke(self, &ctx, "window.shellTab", json!({ "tab": slug })).is_ok() {
                self.ui.jy_tab_more = false;
            }
        }
        // Esc 收起；和 ☰ 菜单一样保留到用户明确关掉为止。
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.ui.jy_tab_more = false;
        }
    }

    /// Compact / Medium 的 ☰ 菜单：把全局菜单收进一个可滚动列表。
    fn shell_menu(&mut self, ui: &mut egui::Ui, header: Rect, body: Rect, t: &Tokens) {
        let items = crate::menus::menu_items(self);
        let height = (body.height() - header.height() - 16.0).clamp(160.0, 460.0);
        let width = (body.width() - 16.0).min(320.0);
        let popup = Rect::from_min_size(egui::pos2(header.min.x + 8.0, header.max.y + 6.0), egui::vec2(width, height));
        ui.painter().rect_filled(popup, 8.0, t.header_bg);
        ui.painter().rect_stroke(popup, 8.0, egui::Stroke::new(1.0, t.text_dim.gamma_multiply(0.4)), StrokeKind::Inside);
        self.auto.add("jy.menu.panel", popup, "Menu");
        let inner = popup.shrink(8.0);
        let search_rect = Rect::from_min_size(inner.min, egui::vec2(inner.width(), 26.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).id_salt("jy-menu-list"));
        child.set_clip_rect(popup);
        // 顶部过滤框：150+ 条命令靠滚动找太慢，先支持子串过滤。
        {
            let mut search_ui = child.new_child(egui::UiBuilder::new().max_rect(search_rect).id_salt("jy-menu-search"));
            let response = search_ui.add(
                egui::TextEdit::singleline(&mut self.ui.jy_menu_filter).hint_text("Filter…").desired_width(search_rect.width()).margin(egui::vec2(6.0, 3.0)),
            );
            self.auto.add("jy.menu.filter", response.rect, "Filter");
        }
        let needle = self.ui.jy_menu_filter.trim().to_lowercase();
        let list_rect = Rect::from_min_max(egui::pos2(inner.min.x, search_rect.max.y + 4.0), inner.max);
        let mut list = child.new_child(egui::UiBuilder::new().max_rect(list_rect).id_salt("jy-menu-rows"));
        list.set_clip_rect(popup);
        // 两级菜单：无过滤词时先列菜单根（File / Edit / …），点开的分组在**悬浮二级面板**里展开；
        // 有过滤词时退回扁平结果——搜索时不该再要求用户先选分组。
        enum Row {
            Group(String),
            Command(String),
        }
        let mut rows: Vec<(String, String, bool, Row)> = Vec::new();
        if !needle.is_empty() {
            for item in items.iter().filter(|item| !item.path.is_empty()) {
                let label = format!("{} / {}", item.path.join(" / "), item.label);
                if label.to_lowercase().contains(&needle) {
                    rows.push((format!("jy.menu.{}", item.id), label, item.enabled, Row::Command(item.id.clone())));
                }
            }
        } else {
            let mut roots: Vec<(String, usize)> = Vec::new();
            for item in items.iter().filter(|item| !item.path.is_empty()) {
                let root = item.path[0].clone();
                match roots.iter_mut().find(|(name, _)| *name == root) {
                    Some((_, count)) => *count += 1,
                    None => roots.push((root, 1)),
                }
            }
            for (root, count) in roots {
                rows.push((format!("jy.menu.group.{root}"), format!("{root} ({count})"), true, Row::Group(root)));
            }
        }
        // 点击分组时记住那一行的矩形，悬浮二级面板贴着分组标题栏出现。
        let mut next_group: Option<Option<String>> = None;
        let mut chosen: Option<String> = None;
        let mut root_rows: Vec<(String, Rect)> = Vec::new();
        egui::ScrollArea::vertical().id_salt("jy-menu-scroll").auto_shrink([false, false]).show(&mut list, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (id, label, enabled, row) in &rows {
                let color = if *enabled { t.text } else { t.text_dim };
                let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), Sense::hover());
                let response = ui.interact(rect, egui::Id::new(("jy-menu", id.as_str())), Sense::click());
                if response.hovered() && *enabled {
                    ui.painter().rect_filled(rect, 4.0, t.hover);
                }
                if let Row::Group(name) = row {
                    root_rows.push((name.clone(), rect));
                }
                let galley = ui.painter().layout_no_wrap(label.clone(), Tokens::ui(12.5), color);
                ui.painter().galley(egui::pos2(rect.min.x + 10.0, rect.center().y - galley.size().y / 2.0), galley, color);
                self.auto.add(id, rect, label);
                if response.clicked() && *enabled {
                    match row {
                        // 根列表原地保留；分组命令画在它上面的悬浮二级面板里。
                        Row::Group(name) => next_group = Some(Some(name.clone())),
                        Row::Command(command) => chosen = Some(command.clone()),
                    }
                }
            }
        });
        if let Some(group) = next_group {
            self.ui.jy_menu_group = group;
        }
        if let Some(id) = chosen {
            self.ui.jy_menu_open = false;
            let ctx = ui.ctx().clone();
            let _ = crate::menus::invoke(self, &ctx, &id, json!({}));
        }
        // 点开的分组：命令不再整页替换根列表，而是出现在一个悬浮二级面板里（原内容仍可见）。
        if let Some(group) = self.ui.jy_menu_group.clone().filter(|_| needle.is_empty()) {
            let anchor = root_rows.iter().find(|(root, _)| *root == group).map(|(_, rect)| *rect).unwrap_or(popup);
            if let Some(id) = self.shell_menu_group_panel(ui, body, popup, anchor, &group, &items, t) {
                self.ui.jy_menu_open = false;
                let ctx = ui.ctx().clone();
                let _ = crate::menus::invoke(self, &ctx, &id, json!({}));
            }
        }
    }

    /// ☰ 菜单的悬浮二级面板：点开的分组命令列在这里，根列表仍留在下面的主弹层里。
    ///
    /// 面板优先贴在主弹层右侧；窄窗口放不下时叠在弹层内侧。竖直方向先量锚点上下两侧的可用
    /// 空间：下方放不下就向上翻，高度取 `min(期望, 可用)`，最后与 `body.shrink(8.0)` 求交——
    /// 矮窗口里是变矮，而不是被 `.max(body.min.y + 8.0)` 反推出 body 下边界。
    /// 分隔符只用 ASCII（`<` / `-` / `/`），实测字体里 `▸` 会渲染成豆腐块。
    /// 返回被点中的命令 id；调用方负责关菜单并执行。
    fn shell_menu_group_panel(
        &mut self,
        ui: &mut egui::Ui,
        body: Rect,
        popup: Rect,
        anchor: Rect,
        group: &str,
        items: &[MenuItem],
        t: &Tokens,
    ) -> Option<String> {
        let commands: Vec<&MenuItem> = items.iter().filter(|item| item.path.first().is_some_and(|first| first == group)).collect();
        let row_h = 22.0_f32;
        let header_h = 24.0_f32;
        let pad = 12.0_f32;
        let bounds = body.shrink(8.0);
        // 宽度按内容区收缩：300 px 是期望宽度，但绝不能宽过 body（下限 160 只作可读性兜底）。
        let width = 300.0_f32.min((body.width() - 24.0).max(160.0)).min(bounds.width()).max(0.0);
        // 面板高度按行数算，但最多占内容区的 62%：它是悬浮二级面板，不是新的整页。
        let wanted = header_h + 2.0 + commands.len() as f32 * row_h + pad;
        let desired = wanted.min((body.height() * 0.62).max(140.0));
        // 水平：优先贴在主弹层右侧；放不下就叠回主弹层内侧，并保证右边不出 body。
        let right = popup.max.x + 6.0;
        let x = if right + width <= bounds.max.x { right } else { popup.max.x - width };
        let x = x.min(bounds.max.x - width).max(bounds.min.x);
        // 竖直：先算上下可用空间，下方放不下就向上翻（高度取 min(期望, 可用)）。
        let (y, height) = popover_top_and_height(anchor, desired, bounds, 2.0);
        let panel = Rect::from_min_size(egui::pos2(x, y), egui::vec2(width, height)).intersect(bounds);
        let id = egui::Id::new("jy-menu-group-panel");
        let mut chosen: Option<String> = None;
        egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(panel.min).constrain(false).show(ui.ctx(), |ui| {
            // 面板吃掉落在自己身上的点击，否则会漏到下面的根列表上。
            let _ = ui.interact(panel, id.with("surface"), Sense::click());
            ui.painter().rect_filled(panel, 8.0, t.header_bg);
            ui.painter().rect_stroke(panel, 8.0, egui::Stroke::new(1.0, t.text_dim.gamma_multiply(0.45)), StrokeKind::Inside);
            self.auto.add("group.panel", panel, group);
            let inner = panel.shrink(6.0);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).id_salt("jy-menu-group-body"));
            child.set_clip_rect(panel);
            // 标题栏就是「返回」：`< File  -  51`（ASCII 分隔符）。
            let header = Rect::from_min_size(inner.min, egui::vec2(inner.width(), header_h));
            let back = child.interact(header, id.with("back"), Sense::click());
            if back.hovered() {
                child.painter().rect_filled(header, 4.0, t.hover);
            }
            let title = format!("< {group}  -  {}", commands.len());
            let galley = child.painter().layout_no_wrap(title, Tokens::ui(12.5), t.text);
            child.painter().galley(egui::pos2(header.min.x + 4.0, header.center().y - galley.size().y / 2.0), galley, t.text);
            self.auto.add("group.back", header, group);
            if back.clicked() {
                self.ui.jy_menu_group = None;
            }
            child.painter().line_segment([egui::pos2(inner.min.x, header.max.y), egui::pos2(inner.max.x, header.max.y)], egui::Stroke::new(1.0, t.separator));
            let list_rect = Rect::from_min_max(egui::pos2(inner.min.x, header.max.y + 2.0), inner.max);
            let mut list = child.new_child(egui::UiBuilder::new().max_rect(list_rect).id_salt("jy-menu-group-rows"));
            list.set_clip_rect(panel);
            egui::ScrollArea::vertical().id_salt("jy-menu-group-scroll").auto_shrink([false, false]).show(&mut list, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for command in &commands {
                    let color = if command.enabled { t.text } else { t.text_dim };
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 20.0), Sense::hover());
                    let response = ui.interact(rect, egui::Id::new(("group-row", command.id.as_str())), Sense::click());
                    if response.hovered() && command.enabled {
                        ui.painter().rect_filled(rect, 4.0, t.hover);
                    }
                    let rest: Vec<&str> = command.path.iter().skip(1).map(String::as_str).collect();
                    let label = if rest.is_empty() { command.label.clone() } else { format!("{} / {}", rest.join(" / "), command.label) };
                    let galley = ui.painter().layout_no_wrap(label.clone(), Tokens::ui(12.0), color);
                    ui.painter().galley(egui::pos2(rect.min.x + 8.0, rect.center().y - galley.size().y / 2.0), galley, color);
                    self.auto.add(&format!("group.row.{}", command.id), rect, &label);
                    if command.enabled && response.clicked() {
                        chosen = Some(command.id.clone());
                    }
                }
            });
        });
        chosen
    }

    /// 时间轴工具条：剪映式高频动作（撤销 / 重做 / 分割 / 删除 / 播放 / 定格 / 倒放）。
    /// Compact 挂在时间轴上方，`Jianying` 工作区的 Wide 档挂在 Timeline 面板顶部；只做转发。
    ///
    /// 剪映工具条是图标按钮，不是文字按钮：这里复用 `icons.rs` 的既有图标，tooltip 仍显示原来的
    /// 中文动作名，播放中「播放」仍切成「暂停」。`jy.tool.*` 的自动化 id 与 label 不变。
    fn timeline_toolbar(&mut self, ui: &mut egui::Ui, rect: Rect, t: &Tokens) {
        ui.painter().rect_filled(rect, 0.0, t.tl_header_bg);
        // 最后一个 bool 表示"需要选中片段"：删除/定格/倒放在没有选区时必须是灰的。
        let entries: [(&str, &str, Icon, &str, serde_json::Value, bool); 7] = [
            ("jy.tool.undo", "撤销", Icon::Undo, "edit.undo", json!({}), false),
            ("jy.tool.redo", "重做", Icon::Redo, "edit.redo", json!({}), false),
            ("jy.tool.split", "分割", Icon::Razor, "sequence.addEdit", json!({}), false),
            ("jy.tool.delete", "删除", Icon::Trash, "edit.clear", json!({}), true),
            ("jy.tool.play", "播放", Icon::Play, "playback.toggle", json!({}), false),
            ("jy.tool.freeze", "定格", Icon::Camera, "clip.frameHold", json!({}), true),
            ("jy.tool.reverse", "倒放", Icon::Loop, "clip.speedDuration", json!({"reverse": true}), true),
        ];
        // 对照截图的像素：按钮 36×30、图标 22 px、左内边距 12 px；组内间距 4 px、组间 8 px，
        // 分隔线落在组间空隙正中，所以线两侧各 4 px，视觉上正好是「一组 ｜ 一组」。
        const BUTTON_W: f32 = 36.0;
        const BUTTON_H: f32 = 30.0;
        const ICON_SIDE: f32 = 22.0;
        const LEFT_PAD: f32 = 12.0;
        const BUTTON_GAP: f32 = 4.0;
        const GROUP_GAP: f32 = 8.0;
        const DIVIDER_INSET: f32 = 5.0;
        let button_h = BUTTON_H.min(rect.height()).max(0.0);
        let mut x = rect.min.x + LEFT_PAD;
        let mut chosen: Option<(&str, serde_json::Value)> = None;
        let has_selection = !self.session.state.selection.is_empty();
        for (index, (id, label, icon, command, params, needs_selection)) in entries.iter().enumerate() {
            if x + BUTTON_W > rect.max.x - 2.0 {
                break;
            }
            // 三条组间分隔线：撤销/重做 ｜ 分割/删除 ｜ 播放 ｜ 定格/倒放。
            // 竖线 1 px，上下各留 5 px（相对按钮高），颜色走主题的 `separator`。
            if index == 2 || index == 4 || index == 5 {
                let divider_h = (button_h - DIVIDER_INSET * 2.0).max(1.0);
                let divider = Rect::from_center_size(pos2(x - GROUP_GAP / 2.0, rect.center().y), egui::vec2(1.0, divider_h));
                ui.painter().rect_filled(divider, 0.0, t.separator);
            }
            let cell = Rect::from_min_size(pos2(x, rect.center().y - button_h / 2.0), egui::vec2(BUTTON_W, button_h));
            // 分组边界（撤销/重做、删除、播放之后）留 8 px，其余按钮之间 4 px。
            let ends_group = index == 1 || index == 3 || index == 4;
            x = cell.max.x + if ends_group { GROUP_GAP } else { BUTTON_GAP };
            // 按钮跟着命令的启用条件走，并且选区为空时删除/定格/倒放一律灰掉。
            let enabled =
                (!needs_selection || has_selection) && filmcraft_engine::find_command(command).is_none_or(|spec| (spec.enabled)(&self.session).is_ok());
            let response = ui.interact(cell, egui::Id::new(*id), if enabled { Sense::click() } else { Sense::hover() });
            if enabled && response.hovered() {
                ui.painter().rect_filled(cell, 4.0, t.hover);
            }
            let color = if enabled { t.text } else { t.text_dim.gamma_multiply(0.6) };
            // 播放中显示"暂停"，这是剪映工具条的即时反馈。
            let playing = *label == "播放" && self.playback.playing;
            let text = if playing { "暂停" } else { *label };
            let icon = if playing { Icon::Pause } else { *icon };
            let response = response.on_hover_text(text);
            let icon_rect = Rect::from_center_size(cell.center(), vec2(ICON_SIDE, ICON_SIDE).min(cell.size()));
            icons::paint(ui.painter(), icon_rect, icon, color);
            self.auto.add(id, cell, text);
            if enabled && response.clicked() {
                chosen = Some((command, params.clone()));
            }
        }
        if let Some((command, params)) = chosen {
            let ctx = ui.ctx().clone();
            let _ = crate::menus::invoke(self, &ctx, command, params);
        }
    }

    /// 剪映专业版的一级 Tab 行（Wide + `Jianying` 工作区）：素材 / 音频 / 文本 / 贴纸 / 特效 /
    /// 转场 / 字幕 / 智能包装 / 滤镜 / 调节 / 模板 / 数字人 / 插件。
    ///
    /// 对照截图的像素级结构：每个 Tab 是**上下两行**——20 px 线性图标在上，11 px 文字在下；两行
    /// 合起来的视觉中心与 56 pt 顶栏的中心对齐（图标中心在上方 8 px，文字中心在下方 12 px）。
    /// 当前项用主题强调色同时点亮图标与文字（截图里剪映就是青色图标 + 青色文字，不用胶囊底色），
    /// 其余项为灰白；未实现的模块仍画出图标与文字，但压暗且不可点——不摆假入口。
    ///
    /// 只画按钮、不画背景与「导出」——它们属于顶栏（`header::jianying_bar`），这里只负责把这一行
    /// 排进给定的 `rect`。只做**导航**：有对应面板的 Tab 调 `show_panel`；模板/数字人/智能包装/
    /// 插件在本仓库没有实现，画成灰色且不可点。
    ///
    /// 自动化 id 同时登记两套：`jy.tabstrip.<slug>` 是 Wide 档沿用的旧契约（测试与脚本按它点击），
    /// `jy.tab.<slug>` 与 Compact/Medium 顶栏的一级 Tab 同名（`SHELL_TABS`），两套都指向同一块矩形。
    pub(crate) fn jianying_tab_row(&mut self, ui: &mut egui::Ui, rect: Rect, t: &Tokens) {
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }
        // 图标一律复用 `icons.rs` 的自绘线性图标（16×16 坐标系），不引入第三方图标资产。
        let tabs: [(&str, &str, Icon, Option<PanelKind>); 13] = [
            ("素材", "media", Icon::Film, Some(PanelKind::Project)),
            ("音频", "audio", Icon::Audio, Some(PanelKind::Project)),
            ("文本", "text", Icon::Type, Some(PanelKind::Text)),
            ("贴纸", "sticker", Icon::Star, Some(PanelKind::EssentialGraphics)),
            ("特效", "effects", Icon::Fx, Some(PanelKind::Effects)),
            ("转场", "transitions", Icon::Rolling, Some(PanelKind::Effects)),
            ("字幕", "captions", Icon::Captions, Some(PanelKind::Text)),
            ("智能包装", "smart", Icon::Sparkle, None),
            ("滤镜", "filters", Icon::Ellipse, Some(PanelKind::LumetriColor)),
            ("调节", "adjust", Icon::Adjust, Some(PanelKind::LumetriColor)),
            ("模板", "templates", Icon::Folder, None),
            ("数字人", "avatar", Icon::Image, None),
            ("插件", "plugins", Icon::Grid, None),
        ];
        let mut chosen: Option<(String, PanelKind)> = None;
        let mut x = rect.min.x;
        // 没点过 Tab 时默认高亮「素材」：剪映桌面版一进来就停在素材区，而不是一排都不亮。
        let current = self.ui.jianying_tab.as_deref().unwrap_or("media");
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt("jy-header-tabs"));
        child.set_clip_rect(rect);
        let p = child.painter().clone();
        // 截图像素：图标 20 px、文字 11 px、两者间距 3 px。
        const ICON_SIDE: f32 = 20.0;
        const LABEL_SIZE: f32 = 11.0;
        const ICON_LABEL_GAP: f32 = 3.0;
        for (label, slug, icon, target) in tabs {
            let enabled = target.is_some();
            let galley = p.layout_no_wrap(label.to_string(), Tokens::ui(LABEL_SIZE), t.text);
            // 列宽取图标 / 文字里更宽的那个，左右各 10 px：两字标签约 42 px，四个字约 64 px，
            // 13 个 Tab 在 1280 px 下仍能整条排进 ☰ 与「导出」之间。
            let width = galley.size().x.max(ICON_SIDE) + 20.0;
            if x + width > rect.max.x + 0.5 {
                break;
            }
            let cell = Rect::from_min_size(pos2(x, rect.min.y), vec2(width, rect.height()));
            let response = child.interact(cell, egui::Id::new(("jy-tabstrip", slug)), Sense::click());
            let active = current == slug;
            // 当前项只换颜色（截图里没有胶囊底色，也没有下划线）；悬停再给一层很淡的底。
            if enabled && response.hovered() {
                p.rect_filled(cell.shrink2(vec2(1.0, 5.0)), 6.0, t.hover);
            }
            // 当前项：图标用强调色（截图里选中 Tab 的图标是填色块）、文字纯白；其余项统一为灰白；
            // 未实现的模块再压暗一档，和可用项一眼分开。
            let dim = t.text_dim.gamma_multiply(0.7);
            let label_color = if active {
                Color32::WHITE
            } else if enabled {
                t.tab_text
            } else {
                dim
            };
            let icon_color = if active {
                t.accent
            } else if enabled {
                t.icon
            } else {
                dim
            };
            // 图标在上、文字在下：整块内容在顶栏里竖直居中。
            let content_h = ICON_SIDE + ICON_LABEL_GAP + galley.size().y;
            let top = cell.center().y - content_h / 2.0;
            let icon_rect = Rect::from_min_size(pos2(cell.center().x - ICON_SIDE / 2.0, top), vec2(ICON_SIDE, ICON_SIDE));
            icons::paint(&p, icon_rect, icon, icon_color);
            p.galley(pos2(cell.center().x - galley.size().x / 2.0, top + ICON_SIDE + ICON_LABEL_GAP), galley, label_color);
            self.auto.add(&format!("jy.tabstrip.{slug}"), cell, label);
            // 与 Compact/Medium 顶栏同名的第二套 id（标签一一对应）。
            if let Some(shell_id) = SHELL_TABS.iter().find(|tab| tab.0 == label).map(|tab| tab.2) {
                self.auto.add(shell_id, cell, label);
            }
            if response.clicked()
                && let Some(panel) = target
            {
                chosen = Some((slug.to_string(), panel));
            }
            x = cell.max.x + 2.0;
        }
        if let Some((slug, panel)) = chosen {
            self.ui.jianying_tab = Some(slug.clone());
            // 剪映的"音频"Tab 只列音频素材，"素材"Tab 恢复全部。
            self.ui.jy_media_filter = (slug == "audio").then(|| "audio".to_string());
            self.show_panel(panel);
        }
    }

    /// 剪映工作区的轻量面板标题行（26 pt）。
    ///
    /// 剪映专业版没有 Premiere 的 dock 页签（项目 / 媒体浏览器 / 库 / 信息、属性 / 效果控件 /
    /// 基本图形 / 文本），停靠组只留一条标题行：左边面板名，右边「切换面板」（多面板组才有）与
    /// 「面板菜单」。页签虽然不画，自动化契约不缩水——`panel.tab.<Panel>`、`panel.menu.<Panel>`、
    /// `panel.drag.<Panel>` 照旧登记，切换下拉里每个面板另有 `panel.tabs.item.<Panel>`。
    ///
    /// 返回该组**内容区**的矩形（标题行以下），调用方用它画面板体。
    fn jianying_panel_chrome(&mut self, ui: &mut egui::Ui, g: &Group, t: &Tokens, actions: &mut Vec<DockAction>) -> Rect {
        let painter = ui.painter().clone();
        painter.rect_filled(g.rect, t.radius, t.panel_bg);
        let Some(active) = g.panels.get(g.active).copied() else { return g.content };
        // 组太矮时不画标题行：宁可少一条装饰，也不把面板内容压没。
        let row_h = 26.0_f32.min((g.rect.height() - 24.0).max(0.0));
        let strip = Rect::from_min_size(g.rect.min, egui::vec2(g.rect.width(), row_h));
        if row_h >= 24.0 {
            painter.rect_filled(strip, 0.0, t.tl_header_bg);
            painter.line_segment([strip.left_bottom(), strip.right_bottom()], Stroke::new(1.0, t.text_dim.gamma_multiply(0.22)));
        }
        // 左侧 10 pt 拖拽手柄：整组移动 / 拖出浮动，行为与原来的 dock 页签条一致。
        let grip = Rect::from_min_size(strip.min, egui::vec2(10.0_f32.min(strip.width()), row_h));
        let grip_response = ui.interact(grip, egui::Id::new(("panel-drag", g.path.clone())), Sense::drag());
        self.auto.add_response(&format!("panel.drag.{}", active.id()), &grip_response, "Move panel");
        if grip_response.drag_started() {
            actions.push(DockAction::BeginDrag(active));
        }
        let title = crate::i18n::t(active.title()).to_string();
        let galley = painter.layout_no_wrap(title.clone(), Tokens::ui(12.0), t.text_dim);
        if row_h >= 18.0 {
            painter.galley(pos2(strip.min.x + 14.0, strip.center().y - galley.size().y / 2.0), galley.clone(), t.text_dim);
        }
        let title_w = (galley.size().x + 24.0).min((strip.width() - 56.0).max(0.0));
        let title_rect = Rect::from_min_size(egui::pos2(strip.min.x + 12.0, strip.min.y), egui::vec2(title_w, row_h));
        let title_response = ui.interact(title_rect, egui::Id::new(("panel-title", g.path.clone())), Sense::click());
        title_response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, true, title.as_str()));
        self.auto.add_response(&format!("panel.tab.{}", active.id()), &title_response, active.title());
        if title_response.clicked() {
            actions.push(DockAction::Focus(active));
        }
        // 右侧从右往左排：面板菜单 ≡，其左是「切换面板」⌄（只有多面板组才画）。
        let mut rx = strip.max.x - 6.0;
        if row_h >= 18.0 {
            let menu_rect = Rect::from_center_size(pos2((rx - 11.0).max(strip.min.x + 2.0), strip.center().y), egui::vec2(22.0, 18.0));
            rx -= 24.0;
            let menu_response = ui.interact(menu_rect, egui::Id::new(("panel-menu", g.path.clone())), Sense::click());
            menu_response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "panel menu"));
            self.auto.add_response(&format!("panel.menu.{}", active.id()), &menu_response, "panel menu");
            if menu_response.hovered() {
                painter.rect_filled(menu_rect, 4.0, t.hover);
            }
            icons::paint(
                &painter,
                Rect::from_center_size(menu_rect.center(), egui::vec2(14.0, 14.0)),
                Icon::Hamburger,
                if menu_response.hovered() { t.text } else { t.text_dim },
            );
            if menu_response.clicked() {
                actions.push(DockAction::PanelMenu(active, menu_rect.left_bottom()));
            }
            if g.panels.len() > 1 {
                let pick_rect = Rect::from_center_size(pos2((rx - 9.0).max(strip.min.x + 2.0), strip.center().y), egui::vec2(18.0, 18.0));
                let pick_id = egui::Id::new(("panel-pick-area", g.path.clone()));
                let ctx = ui.ctx().clone();
                let open = ctx.data(|d| d.get_temp::<bool>(pick_id)).unwrap_or(false);
                let pick_response = ui.interact(pick_rect, egui::Id::new(("panel-pick", g.path.clone())), Sense::click()).on_hover_text(tl!("Panels"));
                self.auto.add_response(&format!("panel.tabs.switch.{}", active.id()), &pick_response, "Panels");
                if pick_response.hovered() || open {
                    painter.rect_filled(pick_rect, 4.0, t.hover);
                }
                icons::paint(&painter, pick_rect.shrink(2.0), Icon::ChevronDown, if pick_response.hovered() { t.text } else { t.text_dim });
                let clicked = pick_response.clicked();
                if clicked {
                    ctx.data_mut(|d| d.insert_temp(pick_id, !open));
                }
                let opened = if clicked { !open } else { open };
                if opened {
                    let mut picked: Option<PanelKind> = None;
                    let area = egui::Area::new(pick_id.with("area"))
                        .order(egui::Order::Foreground)
                        .pivot(egui::Align2::RIGHT_TOP)
                        .fixed_pos(pick_rect.right_bottom())
                        .show(&ctx, |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                ui.set_min_width(132.0);
                                for panel in &g.panels {
                                    let label = if *panel == active {
                                        format!("✓ {}", crate::i18n::t(panel.title()))
                                    } else {
                                        crate::i18n::t(panel.title()).to_string()
                                    };
                                    let row = ui.button(label);
                                    self.auto.add_response(&format!("panel.tabs.item.{}", panel.id()), &row, panel.title());
                                    if row.clicked() {
                                        picked = Some(*panel);
                                    }
                                }
                            });
                        });
                    if let Some(panel) = picked {
                        actions.push(DockAction::Activate(panel));
                        actions.push(DockAction::Focus(panel));
                        ctx.data_mut(|d| d.remove::<bool>(pick_id));
                    } else if !clicked && area.response.clicked_elsewhere() {
                        // 打开的那一帧里 `clicked_elsewhere` 必然为真（弹层当时还不存在），跳过。
                        ctx.data_mut(|d| d.remove::<bool>(pick_id));
                    }
                }
            }
        }
        Rect::from_min_max(pos2(g.rect.min.x, g.rect.min.y + row_h), g.rect.max)
    }

    /// 剪映专业版左栏的窄列文字导航 rail。
    ///
    /// 对照截图：剪映的 rail 不是大号竖排，而是 72 px 宽的窄列——每行 30 px，12 px 文字左边跟一枚
    /// 13 px 小图标，行尾留白；当前项在**最左侧**有一条 2 px 强调色高亮条（截图里选中的「热门」就是
    /// 青色文字 + 左侧细条，没有胶囊底色）。
    ///
    /// 本仓库有的入口可点（导入 / 素材），其余（子草稿 / 我的 / AI / 云素材 / 官方 / 即梦）没有实现，
    /// 压暗且不可点——不摆假入口。`jy.rail.*` 的自动化 id 与选中值（`ui.jianying_rail` 存标签文本）
    /// 全部保持原契约。
    fn jianying_rail(&mut self, ui: &mut egui::Ui, rect: Rect, t: &Tokens) {
        ui.painter().rect_filled(rect, 0.0, t.header_bg);
        ui.painter()
            .line_segment([egui::pos2(rect.max.x, rect.min.y), egui::pos2(rect.max.x, rect.max.y)], egui::Stroke::new(1.0, t.text_dim.gamma_multiply(0.35)));
        let entries: [(&str, &str, Icon, Option<PanelKind>, Option<&str>); 8] = [
            ("jy.rail.import", "导入", Icon::Import, None, Some("file.import")),
            ("jy.rail.media", "素材", Icon::Film, Some(PanelKind::Project), None),
            ("jy.rail.subdraft", "子草稿", Icon::Sequence, None, None),
            ("jy.rail.mine", "我的", Icon::Home, None, None),
            ("jy.rail.ai", "AI", Icon::Sparkle, None, None),
            ("jy.rail.cloud", "云素材", Icon::Network, None, None),
            ("jy.rail.official", "官方", Icon::Star, None, None),
            ("jy.rail.dreamina", "即梦", Icon::Grid, None, None),
        ];
        // 截图像素：行高 30 px、文字 12 px、小图标 13 px、左侧缩进 12 px。
        const ROW_HEIGHT: f32 = 30.0;
        const ICON_SIDE: f32 = 13.0;
        const LABEL_SIZE: f32 = 12.0;
        const INSET_X: f32 = 12.0;
        let mut y = rect.min.y + 6.0;
        let mut clicked: Option<(&str, Option<PanelKind>, Option<&str>)> = None;
        for (id, label, icon, panel, command) in entries {
            if y + ROW_HEIGHT > rect.max.y {
                break;
            }
            let enabled = panel.is_some() || command.is_some();
            let row = Rect::from_min_size(egui::pos2(rect.min.x, y), egui::vec2(rect.width(), ROW_HEIGHT));
            // 悬停 / 选中底色从 rail 边缘缩进 3 px，左边留给 2 px 高亮条，不压住它。
            let pill = Rect::from_min_max(egui::pos2(row.min.x + 3.0, row.min.y + 2.0), egui::pos2(row.max.x - 3.0, row.max.y - 2.0));
            let response = ui.interact(row, egui::Id::new(id), if enabled { Sense::click() } else { Sense::hover() });
            let active = self.ui.jianying_rail.as_deref() == Some(label);
            if active {
                ui.painter().rect_filled(pill, 4.0, t.row_selected);
                // 选中项左侧 2 px 高亮条：上下各让 3 px，长度 24 px。
                let bar = Rect::from_min_size(egui::pos2(row.min.x + 1.0, row.min.y + 3.0), egui::vec2(2.0, ROW_HEIGHT - 6.0));
                ui.painter().rect_filled(bar, 1.0, t.accent);
            } else if enabled && response.hovered() {
                ui.painter().rect_filled(pill, 4.0, t.hover);
            }
            // 选中项：白色文字 + 强调色图标（剪映原图整行是青色字，但主题里没有青色 token；
            // 白字保证 12 px 的对比度，2 px 高亮条负责"当前项"的识别）。未实现的入口压暗。
            let dim = t.text_dim.gamma_multiply(0.7);
            let label_color = if active {
                Color32::WHITE
            } else if enabled {
                t.text
            } else {
                dim
            };
            let icon_color = if active {
                t.accent
            } else if enabled {
                t.icon
            } else {
                dim
            };
            let icon_rect = Rect::from_center_size(egui::pos2(row.min.x + INSET_X + ICON_SIDE / 2.0, row.center().y), egui::vec2(ICON_SIDE, ICON_SIDE));
            icons::paint(ui.painter(), icon_rect, icon, icon_color);
            let galley = ui.painter().layout_no_wrap(label.to_string(), Tokens::ui(LABEL_SIZE), label_color);
            let text_pos = egui::pos2(icon_rect.max.x + 7.0, row.center().y - galley.size().y / 2.0);
            // 3 个汉字（子草稿 / 云素材）刚好贴到 72 px 列的右缘：裁一下，别画到分隔线上。
            ui.painter().with_clip_rect(pill).galley(text_pos, galley, label_color);
            self.auto.add(id, row, label);
            if enabled && response.clicked() {
                clicked = Some((label, panel, command));
            }
            y += ROW_HEIGHT;
        }
        if let Some((label, panel, command)) = clicked {
            self.ui.jianying_rail = Some(label.to_string());
            if let Some(panel) = panel {
                self.show_panel(panel);
            } else if let Some(command) = command {
                let ctx = ui.ctx().clone();
                let _ = crate::menus::invoke(self, &ctx, command, json!({}));
            }
        }
    }

    /// 绘制编辑模式的 dock 区。`class` 由 `frame` 按窗口宽度每帧算一次，全区共用。
    pub(crate) fn dock_area(&mut self, ui: &mut egui::Ui, body: Rect, class: LayoutClass) {
        // 单页壳（Compact / Medium）照常画；浮动面板**叠加在壳之上**，不再因为存在浮动窗口
        // 就退回 dock 树——用户显式打开的浮动监视器不能因为窗口变窄就消失
        // （tasks.md 已记录缺口：单页壳不绘制浮动面板）。
        if class != LayoutClass::Wide && self.ui.keys.maximized.is_none() {
            self.shell_area(ui, body, class);
            let ctx = ui.ctx().clone();
            self.raise_pending_panel(&ctx);
            self.draw_floating_panels(&ctx, body, class);
            panels::panel_menu_popup(self, ui);
            self.cancel_value_drag(ui.ctx());
            return;
        }
        let t = self.tokens;
        let jianying = class == LayoutClass::Wide && self.ui.workspace == "Jianying";
        // 剪映专业版只有一条 56 pt 顶栏：`lib.rs` 按 `JIANYING_HEADER_HEIGHT` 分配、`header::show`
        // 绘制，body 已经从顶栏下方开始，所以这里不再手工下移内容区。
        // 左栏竖向 rail：导入 / 素材 / 子草稿 / 我的 / AI / 云素材 / 官方 / 即梦。
        // 对照截图的窄列文字分类：72 px 宽（行高 30 px，见 `jianying_rail`），比旧版 56 px 略宽，
        // 刚好放下「12 px 图标 + 3 个汉字」。窗口过窄时按比例收窄，绝不挤掉 dock 内容。
        let body = if jianying && body.width() > 520.0 {
            let rail_width = 72.0_f32.min(body.width() * 0.14);
            let rail = Rect::from_min_size(body.min, egui::vec2(rail_width, body.height()));
            self.jianying_rail(ui, rail, &t);
            Rect::from_min_max(egui::pos2(body.min.x + rail_width, body.min.y), body.max)
        } else {
            body
        };
        let ctx = ui.ctx().clone();
        self.raise_pending_panel(&ctx);
        let maximized =
            self.ui.keys.maximized.filter(|panel| self.ui.dock.contains(*panel) || self.ui.floating.iter().any(|group| group.panels.contains(panel)));
        let mut dock = match maximized {
            Some(panel) => DockNode::Tabs { panels: vec![panel], active: 0 },
            None => std::mem::replace(&mut self.ui.dock, DockNode::Tabs { panels: vec![], active: 0 }),
        };
        let mut groups = vec![];
        let mut layout_actions = Vec::new();
        dock::layout_with_actions(ui, &mut dock, body, &t, "", &mut groups, &mut self.auto, &mut layout_actions);
        let seqs = self.seq_tabs();
        let mut actions = vec![];
        let mut targets = vec![];
        let mut contents: Vec<Rect> = Vec::with_capacity(groups.len());
        for group in &groups {
            // 剪映工作区不画 Premiere 的 dock 页签（项目/媒体浏览器/库/信息、属性/效果控件/…），
            // 每组只留一条轻量标题行；时间轴组例外——它的页签条同时承载「打开中的序列」，
            // 换成标题行会丢掉切换/关闭/重排序列的入口。
            if jianying && !group.panels.contains(&PanelKind::Timeline) {
                let content = self.jianying_panel_chrome(ui, group, &t, &mut actions);
                let strip = Rect::from_min_max(group.rect.min, pos2(group.rect.max.x, content.min.y));
                if let Some(panel) = group.panels.get(group.active).copied()
                    && let Some(target) = target(group, vec![(panel, strip)], true)
                {
                    targets.push(target);
                }
                contents.push(content);
            } else {
                let chrome = dock::draw_group_chrome_details_with(ui, group, &seqs, &t, &mut self.auto, jianying);
                if let Some(target) = target(group, chrome.tabs, true) {
                    targets.push(target);
                }
                actions.extend(chrome.actions);
                contents.push(group.content);
            }
        }
        if maximized.is_none() {
            self.ui.dock = dock;
        }
        for (group, content) in groups.iter().zip(contents) {
            if let Some(panel) = group.panels.get(group.active).copied() {
                self.panel_body(ui, panel, content, class);
            }
        }
        for group in &groups {
            dock::paint_group_focus(ui, group, self.ui.focused, &t);
        }
        // 剪映 Wide：预览与时间轴之间只留一条 1 px 细线（gutter 原样保留 4 pt 命中区）。
        // 必须画在面板与聚焦描边之后，才能盖住 gutter 的底色。
        if jianying {
            self.paint_jianying_pane_seam(ui, &groups, body, &t);
        }
        let floating_order = self.floating_order(&ctx);
        for index in floating_order.into_iter().filter(|_| maximized.is_none()) {
            if let Some(target) = self.floating_group(&ctx, index, &seqs, &mut actions, class) {
                targets.push(target);
            }
        }
        for action in layout_actions {
            let _ = self.dock_command(&ctx, action);
        }
        self.dispatch_dock_actions(&ctx, actions);
        self.panel_drag(&ctx, body, &targets);
        // 剪映顶栏的 ☰ 菜单必须在面板之后画，才盖得住预览与时间轴（顶栏本身先于 dock 绘制）。
        if jianying && self.ui.jy_menu_open {
            let full = ui.max_rect();
            let header = Rect::from_min_size(full.min, egui::vec2(full.width(), crate::header::JIANYING_HEADER_HEIGHT));
            self.shell_menu(ui, header, body, &t);
        }
        panels::panel_menu_popup(self, ui);
        self.cancel_value_drag(ui.ctx());
    }

    /// 剪映专业版：把预览与时间轴之间的横向 dock gutter 画成一条 1 px 细线。
    ///
    /// 来源：`dock::layout_with_actions` 按 `Tokens::gap`（4 pt）在根 Split 上留出一条
    /// `app_bg`（深色主题下比面板的 `panel_bg` 更暗）的空带——截图里预览面板底部与时间轴组头
    /// 之间那条 12–16 px 的纯底色带，就是这 4 pt gutter 加上监听器走带条自身的行内留白。
    /// 剪映里两块面板之间只有一条 1 px 分隔线。
    ///
    /// 做法：面板与聚焦描边画完之后，用面板底色盖掉这条空带，只在时间轴组的上沿画 1 px。
    /// dock 树的几何一点没动：`ResizeHandle` 的 4 pt divider 与 ±3 pt 命中区原样保留，
    /// 指针拖动、方向键与读屏仍然可用；悬停/拖拽 gutter 时细线换成聚焦色，分隔依然能被发现。
    /// 只处理横向的根分隔（时间轴在整行面板之下）；时间轴被拖到侧面或整组最大化时直接跳过。
    fn paint_jianying_pane_seam(&self, ui: &egui::Ui, groups: &[Group], body: Rect, t: &Tokens) {
        let Some(timeline) = groups.iter().find(|group| group.panels.contains(&PanelKind::Timeline)) else { return };
        // 上方一排面板（Project / Program / Properties）：与时间轴横向有交集、且底边在它上方。
        let above = groups
            .iter()
            .filter(|group| !group.panels.contains(&PanelKind::Timeline))
            .filter(|group| group.rect.min.x < timeline.rect.max.x && group.rect.max.x > timeline.rect.min.x)
            .filter(|group| group.rect.max.y <= timeline.rect.min.y + 0.5)
            .map(|group| group.rect.max.y)
            .reduce(f32::max);
        let Some(top) = above else { return };
        let gap = timeline.rect.min.y - top;
        // 只认横向根分隔的 4 pt（留 1 pt 浮点余量）；左右分栏或浮动的 gutter 不动。
        if gap < 0.5 || gap > t.gap + 1.0 {
            return;
        }
        let x0 = body.min.x.max(timeline.rect.min.x);
        let x1 = body.max.x.min(timeline.rect.max.x);
        if x1 <= x0 {
            return;
        }
        // 用面板底色填平 gutter：原先那条更暗的 app_bg 空带消失，两排面板看起来贴在一起。
        ui.painter().rect_filled(Rect::from_min_max(pos2(x0, top), pos2(x1, timeline.rect.min.y)), 0.0, t.panel_bg);
        // 细线落在时间轴组的上沿，正好替换掉聚焦组原本画在那里的 1 px 描边：
        // 聚焦时保持聚焦色（外观与修改前完全一致），未聚焦时用中性分隔色。
        let focused = timeline.panels.get(timeline.active) == Some(&self.ui.focused);
        // 命中区与 craft_ui 的 ResizeHandle 一致：divider 上下各扩 3 pt。
        let hovered =
            ui.ctx().pointer_hover_pos().is_some_and(|pos| Rect::from_min_max(pos2(x0, top - 3.0), pos2(x1, timeline.rect.min.y + 3.0)).contains(pos));
        let color = if focused || hovered { t.focus } else { t.text_dim.gamma_multiply(0.35) };
        // 对齐到整数像素行，保证是干干净净的 1 px，而不是抗锯齿糊出来的两条灰边。
        let y = timeline.rect.min.y.round();
        ui.painter().rect_filled(Rect::from_min_max(pos2(x0, y), pos2(x1, y + 1.0)), 0.0, color);
    }

    /// 把待置顶的浮动面板提到最上面（与点击顺序无关）。
    fn raise_pending_panel(&mut self, ctx: &egui::Context) {
        let Some(panel) = self.pending_panel_raise.take() else { return };
        let Some(owner) = self.ui.floating.iter().find(|group| group.panels.contains(&panel)).and_then(|group| group.panels.first()) else { return };
        ctx.move_to_top(egui::LayerId::new(egui::Order::Middle, egui::Id::new(("floating-panel", owner.id()))));
    }

    /// 当前打开的序列（Timeline 的 tab 条用）。
    fn seq_tabs(&self) -> dock::SeqTabs {
        dock::SeqTabs {
            open: self
                .session
                .state
                .open_sequences
                .iter()
                .filter_map(|id| {
                    self.session
                        .project
                        .item(*id)
                        .filter(|item| matches!(item.kind, filmcraft_project::ItemKind::Sequence(_)))
                        .map(|item| (id.0, item.name.clone()))
                })
                .collect(),
            active: self.session.state.active_sequence.map(|id| id.0),
        }
    }

    /// 处理 dock 组与浮动面板绘制时收集到的交互（激活 / 关闭 / 拖动 / 序列 tab）。
    fn dispatch_dock_actions(&mut self, ctx: &egui::Context, actions: Vec<DockAction>) {
        for action in actions {
            match action {
                DockAction::Activate(panel) => {
                    let _ = self.dock_command(ctx, Action::Activate { panel });
                }
                DockAction::Close(panel) => {
                    let _ = self.dock_command(ctx, Action::Close { panel });
                }
                DockAction::Focus(panel) => self.ui.focused = panel,
                DockAction::BeginDrag(panel) => {
                    ctx.data_mut(|data| data.insert_temp(drag_id(), panel));
                }
                DockAction::OpenSequence(id) => {
                    let _ = self.dock_command(ctx, Action::Activate { panel: PanelKind::Timeline });
                    if self.session.state.active_sequence.map(|item| item.0) != Some(id)
                        && let Err(error) = self.session.execute("sequence.open", json!({"item": id}))
                    {
                        self.ui.status = error.to_string();
                    }
                }
                DockAction::CloseSequence(id) => {
                    if let Err(error) = self.session.execute("sequence.close", json!({"item": id})) {
                        self.ui.status = error.to_string();
                    }
                }
                DockAction::MoveSequence(id, index) => {
                    if let Err(error) = self.session.execute("sequence.moveTab", json!({"item": id, "index": index})) {
                        self.ui.status = error.to_string();
                    }
                }
                DockAction::PanelMenu(panel, pos) => {
                    let frame = ctx.cumulative_frame_nr();
                    ctx.data_mut(|data| {
                        data.insert_temp(egui::Id::new("panel-menu"), (panel, pos));
                        data.insert_temp(egui::Id::new("panel-menu-opened"), frame);
                    });
                }
            }
        }
    }

    fn is_floating(&self, panel: PanelKind) -> bool {
        self.ui.floating.iter().any(|f| f.panels.contains(&panel))
    }
    fn draw_floating_panels(&mut self, ctx: &egui::Context, body: Rect, class: LayoutClass) {
        if self.ui.floating.is_empty() {
            return;
        }
        let seqs = self.seq_tabs();
        let mut actions = vec![];
        let mut targets = vec![];
        for index in self.floating_order(ctx) {
            if let Some(target) = self.floating_group(ctx, index, &seqs, &mut actions, class) {
                targets.push(target);
            }
        }
        self.dispatch_dock_actions(ctx, actions);
        self.panel_drag(ctx, body, &targets);
    }

    fn floating_group(
        &mut self,
        ctx: &egui::Context,
        index: usize,
        seqs: &dock::SeqTabs,
        actions: &mut Vec<DockAction>,
        class: LayoutClass,
    ) -> Option<DropTarget<PanelKind>> {
        let floating = self.ui.floating.get(index)?.clone();
        let panel = *floating.panels.first()?;
        let rect = floating_rect(&floating, ctx.content_rect());
        let t = self.tokens;
        let id = egui::Id::new(("floating-panel", panel.id()));
        let mut output = None;
        egui::Area::new(id).order(egui::Order::Middle).fixed_pos(rect.min).constrain(false).interactable(true).show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(rect.size(), Sense::hover());
            let compact = floating.panels.len() == 1 && panel.compact();
            let header = if compact { 8.0 } else { t.tab_h };
            let strip = Rect::from_min_size(rect.min, vec2(rect.width(), header));
            let response = ui.interact(strip, id.with("move"), Sense::drag());
            self.auto.add_response(&format!("panel.float.{index}.move"), &response, "Move floating panel");
            let mut next = if response.dragged() { rect.translate(response.drag_delta()) } else { rect };
            let group = Group {
                path: format!("f{index}"),
                rect,
                content: Rect::from_min_max(pos2(rect.min.x, rect.min.y + header), rect.max),
                panels: floating.panels.clone(),
                active: floating.active,
            };
            let chrome =
                dock::draw_group_chrome_details_with(ui, &group, seqs, &t, &mut self.auto, class == LayoutClass::Wide && self.ui.workspace == "Jianying");
            actions.extend(chrome.actions);
            if let Some(active) = group.panels.get(group.active).copied() {
                self.panel_body(ui, active, group.content, class);
            }
            ui.painter().rect_stroke(rect, t.radius, Stroke::new(1.0, t.focus), StrokeKind::Outside);
            let grip = Rect::from_min_max(rect.max - vec2(14.0, 14.0), rect.max);
            let response = ui.interact(grip, id.with("resize"), Sense::click_and_drag());
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(), tl!("Resize floating panel")));
            self.auto.add_response(&format!("panel.float.{index}.resize"), &response, "Resize floating panel");
            for step in 0..3 {
                let offset = 4.0 + step as f32 * 4.0;
                ui.painter()
                    .line_segment([pos2(rect.max.x - offset, rect.max.y - 2.0), pos2(rect.max.x - 2.0, rect.max.y - offset)], Stroke::new(1.0, t.text_faint));
            }
            if response.hovered() || response.dragged() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            }
            let delta = floating_resize_delta(ui, &response);
            if response.has_focus() {
                ui.painter().rect_stroke(grip, 2.0, Stroke::new(1.5, t.focus), StrokeKind::Inside);
            }
            if delta != egui::Vec2::ZERO {
                next = Rect::from_min_size(next.min, (next.size() + delta).max(vec2(160.0, 100.0)));
            }
            if next != rect {
                let _ = self.dock_command(ctx, Action::MoveFloating { panel, rect: [next.min.x, next.min.y, next.width(), next.height()] });
            }
            dock::paint_group_focus(ui, &group, self.ui.focused, &t);
            output = target(&group, chrome.tabs, false);
        });
        output
    }

    fn panel_drag(&mut self, ctx: &egui::Context, body: Rect, targets: &[DropTarget<PanelKind>]) {
        let Some(panel) = ctx.data(|data| data.get_temp::<PanelKind>(drag_id())) else { return };
        let canceled =
            ctx.input(|input| input.key_pressed(egui::Key::Escape) || !input.focused || (!input.pointer.primary_down() && !input.pointer.primary_released()));
        if canceled || (!self.ui.dock.contains(panel) && !self.ui.floating.iter().any(|group| group.panels.contains(&panel))) {
            ctx.data_mut(|data| data.remove::<PanelKind>(drag_id()));
            return;
        }
        let Some(pos) = ctx.pointer_latest_pos() else { return };
        let force_float = ctx.input(|input| input.modifiers.command);
        let edge = (!force_float).then(|| workspace_edge(body, pos, targets)).flatten();
        let proposal = (!force_float && edge.is_none()).then(|| resolve_drop(&panel, pos, targets)).flatten();
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("dock-drop")));
        let t = self.tokens;
        if let Some((_, _, preview)) = edge {
            painter.rect_filled(preview.shrink(2.0), t.radius, t.focus.gamma_multiply(0.2));
            painter.rect_stroke(preview.shrink(2.0), t.radius, Stroke::new(2.0, t.focus), StrokeKind::Inside);
        }
        if let Some(proposal) = &proposal {
            painter.rect_filled(proposal.preview.shrink(2.0), t.radius, t.focus.gamma_multiply(0.2));
            painter.rect_stroke(proposal.preview.shrink(2.0), t.radius, Stroke::new(2.0, t.focus), StrokeKind::Inside);
            if let Some(mark) = proposal.insertion {
                painter.rect_filled(mark, 0.0, t.focus);
            }
        }
        let title = painter.layout_no_wrap(panel.title().to_owned(), crate::theme::Tokens::ui(12.0), Color32::WHITE);
        let ghost = Rect::from_min_size(pos + vec2(12.0, 10.0), title.size() + vec2(16.0, 8.0));
        painter.rect_filled(ghost, 4.0, t.focus.gamma_multiply(0.85));
        painter.galley(ghost.min + vec2(8.0, 4.0), title, Color32::WHITE);
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        ctx.request_repaint();
        if ctx.input(|input| input.pointer.primary_released()) {
            ctx.data_mut(|data| data.remove::<PanelKind>(drag_id()));
            if let Some((_, zone, _)) = edge {
                if crate::menus::invoke(self, ctx, "window.dockPanel", json!({"panel": panel.id(), "zone": zone})).is_ok() {
                    self.ui.focused = panel;
                }
                return;
            }
            let action = match proposal {
                Some(proposal) => Some(Action::Move { panel, anchor: proposal.anchor, placement: proposal.placement }),
                None if force_float || !targets.iter().any(|target| target.rect.contains(pos)) => {
                    Some(Action::Float { panel, rect: [pos.x - 40.0, pos.y - 12.0, 420.0, 320.0] })
                }
                None => None,
            };
            if let Some(action) = action
                && self.dock_command(ctx, action).is_ok()
            {
                self.ui.focused = panel;
            }
        }
    }
}

/// Width of the band along the workspace border that docks beside the whole layout.
const EDGE_BAND: f32 = 8.0;

/// The outermost drop zone: near the workspace border a drop splits the root, not the group
/// under the pointer. A floating group over the border (it rejects splits) keeps the drop.
fn workspace_edge(body: Rect, pos: egui::Pos2, targets: &[DropTarget<PanelKind>]) -> Option<(Zone, &'static str, Rect)> {
    if !body.is_finite() || !body.contains(pos) || targets.iter().any(|target| !target.allow_splits && target.rect.contains(pos)) {
        return None;
    }
    let band = EDGE_BAND.min(body.width() / 4.0).min(body.height() / 4.0);
    let edges = [
        (pos.x - body.left(), Zone::Left, "left"),
        (body.right() - pos.x, Zone::Right, "right"),
        (pos.y - body.top(), Zone::Top, "top"),
        (body.bottom() - pos.y, Zone::Bottom, "bottom"),
    ];
    let (distance, zone, name) = edges.into_iter().min_by(|a, b| a.0.total_cmp(&b.0))?;
    if distance > band {
        return None;
    }
    let size = body.size() * 0.25;
    let preview = match zone {
        Zone::Left => Rect::from_min_size(body.min, vec2(size.x, body.height())),
        Zone::Right => Rect::from_min_max(pos2(body.right() - size.x, body.top()), body.max),
        Zone::Top => Rect::from_min_size(body.min, vec2(body.width(), size.y)),
        Zone::Bottom | Zone::Center => Rect::from_min_max(pos2(body.left(), body.bottom() - size.y), body.max),
    };
    Some((zone, name, preview))
}

fn target(group: &Group, mut tabs: Vec<(PanelKind, Rect)>, allow_splits: bool) -> Option<DropTarget<PanelKind>> {
    let strip = Rect::from_min_max(group.rect.min, pos2(group.rect.max.x, group.content.min.y));
    // A compact grip still identifies the source group to the shared resolver.
    for panel in &group.panels {
        if !tabs.iter().any(|(candidate, _)| candidate == panel) {
            tabs.push((*panel, Rect::NOTHING));
        }
    }
    Some(DropTarget { anchor: *group.panels.get(group.active)?, rect: group.rect, tab_strip: Some(strip), tabs, allow_splits, accepts_tabs: true })
}

/// A corner is a two-axis resize control; focused arrows belong to it and Tab moves on.
fn floating_resize_delta(ui: &mut egui::Ui, response: &egui::Response) -> egui::Vec2 {
    if response.drag_started() {
        response.request_focus();
    }
    let mut delta = if response.dragged() { response.drag_delta() } else { egui::Vec2::ZERO };
    if response.has_focus() {
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(response.id, egui::EventFilter { horizontal_arrows: true, vertical_arrows: true, ..Default::default() })
        });
        ui.input_mut(|input| {
            for (key, amount) in [
                (egui::Key::ArrowLeft, vec2(-8.0, 0.0)),
                (egui::Key::ArrowRight, vec2(8.0, 0.0)),
                (egui::Key::ArrowUp, vec2(0.0, -8.0)),
                (egui::Key::ArrowDown, vec2(0.0, 8.0)),
            ] {
                if input.consume_key(egui::Modifiers::NONE, key) {
                    delta += amount;
                }
            }
        });
    }
    if response.enabled() { delta } else { egui::Vec2::ZERO }
}
