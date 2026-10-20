//! 中文配音的**机器回读**：合成 → 写 WAV → 用 Windows 的 zh-CN 识别器听写 → 与原文比对。
//!
//! 这比"非静音"强：能证明 SAPI 真的把汉字读成了中文识别器能听懂的语音。
//! 仍不能证明"自然/像真人"——那需要人耳，见 `docs/runbooks/skill-reruns.md`。
//!
//! 没有中文系统音色或没有 zh-CN 识别器时明确 skip（打印原因），不静默通过。

#![cfg(all(windows, not(target_arch = "wasm32")))]

use std::process::Command;

use filmcraft_tts as tts;

fn b64(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// 写一个 16-bit PCM 单声道 WAV，供 `System.Speech` 读取。
fn write_wav(path: &std::path::Path, audio: &tts::Audio) -> std::io::Result<()> {
    let rate = audio.sample_rate;
    let data_len = (audio.samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in &audio.samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, out)
}

/// 用 PowerShell + `System.Speech` 的 zh-CN 识别器听写一个 WAV；识别器缺失时得到空串。
fn recognize(path: &std::path::Path) -> Option<String> {
    let escaped = path.display().to_string().replace('\'', "''");
    let script = format!(
        r#"Add-Type -AssemblyName System.Speech
$zh = [System.Speech.Recognition.SpeechRecognitionEngine]::InstalledRecognizers() | Where-Object {{ $_.Culture.Name -eq 'zh-CN' }} | Select-Object -First 1
if (-not $zh) {{ exit 0 }}
$engine = New-Object System.Speech.Recognition.SpeechRecognitionEngine($zh)
$engine.LoadGrammar((New-Object System.Speech.Recognition.DictationGrammar))
$engine.SetInputToWaveFile('{escaped}')
$result = $engine.Recognize()
if ($result) {{ Write-Output $result.Text }}
"#
    );
    let utf16: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let output = Command::new("powershell").args(["-NoProfile", "-EncodedCommand", &b64(&utf16)]).output().ok()?;
    if !output.status.success() {
        eprintln!("recognizer failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn han(text: &str) -> Vec<char> {
    text.chars().filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c)).collect()
}

#[test]
fn the_synthesized_chinese_is_recognised_as_chinese() {
    let Some(listed) = tts::voices_in(None).into_iter().find(|v| v.engine == "system" && v.language == "zh-CN") else {
        eprintln!("skipped: no zh-CN SAPI voice on this machine");
        return;
    };
    let voice = tts::voice_in(listed.id, None).expect("the listed zh-CN voice resolves");
    let spoken = "你好，世界。今天天气很好。";
    let audio = voice.synthesize(spoken, &tts::Params::default()).expect("synthesize Chinese");
    let dir = std::env::temp_dir().join(format!("jc-zh-recognition-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("spoken.wav");
    write_wav(&path, &audio).expect("write wav");
    let heard = recognize(&path);
    let _ = std::fs::remove_dir_all(&dir);
    let Some(heard) = heard else {
        eprintln!("skipped: PowerShell could not run");
        return;
    };
    if heard.is_empty() {
        eprintln!("skipped: no zh-CN speech recognizer installed (the voice exists, the recognizer does not)");
        return;
    }
    eprintln!("spoke: {spoken}\nheard: {heard}");
    let spoken_han = han(spoken);
    let heard_han = han(&heard);
    assert!(heard_han.len() >= 4, "识别器没听出中文（{} 个汉字）：{heard}", heard_han.len());
    let shared = spoken_han.iter().filter(|c| heard.contains(**c)).count();
    assert!(shared >= spoken_han.len() / 2, "只有 {shared}/{} 个汉字被听写出来（原文 {spoken}，识别 {heard}）", spoken_han.len());
}
