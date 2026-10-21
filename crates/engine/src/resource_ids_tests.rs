//! The visible editor can carry Jianying's official resource numbering ("编号选型"): `effects.apply`
//! and `sequence.applyVideoTransition` take `resourceId` / `effectId`, `sequence.inspect` reports
//! them, and a save/reopen keeps them. Without the parameters neither key appears anywhere, and a
//! number, an empty or a blank string is refused before anything is applied.
//!
//! These tests pass an explicit `effect` beside the number (the number is then metadata, whatever it
//! is). A `resourceId` *alone* must be a hand-verified number from [`crate::resource_map`]; see
//! `resource_map_tests.rs` for that path and for what an unmapped number does.

use serde_json::{Value, json};

use crate::Session;

/// 19-digit ids, as Jianying writes them: digits only, leading zeros significant, so a string.
const RESOURCE_ID: &str = "7012345678901234567";
const EFFECT_ID: &str = "6791234567890123456";

fn demo() -> Session {
    let mut s = Session::default();
    s.execute("file.openDemoProject", json!({})).unwrap();
    s
}

/// The `sequence.inspect` entry of `clip`'s effect `effect` (the clip is on a video or audio track).
fn effect_entry<'a>(js: &'a Value, clip: u64, effect: &str) -> &'a Value {
    for key in ["video", "audio"] {
        for t in js[key].as_array().map(Vec::as_slice).unwrap_or_default() {
            for i in t["items"].as_array().map(Vec::as_slice).unwrap_or_default() {
                if i["clip"].as_u64() != Some(clip) {
                    continue;
                }
                if let Some(fx) = i["effects"].as_array().and_then(|a| a.iter().find(|e| e["effect"] == effect)) {
                    return fx;
                }
            }
        }
    }
    panic!("clip {clip} has no effect `{effect}` in sequence.inspect: {js}");
}

/// Every transition the inspector reports (the demo project ships four of its own, none numbered).
fn transitions(js: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    for key in ["video", "audio"] {
        for t in js[key].as_array().map(Vec::as_slice).unwrap_or_default() {
            out.extend(t["transitions"].as_array().map(Vec::as_slice).unwrap_or_default());
        }
    }
    out
}

/// The single transition carrying `resource_id`; finding it by the id keeps the test independent
/// of the transitions the demo project already ships.
fn numbered_transition<'a>(js: &'a Value, resource_id: &str) -> &'a Value {
    let found: Vec<&Value> = transitions(js).into_iter().filter(|x| x["resourceId"] == resource_id).collect();
    assert_eq!(found.len(), 1, "exactly one transition carries {resource_id}: {js}");
    found[0]
}

/// Every effect and transition the inspector reports, for "neither key appears" assertions.
fn all_inspect_entries(js: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    for key in ["video", "audio"] {
        for t in js[key].as_array().map(Vec::as_slice).unwrap_or_default() {
            for i in t["items"].as_array().map(Vec::as_slice).unwrap_or_default() {
                out.extend(i["effects"].as_array().cloned().unwrap_or_default());
            }
            out.extend(t["transitions"].as_array().cloned().unwrap_or_default());
        }
    }
    out
}

fn effect_count(s: &Session, clip: u64) -> usize {
    s.active_sequence().and_then(|q| q.find_item(filmcraft_project::ClipId(clip))).map(|(_, it)| it.effects.len()).unwrap_or(0)
}

#[test]
fn an_applied_effect_carries_the_jianying_resource_ids_into_inspect() {
    let mut s = demo();
    let clip = s.active_sequence().unwrap().video_tracks[0].items[1].id;
    s.execute("effects.apply", json!({"clips": [clip.0], "effect": "gaussian_blur", "resourceId": RESOURCE_ID, "effectId": EFFECT_ID})).unwrap();

    let js = s.execute("sequence.inspect", json!({})).unwrap();
    let fx = effect_entry(&js, clip.0, "gaussian_blur");
    assert_eq!(fx["resourceId"], RESOURCE_ID);
    assert_eq!(fx["effectId"], EFFECT_ID);
    // the effect is still the effect: the ids are metadata
    assert_eq!(fx["enabled"], true);
}

#[test]
fn an_applied_transition_carries_the_jianying_resource_ids_into_inspect() {
    let mut s = demo();
    let clip = s.active_sequence().unwrap().video_tracks[0].items[1].id;
    // the id is metadata here: with an explicit `effect` the number need not be in the verified map
    s.execute(
        "sequence.applyVideoTransition",
        json!({"clip": clip.0, "edge": "out", "frames": 6, "effect": "cross_dissolve", "resourceId": RESOURCE_ID, "effectId": EFFECT_ID}),
    )
    .unwrap();

    let js = s.execute("sequence.inspect", json!({})).unwrap();
    let x = numbered_transition(&js, RESOURCE_ID);
    assert_eq!(x["effect"], "cross_dissolve");
    assert_eq!(x["effectId"], EFFECT_ID);
}

#[test]
fn the_ids_survive_a_save_and_reopen() {
    let d = crate::media_test_util::tmp_dir("resource-ids");
    let path = d.join("ids.fcproj").to_string_lossy().to_string();
    let mut s = demo();
    let clip = s.active_sequence().unwrap().video_tracks[0].items[1].id;
    s.execute("effects.apply", json!({"clips": [clip.0], "effect": "gaussian_blur", "resourceId": RESOURCE_ID})).unwrap();
    s.execute(
        "sequence.applyVideoTransition",
        json!({"clip": clip.0, "edge": "out", "frames": 6, "effect": "cross_dissolve", "resourceId": RESOURCE_ID, "effectId": EFFECT_ID}),
    )
    .unwrap();
    s.execute("file.saveAs", json!({"path": path})).unwrap();

    let mut t = Session::default();
    t.execute("file.open", json!({"path": path})).unwrap();
    let js = t.execute("sequence.inspect", json!({})).unwrap();
    assert_eq!(effect_entry(&js, clip.0, "gaussian_blur")["resourceId"], RESOURCE_ID);
    let x = numbered_transition(&js, RESOURCE_ID);
    assert_eq!(x["effectId"], EFFECT_ID);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn without_the_parameters_neither_key_appears_anywhere() {
    let d = crate::media_test_util::tmp_dir("resource-ids-absent");
    let path = d.join("plain.fcproj").to_string_lossy().to_string();
    let mut s = demo();
    let clip = s.active_sequence().unwrap().video_tracks[0].items[1].id;
    s.execute("effects.apply", json!({"clips": [clip.0], "effect": "gaussian_blur"})).unwrap();
    s.execute("sequence.applyVideoTransition", json!({"clip": clip.0, "edge": "out", "frames": 6})).unwrap();

    let js = s.execute("sequence.inspect", json!({})).unwrap();
    let entries = all_inspect_entries(&js);
    assert!(entries.len() > 3, "the demo project has effects and a transition: {js}");
    for e in &entries {
        assert!(e.get("resourceId").is_none(), "no numbering = no `resourceId`: {e}");
        assert!(e.get("effectId").is_none(), "no numbering = no `effectId`: {e}");
    }

    // ... and nothing is written to the project file either
    s.execute("file.saveAs", json!({"path": path})).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("resource_id"), "no numbering = no `resource_id` key in the .fcproj");
    assert!(!text.contains("effect_id"), "no numbering = no `effect_id` key in the .fcproj");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn bad_resource_ids_are_refused_and_apply_nothing() {
    let mut s = demo();
    let clip = s.active_sequence().unwrap().video_tracks[0].items[1].id;
    let before = effect_count(&s, clip.0);
    let revision = s.revision;
    // a bare number, an empty string and a blank string are all readable errors
    for (key, value) in [("resourceId", json!(7_012_345_678_901_234_567u64)), ("resourceId", json!("")), ("resourceId", json!("   ")), ("effectId", json!(42))]
    {
        let mut params = json!({"clips": [clip.0], "effect": "gaussian_blur"});
        params[key] = value;
        let e = s.execute("effects.apply", params).unwrap_err().to_string();
        assert!(e.contains(key), "the error names `{key}`: {e}");
        assert_eq!(effect_count(&s, clip.0), before, "a refused apply leaves no effect behind");
        assert_eq!(s.revision, revision, "a refused apply is not an edit");
    }
    // a transition with a bad id creates no transition either
    let transitions = |s: &Session| s.active_sequence().map(|q| q.video_tracks[0].transitions.len()).unwrap_or(0);
    let n = transitions(&s);
    let e = s.execute("sequence.applyVideoTransition", json!({"clip": clip.0, "edge": "out", "frames": 6, "resourceId": 5})).unwrap_err().to_string();
    assert!(e.contains("resourceId"), "{e}");
    assert_eq!(transitions(&s), n);
}
