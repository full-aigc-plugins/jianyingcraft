//! Windows system voices (SAPI) for Chinese.
//!
//! Windows ships Chinese voices (Microsoft Huihui, Kangkang, Yaoyao …) as SAPI voice tokens, and
//! `System.Speech` speaks them. This module asks the same engine that is about to speak — Windows
//! PowerShell 5.1 + `System.Speech` — which zh-CN voices it has, so every voice listed here can
//! actually say the text: nothing is listed that cannot be spoken, and nothing is invented.
//!
//! How it works:
//!
//! - **Enumeration** runs one PowerShell process (cached for the life of the process: the panel asks
//!   for the catalogue every frame) that prints `name|culture|gender|age` per installed, enabled
//!   voice; only `zh-CN` voices are kept. English system voices are deliberately not listed: English
//!   already has the built-in formant voices and the Kokoro voices, and listing the OS voices too
//!   would change `tts.voices` on every Windows machine for no gain.
//! - **Synthesis** writes the script as SSML (pause markers become `<break>`s, pace and vocal pitch
//!   become `prosody` percentages) into a private temporary directory and runs one PowerShell process
//!   that asks `System.Speech` for a 24 kHz, 16-bit, mono WAV. The WAV is decoded into [`Audio`].
//!   Nothing of the user's text is ever put on a command line: only file paths and the voice name
//!   travel through environment variables, and the text goes through a UTF-8 file that PowerShell
//!   reads (no shell, no eval).
//! - Elsewhere (macOS, Linux, wasm, or a Windows machine where PowerShell cannot start) there are no
//!   system voices; [`crate::zh`] then lists the placeholder instead.
//!
//! Trade-offs, recorded on purpose:
//!
//! - The audio is produced by the operating system's synthesizer, not by FilmCraft, so it is **not**
//!   deterministic across Windows versions or voice packages (the built-in and neural voices are).
//! - Pace and pitch are handed to SAPI as SSML percentages (`pace 1.2` → `rate +20%`, `+6 semitones`
//!   → `pitch +41%`). That the audio changes and that the length follows the pace is tested; whether
//!   SAPI shifts the pitch by exactly the requested interval is not measured (see the crate README).
//! - A SAPI COM binding would avoid the child process but needs `unsafe` (forbidden in this
//!   workspace) or a new dependency (`windows-sys`), which is why this stays on the editable text
//!   interface of the speech engine.

// The parsers below are platform independent and unit-tested everywhere; only Windows calls them in
// production, so the other builds would report them as dead code.
#![cfg_attr(not(all(windows, not(target_arch = "wasm32"))), allow(dead_code))]

use std::sync::OnceLock;

use crate::script::{self, Segment};
use crate::zh;
use crate::{Audio, Gender, Params, TtsError, Voice, VoiceInfo, check_text};

/// `VoiceInfo::engine` of a system voice.
pub const ENGINE: &str = "system";

/// Licence of the audio: it comes from the operating system.
const LICENSE: &str = "Windows system voice (part of the operating system)";

/// Longest a single PowerShell call may take before it is killed. The text is capped at 64 KB and
/// SAPI runs several times faster than real time; this only stops a stuck process from hanging the
/// narration job forever.
const MIN_TIMEOUT_SECS: u64 = 60;
const MAX_TIMEOUT_SECS: u64 = 600;

/// A Windows system voice, ready to speak.
///
/// `info.name` is the exact name SAPI knows it by (`SelectVoice`); `info.id` is derived from it.
#[derive(Clone, Copy, Debug)]
pub struct SystemVoice {
    info: VoiceInfo,
}

impl Voice for SystemVoice {
    fn info(&self) -> VoiceInfo {
        self.info
    }

    fn synthesize(&self, text: &str, params: &Params) -> Result<Audio, TtsError> {
        check_text(text)?;
        params.check()?;
        speak(self.info, text, params)
    }
}

/// Every system zh-CN voice on this machine (empty when there are none). Cached: enumeration starts
/// a PowerShell process and the Text to Speech panel asks for the catalogue every frame.
pub fn voices() -> &'static [SystemVoice] {
    static VOICES: OnceLock<Vec<SystemVoice>> = OnceLock::new();
    VOICES.get_or_init(enumerate)
}

/// The system voice `id`, if this machine has it.
pub fn find(id: &str) -> Option<SystemVoice> {
    voices().iter().copied().find(|v| v.info.id == id)
}

// ---------------------------------------------------------------- enumeration

/// Prints one line per installed and enabled voice: `name|culture|gender|age`.
///
/// The text is fixed: nothing from the user reaches this script.
const LIST_SCRIPT: &str = r#"
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$ErrorActionPreference = 'Stop'
try {
  Add-Type -AssemblyName System.Speech
  $s = New-Object System.Speech.Synthesis.SpeechSynthesizer
  foreach ($v in $s.GetInstalledVoices()) {
    if ($v.Enabled -ne $true) { continue }
    $i = $v.VoiceInfo
    Write-Output ($i.Name + '|' + $i.Culture.Name + '|' + $i.Gender + '|' + $i.Age)
  }
  $s.Dispose()
} catch {
  [Console]::Error.WriteLine($_.Exception.Message)
  exit 1
}
"#;

#[cfg(all(windows, not(target_arch = "wasm32")))]
fn enumerate() -> Vec<SystemVoice> {
    // No PowerShell, an execution policy, a missing System.Speech or no voices: no system voices.
    match run(LIST_SCRIPT, &[], std::time::Duration::from_secs(MIN_TIMEOUT_SECS)) {
        Ok(done) if done.success => parse_voices(&String::from_utf8_lossy(&done.stdout)),
        _ => Vec::new(),
    }
}

#[cfg(not(all(windows, not(target_arch = "wasm32"))))]
fn enumerate() -> Vec<SystemVoice> {
    Vec::new()
}

/// Parse the `name|culture|gender|age` lines, keeping the zh-CN voices.
fn parse_voices(stdout: &str) -> Vec<SystemVoice> {
    let mut out: Vec<SystemVoice> = Vec::new();
    for line in stdout.lines() {
        let mut fields = line.split('|');
        let (Some(name), Some(culture), Some(gender)) = (fields.next(), fields.next(), fields.next()) else {
            continue;
        };
        let (name, culture) = (name.trim(), culture.trim());
        if name.is_empty() || culture.is_empty() {
            continue;
        }
        // `VoiceInfo` is `Copy` and holds `&'static str`; a handful of names is leaked once per
        // process, together with the cached catalogue
        let name = leak(name.to_string());
        let language = leak(culture.to_string());
        let id = leak(unique_id(&out, name));
        let description = "Windows system voice (zh-CN) spoken by the Windows speech engine (SAPI); pace and vocal pitch are applied as SSML prosody percentages. Enumerated from the system, so it really speaks Chinese.";
        out.push(SystemVoice {
            info: VoiceInfo {
                id,
                name,
                language,
                gender: parse_gender(gender.trim()),
                engine: ENGINE,
                description,
                license: LICENSE,
                installed: true,
            },
        });
    }
    out
}

/// SAPI reports `Female` / `Male`; anything else is treated as female (the voices that matter report
/// one of the two, and `Gender` has no "unknown" case).
fn parse_gender(gender: &str) -> Gender {
    if gender.eq_ignore_ascii_case("male") { Gender::Male } else { Gender::Female }
}

/// `Microsoft Huihui Desktop` → `system-microsoft-huihui-desktop`, made unique within `existing`.
fn unique_id(existing: &[SystemVoice], name: &str) -> String {
    let base = format!("system-windows-{}", slug(name));
    if !existing.iter().any(|v| v.info.id == base) {
        return base;
    }
    for n in 2..1000u32 {
        let id = format!("{base}-{n}");
        if !existing.iter().any(|v| v.info.id == id) {
            return id;
        }
    }
    format!("{base}-{}", existing.len().saturating_add(1))
}

/// Lowercase ASCII letters and digits, every other run of characters collapsed to one `-`.
fn slug(name: &str) -> String {
    let mut s = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if !s.ends_with('-') {
            s.push('-');
        }
    }
    let s = s.trim_matches('-');
    if s.is_empty() { "voice".to_string() } else { s.to_string() }
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

// ---------------------------------------------------------------- synthesis

/// Speaks SSML read from `FILMCRAFT_TTS_SSML` with the voice `FILMCRAFT_TTS_VOICE`, writing a 24 kHz
/// 16-bit mono WAV to `FILMCRAFT_TTS_WAV`. The text never touches the command line.
const SPEAK_SCRIPT: &str = r#"
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$ErrorActionPreference = 'Stop'
try {
  Add-Type -AssemblyName System.Speech
  $s = New-Object System.Speech.Synthesis.SpeechSynthesizer
  $s.SelectVoice($env:FILMCRAFT_TTS_VOICE)
  $fmt = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(24000, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, [System.Speech.AudioFormat.AudioChannel]::Mono)
  $s.SetOutputToWaveFile($env:FILMCRAFT_TTS_WAV, $fmt)
  $s.SpeakSsml([IO.File]::ReadAllText($env:FILMCRAFT_TTS_SSML, [Text.Encoding]::UTF8))
  $s.SetOutputToNull()
  $s.Dispose()
} catch {
  [Console]::Error.WriteLine($_.Exception.Message)
  exit 1
}
"#;

#[cfg(all(windows, not(target_arch = "wasm32")))]
fn speak(info: VoiceInfo, text: &str, params: &Params) -> Result<Audio, TtsError> {
    let chars = text.chars().count();
    if !zh::fits_in_max_seconds(chars, zh::pause_millis(text)) {
        return Err(TtsError::AudioTooLong);
    }
    let dir = TempDir::new()?;
    let request = dir.0.join("speech.ssml");
    let wav = dir.0.join("speech.wav");
    std::fs::write(&request, ssml(text, params)).map_err(|e| TtsError::System(format!("cannot write the speech request: {e}")))?;
    let done = run(
        SPEAK_SCRIPT,
        &[
            ("FILMCRAFT_TTS_VOICE", std::ffi::OsString::from(info.name)),
            ("FILMCRAFT_TTS_SSML", request.into_os_string()),
            ("FILMCRAFT_TTS_WAV", wav.clone().into_os_string()),
        ],
        timeout_for(chars),
    )?;
    if !done.success {
        return Err(TtsError::System(format!("the Windows speech engine failed: {}", tail(&done.stderr))));
    }
    let bytes = std::fs::read(&wav).map_err(|e| TtsError::System(format!("the Windows speech engine wrote no audio: {e}")))?;
    let audio = decode_wav(&bytes)?;
    if audio.samples.len() > (crate::MAX_SECONDS * f64::from(audio.sample_rate)) as usize {
        return Err(TtsError::AudioTooLong);
    }
    Ok(audio)
}

#[cfg(not(all(windows, not(target_arch = "wasm32"))))]
fn speak(_info: VoiceInfo, _text: &str, _params: &Params) -> Result<Audio, TtsError> {
    Err(TtsError::System("Windows system voices are only available on Windows".into()))
}

/// The SSML handed to the speech engine: one `prosody` element per speech segment (pace and pitch as
/// percentages), a `<break>` per pause marker.
fn ssml(text: &str, params: &Params) -> String {
    // `Params` is validated before, so these are finite and in range; the casts saturate anyway
    let rate = ((params.pace - 1.0) * 100.0).round() as i64;
    let pitch = ((2f64.powf(params.semitones / 12.0) - 1.0) * 100.0).round() as i64;
    let mut body = String::with_capacity(text.len().saturating_mul(2).saturating_add(128));
    for segment in script::parse(text) {
        match segment {
            Segment::Pause { millis } => {
                body.push_str(&format!("<break time=\"{millis}ms\"/>"));
            }
            Segment::Speech(s) => {
                body.push_str(&format!("<prosody rate=\"{rate:+}%\" pitch=\"{pitch:+}%\">"));
                push_xml_escaped(&mut body, &s);
                body.push_str("</prosody>");
            }
        }
    }
    format!("<speak version=\"1.0\" xmlns=\"http://www.w3.org/2001/10/synthesis\" xml:lang=\"{}\">{body}</speak>", zh::LANGUAGE)
}

/// Append `text` escaped for XML, dropping the control characters XML 1.0 cannot carry (so a script
/// with a stray NUL fails neither the engine nor the user).
fn push_xml_escaped(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' => {}
            c => out.push(c),
        }
    }
}

/// The last part of a PowerShell error, collapsed to one line (stderr is CLIXML on some systems).
#[cfg(all(windows, not(target_arch = "wasm32")))]
fn tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return "no message".into();
    }
    let skip = flat.chars().count().saturating_sub(300);
    flat.chars().skip(skip).collect()
}

#[cfg(all(windows, not(target_arch = "wasm32")))]
fn timeout_for(chars: usize) -> std::time::Duration {
    // Chinese speech is roughly 6 characters a second and SAPI runs several times faster than real
    // time; two seconds of budget per estimated second of speech is generous without being unbounded.
    let estimate = (chars / 6) as u64;
    std::time::Duration::from_secs(MIN_TIMEOUT_SECS.saturating_add(estimate.saturating_mul(2)).min(MAX_TIMEOUT_SECS))
}

/// A private temporary directory, removed when it goes out of scope (also on the error paths).
#[cfg(all(windows, not(target_arch = "wasm32")))]
struct TempDir(std::path::PathBuf);

#[cfg(all(windows, not(target_arch = "wasm32")))]
impl TempDir {
    fn new() -> Result<TempDir, TtsError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("filmcraft-tts-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| TtsError::System(format!("cannot create {}: {e}", dir.display())))?;
        Ok(TempDir(dir))
    }
}

#[cfg(all(windows, not(target_arch = "wasm32")))]
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------- the WAV

/// Decode the speech engine's WAV: 16-bit PCM, mono (whatever rate it wrote).
fn decode_wav(bytes: &[u8]) -> Result<Audio, TtsError> {
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(TtsError::System("the speech engine did not write a RIFF/WAVE file".into()));
    }
    let mut pos = 12usize;
    let mut format: Option<(u16, u16, u32, u16)> = None; // (format tag, channels, rate, bits)
    let mut data: Option<&[u8]> = None;
    while pos.saturating_add(8) <= bytes.len() {
        let id = bytes.get(pos..pos.saturating_add(4)).ok_or_else(|| TtsError::System("truncated WAV chunk id".into()))?;
        let len = usize::try_from(read_u32(bytes, pos.saturating_add(4)).ok_or_else(|| TtsError::System("truncated WAV chunk length".into()))?)
            .map_err(|_| TtsError::System("impossible WAV chunk length".into()))?;
        let start = pos.saturating_add(8);
        let end = start.checked_add(len).ok_or_else(|| TtsError::System("impossible WAV chunk length".into()))?;
        let body = bytes.get(start..end).ok_or_else(|| TtsError::System("truncated WAV chunk".into()))?;
        match id {
            b"fmt " => {
                let channels = read_u16(body, 2).ok_or_else(|| TtsError::System("truncated WAV format".into()))?;
                let rate = read_u32(body, 4).ok_or_else(|| TtsError::System("truncated WAV format".into()))?;
                let bits = read_u16(body, 14).ok_or_else(|| TtsError::System("truncated WAV format".into()))?;
                let mut tag = read_u16(body, 0).ok_or_else(|| TtsError::System("truncated WAV format".into()))?;
                // WAVE_FORMAT_EXTENSIBLE: the real tag is the first two bytes of the sub-format GUID
                if tag == 0xFFFE {
                    tag = read_u16(body, 24).ok_or_else(|| TtsError::System("truncated WAV sub-format".into()))?;
                }
                format = Some((tag, channels, rate, bits));
            }
            b"data" => data = Some(body),
            _ => {}
        }
        // chunks are word aligned
        pos = end.checked_add(len % 2).ok_or_else(|| TtsError::System("impossible WAV chunk length".into()))?;
    }
    let (tag, channels, rate, bits) = format.ok_or_else(|| TtsError::System("the WAV has no format chunk".into()))?;
    let data = data.ok_or_else(|| TtsError::System("the WAV has no audio".into()))?;
    if data.is_empty() {
        return Err(TtsError::System("the speech engine wrote no audio".into()));
    }
    if !(1..=384_000).contains(&rate) {
        return Err(TtsError::System(format!("the speech engine wrote {rate} Hz audio")));
    }
    if tag != 1 || bits != 16 {
        return Err(TtsError::System(format!("the speech engine wrote {bits}-bit format {tag} audio (16-bit PCM is required)")));
    }
    if channels != 1 {
        return Err(TtsError::System(format!("the speech engine wrote {channels}-channel audio (mono is required)")));
    }
    let mut samples: Vec<f32> = Vec::new();
    samples.try_reserve_exact(data.len() / 2).map_err(|e| TtsError::System(format!("unable to allocate the narration: {e}")))?;
    for pair in data.as_chunks::<2>().0 {
        samples.push(f32::from(i16::from_le_bytes(*pair)) / 32_768.0);
    }
    if !samples.iter().any(|s| *s != 0.0) {
        return Err(TtsError::System("the system voice returned silence".into()));
    }
    Ok(Audio { samples, sample_rate: rate })
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let s = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes(s.try_into().ok()?))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let s = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(s.try_into().ok()?))
}

// ---------------------------------------------------------------- the child process

#[cfg(all(windows, not(target_arch = "wasm32")))]
struct Done {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    success: bool,
}

/// Run a fixed PowerShell script with `env` added, bounded by `timeout`.
///
/// `-EncodedCommand` (the script is base64 of UTF-16LE) avoids every quoting rule of the command
/// line; the script itself only ever comes from this module.
#[cfg(all(windows, not(target_arch = "wasm32")))]
fn run(script: &str, env: &[(&str, std::ffi::OsString)], timeout: std::time::Duration) -> Result<Done, TtsError> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::Instant;

    let mut cmd = Command::new(powershell());
    cmd.arg("-NoProfile").arg("-NonInteractive").arg("-EncodedCommand").arg(encode_command(script));
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (key, value) in env {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().map_err(|e| TtsError::System(format!("cannot start Windows PowerShell: {e}")))?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        pipe.map(|mut pipe| {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = pipe.read_to_end(&mut buf);
                buf
            })
        })
    };
    let stdout = drain(child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let stderr = drain(child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(TtsError::System(format!("the Windows speech engine did not answer within {} s", timeout.as_secs())));
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(TtsError::System(format!("waiting for Windows PowerShell: {e}")));
            }
        }
    };
    let collect = |handle: Option<std::thread::JoinHandle<Vec<u8>>>| handle.and_then(|h| h.join().ok()).unwrap_or_default();
    Ok(Done { stdout: collect(stdout), stderr: collect(stderr), success: status.success() })
}

#[cfg(all(windows, not(target_arch = "wasm32")))]
fn powershell() -> std::path::PathBuf {
    if let Some(root) = std::env::var_os("SystemRoot") {
        let exe = std::path::Path::new(&root).join("System32").join("WindowsPowerShell").join("v1.0").join("powershell.exe");
        if exe.is_file() {
            return exe;
        }
    }
    std::path::PathBuf::from("powershell.exe")
}

/// Base64 of the script as UTF-16LE, which is what `-EncodedCommand` reads.
fn encode_command(script: &str) -> String {
    let mut utf16 = Vec::with_capacity(script.len().saturating_mul(2));
    for unit in script.encode_utf16() {
        utf16.extend_from_slice(&unit.to_le_bytes());
    }
    base64(&utf16)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let symbol = |i: usize| char::from(TABLE.get(i & 63).copied().unwrap_or(b'A'));
    let mut out = String::with_capacity(bytes.len().div_ceil(3).saturating_mul(4));
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk.first().copied().unwrap_or(0));
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(symbol((triple >> 18) as usize));
        out.push(symbol((triple >> 12) as usize));
        out.push(if chunk.len() > 1 { symbol((triple >> 6) as usize) } else { '=' });
        out.push(if chunk.len() > 2 { symbol(triple as usize) } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_zh_cn_voices_are_kept_and_they_get_stable_ids() {
        let stdout = "Microsoft Huihui Desktop|zh-CN|Female|Adult\n\
                      Microsoft Kangkang|zh-CN|Male|Adult\n\
                      Microsoft Zira Desktop|en-US|Female|Adult\n\
                      Microsoft Hanhan|zh-TW|Female|Adult\n\
                      |zh-CN|Female|Adult\n\
                      broken line\n";
        let voices = parse_voices(stdout);
        let names: Vec<&str> = voices.iter().map(|v| v.info.name).collect();
        assert_eq!(names, vec!["Microsoft Huihui Desktop", "Microsoft Kangkang"], "{names:?}");
        assert_eq!(voices[0].info.id, "system-microsoft-huihui-desktop");
        assert_eq!(voices[1].info.id, "system-microsoft-kangkang");
        assert_eq!(voices[0].info.language, "zh-CN");
        assert_eq!(voices[0].info.engine, ENGINE);
        assert!(voices[0].info.installed, "a listed system voice can speak");
        assert_eq!(voices[0].info.gender, Gender::Female);
        assert_eq!(voices[1].info.gender, Gender::Male);
        // a duplicate name still gets a unique id
        let again = parse_voices("Microsoft Huihui Desktop|zh-CN|Female|Adult\nMicrosoft Huihui Desktop|zh-CN|Female|Adult\n");
        assert_eq!(again.len(), 2);
        assert_eq!(again[1].info.id, "system-microsoft-huihui-desktop-2");
        // an empty listing is the normal no-Chinese-voice case, not an error
        assert!(parse_voices("").is_empty());
    }

    #[test]
    fn slugs_are_safe_and_bounded() {
        assert_eq!(slug("Microsoft Huihui Desktop"), "microsoft-huihui-desktop");
        assert_eq!(slug("  A  B  "), "a-b");
        assert_eq!(slug("微软慧慧"), "voice");
        assert_eq!(slug(""), "voice");
    }

    /// Whitespace and `=` insensitive base64 decoder, the reference for the encoder.
    fn decode64(text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buf = 0u32;
        let mut bits = 0u32;
        for c in text.chars().filter(|c| !c.is_whitespace() && *c != '=') {
            let v = match c {
                'A'..='Z' => c as u32 - 'A' as u32,
                'a'..='z' => c as u32 - 'a' as u32 + 26,
                '0'..='9' => c as u32 - '0' as u32 + 52,
                '+' => 62,
                '/' => 63,
                _ => return out,
            };
            buf = (buf << 6) | v;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buf >> bits) as u8);
            }
        }
        out
    }

    #[test]
    fn the_encoded_command_is_utf16le_base64() {
        assert_eq!(encode_command(""), "");
        assert_eq!(decode64(&encode_command("hi")), b"h\0i\0");
        assert_eq!(decode64(&encode_command("echo 1")), "echo 1".encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>());
        // the script PowerShell receives is the script we wrote, byte for byte
        let round = decode64(&encode_command(LIST_SCRIPT));
        assert_eq!(round, LIST_SCRIPT.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>());
    }

    #[test]
    fn ssml_escapes_the_script_and_keeps_pauses_and_settings() {
        let s = ssml("你好 & <world> \"x\" [pause 500ms] 再见", &Params { semitones: 6.0, pace: 1.2 });
        assert!(s.starts_with("<speak version=\"1.0\""), "{s}");
        assert!(s.ends_with("</speak>"), "{s}");
        assert!(s.contains("xml:lang=\"zh-CN\""), "{s}");
        assert!(s.contains("<prosody rate=\"+20%\" pitch=\"+41%\">"), "{s}");
        assert!(s.contains("&amp; &lt;world&gt; &quot;x&quot;"), "{s}");
        assert!(s.contains("<break time=\"500ms\"/>"), "{s}");
        assert!(!s.contains("<world>"), "text is never markup: {s}");
        assert_eq!(ssml("a", &Params::default()).matches("<prosody").count(), 1);
        assert!(ssml("a", &Params::default()).contains("rate=\"+0%\" pitch=\"+0%\""), "{}", ssml("a", &Params::default()));
        // characters XML cannot carry are dropped instead of failing the request
        let hostile = ssml("a\u{0}b\u{1f}c", &Params::default());
        assert!(!hostile.contains('\u{0}') && !hostile.contains('\u{1f}'), "{hostile}");
        // pace and pitch saturate instead of overflowing
        let extreme = ssml("x", &Params { semitones: f64::NEG_INFINITY, pace: f64::MAX });
        assert!(extreme.contains("<prosody"), "{extreme}");
    }

    /// A WAV in the shape System.Speech writes: `fmt ` (18 bytes) then `data`, mono, 16-bit.
    fn wav(rate: u32, samples: &[i16]) -> Vec<u8> {
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36u32 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&18u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // mono
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 2).to_le_bytes()); // byte rate
        out.extend_from_slice(&2u16.to_le_bytes()); // block align
        out.extend_from_slice(&16u16.to_le_bytes()); // bits
        out.extend_from_slice(&0u16.to_le_bytes()); // cbSize
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&data);
        out
    }

    #[test]
    fn the_wav_is_decoded_to_samples() {
        let bytes = wav(24_000, &[0, 16_384, -32_768, 32_767]);
        let audio = decode_wav(&bytes).expect("a well-formed WAV decodes");
        assert_eq!(audio.sample_rate, 24_000);
        assert_eq!(audio.samples.len(), 4);
        assert!((audio.samples[1] - 0.5).abs() < 1e-6, "{:?}", audio.samples);
        assert!((audio.samples[2] + 1.0).abs() < 1e-6, "{:?}", audio.samples);
        assert!(audio.samples[3] < 1.0);
    }

    #[test]
    fn broken_wavs_are_errors_not_panics() {
        assert!(decode_wav(b"").is_err());
        assert!(decode_wav(b"not a wav at all").is_err());
        assert!(decode_wav(b"RIFF\x00\x00\x00\x00WAVE").is_err(), "no format chunk");
        let good = wav(24_000, &[1, 2, 3]);
        for cut in 0..good.len() {
            let _ = decode_wav(good.get(..cut).unwrap_or_default());
        }
        // a declared chunk longer than the file
        let mut lying = good.clone();
        lying[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_wav(&lying).is_err());
        // stereo, 8-bit and a nonsensical rate are refused with a message, never misread
        let mut stereo = wav(24_000, &[1, 2]);
        stereo[22..24].copy_from_slice(&2u16.to_le_bytes());
        assert!(decode_wav(&stereo).is_err());
        let mut eight = wav(24_000, &[1, 2]);
        eight[34..36].copy_from_slice(&8u16.to_le_bytes());
        assert!(decode_wav(&eight).is_err());
        let empty = wav(24_000, &[]);
        assert!(decode_wav(&empty).is_err());
        let bad_rate = wav(0, &[1]);
        assert!(decode_wav(&bad_rate).is_err());
    }

    #[test]
    fn the_timeout_grows_with_the_script_and_is_capped() {
        #[cfg(all(windows, not(target_arch = "wasm32")))]
        {
            assert_eq!(timeout_for(0).as_secs(), MIN_TIMEOUT_SECS);
            assert!(timeout_for(6_000) > timeout_for(0));
            assert!(timeout_for(usize::MAX).as_secs() <= MAX_TIMEOUT_SECS);
        }
    }

    /// The real thing: if this machine has a zh-CN SAPI voice, the listed voice must speak Chinese and
    /// honour pace and pitch. Skips (like the oracle tests) when the machine has none.
    #[test]
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    fn a_listed_zh_cn_voice_speaks_chinese() {
        let Some(voice) = voices().first().copied() else {
            eprintln!("no zh-CN system voice on this machine: skipping");
            return;
        };
        assert_eq!(voice.info().language, "zh-CN");
        let text = "你好，这是中文配音测试。";
        let audio = voice.synthesize(text, &Params::default()).expect("a listed system voice speaks");
        assert_eq!(audio.sample_rate, crate::SAMPLE_RATE, "we ask SAPI for 24 kHz");
        assert!(audio.samples.len() > (crate::SAMPLE_RATE as usize) / 4, "{} samples", audio.samples.len());
        assert!(audio.samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        let peak = audio.samples.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.01, "the audio is silent (peak {peak})");
        // pace is honoured (SSML rate) …
        let fast = voice.synthesize(text, &Params { pace: 1.5, ..Default::default() }).expect("pace 1.5 speaks");
        assert!(fast.samples.len() < audio.samples.len(), "pace 1.5: {} vs {}", fast.samples.len(), audio.samples.len());
        // … and the vocal pitch setting reaches the engine (the samples differ, the length does not)
        let higher = voice.synthesize(text, &Params { semitones: 6.0, ..Default::default() }).expect("+6 semitones speaks");
        assert_eq!(higher.samples.len(), audio.samples.len(), "pitch keeps the length");
        assert!(higher.samples != audio.samples, "+6 semitones changed nothing");
    }
}
