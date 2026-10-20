# 技能实跑验证（智能字幕）

> 目的：证明 `jianyingcraft-captions` 里写的命令序列**真能跑通**，不只是 id 对得上。
> 环境：Windows，`target\debug\filmcraft-cli.exe`，`--demo` 内置示例工程（无 GUI）。
> 日期：2026-10-18

## 1. 用到的素材

`--demo` 工程的媒体项不落地文件，但结构完整：

| 项 | 值 |
|---|---|
| 工程 | JianyingCraft Demo（revision 2） |
| 活动序列 | 21（Main Edit，6801722928000 tick） |
| 音频媒体项 | **11** = `Ambient_Score.wav` |
| 音频轨 | Audio 1（6 段）、Audio 2（1 段）、Audio 3（空） |

## 2. 脚本（`_ref/captions-script.jsonl`）

```jsonl
{"id":"transcript.set","params":{"item":11,"transcript":{"language":"zh","speakers":[{"name":"Speaker 1"}],"words":[{"text":"剪映式","start":0,"end":203212800000,"speaker":0}, …]}}
{"id":"transcript.inspect","params":{}}
{"id":"transcript.createCaptions","params":{"maxChars":18,"lines":1}}
{"id":"captions.list","params":{}}
{"id":"captions.export","params":{"path":"...\\_ref\\captions-out.srt"}}
```

执行：`filmcraft-cli --demo run <script.jsonl>`

## 3. 实际输出（逐行）

```json
{"id":"transcript.set","line":1,"ok":true,"result":{"item":11,"words":11}}
{"id":"transcript.inspect","line":2,"ok":true,"result":{"current":4,"items":[{"item":11,"language":"zh","source":"imported","speakers":["Speaker 1"],"words":11}], …}}
{"id":"transcript.createCaptions","line":3,"ok":true,"result":{"captions":2,"track":48}}
{"id":"captions.list","line":4,"ok":true,"result":{"selection":[],"tracks":[{"captions":[…]}]}}
{"id":"captions.export","line":5,"ok":true,"result":{"bytes":155,"captions":2,"format":"SubRip (SRT)","path":"…\\captions-out.srt"}}
```

## 4. 产物核对

导出的 SRT（155 字节，2 条）：

```srt
1
00:00:00,000 --> 00:00:04,838
剪映式 工作流 从字幕 开始 验证

2
00:00:04,963 --> 00:00:10,802
命令面 是否 真的 可用 最后 一条
```

关键断言：

| 断言 | 实测 |
|---|---|
| 转写词数 | 11（脚本写入 11 个词） |
| 切分条数 | 2（`maxChars: 18`） |
| 导出 cue 数 | 2（`Select-String ' --> '` = 2） |
| 中文不乱码 | 是（UTF-8，无 BOM） |
| 时间码单调 | 是（0 → 4.838 s，4.963 → 10.802 s） |

## 5. 过程中发现并修掉的两个问题

1. **技能缺一条导入路径**：原文只写 `captions.import`（SRT），但引擎还有
   `transcript.set`（词级 JSON 直接导入）。已补进技能 1b，并写明 `item` 是**工程面板媒体项 id**、
   时间是 tick。
2. **CLI 不吃 BOM**：PowerShell 5.1 的 `Set-Content -Encoding utf8` 会写 BOM，
   `filmcraft-cli run` 报 `line 1: expected value at line 1 column 1`。改为
   `UTF8Encoding($false)` 后通过。这条已写进验证脚本说明，避免下一个人再踩。

## 6. 这条验证覆盖了什么、没覆盖什么

**覆盖**：`transcript.set → transcript.inspect → transcript.createCaptions → captions.list →
captions.export` 五步真实执行；命令返回值、字幕轨创建、SRT 落盘与内容。

**没覆盖**：

- `transcript.generate`（Whisper 识别）——本机没有语音模型，跑不了；技能里已经写明"没模型就走导入"。
- GUI 侧的字幕面板交互与样式预览（需要窗口；headless 下不验证）。
- 样式命令 `captions.setStyle`（本次只验证时序与导出，未验证渲染效果）。

## 7. 复现

```powershell
cd E:\workspaces\workspace-agent-skills\full-aigc-plugins-repositories\jianyingcraft
# 脚本必须是 UTF-8 无 BOM
target\debug\filmcraft-cli.exe --demo run E:\...\_ref\captions-script.jsonl
Get-Content E:\...\_ref\captions-out.srt
```

# 批量混剪：模板 → 替换 → 导出 → 对账

> 目的：验证 `jianyingcraft-batch-remix` 的核心链路（替换占位片段、工作副本、导出、离线素材检查）。
> 环境同上（headless CLI + `--demo`），日期 2026-10-18。

## 8. 执行

```jsonl
{"id":"clip.replaceFromBin","params":{"clips":[29],"item":6}}
{"id":"file.saveCopy","params":{"path":"...\\_ref\\remix-copy.fcproj"}}
{"id":"file.exportMedia","params":{"path":"...\\_ref\\remix-out.mp4","wait":true,"preset":"Match Source – Adaptive Medium Bitrate"}}
```

实际输出：

```json
{"id":"clip.replaceFromBin","ok":true,"result":{"clips":[29,30],"item":6,"short":[]}}
{"id":"file.saveCopy","ok":true,"result":{"bytes":28245,"ms":5.82,"path":"...\\remix-copy.fcproj"}}
{"id":"file.exportMedia","ok":true,"result":{"job":1,"offlineMedia":[],"bytes":17984537,"preset":"Match Source – Adaptive Medium Bitrate"}}
```

## 9. 对账

| 检查（技能 §4 的判据） | 实测 |
|---|---|
| 占位片段被替换 | clip 29 → `item=6`、`name=Aurora_Timelapse.mp4` |
| 联动音频一起替换 | 返回 `clips:[29,30]`（29 画面 + 30 声音） |
| 不改模板、只写副本 | `file.saveCopy` → 28,245 字节新文件；模板保持只读语义 |
| 输出文件存在且 > 0 | `remix-out.mp4` = **17,984,537 字节（17.15 MB）** |
| 离线素材 | `offlineMedia: []` |
| 时长策略 | clip 29 时长不变（119 帧）→ 属于技能里写的"未自适应，时长按模板" |

## 10. 这条验证覆盖了什么、没覆盖什么

**覆盖**：占位替换（含联动音频）、工作副本、真实导出（17 MB mp4）、离线素材检查、时长不漂移。

**没覆盖**：

- 多素材批量循环（本次 1 条；N 条的调度与清单生成是技能逻辑，未脚本化执行）；
- 导出预设之外的参数组合（码率/编码器切换）；
- 磁盘不足、队列卡住等失败路径（技能里写了处理方式，本次未注入故障）；
- 用 `ffprobe` 做外部一致性校验（本机无 ffprobe；仅用引擎自身返回值）。

# 图文成片：竖屏 + 配音 + 字幕 + 导出

> 目的：验证 `jianyingcraft-text-to-video` 的核心链路。环境同上，日期 2026-10-18。
> 限制：本机没有外部素材文件，画面用示例工程的合成片段（Color Matte / Bars and Tone）；
> 音色只有 `en-US`，所以文案用英文。

## 11. 执行

```jsonl
{"id":"file.newSequence","params":{"name":"TTV Vertical","width":1080,"height":1920,"fps":30}}
{"id":"timeline.place","params":{"item":14,"track":"V1","seconds":0}}
{"id":"timeline.place","params":{"item":12,"track":"V1","seconds":5}}
{"id":"tts.create","params":{"text":"JianyingCraft text to video check.","voice":"basic-female","track":"A1","time":0}}
{"id":"tts.render","params":{"wait":true}}
{"id":"captions.import","params":{"path":"...\\ttv.srt"}}
{"id":"captions.list","params":{}}
{"id":"sequence.inspect","params":{}}
{"id":"file.exportMedia","params":{"path":"...\\ttv-out.mp4","wait":true,"preset":"Match Source – Adaptive Medium Bitrate"}}
```

同一条链的前两步（建工程 + 建竖屏序列）现在也能一条命令完成（#84）：

```jsonl
{"id":"project.quickShortVideo","params":{"name":"TTV Vertical","width":1080,"height":1920,"fps":30,"narration":{"text":"JianyingCraft text to video check."},"captions":true}}
```

它按顺序做四件事：没有工程时建工程（同 `file.newProject`，已有工程不会被顶掉）、建序列
（同 `file.newSequence`）、可选把旁白放到 A1（同 `tts.create`）、可选建一条**空**字幕轨
（同 `captions.newTrack`，不凭空生成字幕）。参数在动手前先校验，四步在同一撤销步里，
任一步失败整体回滚，报错就是失败那一步自己的错误。

## 12. 断言（对应技能 §5）

| 技能判据 | 实测 |
|---|---|
| 序列 1080×1920 | `settings.width=1080`、`settings.height=1920` |
| 分镜数 | 视频轨 2 个片段（Color Matte + Bars and Tone） |
| 音频条数 | 音频轨 2 个片段（配音 89 帧 + 素材原声 300 帧） |
| 总时长 | `durationFrames=450`（15 s @30fps，与两段素材之和一致） |
| 字幕条数 | `captions.list`：1 条轨道 / **2 条字幕**，与 SRT 一致 |
| 导出 | **909,010 字节 mp4**，`offlineMedia: []` |
| 配音 | `tts.create` → clip 60，`speechDuration=757475712000`（2.98 s），`tts.render` 缓存命中 |

## 13. 这条验证覆盖了什么、没覆盖什么

**覆盖**：竖屏序列创建 → 两段画面放置 → 配音生成与渲染 → SRT 导入 → 结构断言 → 导出。

**没覆盖**：

- `file.import` 外部素材（本机无素材文件；用合成片段替代）；
- 中文配音的**发音质量**（2026-10-20 起 `tts.voices` 已含 zh-CN：装有 Windows 中文 SAPI 音色的机器上
  列出真实的 `system-…` 条目并由系统引擎发声，没有时列 `zh-cn-placeholder` 占位并明确报错。本轮实测
  `tts.create voice=system-microsoft-huihui-desktop` 产出 4.23 s / 24000 Hz 非静音 WAV，但**没有人耳或
  主观评分验证发音是否正确/自然**，也没有多音色对比）；
- 每段配音与画面时长对齐的自动裁剪（本次靠素材自然时长）；
- 重跑 `file.exportMedia` 之后的成片抽帧检查（未做像素级校验）。


