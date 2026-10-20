//! Automated evidence that `captions.setStyle` changes what is rasterised (openspec #483, test A).
//!
//! The export path composes captions through [`filmcraft_render::caption_overlays`] (which wraps
//! `filmcraft_captions::burn::sequence_overlays`, the function `crates/render/src/lib.rs` uses when
//! it renders a frame). Here a transcript is turned into captions with `transcript.createCaptions`,
//! the overlay is composited over an **opaque black** `1920x1080` canvas at one fixed caption time,
//! and then `captions.setStyle` changes **one property at a time**:
//!
//! | step | style change | what the pixels must do |
//! |---|---|---|
//! | 1 | `size: 54 → 96` | the lit bounding box and the overlay rect grow |
//! | 2 | `color: #ffffffff → #ff0000ff` | the dominant opaque colour becomes red |
//! | 3 | `background: false` | the translucent box pixels disappear |
//! | 4 | `outline: 0 → 8`, `outlineColor: #00ff00ff` | opaque green outline pixels appear |
//!
//! After every step the test also asserts that nothing outside the caption band changed: the overlay
//! rectangles reported by the rasteriser are the only place a caption may touch the frame, the band
//! stays in the lower half (bottom anchor), and every pixel outside it is exactly black.
//!
//! Nothing here needs a GPU, a clock or a host window: sizes, times and colours are fixed, so the
//! assertions hold on every platform. Set `FILMCRAFT_UI_SNAPSHOT_DIR=<dir>` to also write the
//! before/after frames there (`captions-style-*.png`, the directory the other `*_ui.rs` snapshot
//! tests use).

use std::path::Path;

use filmcraft_color::linear_to_srgb_u8;
use filmcraft_engine::Session;
use filmcraft_time::Tick;
use serde_json::{Value, json};

/// The frame every overlay is rasterised at, so `style.size` (px at 1080 lines) is not rescaled.
const W: usize = 1920;
const H: usize = 1080;

/// `(x0, y0, x1, y1)`, inclusive, in frame pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RectI {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

impl RectI {
    fn w(self) -> usize {
        self.x1.saturating_sub(self.x0) + 1
    }
    fn h(self) -> usize {
        self.y1.saturating_sub(self.y0) + 1
    }
    fn union(self, other: Self) -> Self {
        Self { x0: self.x0.min(other.x0), y0: self.y0.min(other.y0), x1: self.x1.max(other.x1), y1: self.y1.max(other.y1) }
    }
    fn contains(self, x: usize, y: usize) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
}

/// The demo project (the fixture `crates/engine/src/transcript_tests.rs` uses) with one line of
/// transcript on its first A1 clip, turned into a caption track by `transcript.createCaptions`.
/// `transcript.set` addresses a media item, so the fixture needs an A1 clip: a brand-new empty
/// project has none, and the demo project is the deterministic one every other test seeds from.
fn project_with_captions() -> (Session, u64) {
    let mut s = Session::default();
    s.execute("file.openDemoProject", json!({})).expect("demo project");
    let (item, source_in) = {
        let a = &s.active_sequence().expect("a sequence").audio_tracks[0].items[0];
        (a.item, a.source_in)
    };
    let at = |seconds: f64| source_in + Tick::from_seconds_f64(seconds);
    let words = json!([
        {"text": "Style", "start": at(0.2).0, "end": at(0.8).0},
        {"text": "test", "start": at(0.9).0, "end": at(1.5).0},
    ]);
    let r = s.execute("transcript.set", json!({"item": item.0, "transcript": {"language": "en", "words": words}})).expect("transcript.set");
    assert_eq!(r["words"], 2, "{r}");
    let r = s.execute("transcript.createCaptions", json!({"maxChars": 40, "lines": 1})).expect("transcript.createCaptions");
    (s, r["track"].as_u64().expect("a caption track"))
}

/// The track's style as `captions.list` reports it.
fn style_of(s: &mut Session, track: u64) -> Value {
    let r = s.execute("captions.list", json!({})).expect("captions.list");
    let t = r["tracks"].as_array().expect("tracks").iter().find(|t| t["id"] == json!(track)).expect("the caption track");
    t["style"].clone()
}

/// Start and end ticks of the track's first caption.
fn caption_bounds(s: &mut Session, track: u64) -> (i64, i64) {
    let r = s.execute("captions.list", json!({})).expect("captions.list");
    let t = r["tracks"].as_array().expect("tracks").iter().find(|t| t["id"] == json!(track)).expect("the caption track");
    let c = &t["captions"][0];
    (c["start"].as_i64().expect("start"), c["end"].as_i64().expect("end"))
}

/// The caption overlay for the active sequence at `t`, composited over an opaque black frame.
fn overlay_canvas(s: &Session, t: Tick) -> Vec<f32> {
    let seq = s.active_sequence().expect("a sequence");
    let mut canvas = vec![0.0f32; W * H * 4];
    let overlays = filmcraft_render::caption_overlays(seq, t, W, H);
    assert!(!overlays.is_empty(), "the caption is on screen at {t:?}");
    for o in overlays {
        o.composite_onto(&mut canvas, W, H);
    }
    canvas
}

/// The band a caption may paint in: the union of the overlay rectangles the rasteriser reports.
fn caption_band(s: &Session, t: Tick) -> RectI {
    let seq = s.active_sequence().expect("a sequence");
    let overlays = filmcraft_render::caption_overlays(seq, t, W, H);
    assert!(!overlays.is_empty(), "the caption is on screen at {t:?}");
    let mut band: Option<RectI> = None;
    for o in overlays {
        let x0 = o.x.max(0) as usize;
        let y0 = o.y.max(0) as usize;
        let x1 = ((o.x + o.w as i32) as usize).min(W) - 1;
        let y1 = ((o.y + o.h as i32) as usize).min(H) - 1;
        let r = RectI { x0, y0, x1, y1 };
        band = Some(band.map_or(r, |b| b.union(r)));
    }
    band.expect("an overlay rect")
}

fn pixel(canvas: &[f32], x: usize, y: usize) -> &[f32] {
    &canvas[(y * W + x) * 4..(y * W + x) * 4 + 4]
}

/// Pixels the caption covers at all (any alpha).
fn lit(canvas: &[f32]) -> usize {
    canvas.as_chunks::<4>().0.iter().filter(|p| p[3] > 0.01).count()
}

/// Bounding box of the lit pixels.
fn bbox(canvas: &[f32]) -> Option<RectI> {
    let mut out: Option<RectI> = None;
    for (i, p) in canvas.as_chunks::<4>().0.iter().enumerate() {
        if p[3] <= 0.01 {
            continue;
        }
        let (x, y) = (i % W, i / W);
        out = Some(match out {
            None => RectI { x0: x, y0: y, x1: x, y1: y },
            Some(b) => RectI { x0: b.x0.min(x), y0: b.y0.min(y), x1: b.x1.max(x), y1: b.y1.max(y) },
        });
    }
    out
}

/// Pixels from the translucent background box: partly transparent and black on the black canvas.
fn box_pixels(canvas: &[f32]) -> usize {
    canvas.as_chunks::<4>().0.iter().filter(|p| p[3] > 0.5 && p[3] < 0.95 && p[0] < 0.02 && p[1] < 0.02 && p[2] < 0.02).count()
}

/// Opaque green pixels (the outline colour `#00ff00` in linear light).
fn outline_pixels(canvas: &[f32]) -> usize {
    canvas.as_chunks::<4>().0.iter().filter(|p| p[3] > 0.9 && p[1] > 0.5 && p[0] < 0.01 && p[2] < 0.01).count()
}

fn quantise(c: [u8; 3]) -> [u8; 3] {
    [c[0] & 0xf8, c[1] & 0xf8, c[2] & 0xf8]
}

const WHITE: [u8; 3] = [0xf8, 0xf8, 0xf8];
const RED: [u8; 3] = [0xf8, 0x00, 0x00];
const GREEN: [u8; 3] = [0x00, 0xf8, 0x00];

/// sRGB colour of a premultiplied linear-light pixel, quantised to 5 bits per channel.
fn srgb_of(p: &[f32]) -> [u8; 3] {
    let a = p[3].max(1e-6);
    [linear_to_srgb_u8((p[0] / a).clamp(0.0, 1.0)), linear_to_srgb_u8((p[1] / a).clamp(0.0, 1.0)), linear_to_srgb_u8((p[2] / a).clamp(0.0, 1.0))]
}

/// The most common opaque colour: fully covered text and outline pixels only, so antialiased edges
/// and the translucent box do not win.
fn dominant(canvas: &[f32]) -> Option<[u8; 3]> {
    let mut counts: std::collections::HashMap<[u8; 3], usize> = std::collections::HashMap::new();
    for p in canvas.as_chunks::<4>().0 {
        if p[3] < 0.995 {
            continue;
        }
        *counts.entry(quantise(srgb_of(p))).or_default() += 1;
    }
    // ties are broken by the colour value so the result does not depend on hash order
    counts.into_iter().max_by_key(|(c, n)| (*n, *c)).map(|(c, _)| c)
}

/// How many opaque pixels have exactly `want` (5-bit quantised).
fn solid(canvas: &[f32], want: [u8; 3]) -> usize {
    canvas.as_chunks::<4>().0.iter().filter(|p| p[3] >= 0.995 && quantise(srgb_of(p.as_slice())) == want).count()
}

/// Every pixel outside `band` must be exactly the black canvas: a caption may not paint elsewhere.
fn assert_black_outside(canvas: &[f32], band: RectI, what: &str) {
    for y in 0..H {
        for x in 0..W {
            if band.contains(x, y) {
                continue;
            }
            let p = pixel(canvas, x, y);
            assert!(p.iter().all(|v| *v == 0.0), "{what}: pixel ({x}, {y}) outside the caption band {band:?} changed: {p:?}");
        }
    }
}

/// Every pixel that changed between two frames must be inside the union of the two caption bands.
fn assert_diff_inside(before: &[f32], after: &[f32], band: RectI, what: &str) {
    for y in 0..H {
        for x in 0..W {
            if band.contains(x, y) {
                continue;
            }
            let (a, b) = (pixel(before, x, y), pixel(after, x, y));
            assert!(a.iter().zip(b).all(|(u, v)| (u - v).abs() < 1e-6), "{what}: pixel ({x}, {y}) outside the caption band {band:?} changed: {a:?} → {b:?}");
        }
    }
}

/// Write the frame as an opaque sRGB PNG (`linear_to_srgb` of the un-premultiplied colour).
fn write_png(canvas: &[f32], path: &Path) {
    let mut bytes = vec![0u8; W * H * 4];
    for (i, p) in canvas.as_chunks::<4>().0.iter().enumerate() {
        let rgb = srgb_of(p);
        bytes[i * 4..i * 4 + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
    let Some(img) = image::RgbaImage::from_raw(W as u32, H as u32, bytes) else {
        panic!("frame buffer is {W}x{H}");
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).expect("snapshot directory");
    }
    img.save(path).expect("write PNG");
}

fn snapshot(canvas: &[f32], name: &str) {
    let Some(dir) = filmcraft_ui_egui::test_support::snapshot_dir() else { return };
    let path = dir.join(format!("captions-style-{name}.png"));
    write_png(canvas, &path);
    println!("wrote {}", path.display());
}

#[test]
fn set_style_repaints_the_caption_band() {
    let (mut s, track) = project_with_captions();
    let (start, end) = caption_bounds(&mut s, track);
    let t = Tick(start + (end - start) / 2);

    // ---- the default style rasterises: white text in a translucent box, bottom centre
    let default = style_of(&mut s, track);
    assert_eq!(default["font"], "Inter", "{default}");
    assert_eq!(default["size"], 54.0, "{default}");
    assert_eq!(default["color"], "#ffffffff", "{default}");
    assert_eq!(default["background"], true, "{default}");
    assert_eq!(default["align"], "center", "{default}");
    assert_eq!(default["outline"], 0.0, "{default}");

    let cam0 = overlay_canvas(&s, t);
    let band0 = caption_band(&s, t);
    let lit0 = lit(&cam0);
    let bbox0 = bbox(&cam0).expect("lit pixels");
    println!("default: lit={lit0} bbox={bbox0:?} band={band0:?} box={} dominant={:?}", box_pixels(&cam0), dominant(&cam0));
    assert!(lit0 > 500, "the caption rasterises ({lit0} lit pixels)");
    assert!(box_pixels(&cam0) > 1000, "the default background box draws ({} px)", box_pixels(&cam0));
    assert_eq!(outline_pixels(&cam0), 0, "no outline by default");
    assert_eq!(dominant(&cam0), Some(WHITE), "default text is white");
    assert!(band0.y0 > H / 2, "bottom-anchored captions stay in the lower half: {band0:?}");
    assert_black_outside(&cam0, band0, "default");
    snapshot(&cam0, "0-default");

    // ---- 1. size: the bound box grows
    s.execute("captions.setStyle", json!({"track": track, "size": 96.0})).expect("captions.setStyle size");
    assert_eq!(style_of(&mut s, track)["size"], 96.0, "the command reached the track style");
    let cam1 = overlay_canvas(&s, t);
    let band1 = caption_band(&s, t);
    let bbox1 = bbox(&cam1).expect("lit pixels");
    println!("size 96: lit={} bbox={bbox1:?} band={band1:?}", lit(&cam1));
    assert!(bbox1.w() > bbox0.w() && bbox1.h() > bbox0.h(), "the caption grows with the size: {bbox0:?} → {bbox1:?}");
    assert!(band1.w() > band0.w() && band1.h() > band0.h(), "the overlay rectangle grows too: {band0:?} → {band1:?}");
    assert!(lit(&cam1) > lit0, "more pixels are covered: {lit0} → {}", lit(&cam1));
    assert_eq!(dominant(&cam1), Some(WHITE), "the text colour is unchanged");
    assert_black_outside(&cam1, band1, "size");
    assert_diff_inside(&cam0, &cam1, band0.union(band1), "size");
    snapshot(&cam1, "1-size");

    // ---- 2. colour: the dominant opaque colour is the new colour
    s.execute("captions.setStyle", json!({"track": track, "color": "#ff0000"})).expect("captions.setStyle color");
    assert_eq!(style_of(&mut s, track)["color"], "#ff0000ff", "the command reached the track style");
    let cam2 = overlay_canvas(&s, t);
    let band2 = caption_band(&s, t);
    println!("colour red: lit={} dominant={:?} red={} box={}", lit(&cam2), dominant(&cam2), solid(&cam2, RED), box_pixels(&cam2));
    assert_eq!(dominant(&cam2), Some(RED), "the caption is painted in the new colour");
    assert!(solid(&cam2, RED) > 500, "solid red text pixels: {}", solid(&cam2, RED));
    assert_eq!(solid(&cam2, WHITE), 0, "no white text pixels are left");
    assert_eq!(outline_pixels(&cam2), 0, "still no outline");
    assert!(box_pixels(&cam2) > 1000, "the box is still drawn");
    assert_eq!(bbox(&cam2), Some(bbox1), "a colour change does not move the glyphs");
    assert_black_outside(&cam2, band2, "colour");
    assert_diff_inside(&cam1, &cam2, band1.union(band2), "colour");
    snapshot(&cam2, "2-colour");

    // ---- 3. background off: the box pixels disappear
    s.execute("captions.setStyle", json!({"track": track, "background": false})).expect("captions.setStyle background");
    assert_eq!(style_of(&mut s, track)["background"], false, "the command reached the track style");
    let cam3 = overlay_canvas(&s, t);
    let band3 = caption_band(&s, t);
    let bbox3 = bbox(&cam3).expect("lit pixels");
    println!("box off: lit={} bbox={bbox3:?} box={} red={}", lit(&cam3), box_pixels(&cam3), solid(&cam3, RED));
    assert_eq!(box_pixels(&cam3), 0, "the translucent box pixels are gone");
    assert!(lit(&cam3) < lit(&cam2), "the box area is no longer covered: {} → {}", lit(&cam2), lit(&cam3));
    assert!(bbox3.w() < bbox1.w(), "the lit bounds shrink to the text: {bbox1:?} → {bbox3:?}");
    assert_eq!(dominant(&cam3), Some(RED), "the red text is unchanged");
    assert_black_outside(&cam3, band3, "background off");
    assert_diff_inside(&cam2, &cam3, band2.union(band3), "background off");
    snapshot(&cam3, "3-box-off");

    // ---- 4. outline: opaque outline pixels appear around the glyphs
    s.execute("captions.setStyle", json!({"track": track, "outline": 8.0, "outlineColor": "#00ff00"})).expect("captions.setStyle outline");
    let styled = style_of(&mut s, track);
    assert_eq!(styled["outline"], 8.0, "the command reached the track style: {styled}");
    assert_eq!(styled["outlineColor"], "#00ff00ff", "{styled}");
    let cam4 = overlay_canvas(&s, t);
    let band4 = caption_band(&s, t);
    let bbox4 = bbox(&cam4).expect("lit pixels");
    println!("outline 8 green: lit={} bbox={bbox4:?} green={} red={} dominant={:?}", lit(&cam4), outline_pixels(&cam4), solid(&cam4, RED), dominant(&cam4));
    assert_eq!(outline_pixels(&cam3), 0, "no outline pixels before the change");
    assert!(outline_pixels(&cam4) > 200, "green outline pixels appear: {}", outline_pixels(&cam4));
    assert!(solid(&cam4, GREEN) > 200, "the outline uses the requested colour: {}", solid(&cam4, GREEN));
    // the thick outline is the largest opaque area now — as it should be — so assert the red text
    // is still painted rather than still dominant
    assert!(solid(&cam4, RED) > 500, "the text itself stays red: {}", solid(&cam4, RED));
    assert!(bbox4.w() > bbox3.w() || bbox4.h() > bbox3.h(), "the outline grows the glyph bounds: {bbox3:?} → {bbox4:?}");
    assert_black_outside(&cam4, band4, "outline");
    assert_diff_inside(&cam3, &cam4, band3.union(band4), "outline");
    snapshot(&cam4, "4-outline");

    // the whole style change is a real repaint, not a no-op
    assert_ne!(cam0, cam4, "the styled caption is not the default one");
}
