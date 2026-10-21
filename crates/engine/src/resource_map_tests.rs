//! 「按剪映编号选型」的映射子集（[`crate::resource_map`]）：只有表里逐条人工核对过的编号
//! 能解析成引擎转场；未映射的编号必须报错（错误里带编号与「已映射」），绝不回退到默认转场。
//!
//! 这些测试是映射表的守门人：表里加一行，就要在这里证明它能被 `find_transition` 查到，
//! 并且只给编号时命令真的能执行。

use serde_json::{Value, json};

use crate::Session;
use crate::resource_map::{RESOURCE_TRANSITIONS, engine_transition_for, jianying_name, mapped_count};

/// 叠化：表里的第一条，用来验证「只给编号」的完整执行路径。
const CROSS_DISSOLVE_ID: &str = "6724845717472416269";
/// 格式合法但表中没有的 19 位编号：语义未核对，必须被拒绝。
const UNMAPPED_ID: &str = "7000000000000000001";

fn demo() -> Session {
    let mut s = Session::default();
    s.execute("file.openDemoProject", json!({})).unwrap();
    s
}

/// `sequence.inspect` 里所有转场（视频 + 音频轨）。
fn transitions(js: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    for key in ["video", "audio"] {
        for t in js[key].as_array().map(Vec::as_slice).unwrap_or_default() {
            out.extend(t["transitions"].as_array().map(Vec::as_slice).unwrap_or_default());
        }
    }
    out
}

/// 唯一携带 `resource_id` 的转场；按编号找，测试就不依赖演示工程自带的转场。
fn numbered_transition<'a>(js: &'a Value, resource_id: &str) -> &'a Value {
    let found: Vec<&Value> = transitions(js).into_iter().filter(|x| x["resourceId"] == resource_id).collect();
    assert_eq!(found.len(), 1, "只有一个转场携带 {resource_id}: {js}");
    found[0]
}

fn transition_count(s: &Session) -> usize {
    s.active_sequence().map(|q| q.all_tracks().map(|t| t.transitions.len()).sum()).unwrap_or(0)
}

/// 只给编号，`sequence.applyVideoTransition` 也能应用叠化，并在 inspect 里回显编号。
#[test]
fn a_video_transition_is_applied_from_a_resource_id_alone() {
    let mut s = demo();
    let clip = s.active_sequence().unwrap().video_tracks[0].items[1].id;
    s.execute("sequence.applyVideoTransition", json!({"clip": clip.0, "edge": "out", "frames": 6, "resourceId": CROSS_DISSOLVE_ID})).unwrap();

    let js = s.execute("sequence.inspect", json!({})).unwrap();
    let x = numbered_transition(&js, CROSS_DISSOLVE_ID);
    assert_eq!(x["effect"], "cross_dissolve");
}

/// 只给编号，`effects.apply` 也解析成引擎效果（叠化是转场，走转场应用路径）。
#[test]
fn effects_apply_resolves_a_resource_id_to_the_engine_effect() {
    let mut s = demo();
    let clip = s.active_sequence().unwrap().video_tracks[0].items[1].id;
    // `clips` is effects.apply's documented target; `clip`/`edge` are what the delegated
    // transition application reads (an effect that is a transition is applied at a cut).
    let params = json!({"clips": [clip.0], "clip": clip.0, "edge": "out", "frames": 6, "resourceId": CROSS_DISSOLVE_ID});
    s.execute("effects.apply", params).unwrap();

    let js = s.execute("sequence.inspect", json!({})).unwrap();
    let x = numbered_transition(&js, CROSS_DISSOLVE_ID);
    assert_eq!(x["effect"], "cross_dissolve");
}

/// 未映射的编号：视频转场报错，错误里有编号和「已映射」，且什么都没应用。
#[test]
fn an_unmapped_resource_id_is_refused_by_a_video_transition() {
    let mut s = demo();
    let clip = s.active_sequence().unwrap().video_tracks[0].items[1].id;
    let before = transition_count(&s);
    let revision = s.revision;

    let e = s.execute("sequence.applyVideoTransition", json!({"clip": clip.0, "edge": "out", "frames": 6, "resourceId": UNMAPPED_ID})).unwrap_err().to_string();
    assert!(e.contains(UNMAPPED_ID), "错误里带编号: {e}");
    assert!(e.contains("已映射"), "错误里带已映射条数: {e}");
    assert_eq!(transition_count(&s), before, "被拒绝的调用不留下转场");
    assert_eq!(s.revision, revision, "被拒绝的调用不是一次编辑");
}

/// 未映射的编号：`effects.apply` 同样报错，错误里有编号和「已映射」。
#[test]
fn an_unmapped_resource_id_is_refused_by_effects_apply() {
    let mut s = demo();
    let clip = s.active_sequence().unwrap().video_tracks[0].items[1].id;
    let e = s.execute("effects.apply", json!({"clips": [clip.0], "resourceId": UNMAPPED_ID})).unwrap_err().to_string();
    assert!(e.contains(UNMAPPED_ID), "错误里带编号: {e}");
    assert!(e.contains("已映射"), "错误里带已映射条数: {e}");
}

/// 音频过渡不在本次映射内：只给编号时明确说「音频过渡暂无映射」，不套用默认音频转场。
#[test]
fn audio_transitions_are_not_mapped_yet() {
    let mut s = demo();
    let clip = s.active_sequence().unwrap().audio_tracks[0].items[1].id;
    let before = transition_count(&s);
    let e = s
        .execute("sequence.applyAudioTransition", json!({"clip": clip.0, "edge": "out", "frames": 6, "resourceId": CROSS_DISSOLVE_ID}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("音频过渡暂无映射"), "{e}");
    assert_eq!(transition_count(&s), before);
}

/// 表里每一条都必须能在 `filmcraft_project::vtransition::find_transition` 里查到，
/// 并且编号、剪映名称、引擎 id 三者一一对应。
#[test]
fn every_mapped_row_names_a_real_video_transition() {
    assert!(!RESOURCE_TRANSITIONS.is_empty(), "映射表不能为空");
    assert_eq!(mapped_count(), RESOURCE_TRANSITIONS.len());
    for (resource_id, name, engine) in RESOURCE_TRANSITIONS {
        assert!(resource_id.chars().all(|c| c.is_ascii_digit()), "编号必须是数字: {resource_id}");
        assert!(!name.is_empty(), "剪映名称不能为空: {resource_id}");
        let def = filmcraft_project::vtransition::find_transition(engine, filmcraft_project::EffectKind::VideoTransition)
            .unwrap_or_else(|| panic!("表里的引擎 id `{engine}` 必须能在 find_transition 里查到"));
        assert_eq!(def.id, *engine, "表里的引擎 id 必须就是找到的转场");
        assert_eq!(engine_transition_for(resource_id), Some(*engine), "编号 `{resource_id}` 必须解析成 `{engine}`");
        assert_eq!(jianying_name(resource_id), Some(*name), "编号 `{resource_id}` 的剪映名称必须回显");
    }
    // 一个编号只能有一行：重复会让「以哪一行为准」变得不可预测。
    let mut ids: Vec<&str> = RESOURCE_TRANSITIONS.iter().map(|(resource_id, _, _)| *resource_id).collect();
    ids.sort_unstable();
    let total = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), total, "映射表里不能有重复编号");
}
