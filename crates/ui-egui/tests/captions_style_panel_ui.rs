//! Automated evidence that the Text panel's caption style strip previews the model and dispatches
//! `captions.setStyle` (openspec #483, test B).
//!
//! The Captions tab draws a style strip whose automation ids are `text.captions.style.font`,
//! `.size`, `.color`, `.background`, `.align.left|center|right` and `.anchor.top|middle|bottom`
//! (`crates/ui-egui/src/panels/text.rs::style_strip`). The strip reads the active track's
//! `CaptionStyle` every frame and pushes `captions.setStyle` when a widget changes.
//!
//! Three things are asserted, through the same `Driver` the other `*_ui.rs` tests use:
//!
//! 1. every id exists, has a real rectangle and sits inside the visible Text panel;
//! 2. the widgets **report the track's current values**: the font combo's value, the size
//!    `DragValue`'s number (read from the AccessKit tree, not from a private field), the colour
//!    swatch, the "Box" checkbox and the three alignment buttons' selected state;
//! 3. changing a value **through the widgets** changes `CaptionTrack.style` (the drag of the size
//!    field, a click on "align right", a click on "Box"), the strip then shows the new value, and a
//!    style change made through the engine is picked up by the strip too — so the preview is
//!    derived from the model, not a stale copy.
//!
//! Set `FILMCRAFT_UI_SNAPSHOT_DIR=<dir>` to also render the window offscreen with wgpu and write
//! `captions-style-panel.png` there; without it no GPU is needed.

use std::path::PathBuf;
use std::sync::mpsc::{Sender, channel};

use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use filmcraft_engine::Session;
use filmcraft_time::Tick;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::ControlRequest;
use serde_json::{Value, json};

/// Wide enough that the whole strip (font, size, colour, box, align, position) fits in the Text
/// panel: in the "Captions and Graphics" workspace it gets 40% of 1590 points.
const W: f32 = 1920.0;
const H: f32 = 1080.0;

/// The style-strip ids this test drives.
const STRIP_IDS: [&str; 9] = [
    "text.captions.style.font",
    "text.captions.style.size",
    "text.captions.style.color",
    "text.captions.style.background",
    "text.captions.style.align.left",
    "text.captions.style.align.center",
    "text.captions.style.align.right",
    "text.captions.style.anchor.top",
    "text.captions.style.anchor.bottom",
];

struct Driver {
    harness: Harness<'static, FilmcraftApp>,
    tx: Sender<ControlRequest>,
    snapshots: Option<PathBuf>,
}

impl Driver {
    /// The demo project with one line of transcript turned into a caption track, the way
    /// `crates/engine/src/transcript_tests.rs` seeds it: `transcript.set` needs a media item that
    /// plays on an audio track, and the demo project is the deterministic fixture that has one.
    fn demo() -> Self {
        let mut session = Session::default();
        session.execute("file.openDemoProject", json!({})).expect("demo project");
        let (item, source_in) = {
            let a = &session.active_sequence().expect("a sequence").audio_tracks[0].items[0];
            (a.item, a.source_in)
        };
        let at = |seconds: f64| source_in + Tick::from_seconds_f64(seconds);
        let words = json!([
            {"text": "Style", "start": at(0.2).0, "end": at(0.8).0},
            {"text": "test", "start": at(0.9).0, "end": at(1.5).0},
        ]);
        session.execute("transcript.set", json!({"item": item.0, "transcript": {"language": "en", "words": words}})).expect("transcript.set");
        let r = session.execute("transcript.createCaptions", json!({"maxChars": 40, "lines": 1})).expect("transcript.createCaptions");
        assert_eq!(r["captions"], 1, "{r}");

        let (tx, rx) = channel();
        let app = FilmcraftApp::new(session).with_control(rx);
        let snapshots = filmcraft_ui_egui::test_support::snapshot_dir();
        let mut b = Harness::builder().with_size(egui::vec2(W, H)).with_max_steps(10_000);
        if snapshots.is_some() {
            b = b.wgpu();
        }
        let harness = b.build_eframe(move |_cc| app);
        let mut d = Driver { harness, tx, snapshots };
        d.frames(4);
        d
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            let ctx = self.harness.ctx.clone();
            let mut raw = std::mem::take(self.harness.input_mut());
            eframe::App::raw_input_hook(self.harness.state_mut(), &ctx, &mut raw);
            *self.harness.input_mut() = raw;
            self.harness.step();
        }
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        let (req, reply) = ControlRequest::new(method, params.clone());
        self.tx.send(req).unwrap();
        for _ in 0..600 {
            self.frames(1);
            if let Ok(v) = reply.try_recv() {
                assert_eq!(v["ok"], json!(true), "{method} {params} failed: {v}");
                return v["result"].clone();
            }
        }
        panic!("no reply to {method} {params}");
    }

    fn ids(&mut self, prefix: &str) -> Vec<String> {
        let v = self.ok("ui.elements", json!({"prefix": prefix}));
        v.as_array().unwrap().iter().filter_map(|e| e["id"].as_str().map(str::to_string)).collect()
    }

    /// The on-screen rectangle the automation registry reports for `id`.
    fn rect(&mut self, id: &str) -> egui::Rect {
        let v = self.ok("ui.elements", json!({"prefix": id}));
        let e = v.as_array().and_then(|a| a.iter().find(|e| e["id"] == json!(id))).unwrap_or_else(|| panic!("`{id}` is not on screen: {v}"));
        let r = e["rect"].as_array().expect("rect");
        egui::Rect::from_min_size(
            egui::pos2(r[0].as_f64().unwrap_or(0.0) as f32, r[1].as_f64().unwrap_or(0.0) as f32),
            egui::vec2(r[2].as_f64().unwrap_or(0.0) as f32, r[3].as_f64().unwrap_or(0.0) as f32),
        )
    }

    /// `CaptionTrack.style` of `track`, as `captions.list` reports it (the model the panel reads).
    fn style(&mut self, track: u64) -> Value {
        let r = self.ok("engine.execute", json!({"command": "captions.list", "params": {}}));
        let t = r["tracks"].as_array().and_then(|a| a.iter().find(|t| t["id"] == json!(track))).unwrap_or_else(|| panic!("no track {track}: {r}"));
        t["style"].clone()
    }

    /// The AccessKit node of `role` whose bounds are the on-screen rectangle of `id`: this is what
    /// the widget actually shows, independent of the panel's code.
    fn widget(&self, rect: egui::Rect, role: Role) -> egui_kittest::Node<'_> {
        self.harness
            .query_all_by_role(role)
            .find(|n| {
                let b = n.rect();
                (b.min.x - rect.min.x).abs() < 1.5
                    && (b.min.y - rect.min.y).abs() < 1.5
                    && (b.width() - rect.width()).abs() < 1.5
                    && (b.height() - rect.height()).abs() < 1.5
            })
            .unwrap_or_else(|| panic!("no {role:?} widget at {rect:?}"))
    }

    fn shown_text(&self, rect: egui::Rect, role: Role) -> String {
        self.widget(rect, role).value().unwrap_or_default()
    }

    fn shown_number(&self, rect: egui::Rect, role: Role) -> f64 {
        self.widget(rect, role).accesskit_node().numeric_value().unwrap_or_else(|| panic!("no numeric value at {rect:?}"))
    }

    fn shown_toggled(&self, rect: egui::Rect, role: Role) -> Toggled {
        self.widget(rect, role).accesskit_node().toggled().unwrap_or_else(|| panic!("no toggled state at {rect:?}"))
    }

    fn snapshot(&mut self, name: &str) {
        let Some(dir) = self.snapshots.clone() else { return };
        self.frames(2);
        match self.harness.render() {
            Ok(img) => {
                std::fs::create_dir_all(&dir).unwrap();
                img.save(dir.join(format!("{name}.png"))).unwrap();
                println!("wrote {}", dir.join(format!("{name}.png")).display());
            }
            Err(e) => eprintln!("snapshot {name} skipped: {e}"),
        }
    }
}

/// A float from the style JSON, so comparisons do not depend on integer/float typing.
fn number(v: &Value, key: &str) -> f64 {
    v[key].as_f64().unwrap_or_else(|| panic!("{key} in {v}"))
}

#[test]
fn style_strip_previews_and_dispatches_the_caption_style() {
    let mut d = Driver::demo();
    d.ok("ui.set", json!({"workspace": "Captions and Graphics"}));
    d.frames(3);
    d.ok("ui.click", json!({"id": "text.tab.Captions"}));
    d.frames(4);

    let r = d.ok("engine.execute", json!({"command": "captions.list", "params": {}}));
    let track = r["tracks"][0]["id"].as_u64().expect("the caption track");

    // ---- 1. the ids exist and are really on screen inside the panel
    let ids = d.ids("text.captions.style.");
    for want in STRIP_IDS {
        assert!(ids.iter().any(|id| id == want), "`{want}` is missing from the style strip: {ids:?}");
        let rect = d.rect(want);
        assert!(rect.width() > 0.0 && rect.height() > 0.0, "`{want}` has an empty rectangle: {rect:?}");
        assert!(rect.min.x >= 0.0 && rect.min.y >= 0.0 && rect.max.x <= W && rect.max.y <= H, "`{want}` is outside the visible window ({W}x{H}): {rect:?}");
    }
    println!("strip ids: {ids:?}");

    // ---- 2. the strip reports the model's current values
    let style = d.style(track);
    assert_eq!(style["font"], "Inter", "{style}");
    assert_eq!(number(&style, "size"), 54.0, "{style}");
    assert_eq!(style["color"], "#ffffffff", "{style}");
    assert_eq!(style["background"], true, "{style}");
    assert_eq!(style["align"], "center", "{style}");

    let font = d.rect("text.captions.style.font");
    assert_eq!(d.shown_text(font, Role::ComboBox), "Inter", "the font picker shows the track's font");

    let size = d.rect("text.captions.style.size");
    assert_eq!(d.shown_number(size, Role::SpinButton), 54.0, "the size field shows the track's size");
    assert!(d.shown_text(size, Role::SpinButton).contains("px"), "the size field shows its unit: {:?}", d.shown_text(size, Role::SpinButton));

    // no text representation for a colour swatch; its presence at the id's rectangle is the preview
    let colour = d.rect("text.captions.style.color");
    assert_eq!(d.widget(colour, Role::ColorWell).accesskit_node().role(), Role::ColorWell);

    let background = d.rect("text.captions.style.background");
    assert_eq!(d.shown_toggled(background, Role::CheckBox), Toggled::True, "the box checkbox is ticked");

    let left = d.rect("text.captions.style.align.left");
    let centre = d.rect("text.captions.style.align.center");
    let right = d.rect("text.captions.style.align.right");
    assert_eq!(d.shown_toggled(centre, Role::Button), Toggled::True, "centre is the selected alignment");
    assert_eq!(d.shown_toggled(left, Role::Button), Toggled::False, "left is not selected");
    assert_eq!(d.shown_toggled(right, Role::Button), Toggled::False, "right is not selected");
    d.snapshot("captions-style-panel");

    // ---- 3a. editing the size field changes the track and the strip follows
    // (a click enters the field's text-edit mode with the value selected, like a user typing)
    d.ok("ui.click", json!({"id": "text.captions.style.size"}));
    d.frames(3);
    d.ok("ui.type", json!({"text": "96"}));
    d.frames(3);
    d.ok("ui.key", json!({"key": "Enter"}));
    d.frames(3);
    let edited = d.style(track);
    let size_now = number(&edited, "size");
    println!("size after typing 96: {size_now}");
    assert_eq!(size_now, 96.0, "the size field writes the typed value to the style: {edited}");
    let size = d.rect("text.captions.style.size");
    let shown = d.shown_number(size, Role::SpinButton);
    assert!((shown - size_now).abs() < 0.01, "the size field shows the model's new size ({shown} vs {size_now})");
    assert!(d.shown_text(size, Role::SpinButton).contains("96"), "the size field shows 96: {:?}", d.shown_text(size, Role::SpinButton));

    // ---- 3b. clicking an alignment button changes the track and the strip follows
    d.ok("ui.click", json!({"id": "text.captions.style.align.right"}));
    d.frames(3);
    assert_eq!(d.style(track)["align"], "right", "align right reaches CaptionTrack.style");
    let right = d.rect("text.captions.style.align.right");
    assert_eq!(d.shown_toggled(right, Role::Button), Toggled::True);
    let centre = d.rect("text.captions.style.align.center");
    assert_eq!(d.shown_toggled(centre, Role::Button), Toggled::False);
    let left = d.rect("text.captions.style.align.left");
    assert_eq!(d.shown_toggled(left, Role::Button), Toggled::False);

    // ---- 3c. the box checkbox changes the track and the strip follows
    d.ok("ui.click", json!({"id": "text.captions.style.background"}));
    d.frames(3);
    assert_eq!(d.style(track)["background"], false, "the Box checkbox reaches CaptionTrack.style");
    let background = d.rect("text.captions.style.background");
    assert_eq!(d.shown_toggled(background, Role::CheckBox), Toggled::False);

    // ---- 4. the preview is rebuilt from the model, not a stale copy: a style set outside the
    // panel (engine command, as an agent would) shows up in the widgets on the next frame
    d.ok("engine.execute", json!({"command": "captions.setStyle", "params": {"track": track, "size": 72.0, "font": "Noto Serif", "align": "left"}}));
    d.frames(3);
    let outside = d.style(track);
    assert_eq!(number(&outside, "size"), 72.0, "{outside}");
    assert_eq!(outside["font"], "Noto Serif", "{outside}");
    assert_eq!(outside["align"], "left", "{outside}");
    let size = d.rect("text.captions.style.size");
    assert!((d.shown_number(size, Role::SpinButton) - 72.0).abs() < 0.01, "the size field shows 72");
    let font = d.rect("text.captions.style.font");
    assert_eq!(d.shown_text(font, Role::ComboBox), "Noto Serif", "the font picker shows the new font");
    let left = d.rect("text.captions.style.align.left");
    assert_eq!(d.shown_toggled(left, Role::Button), Toggled::True, "left is selected now");
    let right = d.rect("text.captions.style.align.right");
    assert_eq!(d.shown_toggled(right, Role::Button), Toggled::False);
    d.snapshot("captions-style-panel-after");
}
