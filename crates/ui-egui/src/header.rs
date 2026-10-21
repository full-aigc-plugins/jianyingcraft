//! The header bar (Premiere 26 layout, 38 pt): Home and Import / Edit / Export on the left, the
//! document title centred, the workspace name and quick actions on the right. On macOS the menus
//! live in the native menu bar (set up by the app); elsewhere a compact in-window menu bar follows
//! the mode tabs.
//!
//! 剪映专业版工作区（Wide + `Jianying` + 编辑模式）只有**一条** 56 pt 顶栏：左侧 ☰（打开 shell
//! 菜单）→ 13 个一级 Tab（20 px 图标在上、11 px 文字在下的两行式）→ 右侧「导出」。这条顶栏取代原来
//! 的「Premiere 全局菜单条 + 42 pt Tab 条」两行结构；其余工作区与导入/导出模式仍使用上面的 Premiere
//! 顶栏。

use egui::{Color32, Rect, Sense, Stroke, pos2, vec2};

use crate::FilmcraftApp;
use crate::icons::{self, Icon};
use crate::layout::LayoutClass;
use crate::state::Mode;
use crate::theme::Tokens;

/// 剪映专业版单行顶栏的高度（Wide + `Jianying`）。
///
/// 对照剪映专业版截图：每个一级 Tab 是「20 px 图标 + 11 px 文字」两行，顶栏因此是 56 pt（不是
/// Premiere 的 38 pt）。`lib.rs` 直接用本常量分配顶栏矩形，dock 内容区随之下移，不需要另改窗口
/// 骨架。
pub(crate) const JIANYING_HEADER_HEIGHT: f32 = 56.0;

/// 该帧是否走剪映专业版的单行顶栏。
///
/// 只在**编辑模式**下生效：导入/导出模式仍复用 Premiere 顶栏（快速导出、外观、工作区等入口
/// 都在那里），否则两套出口会互相盖住。
fn is_jianying_desktop(app: &FilmcraftApp, rect: Rect) -> bool {
    app.ui.mode == Mode::Edit && app.ui.workspace == "Jianying" && LayoutClass::from_width(rect.width()) == LayoutClass::Wide
}

pub fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect) {
    if is_jianying_desktop(app, rect) {
        jianying_bar(app, ui, rect);
        return;
    }
    let t = app.tokens;
    let p = ui.painter().clone();
    p.rect_filled(rect, 0.0, t.header_bg);
    p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, Color32::BLACK));
    let drag = ui.interact(rect, egui::Id::new("header-drag"), Sense::click_and_drag());
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if drag.double_clicked() {
        let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
    }
    let mut x = rect.min.x + if app.integrated_titlebar { 104.0 } else { 14.0 };
    // Home
    let home = Rect::from_center_size(pos2(x + 10.0, rect.center().y), vec2(26.0, 26.0));
    let hresp = ui.interact(home, egui::Id::new("hdr-home"), Sense::click()).on_hover_text(tl!("Home"));
    app.auto.add("header.home", home, "Home");
    if hresp.hovered() {
        p.rect_filled(home, 4.0, t.hover);
    }
    icons::paint(&p, home.shrink(5.0), Icon::Home, if hresp.hovered() { t.text } else { t.text_dim });
    if hresp.clicked() {
        app.ui.mode = Mode::Import;
    }
    x = home.max.x + 16.0;
    // Mode tabs (14 pt; active = primary text with a 2 pt underline under the label)
    for (m, label, shown) in [(Mode::Import, "Import", tl!("Import")), (Mode::Edit, "Edit", tl!("Edit")), (Mode::Export, "Export", tl!("Export"))] {
        let active = app.ui.mode == m;
        let galley = p.layout_no_wrap(shown.to_string(), Tokens::ui(14.0), if active { t.tab_text_active } else { t.tab_text });
        let r = Rect::from_min_size(pos2(x - 6.0, rect.min.y + 6.0), vec2(galley.size().x + 12.0, rect.height() - 12.0));
        let resp = ui.interact(r, egui::Id::new(("mode", label)), Sense::click());
        app.auto.add(&format!("header.mode.{}", label.to_ascii_lowercase()), r, label);
        let col = if active || resp.hovered() { t.tab_text_active } else { t.tab_text };
        let gw = galley.size().x;
        p.galley_with_override_text_color(pos2(x, rect.center().y - galley.size().y / 2.0), galley, col);
        if active {
            p.line_segment([pos2(x, rect.min.y + 29.0), pos2(x + gw, rect.min.y + 29.0)], Stroke::new(2.0, t.tab_text_active));
        }
        if resp.clicked() {
            app.ui.mode = m;
        }
        x += gw + 24.0;
    }
    let mut left_end = x;
    // In-window menus when there is no native menu bar.
    if app.ui.show_menu_bar {
        let menu_rect = Rect::from_min_max(pos2(x + 6.0, rect.min.y + 7.0), pos2(x + 520.0, rect.max.y - 7.0));
        let mut mu = ui.new_child(egui::UiBuilder::new().max_rect(menu_rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
        mu.style_mut().visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        mu.style_mut().visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        mu.style_mut().visuals.override_text_color = Some(t.text_dim);
        crate::menus::menu_bar(app, &mut mu);
        left_end = mu.min_rect().max.x;
    }
    // Document title, centred
    let title = if app.session.is_dirty() { tlf!("{name} - Edited", name = app.session.project.name) } else { app.session.project.name.clone() };
    // Right cluster: icons at ~38 pt pitch, then workspace name in caps.
    let mut rx = rect.max.x - 14.0;
    let mut btn = |ui: &mut egui::Ui, icon: Icon, id: &str, tip: &str, app: &mut FilmcraftApp| -> egui::Response {
        let r = Rect::from_center_size(pos2(rx - 14.0, rect.center().y), vec2(28.0, 28.0));
        rx -= 38.0;
        let resp = ui.interact(r, egui::Id::new(("hdr", id)), Sense::click()).on_hover_text(tip);
        app.auto.add(&format!("header.{id}"), r, tip);
        if resp.hovered() {
            ui.painter().rect_filled(r, 4.0, t.hover);
        }
        icons::paint(ui.painter(), r.shrink(6.0), icon, if resp.hovered() { t.text } else { t.text_dim });
        resp
    };
    if btn(ui, Icon::Fullscreen, "fullscreen", tl!("Full screen"), app).clicked() {
        let fs = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
    }
    if btn(ui, Icon::Speaker, "volume", tl!("Volume"), app).clicked() {
        app.ui.status = tl!("Master volume: use the Audio Track Mixer").into();
    }
    if btn(ui, Icon::Search, "search", tl!("Search"), app).clicked() {
        app.show_panel(crate::dock::PanelKind::Effects);
    }
    if btn(ui, Icon::Bell, "notifications", tl!("Progress"), app).clicked() {
        app.ui.mode = Mode::Export;
    }
    // Quick Export: a popup with File Name & Location, a preset list and Export (Premiere 26)
    let qx = rect.max.x - 14.0 - 4.0 * 38.0; // the fifth button from the right
    if btn(ui, Icon::Export, "quickExport", tl!("Quick Export"), app).clicked() {
        app.ui.export.quick_open = !app.ui.export.quick_open;
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("quick-export-toggled"), true));
    }
    crate::panels::export_mode::quick_export(app, ui.ctx(), pos2(qx - 340.0, rect.max.y + 4.0));
    // Appearance Mode: click for the next one (Auto, Light, Dark); the icon shows the current one.
    let (icon, mode) = match app.session.prefs.appearance.appearance_mode.as_str() {
        "auto" => (Icon::Monitor, tl!("Sync with system")),
        "light" => (Icon::Sun, tl!("Light")),
        _ => (Icon::Moon, tl!("Dark")),
    };
    let tip = format!("{}: {mode}", tl!("Appearance Mode"));
    let appearance = btn(ui, icon, "appearance", &tip, app);
    appearance.context_menu(|ui| crate::panels::settings::appearance_context_menu(app, ui));
    if appearance.clicked() {
        crate::panels::settings::cycle_appearance(app, ui.ctx());
    }
    let ws_resp = btn(ui, Icon::Workspaces, "workspaces", tl!("Workspaces"), app);
    // workspace name (caps)
    let ws = crate::i18n::t(&app.ui.workspace).to_uppercase();
    let wg = p.layout_no_wrap(ws.clone(), Tokens::ui(11.0), t.text_dim);
    let wr = Rect::from_min_size(pos2(rx - wg.size().x + 10.0, rect.center().y - 10.0), vec2(wg.size().x + 8.0, 20.0));
    let wresp = ui.interact(wr, egui::Id::new("hdr-ws-name"), Sense::click());
    app.auto.add("header.workspaceName", wr, &ws);
    p.galley_with_override_text_color(pos2(wr.min.x + 4.0, rect.center().y - wg.size().y / 2.0), wg, if wresp.hovered() { t.text } else { t.text_dim });
    // 标题：占满项目名与右侧控件之间的空隙。上游此处曾有 Discord 按钮，
    // 派生品不提供第三方社区入口（品牌许可要求），因此只保留标题绘制。
    {
        let left = left_end + 12.0;
        let right = wr.min.x - 12.0;
        if right > left + 40.0 {
            let mut job = egui::text::LayoutJob::simple_singleline(title, Tokens::ui(14.0), t.tab_text_active);
            job.wrap.max_width = right - left;
            job.wrap.max_rows = 1;
            let galley = p.layout_job(job);
            let half = galley.size().x / 2.0;
            let center = if rect.center().x - half >= left && rect.center().x + half <= right { rect.center().x } else { (left + right) / 2.0 };
            p.with_clip_rect(Rect::from_min_max(pos2(left, rect.min.y), pos2(right, rect.max.y))).galley(
                pos2(center - half, rect.center().y - galley.size().y / 2.0),
                galley,
                t.tab_text_active,
            );
        }
    }
    let popup_id = egui::Id::new("workspaces-popup");
    if ws_resp.clicked() || wresp.clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(popup_id, true));
    }
    let open = ui.ctx().data(|d| d.get_temp::<bool>(popup_id).unwrap_or(false));
    if open {
        let anchor = pos2(wr.min.x, rect.max.y + 4.0);
        let area = egui::Area::new(popup_id.with("area")).order(egui::Order::Foreground).fixed_pos(anchor).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(220.0);
                for w in crate::dock::names(&app.workspaces) {
                    let sel = app.ui.workspace == w;
                    if ui.selectable_label(sel, crate::i18n::t(&w)).clicked() {
                        app.set_workspace(&w);
                        ui.ctx().data_mut(|d| d.insert_temp(popup_id, false));
                    }
                }
                ui.separator();
                if ui.button(tl!("Reset to Saved Layout")).clicked() {
                    let n = app.ui.workspace.clone();
                    app.set_workspace(&n);
                    ui.ctx().data_mut(|d| d.insert_temp(popup_id, false));
                }
            });
        });
        if area.response.clicked_elsewhere() && !(ws_resp.clicked() || wresp.clicked()) {
            ui.ctx().data_mut(|d| d.insert_temp(popup_id, false));
        }
    }
}

/// 剪映专业版的单行顶栏：☰ → 13 个两行式一级 Tab → 「导出」。
///
/// 只画导航与导出，没有 Home / 模式 Tab / 文档标题 / 工作区名 / 快速导出——剪映桌面版把这些
/// 入口放在草稿列表和导出面板里。全局菜单收进 ☰（`ui.jy_menu_open`，弹层由 `dock_area` 在面板
/// 之后绘制，保证盖在预览与时间轴之上）。
///
/// `rect` 由 `lib.rs` 按 [`JIANYING_HEADER_HEIGHT`] 分配（Wide 下 56 pt）。Tab 自身的高度交给
/// `jianying_tab_row`：图标 20 px 在上、11 px 文字在下，两行合起来的视觉中心与顶栏中心对齐。
/// 不再靠下游手工下移内容区。
fn jianying_bar(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect) {
    let t = app.tokens;
    let bar = rect;
    let p = ui.painter().clone();
    p.rect_filled(bar, 0.0, t.header_bg);
    p.line_segment([bar.left_bottom(), bar.right_bottom()], Stroke::new(1.0, Color32::BLACK));
    app.auto.add("jy.topbar", bar, crate::brand::PRODUCT_NAME);
    // 空白处可拖动窗口；双击切换最大化（与 Premiere 顶栏一致）。
    let drag = ui.interact(bar, egui::Id::new("jy-header-drag"), Sense::click_and_drag());
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if drag.double_clicked() {
        let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
    }
    // ☰：全局菜单的唯一入口。和 Compact/Medium 的 ☰ 共用同一份状态与弹层。
    let menu = Rect::from_center_size(pos2(bar.min.x + 25.0, bar.center().y), vec2(30.0, 28.0));
    let menu_response = ui.interact(menu, egui::Id::new("jy.menu"), Sense::click()).on_hover_text(tl!("Menu"));
    if app.ui.jy_menu_open || menu_response.hovered() {
        p.rect_filled(menu, 6.0, if app.ui.jy_menu_open { t.accent_hover } else { t.hover });
    }
    for line in 0..3 {
        let y = menu.center().y - 5.0 + line as f32 * 5.0;
        p.line_segment([pos2(menu.min.x + 9.0, y), pos2(menu.max.x - 9.0, y)], Stroke::new(1.5, t.text));
    }
    app.auto.add("jy.menu", menu, "Menu");
    if menu_response.clicked() {
        app.ui.jy_menu_open = !app.ui.jy_menu_open;
        if app.ui.jy_menu_open {
            // 每次打开都从菜单根开始，不继承上次展开的分组；两个弹层不叠加。
            app.ui.jy_menu_group = None;
            app.ui.jy_tab_more = false;
        }
    }
    // 右上角「导出」：剪映桌面版最显眼的那颗按钮。
    let export = Rect::from_min_size(pos2(bar.max.x - 12.0 - 76.0, bar.center().y - 14.0), vec2(76.0, 28.0));
    let export_response = ui.interact(export, egui::Id::new("jy.export"), Sense::click()).on_hover_text(tl!("Export"));
    p.rect_filled(export, 6.0, if export_response.hovered() { t.accent_hover } else { t.accent });
    let export_label = tl!("Export");
    let galley = p.layout_no_wrap(export_label.to_string(), Tokens::semibold(12.5), Color32::WHITE);
    p.galley(pos2(export.center().x - galley.size().x / 2.0, export.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
    app.auto.add("jy.export", export, export_label);
    if export_response.clicked() {
        app.ui.jy_menu_open = false;
        let ctx = ui.ctx().clone();
        let _ = crate::menus::invoke(app, &ctx, "file.exportMedia", serde_json::json!({}));
    }
    // 13 个一级 Tab：画在 ☰ 与「导出」之间，点哪个就切到哪个面板（`jy.tabstrip.*` 与
    // `jy.tab.*` 两套自动化 id 都登记，见 `dock_ui::jianying_tab_row`）。
    let tabs = Rect::from_min_max(pos2(menu.max.x + 10.0, bar.min.y), pos2((export.min.x - 12.0).max(menu.max.x + 10.0), bar.max.y));
    app.jianying_tab_row(ui, tabs, &t);
}
