//! Chinese (zh-CN): the placeholder voice, and the check that keeps English voices from reading Han
//! characters as if they were English.
//!
//! On Windows the real Chinese voices come from the system's speech engine ([`crate::system`]): when
//! the machine has a zh-CN SAPI voice it is enumerated and listed for real, and it speaks. When it
//! has none, the catalogue still offers [`PLACEHOLDER_ID`] so that `tts.voices` answers "there is a
//! zh-CN entry, and here is why it cannot speak" instead of quietly leaving Chinese out. The
//! placeholder never produces audio: picking it reports an error, so nobody mistakes the built-in
//! English formant synthesizer reading Chinese with English spelling rules for a Chinese voice.

use crate::script::{self, Segment};
use crate::{Audio, Gender, MAX_SECONDS, Params, TtsError, Voice, VoiceInfo, check_text};

/// BCP 47 tag of the Chinese voices.
pub const LANGUAGE: &str = "zh-CN";

/// Id of the entry listed when no zh-CN system voice exists.
pub const PLACEHOLDER_ID: &str = "zh-cn-placeholder";

/// `VoiceInfo::engine` of the placeholder (listed, but it cannot speak).
pub const ENGINE: &str = "placeholder";

/// Why the placeholder cannot speak. Shown as the catalogue entry's description and in the error.
///
/// Phrased so it is true whether or not this machine has a system voice: the placeholder itself never
/// speaks, and the way to a real Chinese voice is `tts.voices` (which lists the system's own zh-CN
/// voices when the machine has them).
const WHY: &str = "the zh-cn-placeholder entry never produces audio; it exists so that tts.voices can \
list a zh-CN voice when no Chinese speech engine is available. Run tts.voices to see whether this \
machine has a Windows system (SAPI) Chinese voice; if it has none, install one (Windows Settings ▸ \
Time & language ▸ Speech) and run tts.voices again.";

/// The catalogue entry listed while no zh-CN system voice exists.
///
/// `installed: false`: no engine can speak it. `engine: "placeholder"` keeps the panel and agents
/// from treating it as a usable voice; the description says why.
pub fn placeholder_info() -> VoiceInfo {
    VoiceInfo {
        id: PLACEHOLDER_ID,
        name: "中文（占位）",
        language: LANGUAGE,
        gender: Gender::Female,
        engine: ENGINE,
        description: "Placeholder, cannot speak Chinese. 占位条目，不支持中文发音：本机没有可用的中文语音引擎。",
        license: "n/a (produces no audio)",
        installed: false,
    }
}

/// The voice behind [`placeholder_info`]: it refuses every script with an explicit error.
pub struct PlaceholderVoice;

impl Voice for PlaceholderVoice {
    fn info(&self) -> VoiceInfo {
        placeholder_info()
    }

    fn synthesize(&self, text: &str, params: &Params) -> Result<Audio, TtsError> {
        // the usual script and parameter checks first, so a bad script reports the usual error
        check_text(text)?;
        params.check()?;
        Err(TtsError::Unsupported(format!("the `{PLACEHOLDER_ID}` voice is a placeholder and cannot speak: {WHY}")))
    }
}

/// `true` for a Han character: CJK Unified Ideographs (and Ext A, compatibility ideographs and
/// Ext B+). Han characters carry the Chinese (and Japanese kanji) text a Latin-script voice cannot
/// pronounce; hiragana/katakana are not included, exactly as before this check existed.
pub fn is_han(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4D_BF | 0x4E00..=0x9F_FF | 0xF900..=0xFA_FF | 0x2_0000..=0x3_134F)
}

/// The first Han character in `text`, if any.
pub fn first_han(text: &str) -> Option<char> {
    text.chars().find(|c| is_han(*c))
}

/// Refuse a script that contains Han characters for a voice that only speaks English.
///
/// The built-in formant voices and the Kokoro voices drop every character their spelling rules and
/// dictionary do not know, so Chinese text used to end in "the script has nothing to say" (or, for
/// mixed text, in English words read in place of the Chinese ones). An explicit error names the
/// voice, the character and the way out.
pub fn reject_han(voice_id: &str, text: &str) -> Result<(), TtsError> {
    match first_han(text) {
        None => Ok(()),
        Some(c) => Err(TtsError::Unsupported(format!(
            "the `{voice_id}` voice speaks English and cannot pronounce Chinese (`{c}`); choose a zh-CN voice in tts.voices, or spell the line in English"
        ))),
    }
}

/// Total length of the pause markers in `text`, in milliseconds (saturating: hostile values cannot
/// overflow).
pub fn pause_millis(text: &str) -> u64 {
    script::parse(text)
        .iter()
        .map(|s| match s {
            Segment::Pause { millis } => u64::from(*millis),
            Segment::Speech(_) => 0,
        })
        .fold(0u64, u64::saturating_add)
}

/// Whether a script of `chars` characters plus `pause_ms` of silence can fit in [`MAX_SECONDS`] at
/// the system voice's speaking rate.
///
/// Chinese speech runs at roughly four to six characters a second; six is used so that a script near
/// the limit is not refused early. Only an estimate: the real limit is enforced on the samples.
pub fn fits_in_max_seconds(chars: usize, pause_ms: u64) -> bool {
    const CHARS_PER_SECOND: f64 = 6.0;
    let speech = chars as f64 / CHARS_PER_SECOND;
    let pauses = pause_ms as f64 / 1000.0;
    (speech + pauses) <= MAX_SECONDS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn han_characters_are_recognised() {
        assert_eq!(first_han("你好，世界"), Some('你'));
        assert_eq!(first_han("Hello"), None);
        assert_eq!(first_han("こんにちは"), None, "kana is not Han");
        assert_eq!(first_han("日本語"), Some('日'), "kanji is Han (and unreadable by an English voice)");
        assert_eq!(first_han("x\u{4E00}y"), Some('\u{4E00}'));
        assert_eq!(first_han("👋 漢"), Some('漢'));
        assert_eq!(first_han(""), None);
    }

    #[test]
    fn english_voices_refuse_chinese_with_a_useful_error() {
        let e = reject_han("basic-female", "你好").unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("basic-female"), "{msg}");
        assert!(msg.contains('你'), "{msg}");
        assert!(msg.contains("zh-CN"), "{msg}");
        // English and other non-Han scripts pass: the voices decide what they can say
        assert!(reject_han("basic-female", "Hello, world!").is_ok());
        assert!(reject_han("basic-female", "Привет").is_ok());
    }

    #[test]
    fn the_placeholder_never_claims_to_speak() {
        let v = PlaceholderVoice;
        assert_eq!(v.info().language, LANGUAGE);
        assert_eq!(v.info().engine, ENGINE);
        assert!(!v.info().installed);
        assert!(v.info().description.contains("cannot speak Chinese"));
        for text in ["你好，世界", "Hello", "Hi [pause 1s] there"] {
            let e = v.synthesize(text, &Params::default()).unwrap_err();
            let msg = e.to_string();
            assert!(msg.contains("placeholder"), "{msg}");
            assert!(msg.contains("zh-CN"), "{msg}");
        }
        // the ordinary script checks still come first
        assert_eq!(v.synthesize("", &Params::default()).err(), Some(TtsError::Empty));
        assert!(v.synthesize("你好", &Params { pace: 9.0, ..Default::default() }).is_err());
    }

    #[test]
    fn pause_and_length_estimates_are_bounded() {
        assert_eq!(pause_millis("a [pause 1s] b [pause 500ms]"), 1500);
        assert_eq!(pause_millis("no pauses"), 0);
        assert_eq!(pause_millis("[pause 999999999999s]"), u64::from(script::MAX_PAUSE_MS));
        assert!(fits_in_max_seconds(0, 0));
        assert!(fits_in_max_seconds(1000, 60_000));
        assert!(!fits_in_max_seconds(usize::MAX / 2, 0));
        assert!(!fits_in_max_seconds(0, u64::MAX / 2));
    }
}
