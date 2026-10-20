//! Text to speech for JianyingCraft's Text to Speech panel (narrations).
//!
//! - [`Voice`]: the trait every voice implements: a script in, mono samples out.
//! - [`voices`] / [`voice`]: the voice catalogue (id, name, language, gender, engine, licence).
//! - [`script`]: splits a script into speech and pause markers (`[pause 1s]`, `[pause 500ms]`).
//! - [`formant`]: the built-in voices, an original source–filter formant synthesizer. They need
//!   no download and work everywhere (also in the web build); they sound robotic and they speak
//!   English. The neural voices (Kokoro-82M, downloaded on first use) are added behind a feature in a
//!   later milestone.
//! - [`system`]: the Windows system (SAPI) **zh-CN** voices, enumerated and spoken through the same
//!   `System.Speech` engine, so a listed Chinese voice really speaks Chinese.
//! - [`zh`]: the zh-CN placeholder listed when the machine has no Chinese speech engine, and the
//!   check that makes English voices refuse Han text with an explanation instead of reading it as if
//!   it were English.
//!
//! The built-in and neural voices are deterministic: the same script and settings give the same
//! samples. A system voice is the operating system's synthesizer, so it can differ between Windows
//! versions and installed voice packages.

pub mod catalog;
pub mod formant;
#[cfg(feature = "kokoro")]
pub mod kokoro;
#[cfg(feature = "kokoro")]
pub mod neural;
pub mod script;
pub mod system;
pub mod zh;

use serde::Serialize;

/// Sample rate of every voice's output.
pub const SAMPLE_RATE: u32 = 24_000;
/// Longest narration a voice produces, in seconds.
pub const MAX_SECONDS: f64 = 30.0 * 60.0;
/// Longest script accepted, in bytes (same as `filmcraft_project::narration::MAX_TEXT_BYTES`).
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
/// Pace limits.
pub const MIN_PACE: f64 = 0.5;
pub const MAX_PACE: f64 = 2.0;
/// Pitch shift limit in semitones (either way).
pub const MAX_SEMITONES: f64 = 12.0;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TtsError {
    #[error("unknown voice `{0}`")]
    UnknownVoice(String),
    #[error("the script has nothing to say")]
    Empty,
    #[error("the script is longer than {} KB", MAX_TEXT_BYTES / 1024)]
    TextTooLong,
    #[error("the narration would be longer than {} minutes", MAX_SECONDS / 60.0)]
    AudioTooLong,
    #[error("{0}")]
    Invalid(String),
    #[error("voice model: {0}")]
    Model(String),
    #[error("the natural voices are not downloaded yet")]
    NotInstalled,
    #[error("natural voices are not available in this build (built without the `kokoro` feature)")]
    Unavailable,
    /// A voice that cannot say this script: an English voice given Chinese, or the zh-CN placeholder.
    #[error("{0}")]
    Unsupported(String),
    /// The Windows system (SAPI) speech engine failed: it did not start, timed out, or wrote no audio.
    #[error("system voice: {0}")]
    System(String),
}

/// How a voice speaks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// Pitch shift in semitones, ±[`MAX_SEMITONES`].
    pub semitones: f64,
    /// Speaking rate, [`MIN_PACE`]–[`MAX_PACE`] (1 = normal).
    pub pace: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params { semitones: 0.0, pace: 1.0 }
    }
}

impl Params {
    pub fn check(&self) -> Result<(), TtsError> {
        if !(MIN_PACE..=MAX_PACE).contains(&self.pace) {
            return Err(TtsError::Invalid(format!("pace must be between {MIN_PACE} and {MAX_PACE}")));
        }
        if !(-MAX_SEMITONES..=MAX_SEMITONES).contains(&self.semitones) {
            return Err(TtsError::Invalid(format!("pitch must be within ±{MAX_SEMITONES} semitones")));
        }
        Ok(())
    }
}

/// Mono audio at [`SAMPLE_RATE`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Audio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl Audio {
    pub fn seconds(&self) -> f64 {
        self.samples.len() as f64 / f64::from(self.sample_rate.max(1))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Gender {
    Female,
    Male,
}

/// A voice in the catalogue.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInfo {
    pub id: &'static str,
    pub name: &'static str,
    /// BCP 47 tag.
    pub language: &'static str,
    pub gender: Gender,
    /// `basic` (built in) or `neural` (downloaded).
    pub engine: &'static str,
    pub description: &'static str,
    pub license: &'static str,
    /// Usable now: built in, downloaded, or backed by the system's speech engine. The zh-CN
    /// placeholder is listed so the gap is visible, and it is `false` because nothing can speak it.
    pub installed: bool,
}

/// A voice that turns a script into audio.
pub trait Voice: Send + Sync {
    fn info(&self) -> VoiceInfo;
    /// Speak `text` (pause markers allowed). Fails on an empty or over-long script, or bad params.
    fn synthesize(&self, text: &str, params: &Params) -> Result<Audio, TtsError>;
}

/// Languages offered: English first (the default), then Chinese.
///
/// `zh-CN` is always offered so the panel never hides the gap; whether a Chinese voice can really
/// speak depends on the machine (see [`voices_in`]).
pub const LANGUAGES: &[(&str, &str)] = &[("en-US", "English (United States)"), (zh::LANGUAGE, "中文（简体）")];

/// Every voice JianyingCraft knows, in catalogue order: the built-in voices, the Chinese voices
/// (this machine's Windows system voices, or the [`zh`] placeholder when it has none), then the neural
/// voices (installed when their package is downloaded into `models_dir`).
pub fn voices_in(models_dir: Option<&std::path::Path>) -> Vec<VoiceInfo> {
    let installed = models_dir.is_some_and(catalog::installed) && cfg!(feature = "kokoro");
    let system = system::voices();
    let mut out: Vec<VoiceInfo> = formant::VOICES.iter().map(|v| v.info).collect();
    out.extend(system.iter().map(|v| v.info()));
    if system.is_empty() {
        // no Chinese speech engine here: say so instead of leaving zh-CN out
        out.push(zh::placeholder_info());
    }
    out.extend(catalog::VOICES.iter().map(|v| VoiceInfo { installed, ..v.info }));
    out
}

/// The built-in voices (always available).
pub fn voices() -> Vec<VoiceInfo> {
    formant::VOICES.iter().map(|v| v.info).collect()
}

/// The voice with this id: a built-in voice, a Windows system zh-CN voice, or the zh-CN placeholder.
/// Neural voices need [`voice_in`] (they live in a downloaded package).
pub fn voice(id: &str) -> Result<Box<dyn Voice>, TtsError> {
    if let Some(v) = system::find(id) {
        return Ok(Box::new(v) as Box<dyn Voice>);
    }
    if id == zh::PLACEHOLDER_ID {
        return Ok(Box::new(zh::PlaceholderVoice) as Box<dyn Voice>);
    }
    formant::VOICES
        .iter()
        .find(|v| v.info.id == id)
        .map(|v| Box::new(formant::FormantVoice::new(*v)) as Box<dyn Voice>)
        .ok_or_else(|| TtsError::UnknownVoice(id.to_string()))
}

/// The voice `id`: built in, or (feature `kokoro`) a neural voice from the package in `models_dir`.
pub fn voice_in(id: &str, models_dir: Option<&std::path::Path>) -> Result<Box<dyn Voice>, TtsError> {
    if catalog::find(id).is_some() {
        #[cfg(feature = "kokoro")]
        {
            let dir = models_dir.ok_or(TtsError::NotInstalled)?;
            return neural::load(id, dir).map(|v| Box::new(v) as Box<dyn Voice>);
        }
        #[cfg(not(feature = "kokoro"))]
        {
            let _ = models_dir;
            return Err(TtsError::Unavailable);
        }
    }
    voice(id)
}

/// The voice used when none is chosen.
pub fn default_voice_id() -> &'static str {
    formant::VOICES.first().map(|v| v.info.id).unwrap_or("basic-female")
}

/// The sentence "Hear this voice" plays.
pub const SAMPLE_SENTENCE: &str = "Hello! This is how I sound when I read your script.";

/// Validate a script before synthesis: length, and that it says something.
pub fn check_text(text: &str) -> Result<(), TtsError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(TtsError::TextTooLong);
    }
    let says_something = script::parse(text).iter().any(|s| matches!(s, script::Segment::Speech(t) if t.chars().any(char::is_alphanumeric)));
    if !says_something {
        return Err(TtsError::Empty);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_ids_are_unique_and_resolvable() {
        let v = voices();
        assert!(v.len() >= 2);
        for (i, a) in v.iter().enumerate() {
            assert!(v.iter().skip(i + 1).all(|b| b.id != a.id), "duplicate {}", a.id);
            assert_eq!(voice(a.id).unwrap().info().id, a.id);
        }
        assert_eq!(voice("nope").err(), Some(TtsError::UnknownVoice("nope".into())));
        assert!(voice(default_voice_id()).is_ok());
    }

    #[test]
    fn params_are_checked() {
        assert!(Params::default().check().is_ok());
        for p in [
            Params { pace: f64::NAN, ..Default::default() },
            Params { pace: 0.1, ..Default::default() },
            Params { semitones: 40.0, ..Default::default() },
            Params { semitones: f64::INFINITY, ..Default::default() },
        ] {
            assert!(p.check().is_err(), "{p:?}");
        }
    }

    #[test]
    fn empty_scripts_are_refused() {
        assert_eq!(check_text(""), Err(TtsError::Empty));
        assert_eq!(check_text("  [pause 1s] ... "), Err(TtsError::Empty));
        assert_eq!(check_text(&"a".repeat(MAX_TEXT_BYTES + 1)), Err(TtsError::TextTooLong));
        assert!(check_text("Hi").is_ok());
    }

    /// `tts.voices` must always answer for Chinese: a real system voice where the machine has one, and
    /// the clearly marked placeholder where it has none.
    #[test]
    fn the_catalogue_always_offers_chinese() {
        assert_eq!(LANGUAGES[0].0, "en-US", "English stays the default language");
        assert!(LANGUAGES.iter().any(|(id, _)| *id == "zh-CN"), "{LANGUAGES:?}");
        let all = voices_in(None);
        for (i, a) in all.iter().enumerate() {
            assert!(all.iter().skip(i + 1).all(|b| b.id != a.id), "duplicate id {}", a.id);
        }
        let chinese: Vec<VoiceInfo> = all.iter().copied().filter(|v| v.language == "zh-CN").collect();
        assert!(!chinese.is_empty(), "no zh-CN entry: {:?}", all.iter().map(|v| (v.id, v.language)).collect::<Vec<_>>());
        for v in &chinese {
            assert_eq!(voice_in(v.id, None).map(|x| x.info().id).ok(), Some(v.id), "{} does not resolve", v.id);
        }
        if system::voices().is_empty() {
            let placeholder = chinese.iter().find(|v| v.id == zh::PLACEHOLDER_ID).expect("the placeholder when no system voice");
            assert_eq!(placeholder.engine, "placeholder");
            assert!(!placeholder.installed, "it cannot speak, so it is not installed");
            assert!(placeholder.description.contains("cannot speak Chinese"), "{}", placeholder.description);
            let err = voice_in(zh::PLACEHOLDER_ID, None).and_then(|v| v.synthesize("你好，世界", &Params::default())).unwrap_err();
            assert!(err.to_string().contains("placeholder"), "{err}");
        } else {
            for v in &chinese {
                assert_eq!(v.engine, "system");
                assert_eq!(v.language, "zh-CN");
                assert!(v.installed, "a system voice can speak: {v:?}");
            }
        }
    }

    /// English voices refuse Chinese with a message that says what to do, instead of dropping the
    /// characters ("the script has nothing to say") or reading them as English.
    #[test]
    fn english_voices_refuse_chinese() {
        for id in ["basic-female", "basic-male"] {
            let v = voice(id).unwrap();
            let msg = v.synthesize("你好，世界", &Params::default()).unwrap_err().to_string();
            assert!(msg.contains(id), "{msg}");
            assert!(msg.contains("Chinese"), "{msg}");
            assert!(msg.contains("zh-CN"), "{msg}");
            // English still works
            assert!(v.synthesize("Hello there.", &Params::default()).is_ok());
        }
    }
}
