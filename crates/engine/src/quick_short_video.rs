//! `project.quickShortVideo` (#84): make a short-video project in one command.
//!
//! | Step | Done by | Result key |
//! |---|---|---|
//! | 1. a project, when the session holds none of its own yet | the body of `file.newProject` | `project` |
//! | 2. a portrait sequence (1080 x 1920 at 30 fps by default) | `file.newSequence` | `sequence` |
//! | 3. an optional narration on A1 (nothing when there is no script) | `tts.create` | `narration` |
//! | 4. an optional *empty* caption track (nothing is invented) | `captions.newTrack` | `captionTrack` |
//!
//! **What "no project is open" means.** A [`Session`] always holds a project: the untouched
//! "Untitled" one it starts with, or one that was opened, saved, named or edited. Only the untouched
//! one is replaced (with the rules of `file.newProject`, which is not undoable either). Any other
//! project is kept as it is and gets the sequence added to it — as `file.newSequence` does every
//! time — so a second call gives the project a second sequence and never throws work away.
//!
//! **Atomicity.** Validation runs before anything is created: parameters, the sequence settings
//! (the same bounds `file.newSequence` checks) and the narration's script, so a refused parameter
//! leaves the session exactly as it was. The steps that do change the project then run in one
//! [`Session::edit_group`]: if any of them fails (an unknown voice, a failed write, no free A1),
//! the group puts the project back as it was — no sequence, no clip, no caption track — and the
//! error is the failing command's own (`file.newSequence`, `tts.create`, `captions.newTrack`),
//! which names the step that failed. The sequence cannot be made last: the narration and the
//! caption track are made *on* it.

use filmcraft_project::narration::MAX_TEXT_BYTES;
use filmcraft_project::{Project, SequenceSettings};
use filmcraft_time::FrameRate;
use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad, checked_u32_p, u64_p};
use crate::{EngineError, Result, Session};

const CMD: &str = "project.quickShortVideo";
/// The name of the project a session starts with (`Session::new`, `file.newProject`).
const UNTITLED: &str = "Untitled";
/// The project a quick short video makes when `name` names nothing.
const PROJECT_NAME: &str = "Short Video";

pub fn commands() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: CMD,
        label: "Short Video Project",
        menu: &["File", "New"],
        shortcut: None,
        params: r#"{"name":str?,"width":u32=1080,"height":u32=1920,"fps":f64=30,"narration":{"text":str,"voice":str?}?,"captions":bool=false}"#,
        enabled: always,
        run,
        journal: true,
    }]
}

/// `name`: absent (the defaults apply), or a string that says something.
fn name_p(p: &Value) -> Result<Option<String>> {
    match p.get("name") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(n)) if !n.trim().is_empty() => Ok(Some(n.clone())),
        Some(_) => Err(bad(CMD, "`name` must be a non-empty string")),
    }
}

/// The sequence settings asked for, validated with the rules `file.newSequence` uses (at most
/// 32 768 px per side, 268 435 456 px in all, a positive rate of at most 1 000 fps). The raw `fps`
/// is returned beside them: `file.newSequence` is handed the caller's value so both parse it the
/// same way, instead of a rounded re-encoding of it.
fn settings_p(p: &Value) -> Result<(SequenceSettings, f64)> {
    let mut st = SequenceSettings { width: 1080, height: 1920, frame_rate: FrameRate::FPS_30, ..SequenceSettings::default() };
    if let Some(w) = checked_u32_p(p, "width", CMD)? {
        st.width = w;
    }
    if let Some(h) = checked_u32_p(p, "height", CMD)? {
        st.height = h;
    }
    let fps = match p.get("fps") {
        None | Some(Value::Null) => 30.0,
        Some(v) => v.as_f64().ok_or_else(|| bad(CMD, "`fps` must be a number"))?,
    };
    st.frame_rate = FrameRate::from_f64(fps);
    st.validate().map_err(|e| bad(CMD, e))?;
    Ok((st, fps))
}

/// `narration`: the script and the optional voice for `tts.create`. The script is checked here, so
/// a command that has nothing to say creates nothing; the voice is left to `tts.create` (it knows
/// the catalogue), and its failure is passed on verbatim.
fn narration_p(p: &Value) -> Result<Option<(String, Option<String>)>> {
    let Some(v) = p.get("narration").filter(|v| !v.is_null()) else { return Ok(None) };
    let o = v.as_object().ok_or_else(|| bad(CMD, r#"`narration` must be an object: {"text": str, "voice": str?}"#))?;
    if let Some(k) = o.keys().find(|k| *k != "text" && *k != "voice") {
        return Err(bad(CMD, format!("unknown `narration` key `{k}` (text, voice)")));
    }
    let text = o.get("text").and_then(Value::as_str).ok_or_else(|| bad(CMD, "`narration.text` is required and must be a string"))?;
    if text.trim().is_empty() {
        return Err(bad(CMD, "`narration.text` is empty: there is nothing to say"));
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(bad(CMD, format!("`narration.text` is longer than {} KB", MAX_TEXT_BYTES / 1024)));
    }
    let voice = match o.get("voice") {
        None | Some(Value::Null) => None,
        Some(Value::String(v)) => Some(v.clone()),
        Some(_) => return Err(bad(CMD, "`narration.voice` must be a string")),
    };
    Ok(Some((text.to_string(), voice)))
}

/// `captions`: a caption track with no captions, or none.
fn captions_p(p: &Value) -> Result<bool> {
    match p.get("captions") {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(bad(CMD, "`captions` must be a boolean")),
    }
}

/// The session holds no project of its own yet: the untouched "Untitled" project a session starts
/// with — nothing in a bin, no sequence, never named, never saved, nothing unsaved (its undo
/// history would have nothing to undo, since even a reverted edit leaves the project modified).
/// Only this one is replaced; it is what `file.newProject` starts from too.
fn untouched(s: &Session) -> bool {
    !s.is_dirty() && s.path.is_none() && s.project.name == UNTITLED && s.project.items.is_empty() && s.project.root.children.is_empty()
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let name = name_p(p)?;
    let (settings, fps) = settings_p(p)?;
    let narration = narration_p(p)?;
    let captions = captions_p(p)?;
    let created = untouched(s);
    if created {
        // The body of `file.newProject`: a new project starts clean, with no history to undo into.
        s.project = std::sync::Arc::new(Project::new(name.as_deref().unwrap_or(PROJECT_NAME)));
        s.history = Default::default();
        s.history.limit = 200;
        s.state = Session::default().state;
        if s.path.take().is_some() {
            s.previews.reset_temp();
        }
        s.revision += 1;
        s.saved_revision = s.revision;
        s.events.push(crate::Event::ProjectChanged { revision: s.revision });
    }
    let seq_name = name.clone();
    let r = s.edit_group("Quick Short Video", |s| {
        let mut q = json!({"width": settings.width, "height": settings.height, "fps": fps});
        if let Some(n) = &seq_name {
            q["name"] = json!(n);
        }
        let seq = u64_p(&s.execute("file.newSequence", q)?, "sequence").ok_or_else(|| no_id("file.newSequence", "sequence"))?;
        let narration = match &narration {
            None => None,
            Some((text, voice)) => {
                let mut q = json!({"text": text, "time": 0, "track": "A1"});
                if let Some(v) = voice {
                    q["voice"] = json!(v);
                }
                Some(s.execute("tts.create", q)?)
            }
        };
        let caption_track =
            if captions { Some(u64_p(&s.execute("captions.newTrack", json!({}))?, "track").ok_or_else(|| no_id("captions.newTrack", "track"))?) } else { None };
        Ok((seq, narration, caption_track))
    });
    if r.is_err() && created {
        // The group put the new project back as it was made: an empty project has nothing unsaved.
        s.saved_revision = s.revision;
    }
    let (seq, narration, caption_track) = r?;
    let mut out = json!({"project": {"name": s.project.name, "created": created}, "sequence": seq});
    if let Some(n) = narration {
        let pick = |k: &str| n.get(k).cloned().unwrap_or(Value::Null);
        out["narration"] = json!({"item": pick("item"), "clip": pick("clip"), "duration": pick("duration")});
    }
    if let Some(t) = caption_track {
        out["captionTrack"] = json!(t);
    }
    Ok(out)
}

/// A command that reported success without the id it promises: a broken invariant of the engine,
/// never the caller's fault.
fn no_id(cmd: &str, key: &str) -> EngineError {
    EngineError::Other(format!("`{cmd}` returned no `{key}`"))
}
