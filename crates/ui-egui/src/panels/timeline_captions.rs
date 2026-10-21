//! Caption track rows of the Timeline: drawn in their own area above the video tracks (Premiere's
//! layout). Each row has a header (C1…, name, format, output eye, lock) and caption blocks in a
//! distinct colour showing their text. Click selects (Cmd/Shift adds), dragging the body moves,
//! dragging an edge trims, double-click opens the Text panel's Captions tab. Every gesture ends in
//! one `captions.*` command; every element registers an automation id (`timeline.caption.*`,
//! `timeline.captionTrack.C1.*`).

use egui::{Align2, Color32, CursorIcon, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use filmcraft_project::{CaptionTrack, ClipId, Sequence};
use filmcraft_time::Tick;
use serde_json::{Value, json};

use super::timeline::Layout;
use crate::FilmcraftApp;
use crate::dock::PanelKind;
use crate::icons::{self, Icon};
use crate::theme::Tokens;

/// Height of one caption track row.
pub const ROW_H: f32 = 28.0;
/// Caption block colours (fill, selected fill).
const FILL: Color32 = Color32::from_rgb(0x6b, 0x4f, 0x8f);
const FILL_SEL: Color32 = Color32::from_rgb(0x8c, 0x6c, 0xb4);

// ---------------------------------------------------------------------------
// 剪映式字幕轨头：与 `timeline.rs` 的 V1/A1 徽标同尺寸、同圆角、同字体
// ---------------------------------------------------------------------------

/// 字幕徽标尺寸（与视频/音频徽标一致）：26×18，左侧 4 px 圆角。
const BADGE_W: f32 = 26.0;
const BADGE_H: f32 = 18.0;
/// 字幕徽标底色：沿用字幕块的紫色系（`#6a4b8f`）。关掉轨道输出时整体压暗。
const JY_BADGE_CAPTION: Color32 = Color32::from_rgb(0x6a, 0x4b, 0x8f);
/// 类型小字右侧给两个图标留的最小宽度：12 px 命中框 + 4 px 间距。
const ICON_ROW_W: f32 = 2.0 * 12.0 + 4.0;

/// 一行图标的命中框与间距：优先 12 px 框 + 4 px 间距（框内画 11 px 图标）；轨道头被拖窄时
/// 逐级收窄到 11 px 框 / 4 px、11 px 框 / 3 px，最后才退到 10 px 框 / 2 px。
/// `None` 表示连最窄的一档都放不下。
fn icon_row(count: usize, space: f32) -> Option<(f32, f32)> {
    if count == 0 {
        return None;
    }
    let n = count as f32;
    [(12.0f32, 4.0f32), (11.0, 4.0), (11.0, 3.0), (10.0, 2.0)].into_iter().find(|(b, g)| n * b + (n - 1.0) * g <= space)
}

/// 徽标右侧的图标按钮（照 `timeline.rs` 的 `jy_icon`）：`r` 是命中框，框内画比命中框小 1 px
/// 的图标（12 px 框 → 11 px 图标），悬停铺 `hover`。
fn badge_icon(ui: &mut egui::Ui, r: Rect, clip: Rect, icon: Icon, color: Color32, t: &Tokens, id: egui::Id) -> egui::Response {
    let painter = ui.painter().with_clip_rect(clip);
    let resp = ui.interact(r.intersect(clip), id, Sense::click());
    if resp.hovered() {
        painter.rect_filled(r, 2.0, t.hover);
    }
    let glyph = (r.width().min(r.height()) - 1.0).max(6.0);
    icons::paint(&painter, Rect::from_center_size(r.center(), vec2(glyph, glyph)), icon, color);
    resp
}

/// Row rects (full width incl. header) for each caption track, top first.
pub fn rows(area: Rect, n: usize) -> Vec<Rect> {
    (0..n).map(|i| Rect::from_min_size(pos2(area.min.x, area.min.y + i as f32 * ROW_H), vec2(area.width(), ROW_H))).collect()
}

#[derive(Clone, Copy, Debug)]
enum Gesture {
    Move,
    TrimIn,
    TrimOut,
}

fn drag_id() -> egui::Id {
    egui::Id::new("caption-drag")
}

/// Live drag state: (caption, gesture, delta ticks).
fn drag_state(ctx: &egui::Context) -> Option<(u64, u8, i64)> {
    ctx.data(|d| d.get_temp::<(u64, u8, i64)>(drag_id()))
}

fn block_rect(c: &filmcraft_project::Caption, row: Rect, layout: &Layout, drag: Option<(u64, u8, i64)>) -> Rect {
    let (mut s, mut e) = (c.start, c.end());
    if let Some((id, g, d)) = drag.filter(|d| d.0 == c.id.0) {
        let _ = id;
        match g {
            0 => {
                s += Tick(d);
                e += Tick(d);
            }
            1 => s = (s + Tick(d)).min(e),
            _ => e = (e + Tick(d)).max(s),
        }
    }
    Rect::from_min_max(pos2(layout.x_of(s), row.min.y + 2.0), pos2(layout.x_of(e).max(layout.x_of(s) + 1.0), row.max.y - 2.0))
}

/// Paint headers and caption blocks.
pub fn paint(app: &mut FilmcraftApp, ui: &mut egui::Ui, seq: &Sequence, area: Rect, layout: &Layout, t: &Tokens) {
    if seq.caption_tracks.is_empty() {
        return;
    }
    // 与视频/音频轨头同一个收敛后的宽度：剪映工作区 ≤96 px，否则字幕头会伸出轨道列、
    // 压到字幕块上（#235）。
    let hw = super::timeline::header_width(app);
    let content = layout.content;
    let painter = ui.painter().with_clip_rect(area);
    let sel = app.session.state.caption_selection.clone();
    let drag = drag_state(ui.ctx());
    let mut actions: Vec<(String, Value)> = Vec::new();
    for (i, (tr, row)) in seq.caption_tracks.iter().zip(rows(area, seq.caption_tracks.len())).enumerate() {
        let lane = Rect::from_min_max(pos2(content.min.x, row.min.y), pos2(content.max.x, row.max.y));
        painter.rect_filled(lane, 0.0, t.caption_track_bg);
        painter.line_segment([pos2(row.min.x, row.max.y - 0.5), pos2(row.max.x, row.max.y - 0.5)], Stroke::new(1.0, t.tl_bg));
        // header：剪映式徽标（26×18、左 4 px 圆角、10 px 白字）、类型小字与右对齐的一行图标。
        // 字幕行固定 28 px（低于 40 px 的两行门槛），因此沿用 V/A 矮行上的降级：只保留单行，
        // 图标用 `icon_row` 逐级收窄，类型小字/轨名放不下就让位——绝不画到相邻行。
        let label = format!("C{}", i + 1);
        let hrect = Rect::from_min_max(row.min, pos2(row.min.x + hw, row.max.y));
        // 活动轨道：这一轨里有选中的字幕（V/A 用源补丁或选中片段判定，字幕轨没有源补丁）。
        let active = tr.captions.iter().any(|c| sel.contains(&c.id));
        // 与 `timeline.track.V1.header` 同名的自动化 id，便于断言字幕头与视频轨头等宽。
        app.auto.add(&format!("timeline.captionTrack.{label}.header"), hrect, &tr.name);
        // 先建 header 的响应，徽标悬停与右键菜单都复用它（图标按钮随后注册，压在最上层）。
        let hresp = ui.interact(hrect, egui::Id::new(("cap-header", tr.id.0)), Sense::click());
        // right-click the header: delete this track or every empty caption track (registered
        // before the lock / eye toggles so those stay on top)
        hresp.context_menu(|ui| {
            for (key, text, cmd, p) in [
                ("delete", tl!("Delete Caption Track"), "captions.deleteTrack", json!({"track": tr.id.0})),
                ("deleteEmpty", tl!("Delete Empty Caption Tracks"), "sequence.deleteTracks", json!({"captions": "empty"})),
            ] {
                let enabled = key != "deleteEmpty" || seq.caption_tracks.iter().any(|t| t.captions.is_empty());
                let r = ui.add_enabled(enabled, egui::Button::new(text));
                app.auto.add(&format!("timeline.captionTrack.{label}.menu.{key}"), r.rect, text);
                if r.clicked() {
                    actions.push((cmd.into(), p));
                    ui.close();
                }
            }
        });
        painter.rect_filled(hrect, 0.0, if active { t.row_selected } else { t.tl_header_bg });
        painter.line_segment([pos2(hrect.min.x, hrect.max.y - 0.5), pos2(hrect.max.x, hrect.max.y - 0.5)], Stroke::new(1.0, t.separator));
        // ---- 徽标：与 V/A 同尺寸、同圆角；字幕轨的紫色，关掉输出时压暗。
        let badge = Rect::from_min_size(pos2(hrect.min.x + 2.0, row.center().y - BADGE_H / 2.0), vec2(BADGE_W, BADGE_H));
        let round_left = egui::CornerRadius { nw: 4, ne: 0, sw: 4, se: 0 };
        painter.rect_filled(badge, round_left, if tr.enabled { JY_BADGE_CAPTION } else { JY_BADGE_CAPTION.gamma_multiply(0.45) });
        if hresp.hover_pos().is_some_and(|p| badge.contains(p)) {
            painter.rect_filled(badge, round_left, Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 0x1c));
        }
        // 活动轨道：徽标描 1 px 白边（行底已铺成 `row_selected`），与 V/A 一致。
        if active {
            painter.rect_stroke(badge, round_left, Stroke::new(1.0, Color32::WHITE), StrokeKind::Inside);
        }
        painter.text(badge.center(), Align2::CENTER_CENTER, &label, Tokens::semibold(10.0), Color32::WHITE);
        // `timeline.captionTrack.C1` 是旧版徽标的只读别名，继续保留；新增 `timeline.track.C1.badge`
        // 与 V/A 的命名习惯对齐。
        app.auto.add(&format!("timeline.captionTrack.{label}"), badge, &tr.name);
        app.auto.add(&format!("timeline.track.{label}.badge"), badge, &label);

        // ---- 类型小字「字幕」：先给两个图标留足 28 px，余下的宽度才给文字；窄头自动省略。
        let right = hrect.max.x - 3.0;
        let kind_x = badge.max.x + 3.0;
        // 复用已有词条（`Subtitle` → 中文「字幕」），不新增未翻译的字面量。
        let kind_text = tl!("Subtitle");
        let galley = painter.layout_no_wrap(kind_text.to_owned(), Tokens::ui(10.0), t.text_dim);
        let kind_room = (right - ICON_ROW_W - 3.0 - kind_x).max(0.0);
        let kind_w = galley.size().x.min(kind_room);
        let show_kind = kind_w > 1.0;
        let kind_r = Rect::from_min_size(pos2(kind_x, badge.min.y), vec2(if show_kind { kind_w } else { 0.0 }, BADGE_H));
        if show_kind {
            painter.with_clip_rect(kind_r).galley(pos2(kind_r.min.x, kind_r.center().y - galley.size().y / 2.0), galley, t.text_dim);
        }

        // ---- 右侧一行图标：锁 + 眼睛，11 px、右对齐（12 px 命中框 / 4 px 间距）。
        let space = (right - kind_r.max.x - 3.0).max(0.0);
        let (box_w, gap) = icon_row(2, space).unwrap_or((10.0, 1.0));
        let x0 = (right - 2.0 * box_w - gap).max(badge.max.x + 2.0);
        let slot = |i: usize| Rect::from_min_size(pos2(x0 + i as f32 * (box_w + gap), badge.center().y - box_w / 2.0), vec2(box_w, box_w));
        let lock_r = slot(0);
        let lresp = badge_icon(
            ui,
            lock_r,
            hrect,
            if tr.locked { Icon::Lock } else { Icon::Unlock },
            if tr.locked { t.icon_active } else { t.text_dim },
            t,
            egui::Id::new(("cap-lock", tr.id.0)),
        );
        app.auto.add(&format!("timeline.captionTrack.{label}.locked"), lock_r, "Toggle Caption Track Lock");
        if lresp.clicked() {
            actions.push(("captions.setTrack".into(), json!({"track": tr.id.0, "locked": !tr.locked})));
        }
        let eye_r = slot(1);
        let eresp = badge_icon(
            ui,
            eye_r,
            hrect,
            if tr.enabled { Icon::Eye } else { Icon::EyeOff },
            if tr.enabled { t.text_dim } else { t.text_faint },
            t,
            egui::Id::new(("cap-eye", tr.id.0)),
        );
        app.auto.add(&format!("timeline.captionTrack.{label}.enabled"), eye_r, "Toggle Caption Track Output");
        if eresp.clicked() {
            actions.push(("captions.setTrack".into(), json!({"track": tr.id.0, "enabled": !tr.enabled})));
        }

        // 96 px 的紧凑头已被徽标/类型小字/锁/眼睛占满，名字放不下就只留徽标（名称仍见 Text
        // 面板的 Captions 页）；宽头照旧在类型小字与图标之间列出“名称 · 格式”。
        if hw > super::timeline::COMPACT_HEADER_W {
            let name = format!("{} · {}", tr.name, crate::i18n::t(tr.format.label()));
            let name_r = Rect::from_min_max(pos2(kind_r.max.x + 6.0, row.min.y), pos2(x0 - 3.0, row.max.y));
            if name_r.width() >= 22.0 {
                painter.with_clip_rect(name_r).text(pos2(name_r.min.x, row.center().y), Align2::LEFT_CENTER, name, Tokens::ui(11.0), t.text_dim);
            }
        }
        // caption blocks
        let p = painter.with_clip_rect(lane);
        let (v0, v1) = (layout.tick_at(content.min.x - 2.0), layout.tick_at(content.max.x + 2.0));
        for c in tr.captions.iter().filter(|c| c.end() >= v0 && c.start <= v1) {
            let r = block_rect(c, row, layout, drag);
            let selected = sel.contains(&c.id);
            p.rect_filled(r, 3.0, if selected { FILL_SEL } else { FILL });
            if selected {
                p.rect_stroke(r, 3.0, Stroke::new(1.0, t.clip_selected_border), StrokeKind::Inside);
            }
            if r.width() > 14.0 {
                let text = filmcraft_project::plain_text(&c.text).replace('\n', " ");
                // in reading order; Arabic and Hebrew start at the block's right edge
                crate::rtl::paint_line(
                    &p.with_clip_rect(r.shrink2(vec2(4.0, 0.0)).intersect(lane)),
                    Rect::from_min_max(pos2(r.min.x + 5.0, r.min.y), pos2(r.max.x - 5.0, r.max.y)),
                    &text,
                    Tokens::ui(11.0),
                    if tr.enabled { Color32::WHITE } else { t.text_dim },
                );
            }
            app.auto.add(&format!("timeline.caption.{}", c.id.0), r.intersect(lane), &c.text);
        }
        if tr.locked {
            p.rect_filled(lane, 0.0, Color32::from_black_alpha(70));
        }
    }
    // column separator
    painter.line_segment([pos2(area.min.x + hw - 0.5, area.min.y), pos2(area.min.x + hw - 0.5, area.max.y)], Stroke::new(1.0, t.separator));
    run(app, ui, actions);
}

/// Pointer interaction on caption blocks (call after the timeline's own interaction so captions
/// are on top).
pub fn interact(app: &mut FilmcraftApp, ui: &mut egui::Ui, seq: &Sequence, area: Rect, layout: &Layout) {
    let ctx = ui.ctx().clone();
    let rate = seq.settings.frame_rate;
    let drag = drag_state(&ctx);
    let mut actions: Vec<(String, Value)> = Vec::new();
    for (tr, row) in seq.caption_tracks.iter().zip(rows(area, seq.caption_tracks.len())) {
        let lane = Rect::from_min_max(pos2(layout.content.min.x, row.min.y), pos2(layout.content.max.x, row.max.y));
        for c in &tr.captions {
            let r = block_rect(c, row, layout, drag).intersect(lane);
            if r.width() <= 0.0 {
                continue;
            }
            interact_block(app, ui, tr, c, r, rate, &mut actions);
        }
    }
    run(app, ui, actions);
}

fn interact_block(
    app: &mut FilmcraftApp,
    ui: &mut egui::Ui,
    tr: &CaptionTrack,
    c: &filmcraft_project::Caption,
    r: Rect,
    rate: filmcraft_time::FrameRate,
    actions: &mut Vec<(String, Value)>,
) {
    let ctx = ui.ctx().clone();
    let resp = ui.interact(r, egui::Id::new(("caption-block", c.id.0)), Sense::click_and_drag());
    let edge = 5.0f32.min(r.width() / 4.0);
    let gesture_at = |x: f32| {
        if x - r.min.x < edge {
            Gesture::TrimIn
        } else if r.max.x - x < edge {
            Gesture::TrimOut
        } else {
            Gesture::Move
        }
    };
    if let Some(p) = resp.hover_pos()
        && !tr.locked
    {
        ctx.set_cursor_icon(match gesture_at(p.x) {
            Gesture::Move => CursorIcon::Default,
            _ => CursorIcon::ResizeColumn,
        });
    }
    if resp.double_clicked() {
        app.show_panel(PanelKind::Text);
        app.ui.text_tab = "Captions".into();
        actions.push(("captions.goTo".into(), json!({"caption": c.id.0})));
        return;
    }
    if resp.clicked() {
        let add = ctx.input(|i| i.modifiers.command || i.modifiers.shift);
        actions.push(("captions.select".into(), json!({"captions": [c.id.0], "add": add})));
    }
    if tr.locked {
        return;
    }
    // decide on what was under the pointer when the button went down, not where egui recognised
    // the drag (6 pt later, past the 5 px edge zones) (#259)
    if resp.drag_started()
        && let Some(p) = ctx.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos())
    {
        let g = match gesture_at(p.x) {
            Gesture::Move => 0u8,
            Gesture::TrimIn => 1,
            Gesture::TrimOut => 2,
        };
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("caption-drag-origin"), p.x));
        ctx.data_mut(|d| d.insert_temp(drag_id(), (c.id.0, g, 0i64)));
        if !app.session.state.caption_selection.contains(&c.id) {
            actions.push(("captions.select".into(), json!({"captions": [c.id.0]})));
        }
    }
    if resp.dragged()
        && let (Some((id, g, _)), Some(p)) = (drag_state(&ctx), resp.interact_pointer_pos())
        && id == c.id.0
    {
        let x0 = ctx.data(|d| d.get_temp::<f32>(egui::Id::new("caption-drag-origin"))).unwrap_or(p.x);
        let pps = app.ui.timeline.pps;
        let d = rate.snap_nearest(Tick::from_seconds_f64((p.x - x0) as f64 / pps));
        ctx.data_mut(|dd| dd.insert_temp(drag_id(), (id, g, d.0)));
    }
    if resp.drag_stopped()
        && let Some((id, g, d)) = drag_state(&ctx)
        && id == c.id.0
    {
        ctx.data_mut(|dd| dd.remove::<(u64, u8, i64)>(drag_id()));
        // egui ends a drag on Escape: that stop abandons it (#580)
        if d != 0 && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            match g {
                0 => {
                    let sel: Vec<u64> = if app.session.state.caption_selection.contains(&ClipId(id)) {
                        app.session.state.caption_selection.iter().map(|c| c.0).collect()
                    } else {
                        vec![id]
                    };
                    actions.push(("captions.move".into(), json!({"captions": sel, "delta": d})));
                }
                1 => actions.push(("captions.trim".into(), json!({"caption": id, "edge": "in", "delta": d}))),
                _ => actions.push(("captions.trim".into(), json!({"caption": id, "edge": "out", "delta": d}))),
            }
        }
    }
}

fn run(app: &mut FilmcraftApp, ui: &egui::Ui, actions: Vec<(String, Value)>) {
    let ctx = ui.ctx().clone();
    for (cmd, p) in actions {
        if let Err(e) = crate::menus::invoke(app, &ctx, &cmd, p) {
            app.ui.status = e;
        }
    }
}
