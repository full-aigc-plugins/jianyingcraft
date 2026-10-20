# 技能实跑：ASR、批量混剪 + ffprobe、图文成片 + 中文配音

> 状态：**流程文档，尚未执行**。本文把三条"因为本机缺模型/缺 ffprobe/缺中文音色而没跑"
> 的技能变成可复制步骤。命令都已在 2026-10-20 的 Windows 工作树上核对过 `--help`
> 或实际跑过；没跑过的步骤单独标注。

## 1. 这条能证明什么

三条独立验收，分别对应 craft 任务（都在
`jianyingcraft/openspec/changes/derive-from-filmcraft/tasks.md`）：

| 验收 | openspec 位置 | 命题 |
|---|---|---|
| A. 语音转写 | craft `tasks.md` 任务 **#478**（`transcript.generate` 未覆盖条） | `transcript.generate` 能用真实 Whisper 模型把中文语音转成词级 transcript，并生成字幕 |
| B. 批量混剪 + 外部 ffprobe | 同上任务 **#484**（批量混剪未覆盖条） | N 条素材套同一模板替换、逐条导出、用**外部 ffprobe** 对账，失败条目留在清单里 |
| C. 图文成片 + 中文 TTS | 同上任务 **#490**（图文成片未覆盖条） | 中文配音驱动竖屏成片；配音时长决定分镜/字幕/总时长；导出 1080×1920 |

**判定标准**：

- A：`transcript.models` 显示模型已安装；`transcript.generate` 产出 transcript；
  `transcript.inspect` 词数 > 0、语言为 `zh`；`transcript.createCaptions` 生成字幕。
- B：每条输出文件存在、`offlineMedia: []`、外部 `ffprobe` 能读出 h264 + 正确画幅/帧率/时长；
  清单里成功 + 失败 == 计划条数。
- C：中文配音 WAV 非空、时长可测；成片 `ffprobe` 为 1080×1920、30 fps、有音频；
  字幕条数与分镜数一致；总时长与配音总时长误差 ≤ 1 帧。

> 三条都不需要剪映 GUI；这是**引擎/CLI 能力**的验收，不是剪映渲染验收。

## 2. 前置条件与逐项检查

| 依赖 | 用途 | 检查命令 | 期望 |
|---|---|---|---|
| `filmcraft-cli` | craft 侧 ASR / 混剪 / TTS | `filmcraft-cli --version` | `filmcraft-cli 0.8.0`（以实际为准） |
| `jianying` CLI | CLI 侧中文 TTS / ASR / 代理渲染 | `jianying --version` | `jianying 1.7.0`（以实际为准） |
| `ffprobe` / `ffmpeg` | 外部对账、代理渲染 | `ffprobe -version` / `ffmpeg -version` | 有版本 |
| Whisper 模型 | craft 内置下载，或 whisper.cpp GGML | 见 §3.2 | 安装后 `installed: true` |
| 中文音色 | Windows SAPI / macOS `say` | 见 §3.4 | 至少一个 `zh-CN` 音色 |
| Python | 解析 JSON、写 SRT | `python --version` / `python3 --version` | 3.10+ |
| 账号 | **不需要**（只用本地模型/系统音色） | — | 不调用付费云 TTS/ASR |

**环境变量（PowerShell）**

```powershell
$CRAFT = "E:\workspaces\workspace-agent-skills\full-aigc-plugins-repositories\jianyingcraft"
$CLI   = "E:\workspaces\workspace-agent-skills\full-aigc-plugins-repositories\jianying-cli"
$FC    = Join-Path $CRAFT "target\release\filmcraft-cli.exe"   # 见 §3.2 的 whisper 构建
$JY    = Join-Path $CLI "target\debug\jianying.exe"
$WS    = Join-Path $env:TEMP "jy-skill-reruns"
New-Item -ItemType Directory -Force -Path $WS | Out-Null
& $FC --version
& $JY --version
```

```sh
CRAFT="$HOME/workspaces/full-aigc-plugins-repositories/jianyingcraft"
CLI="$HOME/workspaces/full-aigc-plugins-repositories/jianying-cli"
FC="$CRAFT/target/release/filmcraft-cli"
JY="$CLI/target/debug/jianying"
WS="${TMPDIR:-/tmp}/jy-skill-reruns"
mkdir -p "$WS"
"$FC" --version
"$JY" --version
```

> `JIANYING_CLI_PROCESS_LIST` 只在 `--help` / 只读检查时指向空文件；真实写草稿前不要设置，
> 否则会绕过"剪映正在运行就拒绝写入"的保护。
>
> PowerShell 5.1 会把**无 BOM 的 UTF-8 `.ps1`** 按系统 ANSI 解码；含中文参数的块请直接
> 粘到控制台，或把脚本保存为 **UTF-8 with BOM**（`[IO.File]::WriteAllText` 默认无 BOM，
> 所以本文把长中文文案放进 `ttv.txt` 用 `--text-file` 传入，避免源代码里的中文被误解码）。

### 2.1 中文音色检查

Windows：

```powershell
Add-Type -AssemblyName System.Speech
$s = New-Object System.Speech.Synthesis.SpeechSynthesizer
$s.GetInstalledVoices() | ForEach-Object { "$($_.VoiceInfo.Name) | $($_.VoiceInfo.Culture.Name)" }
```

本机实测输出：

```text
Microsoft Huihui Desktop | zh-CN
Microsoft Zira Desktop | en-US
```

macOS：

```sh
say -v '?' | grep -i zh
```

期望至少有一个 `zh_CN` / `zh-CN` 音色（例如 `Tingting`）。没有就去
系统设置 → 辅助功能 → 朗读内容 → 系统声音 → 管理声音，下载中文音色。

## 3. 步骤

### 3.1 公共：确认命令面

工作目录：`$CRAFT`。本文所有 `--out` / `--save-as` 目标都必须是**不存在**的路径；
重跑前删掉旧文件/目录，或换一个新名字，否则 `project init` / `quickstart` 会拒绝。

```powershell
& $FC describe transcript.generate
& $FC describe transcript.createCaptions
& $FC describe tts.create
& $FC --demo --compact exec transcript.models
& $FC --demo --compact exec tts.voices
```

```sh
"$FC" describe transcript.generate
"$FC" describe transcript.createCaptions
"$FC" describe tts.create
"$FC" --demo --compact exec transcript.models
"$FC" --demo --compact exec tts.voices
```

未启用 Whisper 时 `transcript.generate` 是 `enabled: false`，`transcript.models` 是
`available: false`；`tts.voices` 只有 `en-US`。这是本机 0.8.0 debug 二进制的真实状态
（`models` 节选，实际还有 `whisper-small`）：

```json
{
  "available": false,
  "default": "whisper-base",
  "dir": "C:\\Users\\me\\AppData\\Roaming\\JianyingCraft\\models",
  "models": [
    {
      "id": "whisper-tiny",
      "installed": false,
      "multilingual": true,
      "name": "Whisper tiny (multilingual)",
      "size": 153547868
    },
    {
      "id": "whisper-base",
      "installed": false,
      "multilingual": true,
      "name": "Whisper base (multilingual)",
      "size": 292890192
    }
  ]
}
```

`tts.voices` 的 `languages` **当时**只有 `en-US`；这正是任务 490 的缺口（上面的实测记录保留不动），
不要用 CLI 的中文 TTS 把它掩盖过去（见 §7）。

> **2026-10-20 更新**：装有 Windows 中文 SAPI 音色的机器上，`tts.voices` 的 `languages` 至少包含
> `en-US` 与 `zh-CN`，`voices` 里有 `engine: "system"`、`installed: true` 的真实条目（本机为
> `system-microsoft-huihui-desktop` = `Microsoft Huihui Desktop`）；没有中文系统音色的机器会列
> `zh-cn-placeholder`（`engine: "placeholder"`, `installed: false`）并在选中时明确报错。
> craft 自身现在能出声，但**发音质量未验证**（见 §7）。

### 3.2 A：`transcript.generate`（craft，真实 Whisper）

#### 3.2.1 带 Whisper 重新构建

工作目录：`$CRAFT`

```powershell
cargo build --release -p filmcraft-cli --features whisper
& $FC describe transcript.generate
```

```sh
cargo build --release -p filmcraft-cli --features whisper
"$FC" describe transcript.generate
```

`--features whisper` 在 `apps/filmcraft-cli/Cargo.toml` 里定义为
`filmcraft-engine/whisper` + `filmcraft-engine/speech-download`；它同时启用模型下载。
首次构建会编译 candle，时间明显长于普通构建。

期望 `describe` 的 `enabled` 变成 `true`。如果仍是 `false`，说明构建没有带上 feature，
不要继续。

#### 3.2.2 下载模型

工作目录：`$CRAFT`（模型落在用户 data dir，不写仓库）

```powershell
& $FC --demo --compact exec transcript.downloadModel model=whisper-base
& $FC --demo --compact exec transcript.models
```

```sh
"$FC" --demo --compact exec transcript.downloadModel model=whisper-base
"$FC" --demo --compact exec transcript.models
```

期望 `whisper-base` 的 `installed: true`；`available: true`。模型约 293 MB，
下载失败会报网络/哈希错误，不要用不完整文件继续。

#### 3.2.3 导入真实语音并转写

准备一段 5–15 秒、16 kHz 或 44.1 kHz 的中文 WAV（`$WS\speech.wav`）。
`import` 返回新素材项 id：

```powershell
& $FC --demo --save-as "$WS\asr.fcproj" import "$WS\speech.wav"
& $FC --project "$WS\asr.fcproj" --save exec transcript.generate 'items=[48]' 'language=zh' 'model=whisper-base'
& $FC --project "$WS\asr.fcproj" exec transcript.inspect
```

```sh
"$FC" --demo --save-as "$WS/asr.fcproj" import "$WS/speech.wav"
"$FC" --project "$WS/asr.fcproj" --save exec transcript.generate 'items=[48]' 'language=zh' 'model=whisper-base'
"$FC" --project "$WS/asr.fcproj" exec transcript.inspect
```

`import` 实测输出（id 会不同，**把 48 换成你的实际 id**）：

```json
{
  "errors": [],
  "items": [48]
}
```

`transcript.inspect` 的字段名来自已有 `transcript.set` 实跑；`generate` 的 `source` 会是
`whisper`，词数取决于音频。示意：

```json
{
  "current": 0,
  "items": [
    {
      "item": 48,
      "language": "zh",
      "source": "whisper",
      "speakers": ["Speaker 1"],
      "words": 42
    }
  ]
}
```

断言（不要只看退出码）：

```powershell
$tr = & $FC --project "$WS\asr.fcproj" exec transcript.inspect | ConvertFrom-Json
if ($tr.items[0].words -le 0) { throw "no words transcribed" }
if ($tr.items[0].language -ne "zh") { throw "unexpected language: $($tr.items[0].language)" }
```

```sh
"$FC" --project "$WS/asr.fcproj" exec transcript.inspect > "$WS/transcript.json"
python3 -c "import json; d=json.load(open('$WS/transcript.json')); assert d['items'][0]['words']>0; assert d['items'][0]['language']=='zh'"
```

#### 3.2.4 生成字幕并核对

```powershell
& $FC --project "$WS\asr.fcproj" --save exec transcript.createCaptions 'maxChars=18' 'lines=1'
& $FC --project "$WS\asr.fcproj" exec captions.list
```

```sh
"$FC" --project "$WS/asr.fcproj" --save exec transcript.createCaptions 'maxChars=18' 'lines=1'
"$FC" --project "$WS/asr.fcproj" exec captions.list
```

期望 `createCaptions` 返回 `captions` > 0（已有导入路径实测是
`{"captions":2,"track":48}` 形状）；`captions.list` 的轨道里有对应条数。

#### 3.2.5 备选：`jianying media transcribe`（whisper.cpp）

如果不想给 craft 加 feature，可以用 CLI 的固定 whisper.cpp 适配器。需要官方
`whisper-cli` 可执行文件和一份本地审核过的 GGML/GGUF 模型；两者都必须是**绝对路径**。

```powershell
& $JY media transcribe "$WS\speech.wav" --out "$WS\speech.srt" `
  --executable "$WHISPER\whisper-cli.exe" --model "$MODELS\ggml-base.bin" `
  --model-id "whisper-base" --format srt --language zh `
  --task-id "asr-canary-20261020" --plan --json
```

```sh
"$JY" media transcribe "$WS/speech.wav" --out "$WS/speech.srt" \
  --executable "$WHISPER/whisper-cli" --model "$MODELS/ggml-base.bin" \
  --model-id "whisper-base" --format srt --language zh \
  --task-id "asr-canary-20261020" --plan --json
```

`--plan` 只校验身份并打印幂等执行计划，不启动 whisper；期望退出 0、没有子进程。
确认后再去掉 `--plan` 实跑，产物 `$WS/speech.srt`。然后导入草稿：

```powershell
$DRAFT = Join-Path $WS "asr-cli-draft"
& $JY project init --out $DRAFT "ASR CLI" --json
& $JY captions import-srt $DRAFT "$WS\speech.srt" --json
& $JY captions list $DRAFT --json
```

```sh
DRAFT="$WS/asr-cli-draft"
"$JY" project init --out "$DRAFT" "ASR CLI" --json
"$JY" captions import-srt "$DRAFT" "$WS/speech.srt" --json
"$JY" captions list "$DRAFT" --json
```

期望 `captions import-srt` 返回 `imported` > 0；`captions list` 里文本能正常读出中文
（UTF-8 无 BOM）。`media transcribe` 失败时先看 §6 的身份错误，不要改 `--model-id` 去"试"。

### 3.3 B：批量混剪 + 外部 ffprobe

#### 3.3.1 单条冒烟（本机实测）

工作目录：`$WS`。用 craft 内置 demo 当模板（真实项目把 `--demo` 换成 `--project`）。

```powershell
& $FC --demo --compact exec file.saveCopy "path=$WS\template.fcproj"
& $FC --project "$WS\template.fcproj" --save --compact exec clip.replaceFromBin 'clips=[29]' 'item=6'
& $FC --project "$WS\template.fcproj" export "$WS\remix-1s.mp4" --range custom --start 0 --end 1
```

```sh
"$FC" --demo --compact exec file.saveCopy "path=$WS/template.fcproj"
"$FC" --project "$WS/template.fcproj" --save --compact exec clip.replaceFromBin 'clips=[29]' 'item=6'
"$FC" --project "$WS/template.fcproj" export "$WS/remix-1s.mp4" --range custom --start 0 --end 1
```

实测输出：

```json
{"backup":null,"bytes":28259,"ms":4.7348,"path":"C:\\Users\\me\\AppData\\Local\\Temp\\jy-skill-reruns\\template.fcproj"}
```

```json
{"clips":[29,30],"item":6,"short":[]}
```

`clips` 里出现 `30` 是**联动音频**一起被替换，符合技能里"替换画面会带走关联音频"的描述。

```json
{
  "job": 1,
  "offlineMedia": [],
  "path": "C:\\Users\\me\\AppData\\Local\\Temp\\jy-skill-reruns\\remix-1s.mp4",
  "result": {
    "bytes": 2481011,
    "frames": 24,
    "path": "C:\\Users\\me\\AppData\\Local\\Temp\\jy-skill-reruns\\remix-1s.mp4",
    "render_fps": 15.026904733744221,
    "seconds": 1.5971353
  }
}
```

#### 3.3.2 外部 ffprobe 对账

```powershell
ffprobe -v error -select_streams v:0 `
  -show_entries stream=codec_name,width,height,avg_frame_rate,nb_frames `
  -show_entries format=duration,size -of json "$WS\remix-1s.mp4"
```

```sh
ffprobe -v error -select_streams v:0 \
  -show_entries stream=codec_name,width,height,avg_frame_rate,nb_frames \
  -show_entries format=duration,size -of json "$WS/remix-1s.mp4"
```

期望（示意，值随模板变化）：

```json
{
  "programs": [],
  "streams": [
    {
      "codec_name": "h264",
      "width": 1920,
      "height": 1080,
      "avg_frame_rate": "24000/1001",
      "nb_frames": "24"
    }
  ],
  "format": {
    "duration": "1.001000",
    "size": "2481011"
  }
}
```

音频也要查：

```powershell
ffprobe -v error -select_streams a:0 -show_entries stream=codec_name,channels,sample_rate -of json "$WS\remix-1s.mp4"
```

期望至少有一个 `codec_name: "aac"` 的音频流；`offlineMedia` 非空时不要交付。

#### 3.3.3 N 条循环与清单

工作目录：`$WS`。模板里的占位片段 id 和素材 bin id 要按你的工程替换；
下面用 demo 的 `clip 29` / `item 6` 只是可跑通的例子。

```powershell
$Template = "$WS\template.fcproj"
$MediaDir = "$WS\media"
$OutDir   = "$WS\renders"
New-Item -ItemType Directory -Force $OutDir | Out-Null
$rows = @()
$materials = Get-ChildItem $MediaDir -File | Sort-Object Name
foreach ($m in $materials) {
  $copy = Join-Path $OutDir ("{0}.fcproj" -f $m.BaseName)
  Copy-Item $Template $copy
  $imp = & $FC --project $copy --save import $m.FullName | ConvertFrom-Json
  $item = $imp.items[0]
  $seq = & $FC --project $copy --compact inspect sequence | ConvertFrom-Json
  $placeholder = $seq.video[0].items[0].clip
  $rep = & $FC --project $copy --save --compact exec clip.replaceFromBin "clips=[$placeholder]" "item=$item" | ConvertFrom-Json
  $out = Join-Path $OutDir ("{0}.mp4" -f $m.BaseName)
  $exp = & $FC --project $copy export $out | ConvertFrom-Json
  $probe = ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,avg_frame_rate -show_entries format=duration,size -of json $out | ConvertFrom-Json
  $rows += [pscustomobject]@{
    output = $out; source = $m.Name; template = $Template
    frames = $exp.result.frames; offline = @($exp.offlineMedia).Count
    codec = $probe.streams[0].codec_name; width = $probe.streams[0].width
    height = $probe.streams[0].height; duration = $probe.format.duration
    status = "ok"; error = ""
  }
}
[IO.File]::WriteAllText("$OutDir\ledger.json", ($rows | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
$rows | Format-Table -AutoSize
```

失败条目不要从清单里删：把 `status` 写成 `failed`、`error` 写原文，继续下一条。
`N > 素材数` 时按实际数量跑，缺的写 `skipped`。

对账断言：

```powershell
$plan = $materials.Count
$ok = @($rows | Where-Object status -eq "ok").Count
$bad = @($rows | Where-Object status -ne "ok").Count
if ($ok + $bad -ne $plan) { throw "ledger lost entries" }
foreach ($r in $rows | Where-Object status -eq "ok") {
  if (-not (Test-Path $r.output)) { throw "missing $($r.output)" }
  if ((Get-Item $r.output).Length -le 0) { throw "empty $($r.output)" }
  if ($r.offline -ne 0) { throw "offline media in $($r.output)" }
  if ($r.codec -ne "h264") { throw "unexpected codec $($r.codec)" }
}
```

完整时长对账要导出**整段**（不要 `--end 2`），然后比较
`$exp.result.frames` 与模板 `sequence.inspect` 的 `durationFrames`，误差 ≤ 1 帧。

### 3.4 C：图文成片 + 中文配音

#### 3.4.1 craft 侧：英文 TTV 冒烟（本机实测，证明管线）

```powershell
& $FC --compact exec file.newProject 'name=TTV' --save-as "$WS\ttv.fcproj"
& $FC --project "$WS\ttv.fcproj" --save --compact exec file.newSequence 'name=TTV' 'width=1080' 'height=1920' 'fps=30'
& $FC --project "$WS\ttv.fcproj" --save --compact exec tts.create 'text=Hello world' 'voice=basic-female'
& $FC --project "$WS\ttv.fcproj" export "$WS\ttv.mp4"
```

前两步（工程 + 竖屏序列）也可以一条命令完成（#84 `project.quickShortVideo`，旁白走 `narration.text`、
空字幕轨走 `captions=true`）：

```powershell
& $FC --compact exec project.quickShortVideo 'name=TTV' 'width=1080' 'height=1920' 'fps=30' 'narration.text=Hello world' --save-as "$WS\ttv.fcproj"
& $FC --project "$WS\ttv.fcproj" export "$WS\ttv.mp4"
```

```sh
"$FC" --compact exec file.newProject 'name=TTV' --save-as "$WS/ttv.fcproj"
"$FC" --project "$WS/ttv.fcproj" --save --compact exec file.newSequence 'name=TTV' 'width=1080' 'height=1920' 'fps=30'
"$FC" --project "$WS/ttv.fcproj" --save --compact exec tts.create 'text=Hello world' 'voice=basic-female'
"$FC" --project "$WS/ttv.fcproj" export "$WS/ttv.mp4"
```

实测输出：

```json
{"sequence":7}
```

```json
{
  "clip": 9,
  "duration": 198386496000,
  "item": 8,
  "path": "C:\\Users\\me\\AppData\\Local\\Temp\\jy-skill-reruns\\Narration 1.wav",
  "sampleRate": 24000,
  "speechDuration": 198386496000,
  "start": 0,
  "track": "A1"
}
```

```json
{
  "job": 1,
  "offlineMedia": [],
  "path": "C:\\Users\\me\\AppData\\Local\\Temp\\jy-skill-reruns\\ttv.mp4",
  "result": { "bytes": 49756, "frames": 24, "path": "C:\\Users\\me\\AppData\\Local\\Temp\\jy-skill-reruns\\ttv.mp4" }
}
```

#### 3.4.2 中文 TTS 的负向对照（很重要）

```powershell
& $FC --project "$WS\ttv.fcproj" --save --compact exec tts.create 'text=你好世界' 'voice=basic-female'
```

```sh
"$FC" --project "$WS/ttv.fcproj" --save --compact exec tts.create 'text=你好世界' 'voice=basic-female'
```

实测失败（退出码 1）：

```text
filmcraft-cli: tts.create: invalid parameters for `tts.create`: the script has nothing to say
```

这条负向对照证明：**当时的** craft 没有中文音色/中文归一化，所以 490 的中文配音不能靠 craft
自身完成。下面用 CLI 的中文 TTS 完成"成片"这一半，并在证据里分开写"CLI 中文配音通过"与
"craft 中文音色"。

> **2026-10-20 更新**（同一台机器，当前二进制）：`tts.voices` 真实列出
> `system-microsoft-huihui-desktop`（`language: "zh-CN"`, `engine: "system"`），
> `tts.create voice=system-microsoft-huihui-desktop` 产出 4.23 s / 24000 Hz / 406 KB 的非静音 WAV，
> `Narration.language` 记为 `zh-CN`。上面那条 `voice=basic-female` 的报错也改了：
> `the 'basic-female' voice speaks English and cannot pronounce Chinese ('你')`，不再是
> `the script has nothing to say`。**发音是否正确/自然仍未验证**。

#### 3.4.3 CLI 侧：中文配音 + 竖屏成片

工作目录：`$WS`

1. 建一个只放配音的草稿，合成中文 WAV：

```powershell
$TextPath = "$WS\ttv.txt"
# 先用编辑器把中文文案写进 $TextPath（UTF-8，无 BOM）；不要把长中文直接写进 .ps1
if (-not (Test-Path $TextPath)) { throw "write your Chinese copy to $TextPath first" }
& $JY project init --out "$WS\voice-draft" "Voice Draft" --json
$tts = & $JY media tts "$WS\voice-draft" 0s --text-file $TextPath `
  --provider system-windows --voice "Microsoft Huihui Desktop" --json | ConvertFrom-Json
$DUR   = [double]$tts.data.duration_us / 1000000.0
$VOICE = $tts.data.path
$DUR
$VOICE
```

```sh
"$JY" project init --out "$WS/voice-draft" "Voice Draft" --json
"$JY" media tts "$WS/voice-draft" 0s --text "你好，世界。这是中文配音验收。" \
  --provider system-macos --voice "Tingting" --json > "$WS/tts.json"
DUR=$(python3 -c "import json; print(json.load(open('$WS/tts.json'))['data']['duration_us']/1000000)")
VOICE=$(python3 -c "import json; print(json.load(open('$WS/tts.json'))['data']['path'])")
echo "$DUR"; echo "$VOICE"
```

实测（Windows，3 秒 shim 时长；真实 ffprobe 会给出实际时长）：

```json
{
  "ok": true,
  "data": {
    "ok": true,
    "segment_id": "c816fcfc4ed54ad5b7e335d1b7d9ee5f",
    "material_id": "32c9d100f3e34a70a4174d1635f9e8fa",
    "track_id": "0d9fbb2b49464d939c4d354390503a64",
    "path": "C:\\Users\\me\\AppData\\Local\\Temp\\jy-skill-reruns\\voice-draft\\assets\\audio\\voiceover.wav",
    "start_us": 0,
    "duration_us": 3000000,
    "bytes": 272004,
    "text_chars": 15,
    "provider": "system-windows",
    "format": "wav",
    "model": null,
    "voice": "Microsoft Huihui Desktop",
    "duration_source": "ffprobe"
  }
}
```

`duration_source` 必须是 `ffprobe`（省略 `DURATION` 参数时），这样后续分镜用的是
真实音频时长，不是估算。用 `data.path`，不要用 `media_probe.path`（那是事务工作副本，
提交后不存在）。

2. 按配音时长生成竖屏背景（大小 1080×1920，30 fps）：

```powershell
ffmpeg -v error -y -f lavfi -i "color=c=0x101820:s=1080x1920:r=30:d=$DUR" `
  -c:v libx264 -pix_fmt yuv420p "$WS\bg.mp4"
```

```sh
ffmpeg -v error -y -f lavfi -i "color=c=0x101820:s=1080x1920:r=30:d=$DUR" \
  -c:v libx264 -pix_fmt yuv420p "$WS/bg.mp4"
```

3. 用同一个时长写 SRT（单段最小示例；真实内容按句拆成多段）：

```powershell
@'
import sys
text = open(sys.argv[1], encoding="utf-8").read().strip()
dur = float(sys.argv[2])
h = int(dur // 3600); m = int(dur % 3600 // 60); s = dur % 60
stamp = f"{h:02d}:{m:02d}:{s:06.3f}".replace(".", ",")
open(sys.argv[3], "w", encoding="utf-8", newline="\n").write(
    f"1\n00:00:00,000 --> {stamp}\n{text}\n")
print(stamp)
'@ | python - $TextPath $DUR "$WS\ttv.srt"
```

```sh
python3 - "$WS/ttv.srt" "$DUR" <<'PY'
import sys
path, dur = sys.argv[1], float(sys.argv[2])
h = int(dur // 3600); m = int(dur % 3600 // 60); s = dur % 60
stamp = f"{h:02d}:{m:02d}:{s:06.3f}".replace(".", ",")
open(path, "w", encoding="utf-8", newline="\n").write(
    f"1\n00:00:00,000 --> {stamp}\n你好，世界。这是中文配音验收。\n")
print(stamp)
PY
```

4. 一次性建竖屏草稿（视频 + 中文配音 + 字幕），绝对路径：

```powershell
& $JY project quickstart --out "$WS\ttv-cli" "TTV Chinese" `
  --video "$WS\bg.mp4" --audio $VOICE --srt "$WS\ttv.srt" `
  --width 1080 --height 1920 --fps 30 --json
& $JY project verify "$WS\ttv-cli" --json
```

```sh
"$JY" project quickstart --out "$WS/ttv-cli" "TTV Chinese" \
  --video "$WS/bg.mp4" --audio "$VOICE" --srt "$WS/ttv.srt" \
  --width 1080 --height 1920 --fps 30 --json
"$JY" project verify "$WS/ttv-cli" --json
```

5. 代理渲染（ffmpeg；预览级，不是剪映原生导出）：

```powershell
& $JY render proxy "$WS\ttv-cli" --out "$WS\ttv-cli.mp4" --scale 1 --burn-captions --json
```

```sh
"$JY" render proxy "$WS/ttv-cli" --out "$WS/ttv-cli.mp4" --scale 1 --burn-captions --json
```

6. 外部 ffprobe 复核竖屏、帧率、音频、时长：

```powershell
ffprobe -v error -show_entries stream=codec_type,codec_name,width,height,avg_frame_rate,sample_rate,channels `
  -show_entries format=duration,size -of json "$WS\ttv-cli.mp4"
```

```sh
ffprobe -v error -show_entries stream=codec_type,codec_name,width,height,avg_frame_rate,sample_rate,channels \
  -show_entries format=duration,size -of json "$WS/ttv-cli.mp4"
```

期望：

```json
{
  "streams": [
    {
      "codec_type": "video",
      "codec_name": "h264",
      "width": 1080,
      "height": 1920,
      "avg_frame_rate": "30/1"
    },
    {
      "codec_type": "audio",
      "codec_name": "aac",
      "sample_rate": "44100",
      "channels": 2
    }
  ],
  "format": { "duration": "3.000000", "size": "1234567" }
}
```

时长与 `$DUR` 误差 ≤ 1 帧（1/30 秒）。若背景视频比配音长，草稿总时长会跟着视频走；
用 `-t $DUR` 截断背景，或在 `quickstart` 后用 `timeline trim` 调整。

## 4. 期望结果（汇总）

| 检查 | 期望 |
|---|---|
| `transcript.generate` 的 `describe.enabled` | `true`（带 `--features whisper` 构建后） |
| `transcript.models` | `available: true`；所用模型 `installed: true` |
| `transcript.inspect` | `words > 0`，`language: "zh"`，`source: "whisper"` |
| `transcript.createCaptions` | `captions > 0` |
| `media transcribe --plan` | 退出 0，不启动 whisper |
| `captions import-srt` | `imported > 0`，中文不乱码 |
| `file.saveCopy` | 返回 `bytes > 0` 的 `.fcproj` 副本 |
| `clip.replaceFromBin` | `{"clips":[<占位>,<联动音频>],"item":<素材>,"short":[]}` |
| `export` | `offlineMedia: []`，`result.bytes > 0`，`result.frames > 0` |
| 外部 `ffprobe` | `h264`，正确宽高/帧率，`duration > 0`，有音频流 |
| `tts.create` 英文 | 返回 `clip`/`item`/`path`/`duration`，`offlineMedia: []` |
| `tts.create` 中文 | **失败**：`the script has nothing to say`（负向对照） |
| `media tts` 中文 | `provider` 正确、`voice` 正确、`duration_source: "ffprobe"`、`bytes > 0` |
| `project verify`（CLI 竖屏草稿） | `data.ok: true`，`issues: []` |
| `render proxy` | 退出 0，输出文件存在 |
| 竖屏 ffprobe | 1080×1920、30 fps、aac、时长 ≈ `$DUR` |

> `project verify` / `quickstart` 的退出码不可靠；一律解析 `data.ok`。
> `offlineMedia` 非空时不要交付，也不要用 `--allow-offline` 掩盖。

## 5. 证据怎么留

### 5.1 主证据：`docs/runbooks/evidence/SKILL_RERUNS_20261020.json`

```json
{
  "schema": "jianyingcraft-skill-rerun/v1",
  "observed_at": "2026-10-20T15:30:00+08:00",
  "host": {
    "os": "windows",
    "version": "10.0.26100",
    "cpu": "x86_64"
  },
  "craft_cli": {
    "version": "0.8.0",
    "features": ["whisper"],
    "binary_sha256": "1111111111111111111111111111111111111111111111111111111111111111"
  },
  "jianying_cli": {
    "version": "1.7.0",
    "binary_sha256": "2222222222222222222222222222222222222222222222222222222222222222"
  },
  "asr": {
    "path": "craft transcript.generate",
    "model": "whisper-base",
    "model_installed": true,
    "source_wav": "C:/Users/me/AppData/Local/Temp/jy-skill-reruns/speech.wav",
    "source_sha256": "3333333333333333333333333333333333333333333333333333333333333333",
    "words": 42,
    "language": "zh",
    "source": "whisper",
    "captions": 3,
    "result": "passed"
  },
  "batch": {
    "template": "C:/Users/me/AppData/Local/Temp/jy-skill-reruns/template.fcproj",
    "planned": 2,
    "items": [
      {
        "output": "C:/Users/me/AppData/Local/Temp/jy-skill-reruns/renders/a.mp4",
        "source": "a.mp4",
        "frames": 642,
        "offline_media": 0,
        "probe": {
          "codec": "h264",
          "width": 1920,
          "height": 1080,
          "fps": "24000/1001",
          "duration": 26.759
        },
        "status": "ok",
        "error": ""
      }
    ],
    "result": "passed"
  },
  "text_to_video": {
    "provider": "system-windows",
    "voice": "Microsoft Huihui Desktop",
    "text_chars": 15,
    "audio_duration_us": 3000000,
    "duration_source": "ffprobe",
    "video": {
      "path": "C:/Users/me/AppData/Local/Temp/jy-skill-reruns/ttv-cli.mp4",
      "width": 1080,
      "height": 1920,
      "fps": "30/1",
      "duration": 3.0,
      "has_audio": true
    },
    "captions": 1,
    "result": "passed"
  },
  "negative_controls": {
    "craft_chinese_tts": "the script has nothing to say"
  },
  "boundaries": [
    "ASR 只覆盖一段清晰中文语音，未测 WER/噪声",
    "批量混剪只跑 1-2 条，未测 50 条上限",
    "中文配音只用系统音色，未评价自然度",
    "craft tts.voices 仍只有 en-US；490 的原生中文音色未完成"
  ],
  "result": "passed"
}
```

### 5.2 可选：CLI 侧 ASR 证据 `provenance/ASR_RUN_20261020.json`

```json
{
  "schema": "jianying-asr-run/v1",
  "observed_at": "2026-10-20T15:30:00+08:00",
  "task_id": "asr-canary-20261020",
  "whisper": {
    "executable": "C:/tools/whisper.cpp/whisper-cli.exe",
    "model": "C:/models/ggml-base.bin",
    "model_id": "whisper-base",
    "model_sha256": "4444444444444444444444444444444444444444444444444444444444444444"
  },
  "source": {
    "path": "C:/Users/me/AppData/Local/Temp/jy-skill-reruns/speech.wav",
    "sha256": "3333333333333333333333333333333333333333333333333333333333333333"
  },
  "transcript": {
    "path": "C:/Users/me/AppData/Local/Temp/jy-skill-reruns/speech.srt",
    "format": "srt",
    "bytes": 321,
    "sha256": "5555555555555555555555555555555555555555555555555555555555555555"
  },
  "ledger": {
    "state_root": ".jianying-asr",
    "reused": false
  },
  "result": "passed"
}
```

原始产物（`speech.wav`、`speech.srt`、`renders/*.mp4`、`ttv-cli.mp4`）留在仓库外，
证据里写路径 + SHA-256；不要提交大媒体。

> 示例里的 SHA-256、路径、词数、帧数、时长都是占位；必须用实际文件、
> 实际 `ffprobe` 和实际 `transcript.inspect` 的值替换。`result: "passed"` 只在三条
> 验收都实际跑通后写。

## 6. 失败时先看这 5 种

| 现象 | 含义 | 处理 |
|---|---|---|
| `speech-to-text is not available in this build` | `filmcraft-cli` 没带 `whisper` feature | `cargo build --release -p filmcraft-cli --features whisper`；再 `describe transcript.generate` |
| `transcript.downloadModel` 仍 `enabled: false` | 同上；CLI 的下载 feature 随 `whisper` 开启 | 重构建；确认 `apps/filmcraft-cli/Cargo.toml` 的 feature 定义 |
| `whisper-cli executable must be an absolute existing file` / `model must be ...` | `media transcribe` 的 `--executable` / `--model` 不是绝对路径或文件不存在 | 用绝对路径；先 `whisper-cli --help`；模型文件必须真实存在 |
| `ffprobe not found on PATH` | 没有 ffprobe | 安装 ffmpeg；`jianying doctor --json` 的 `ffprobe` 应为非 null |
| `the script has nothing to say` | craft 的 TTS 文本归一化没有中文；这是 490 的已知缺口 | 用 §3.4.3 的 CLI 中文 TTS；不要把 craft 的英文 TTS 冒充中文通过 |

其他值得记录的失败：

- `the requested duration exceeds probed media duration`：`--duration` 超过素材实际时长；
  用 `ffprobe` 实测，或省略 `DURATION`。
- `... has no audio stream` / `has no video stream`：素材类型不对；`ffprobe` 先确认。
- `render proxy` 报 `ffmpeg not found`：代理渲染依赖 ffmpeg；原生导出仍不可用。
- `transcript.inspect` 词数为 0：模型没装好/语言不匹配/音频没有人声；先看 `transcript.models`
  和 `media probe`，不要盲目重跑下载。
- 模型下载哈希不匹配：删除半成品重下；不要改 `model_id` 绕过校验。

## 7. 本文不证明什么

- **不证明 ASR 质量**。只验证"能转出词、语言对、能生成字幕"；没有 WER、噪声、口音、
  多人分离的评测。真实质量需要标注集和词错误率。
- **不证明 ffprobe 能判断创作正确性**。它只证明容器/流/画幅/帧率/时长；内容对不对
  仍需人看片。
- **不证明中文 TTS 自然度，也不证明发音正确**。2026-10-20 起 craft 能用 Windows 系统中文音色出声
  （断言只覆盖：非静音、时长 > 0.25 s、24000 Hz、pace 改变长度、音高设置改变波形、英文音色拒绝汉字），
  但没有耳测、没有 whisper 回读、没有多音色/多厂商对比，也没有中文归一化评测（数字/日期/多音字由
  SAPI 决定）。"能出声"不等于"读对了"。
- **不关闭 490 的 craft 原生中文音色**。craft `tts.voices` 仍然只有 `en-US`；
  §3.4.3 是 CLI 侧的中文配音，不等于 craft 的 TTS 引擎支持中文。
- **不证明 50 条批量规模**。只跑 1–2 条；并发、磁盘、超时、失败重试都没覆盖。
- **不证明剪映能打开这些草稿**。CLI 侧只做 `project verify` + 代理渲染；
  真机打开/播放/导出见 `macos-canary.md` 或对应的 Windows canary。

## 8. 中文配音的机器回读（2026-10-20 实跑）

人耳之外，多一层机器证据：`crates/tts/tests/system_zh_recognition.rs` 用 SAPI 合成一句中文，
写成 16-bit PCM WAV，再让 Windows 的 zh-CN 识别器（本机 `Microsoft Speech Recognizer 8.0`）
听写，并与原文比对。

```text
cargo test -p filmcraft-tts --test system_zh_recognition -- --nocapture
spoke: 你好，世界。今天天气很好。
heard: 你好世界
test result: ok. 1 passed
```

判定：识别出的汉字数 ≥4，且与原文共享 ≥ 一半汉字。**它证明"能被中文识别器听懂"，
不证明"自然/像真人"**；也没有多音色对比与噪声条件。机器上没有 zh-CN 识别器时用例会打印原因
跳过，不会静默通过。