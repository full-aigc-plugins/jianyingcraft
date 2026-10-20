//! The Chinese voice path through the crate's public API: what `tts.voices` lists and what `tts.create`
//! would get.
//!
//! The catalogue half runs everywhere (on a machine without a Chinese speech engine it checks the
//! placeholder). The speaking half runs on Windows when the machine has a zh-CN SAPI voice and skips
//! with a message when it has none — never silently "passes" without a voice.

use filmcraft_tts as tts;

#[test]
fn the_catalogue_always_has_a_chinese_entry() {
    assert_eq!(tts::LANGUAGES[0].0, "en-US", "English stays the default language");
    assert!(tts::LANGUAGES.iter().any(|(id, _)| *id == "zh-CN"), "{:?}", tts::LANGUAGES);
    let all = tts::voices_in(None);
    let chinese: Vec<&tts::VoiceInfo> = all.iter().filter(|v| v.language == "zh-CN").collect();
    assert!(!chinese.is_empty(), "no zh-CN entry: {:?}", all.iter().map(|v| (v.id, v.name, v.language)).collect::<Vec<_>>());
    for v in &chinese {
        let resolved = tts::voice_in(v.id, None).expect("every listed zh-CN voice resolves by id");
        assert_eq!(resolved.info().id, v.id);
        match v.engine {
            "system" => assert!(v.installed, "a system voice can speak: {v:?}"),
            "placeholder" => {
                assert!(!v.installed, "the placeholder cannot speak, so it is not installed");
                assert!(v.description.contains("cannot speak Chinese"), "{}", v.description);
                let err = resolved.synthesize("你好", &tts::Params::default()).expect_err("the placeholder never speaks");
                let msg = err.to_string();
                assert!(msg.contains("placeholder"), "{msg}");
                assert!(msg.contains("zh-CN"), "{msg}");
            }
            other => panic!("unexpected engine `{other}` for a zh-CN voice"),
        }
    }
}

#[test]
#[cfg(all(windows, not(target_arch = "wasm32")))]
fn a_listed_zh_cn_system_voice_speaks_chinese() {
    let Some(listed) = tts::voices_in(None).into_iter().find(|v| v.engine == "system" && v.language == "zh-CN") else {
        eprintln!("no zh-CN SAPI voice installed on this machine: skipping the Chinese synthesis");
        return;
    };
    let voice = tts::voice_in(listed.id, None).expect("the listed zh-CN voice resolves");
    let audio = voice.synthesize("你好，这是中文配音测试。", &tts::Params::default()).expect("the listed zh-CN voice speaks");
    assert_eq!(audio.sample_rate, tts::SAMPLE_RATE, "we ask SAPI for 24 kHz");
    assert!(audio.seconds() > 0.25, "duration {} s", audio.seconds());
    assert!(audio.samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    let peak = audio.samples.iter().fold(0f32, |m, s| m.max(s.abs()));
    eprintln!("zh voice {}: {:.2} s, {} samples, peak {peak:.3}", listed.name, audio.seconds(), audio.samples.len());
    assert!(peak > 0.01, "the audio is audible, peak {peak}");
    // pace reaches SAPI (SSML rate): 1.5× is shorter
    let fast = voice.synthesize("你好，这是中文配音测试。", &tts::Params { pace: 1.5, ..Default::default() }).expect("pace 1.5 speaks");
    assert!(fast.samples.len() < audio.samples.len(), "pace 1.5: {} vs {}", fast.samples.len(), audio.samples.len());
    // the vocal pitch setting reaches SAPI (SSML pitch): same length, different samples
    let higher = voice.synthesize("你好，这是中文配音测试。", &tts::Params { semitones: 6.0, ..Default::default() }).expect("+6 semitones speaks");
    assert_eq!(higher.samples.len(), audio.samples.len(), "pitch keeps the length");
    assert!(higher.samples != audio.samples, "+6 semitones changed nothing");
    // an English voice refuses the same text with an explanation instead of mispronouncing it
    let err = tts::voice_in("basic-female", None).expect("the built-in voice").synthesize("你好", &tts::Params::default()).expect_err("English voices cannot say Chinese");
    assert!(err.to_string().contains("Chinese"), "{err}");
}
