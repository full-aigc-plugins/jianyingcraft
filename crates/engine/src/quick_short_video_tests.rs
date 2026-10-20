//! `project.quickShortVideo` (#84): one command makes a short-video project — a portrait sequence,
//! an optional narration on A1 and an optional empty caption track — and creates nothing at all
//! when a parameter is refused.

use filmcraft_project::{ClipId, ItemId, ItemKind, MediaRef};
use filmcraft_time::FrameRate;
use serde_json::json;

use crate::{EngineError, Session};

const CMD: &str = "project.quickShortVideo";

/// Where a narration's WAV went, so the test can remove what it wrote.
fn narration_file(s: &Session, item: ItemId) -> Option<String> {
    match &s.project.item(item)?.kind {
        ItemKind::Media(m) => match &m.media {
            MediaRef::File { path } => Some(path.clone()),
            _ => None,
        },
        _ => None,
    }
}

#[test]
fn defaults_make_a_portrait_sequence_and_invent_nothing() {
    let mut s = Session::default();
    let r = s.execute(CMD, json!({})).unwrap();
    assert_eq!(r["project"]["name"], "Short Video", "the new project is named for what it is");
    assert!(r["project"]["created"].as_bool().expect("`created` is a boolean"), "the untouched project was replaced");
    let id = ItemId(r["sequence"].as_u64().expect("a sequence id"));
    assert_eq!(s.state.active_sequence, Some(id), "the new sequence is the open one");
    {
        let q = s.project.sequence(id).expect("the id is a sequence");
        assert_eq!((q.settings.width, q.settings.height), (1080, 1920), "portrait, not the 1920 x 1080 default");
        assert_eq!(q.settings.frame_rate, FrameRate::FPS_30);
        assert!(q.video_tracks.iter().all(|t| t.items.is_empty()) && q.audio_tracks.iter().all(|t| t.items.is_empty()), "no clip was placed");
        assert!(q.caption_tracks.is_empty(), "no caption track unless `captions` asks for one");
    }
    assert!(r.get("narration").is_none(), "no narration without a script");
    assert!(r.get("captionTrack").is_none());
    assert_eq!(s.project.sequences().count(), 1);
}

#[test]
fn captions_make_one_empty_caption_track_and_report_it() {
    let mut s = Session::default();
    let r = s.execute(CMD, json!({"captions": true})).unwrap();
    let reported = r["captionTrack"].as_u64().expect("the caption track id is reported");
    let id = ItemId(r["sequence"].as_u64().unwrap());
    {
        let q = s.project.sequence(id).expect("a sequence");
        assert_eq!(q.caption_tracks.len(), 1);
        assert_eq!(q.caption_tracks[0].id.0, reported, "the reported id is the track's");
        assert!(q.caption_tracks[0].captions.is_empty(), "an empty track: no caption was invented");
    }
    let list = s.execute("captions.list", json!({})).unwrap();
    assert_eq!(list["tracks"][0]["id"].as_u64().unwrap(), reported);
    assert!(list["tracks"][0]["captions"].as_array().unwrap().is_empty(), "the readback agrees it is empty");
}

#[test]
fn narration_lands_on_a1_for_exactly_as_long_as_the_synthesis() {
    let mut s = Session::default();
    let text = "Welcome to the short video.";
    let r = s.execute(CMD, json!({"narration": {"text": text}})).unwrap();
    let n = &r["narration"];
    let item = ItemId(n["item"].as_u64().expect("the narration item"));
    let clip = ClipId(n["clip"].as_u64().expect("the narration clip"));
    let duration = n["duration"].as_i64().expect("the narration duration");
    assert!(duration > 0, "the synthesis produced speech");
    let id = ItemId(r["sequence"].as_u64().unwrap());
    {
        let q = s.project.sequence(id).expect("a sequence");
        assert_eq!(q.audio_tracks[0].items.len(), 1, "one audio clip, on A1");
        let placed = &q.audio_tracks[0].items[0];
        assert_eq!((placed.id, placed.item), (clip, item));
        assert_eq!(placed.duration.0, duration, "the clip is exactly as long as the speech");
        assert_eq!(crate::mixer::strip_label(q, q.audio_tracks[0].id), "A1");
    }
    // `tts.inspect` reads the same clip, script and speech length back
    let i = s.execute("tts.inspect", json!({"clip": clip.0})).unwrap();
    assert_eq!(i["item"].as_u64().unwrap(), item.0);
    assert_eq!(i["clip"].as_u64().unwrap(), clip.0);
    assert_eq!(i["narration"]["text"], text);
    assert_eq!(i["narration"]["speechDuration"].as_i64().unwrap(), duration);
    assert!(!i["narration"]["voice"].as_str().unwrap_or_default().is_empty(), "the session's default voice was used");
    // and so does `sequence.inspect`
    let js = s.execute("sequence.inspect", json!({})).unwrap();
    let shown = &js["audio"][0]["items"][0];
    assert_eq!(shown["clip"].as_u64().unwrap(), clip.0);
    assert_eq!(shown["item"].as_u64().unwrap(), item.0);
    assert_eq!(shown["duration"].as_i64().unwrap(), duration);
    if let Some(path) = narration_file(&s, item) {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn refused_parameters_create_nothing_at_all() {
    for params in [
        json!({"width": 0}),
        json!({"height": 0}),
        json!({"width": -1}),
        json!({"width": 1.5}),
        json!({"width": 40000}),
        json!({"height": 40000}),
        json!({"fps": 0}),
        json!({"fps": -30}),
        json!({"fps": 5000}),
        json!({"fps": "30"}),
        json!({"name": ""}),
        json!({"name": 5}),
        json!({"captions": "yes"}),
        json!({"narration": "Hello"}),
        json!({"narration": {"text": ""}}),
        json!({"narration": {"text": "   "}}),
        json!({"narration": {"voice": "basic-female"}}),
        json!({"narration": {"text": "Hi.", "voice": 7}}),
        json!({"narration": {"text": "Hi.", "talk": true}}),
    ] {
        let mut s = Session::default();
        let e = s.execute(CMD, params.clone()).unwrap_err();
        assert!(matches!(&e, EngineError::BadParams { cmd, .. } if cmd == CMD), "{params}: {e:?}");
        assert!(s.project.items.is_empty(), "{params} created a project item");
        assert!(s.project.sequences().next().is_none(), "{params} created a sequence");
        assert!(s.active_sequence().is_none(), "{params} opened a sequence");
        assert_eq!(s.project.name, "Untitled", "{params} replaced the untouched project");
        assert!(!s.is_dirty(), "{params} left the project modified");
    }
}

#[test]
fn a_failing_narration_rolls_the_sequence_back() {
    let mut s = Session::default();
    let e = s.execute(CMD, json!({"narration": {"text": "Hello.", "voice": "no-such-voice"}})).unwrap_err();
    assert!(e.to_string().contains("unknown voice"), "the narration's own failure is passed on: {e}");
    assert!(s.project.sequences().next().is_none(), "the sequence was rolled back");
    assert!(s.project.items.is_empty(), "no WAV item was left behind");
    assert!(s.active_sequence().is_none(), "nothing was left open");
    assert!(!s.is_dirty(), "an empty project has nothing unsaved");
    // the session is still usable: the next call works
    s.execute(CMD, json!({})).unwrap();
    assert_eq!(s.project.sequences().count(), 1);
}

#[test]
fn a_second_call_adds_a_second_sequence_to_the_same_project() {
    let mut s = Session::default();
    let first = s.execute(CMD, json!({"name": "Reel"})).unwrap();
    let second = s.execute(CMD, json!({"name": "Reel"})).unwrap();
    assert!(first["project"]["created"].as_bool().expect("`created` is a boolean"), "the first call made the project");
    assert!(!second["project"]["created"].as_bool().expect("`created` is a boolean"), "the open project is kept, not replaced");
    assert_eq!(s.project.name, "Reel", "the second call does not rename the project");
    assert_ne!(first["sequence"], second["sequence"], "a second sequence, as `file.newSequence` makes one");
    assert_eq!(s.project.sequences().count(), 2);
    for r in [&first, &second] {
        let id = ItemId(r["sequence"].as_u64().unwrap());
        let q = s.project.sequence(id).expect("both are sequences of this project");
        assert_eq!((q.settings.width, q.settings.height, q.settings.frame_rate), (1080, 1920, FrameRate::FPS_30));
        assert_eq!(s.project.item(id).map(|i| i.name.as_str()), Some("Reel"), "the name names the sequence too");
    }
}

#[test]
fn overrides_are_used_and_the_spec_is_in_the_file_new_menu() {
    let spec = crate::find_command(CMD).expect("the command is registered");
    assert_eq!(spec.menu, ["File", "New"], "a New item, in the File menu (no new top-level menu)");
    assert!(spec.journal, "an undoable command");

    let mut s = Session::default();
    s.execute(CMD, json!({"name": "Vertical 720", "width": 720, "height": 1280, "fps": 25})).unwrap();
    let q = s.active_sequence().expect("a sequence");
    assert_eq!((q.settings.width, q.settings.height), (720, 1280));
    assert_eq!(q.settings.frame_rate, FrameRate::FPS_25);
    assert_eq!(s.project.name, "Vertical 720");
}
