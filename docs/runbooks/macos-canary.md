# macOS 真机 canary：冷重开、播放、原生导出 + DMG 布局重生成

> 状态：**流程文档，尚未执行**。本文不声称 macOS canary 已经跑过，也不把 GUI 导出
> 冒充 Rust native adapter。历史上唯一一次真实记录是 `../jianying-cli/docs/native-canary-20260921.md`，
> 那次明确没有关闭 9.3。

## 1. 这条能证明什么

**验收命题 A（CLI 9.3）**：在受支持的 macOS 剪映版本上，Rust 生成的草稿能被真实剪映
**打开 → 播放 → 完全退出 → 冷重开 → 再播放 → 原生导出**，导出制品可被哈希、`ffprobe`
和完整解码复核。

**验收命题 B（craft #39）**：`packaging/macos/dmg/dmg-layout.DS_Store` 的
`icvp.backgroundImageAlias` 仍指向上游卷名；用
`python3 packaging/macos/dmg/generate.py` 重生成后，布局与背景别名都指向 `JianyingCraft`。

关闭/推进的 openspec 任务：

| 仓库 | 位置 | 说明 |
|---|---|---|
| `jianying-cli` | `openspec/changes/unify-rust-jianying-engine/tasks.md` **9.3** | 本 runbook 的 A 部分；已有草稿编辑仍可能因不透明元数据失败，不因本 runbook 自动完成 |
| `jianying-cli` | 同上 **9.7** | **不会被本 runbook 关闭**；9.7 还要完整 parity、来源、差分门禁 |
| `jianyingcraft` | `openspec/changes/derive-from-filmcraft/tasks.md` 任务 **#39** | 本 runbook 的 B 部分（DMG 布局重生成） |

**判定标准**：

- A：真实剪映打开草稿、连续播放、完全退出后冷重开、原生导出 MOV；`ffmpeg -v error -i out.mov -f null -` 完整解码无错误；`runtime acceptance verify` 返回 `result: "passed"`。
- B：`generate.py` 退出 0；`background.tiff` 是 2 页 HiDPI TIFF；`dmg-layout.DS_Store`
  不再含 `FilmCraft` 字节，`rename_layout_entry.py --check` 退出 0。

> **原生导出必须是人点出来的 GUI 导出。** 当前 `render.native` 能力是
> `external_dependency`，`jianying render native --plan` 会失败关闭；不要用 `render proxy`
> 或合成 adapter 冒充原生导出。

## 2. 前置条件与逐项检查

| 依赖 | 要求 | 检查命令 | 期望 |
|---|---|---|---|
| macOS | 13+，arm64 或 x64 | `sw_vers` | 记录 `ProductVersion` / `BuildVersion` |
| 剪映专业版 | 受支持版本，能正常打开草稿 | `ls -d /Applications/VideoFusion-macOS.app /Applications/剪映专业版.app 2>/dev/null` | 至少一个存在 |
| `jianying` CLI | **released** 二进制（不是工作树 debug） | `"$JY" --version` | 与 `provenance/MACOS_CANARY_<version>.json` 的 `cli.version` 一致 |
| `ffmpeg` / `ffprobe` | 生成素材、完整解码、复核流 | `ffprobe -version` / `ffmpeg -version` | 第一行含版本 |
| Python | 3.11+，用于 DMG 依赖与 JSON/SHA | `python3 --version` | 3.11+ |
| `resvg` | `generate.py` 渲染 SVG | `resvg --version` | 有版本输出 |
| `ds_store` | 固定 `1.3.3` | `python3 -c "import ds_store; print('ok')"` | `ok` |
| `mac_alias` | 固定 `2.2.3` | `python3 -c "import mac_alias; print('ok')"` | `ok` |
| Pillow | `>= 12`（2x 144 dpi 检查） | `python3 -c "import PIL; print(PIL.__version__)"` | `12.x` 或更高 |
| 授权 | 原生导出会启动 GUI、写用户目录 | 人工确认 | 记录 `approval_id`（见 §5） |
| 账号 | **不需要** | — | 导出时关闭云备份/发布；不上传草稿 |

**released 二进制怎么来**：用 `jianying-cli v1.6.31` 的 GitHub Release 资产，并按
`../jianyingcraft-plugin/ui/cli.lock.json` 校验（这些摘要是仓库里已有的，不是现场编的）：

| 平台 | 资产 | archive SHA-256 | binary SHA-256 |
|---|---|---|---|
| macOS arm64 | `jianying-cli-1.6.31-darwin-arm64.tar.gz` | `76f4f2dc397519ab967f4939915045b441f9d3bcf36c5114d291336acf036757` | `99a65ef0ac9e3dfd1dc5c3dac77fe343ab69cdb2584e0fea6dfef8ce1902fcde` |
| macOS x64 | `jianying-cli-1.6.31-darwin-x64.tar.gz` | `8fd5e59fd8fe43cfff02b3c5f3bce05bd3c0d36caace1301f571b49d574ba861` | `2f0eb0eccb036de9bf4fa0a8aad00e144fb78cc3818802724db9b9a784ef3601` |
| Windows x64 | `jianying-cli-1.6.31-win32-x64.zip` | `cdb67e7ce4e26a92abcd304630002ea1ba115afb785761368cf7f5dfe1c766a0` | `fdda1e76dd0a4d63faafee5fd808e89f44186e392f50fb3a5b4787de61956980` |

> 工作树 debug 二进制的 capability 里 `contract_state` 是 `working_tree_unreleased`、
> `release_ref` 是 `null`。`runtime acceptance verify` 要求记录里声明 `released` 且
> `release_ref == v<version>`；用工作树二进制跑 GUI canary 仍然有价值，但**不能**产出
> 9.3 的机器可验证记录。先在别的机器/临时目录准备 released 二进制，再开始 A 部分。

> 本文的命令面按 1.7.0 工作树核对。开始前跑一次
> `"$JY" runtime acceptance verify --help` 和 `"$JY" project quickstart --help`；
> 如果 released 版本缺少某个子命令，换一个包含它的 released 版本，并把证据里的
> `cli.version` / `release_ref` 改成实际值。不要用工作树二进制冒充 released。

**环境变量（macOS，先执行一次）**

```sh
CRAFT="$HOME/workspaces/full-aigc-plugins-repositories/jianyingcraft"
CLI="$HOME/workspaces/full-aigc-plugins-repositories/jianying-cli"
JY="$HOME/opt/jianying-cli-1.6.31/jianying"       # released 二进制
CANARY="$HOME/canary-jianying-20261020"
mkdir -p "$CANARY"
"$JY" --version
shasum -a 256 "$JY"
```

```powershell
# 如果 canary 在 Windows 上做对照（不是本 runbook 的 macOS 结论）
$JY = "C:\path\to\jianying.exe"
```

> **不要**在真实 canary 里设置 `JIANYING_CLI_PROCESS_LIST`。它会把进程清单替换成空文件，
> 让"剪映正在运行就拒绝写草稿"的保护失效。只有跑 `--help` / 只读检查时才临时使用它。

## 3. 步骤

### 3.1 记录编辑器身份（只读）

工作目录：`$CLI`

```sh
"$JY" doctor --json
"$JY" runtime discover --json | tee "$CANARY/discover.json"
```

```powershell
& $JY doctor --json
& $JY runtime discover --json | Tee-Object "$CANARY\discover.json"
```

期望（macOS 形状；下面是本机 Windows 的实测形状，字段同名）：

```json
{
  "ok": true,
  "data": {
    "state": "found_unverified",
    "platform": "windows",
    "installations": [
      {
        "product_id": "jianying",
        "product": "JianYing Pro",
        "bundle_id": null,
        "version": "11.6.0.14552",
        "platform": "windows",
        "installation_root": "C:\\Users\\me\\AppData\\Local\\JianyingPro",
        "executable_identity": {
          "path": "C:\\Users\\me\\AppData\\Local\\JianyingPro\\Apps/JianyingPro.exe",
          "sha256": "4611aaa8ce00efc6afb41384d3f0313206d403a9bb1a1f83d6b6514cd2e5025a",
          "byte_length": 5410328
        },
        "process_name": "JianyingPro.exe",
        "draft_roots": [
          "C:\\Users\\me\\AppData\\Local\\JianyingPro/User Data/Projects/com.lveditor.draft"
        ],
        "support_status": "unverified",
        "automatic_routing": false
      }
    ],
    "automatic_routing": false,
    "support_status": "unverified"
  }
}
```

macOS 上应看到 `platform: "mac_os"`、`product: "VideoFusion-macOS"` 或中文显示名，
`draft_roots` 通常是 `~/Movies/JianyingPro/User Data/Projects/com.lveditor.draft`
或 `.../com.lemon.lvpro`。**记录 `executable_identity.sha256` 和 `version`**，
它们是验收记录里的 `runtime_identity` 与 `editor.version`。

### 3.2 生成 6 秒合成素材（不依赖剪映）

工作目录：`$CANARY`

```sh
ffmpeg -v error -y \
  -f lavfi -i "testsrc2=size=1280x720:rate=30:duration=6" \
  -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=6" \
  -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest \
  "$CANARY/canary.mp4"
ffmpeg -v error -y -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=6" "$CANARY/canary.wav"
shasum -a 256 "$CANARY/canary.mp4" "$CANARY/canary.wav"
```

```powershell
ffmpeg -v error -y `
  -f lavfi -i "testsrc2=size=1280x720:rate=30:duration=6" `
  -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=6" `
  -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest `
  "$CANARY\canary.mp4"
ffmpeg -v error -y -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=6" "$CANARY\canary.wav"
Get-FileHash -Algorithm SHA256 "$CANARY\canary.mp4", "$CANARY\canary.wav"
```

字幕（`$CANARY/canary.srt`，UTF-8 无 BOM）：

```srt
1
00:00:00,000 --> 00:00:02,000
Rust Canary A 你好

2
00:00:02,000 --> 00:00:04,000
Rust Canary B 冷重开

3
00:00:04,000 --> 00:00:06,000
Rust Canary C 原生导出
```

PowerShell 写文件时不要用 `Set-Content -Encoding utf8`（会带 BOM）；用：

```powershell
[IO.File]::WriteAllText("$CANARY\canary.srt", $srt, [Text.UTF8Encoding]::new($false))
```

### 3.3 构建并校验草稿（绝对路径）

工作目录：`$CANARY`。`$DRAFT` 必须是**不存在**的目录；重跑前删掉旧目录或换名。

```sh
DRAFT="$CANARY/draft"
"$JY" project quickstart \
  --out "$DRAFT" "Rust Canary 20261020" \
  --video "$CANARY/canary.mp4" \
  --audio "$CANARY/canary.wav" \
  --srt "$CANARY/canary.srt" \
  --width 1280 --height 720 --fps 30 --json

"$JY" project verify "$DRAFT" --json
```

```powershell
$DRAFT = Join-Path $CANARY "draft"
& $JY project quickstart --out $DRAFT "Rust Canary 20261020" --video "$CANARY\canary.mp4" --audio "$CANARY\canary.wav" --srt "$CANARY\canary.srt" --width 1280 --height 720 --fps 30 --json
& $JY project verify $DRAFT --json
```

期望（结构字段来自本机 `quickstart` 实测；macOS 路径与 `tracks` 数按实际替换）：

```json
{
  "ok": true,
  "data": {
    "ok": true,
    "name": "Rust Canary 20261020",
    "draft_path": "/Users/me/canary-jianying-20261020/draft",
    "file_path": "/Users/me/canary-jianying-20261020/draft/draft_content.json",
    "registered": false,
    "canvas": { "width": 1280, "height": 720, "fps": 30 },
    "added": { "video": true, "audio": true, "captions": 3 },
    "lint": { "errors": 0 },
    "open_hint": ["Open or publish /Users/me/canary-jianying-20261020/draft in JianYing/CapCut"]
  }
}
```

```json
{
  "ok": true,
  "data": {
    "ok": true,
    "name": "Rust Canary 20261020",
    "duration_us": 6000000,
    "tracks": 3,
    "issues": []
  }
}
```

> `project quickstart` 和 `project verify` **即使 `data.ok:false` 也返回退出码 0**。
> 必须解析 `data.ok`（见 `voice-change-listening.md` §4 的 PowerShell/sh 断言）。
> `tracks` 的实际数量取决于视频/音频/字幕轨的合并方式，以 `issues: []` 为准。

把草稿文件摘要记下来：

```sh
shasum -a 256 "$DRAFT/draft_content.json" "$DRAFT/draft_info.json" | tee "$CANARY/draft.sha256"
```

### 3.4 发布到剪映草稿库

工作目录：`$CLI`。发布前完全退出剪映。

```sh
"$JY" store directories --json
DRAFT_ROOT="<store directories 里 namespace=jianying 且 exists=true 的 path>"
"$JY" store publish "$DRAFT" --root "$DRAFT_ROOT" --json
"$JY" store list --root "$DRAFT_ROOT" --json
```

```powershell
& $JY store directories --json
$DRAFT_ROOT = "<store directories 里 namespace=jianying 且 exists=true 的 path>"
& $JY store publish $DRAFT --root $DRAFT_ROOT --json
& $JY store list --root $DRAFT_ROOT --json
```

如果 `store directories` 里没有 `exists=true` 的剪映 root，先启动一次剪映让它创建草稿根；
`store publish` 不会替你创建 root。

期望：

```json
{
  "ok": true,
  "data": {
    "status": "published",
    "name": "Rust Canary 20261020",
    "draft": "<DRAFT_ROOT>/Rust Canary 20261020",
    "store_key": "all_draft_store",
    "editor_running": []
  }
}
```

### 3.5 精确 Runtime Profile 与只读 probe

工作目录：`$CANARY`。把 `runtime discover` 的值填进 `profile.json`（字段是
`RuntimeProfile` 的 serde 形状；`platform` 用 `macos`）。

`$CANARY/profile.json`：

```json
{
  "id": "jianying-macos-11.5.13239",
  "product": "VideoFusion-macOS",
  "version": "11.5.13239",
  "platform": "macos",
  "capabilities": ["runtime.discovery", "runtime.controls", "runtime.profile"],
  "file_identity": {
    "path": "/Applications/VideoFusion-macOS.app/Contents/MacOS/VideoFusion-macOS",
    "sha256": "20cdaeb576fed9cdf85fc1858336a90291173d0ab03ccd0c225abe580ceae428",
    "byte_length": 447632
  },
  "process_names": ["VideoFusion-macOS"],
  "draft_roots": ["/Users/me/Movies/JianyingPro/User Data/Projects/com.lveditor.draft"]
}
```

```sh
"$JY" runtime probe \
  --runtime-profile "$CANARY/profile.json" \
  --executable "/Applications/VideoFusion-macOS.app/Contents/MacOS/VideoFusion-macOS" \
  --draft-root "/Users/me/Movies/JianyingPro/User Data/Projects/com.lveditor.draft" \
  --json
```

期望（实测形状）：

```json
{
  "ok": true,
  "data": {
    "supported": true,
    "editor_running": false,
    "materials_available": true,
    "executable_identity": {
      "path": "/Applications/VideoFusion-macOS.app/Contents/MacOS/VideoFusion-macOS",
      "sha256": "20cdaeb576fed9cdf85fc1858336a90291173d0ab03ccd0c225abe580ceae428",
      "byte_length": 447632
    },
    "permissions": {
      "executable_readable": true,
      "draft_root_readable": true,
      "draft_root_writable": true
    }
  }
}
```

`--runtime-profile` 接受的是**profile JSON 文件路径**，不是 id。传 id 会得到
`系统找不到指定的文件 (os error 2)`。

### 3.6 人工 GUI canary（不可脚本化）

在开始前抄下 `approval_id`（例如 `user-approval-20261020-macos-canary`），
它只表示"用户明确同意这次 GUI 导出"，不是 CLI 的审批记录。

1. 打开剪映。草稿列表里应有 `Rust Canary 20261020`。
2. 打开草稿，确认预览显示 `testsrc2` 测试图 + `Rust Canary A 你好`，时间线有 6 秒视频、音频和 3 条字幕。
3. 点播放，从 `00:00:00:xx` 看到结尾；确认画面变化、字幕依次切到 B、C。
4. **完全退出**剪映（`Cmd+Q`，不是关窗口）。确认进程消失：
   ```sh
   pgrep -fl "VideoFusion-macOS" || echo "no editor process"
   ```
5. 冷启动剪映，重新打开同名草稿；确认 6 秒时间线、3 条字幕、音频仍在。
6. 再播放一次，确认能从头到尾。
7. 导出：
   - 选 **MOV**、H.264、1280×720、30 fps、AAC 44.1 kHz；
   - 关闭云备份/发布，只写本地目录；
   - 输出到 `$CANARY/native.mov`；
   - 等界面明确显示"导出成功"。
8. 记录界面上看到的导出设置和成功提示原文。

> 如果剪映保存后把 `draft_meta_info.json` 写成不透明非 JSON，**不要**用
> `job run edit` 去改它；那会失败并可能拒绝提交。现有证据
> `../jianying-cli/docs/native-canary-20260921.md` 已经记录了
> `unsupported_draft_encoding`，这是 9.3 仍不完成的真实原因之一。

### 3.7 复核导出制品

工作目录：`$CANARY`

```sh
shasum -a 256 "$CANARY/native.mov"
ffprobe -v error -show_streams -show_format -of json "$CANARY/native.mov" | tee "$CANARY/native.probe.json"
ffmpeg -v error -i "$CANARY/native.mov" -f null -
ffmpeg -v error -y -ss 0   -i "$CANARY/native.mov" -frames:v 1 "$CANARY/f0.png"
ffmpeg -v error -y -ss 3   -i "$CANARY/native.mov" -frames:v 1 "$CANARY/f3.png"
ffmpeg -v error -y -ss 5.8 -i "$CANARY/native.mov" -frames:v 1 "$CANARY/f58.png"
ffmpeg -v error -y -i "$CANARY/f0.png" -i "$CANARY/f3.png" -i "$CANARY/f58.png" \
  -filter_complex "hstack=inputs=3" "$CANARY/contact-sheet.png"
shasum -a 256 "$CANARY/contact-sheet.png"
```

期望 `ffprobe` 里至少有：

```json
{
  "streams": [
    {
      "codec_type": "video",
      "codec_name": "h264",
      "width": 1280,
      "height": 720,
      "avg_frame_rate": "30/1"
    },
    {
      "codec_type": "audio",
      "codec_name": "aac",
      "sample_rate": "44100",
      "channels": 2
    }
  ],
  "format": {
    "format_name": "mov,mp4,m4a,3gp,3g2,mj2",
    "duration": "6.000000"
  }
}
```

> 这是示意结构，实际值以你的文件为准。`aac` 的采样率/声道数必须与验收记录里填的完全一致，
> 否则 `runtime acceptance verify` 会报 `audio_sample_rate_mismatch` / `audio_channels_mismatch`。

### 3.8 写验收记录并跑机器校验

先写来源清单 `provenance/MACOS_CANARY_20261020.sources.json`（CLI 仓）：

```json
{
  "case_id": "MACOS-CANARY-20261020",
  "license": {
    "provider": "self-generated",
    "url": "https://ffmpeg.org/",
    "summary": "synthetic testsrc2 video + 440 Hz sine audio + original SRT, generated locally"
  },
  "assets": [
    {
      "id": "canary-video",
      "local_path": "/Users/me/canary-jianying-20261020/canary.mp4",
      "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    },
    {
      "id": "canary-audio",
      "local_path": "/Users/me/canary-jianying-20261020/canary.wav",
      "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    }
  ]
}
```

把 `aaaaaaaa...` / `bbbb...` 换成真实的 `shasum -a 256` 输出。

再写 `provenance/MACOS_CANARY_20261020.json`：

```json
{
  "schema": "jianying-video-acceptance/v1",
  "case_id": "MACOS-CANARY-20261020",
  "scenario": "macos cold reopen + playback + native export",
  "status": "native_export_verified_human_review_passed",
  "created_at": "2026-10-20T15:30:00+08:00",
  "environment": {
    "os": "macOS 15.6",
    "editor": {
      "name": "VideoFusion-macOS",
      "version": "11.5.13239",
      "bundle_version": "11.6.0-beta1",
      "edition": "professional",
      "runtime_identity": "sha256:20cdaeb576fed9cdf85fc1858336a90291173d0ab03ccd0c225abe580ceae428"
    },
    "cli": {
      "version": "1.6.31",
      "release_ref": "v1.6.31",
      "contract_state": "released",
      "sha256": "99a65ef0ac9e3dfd1dc5c3dac77fe343ab69cdb2584e0fea6dfef8ce1902fcde"
    }
  },
  "workflow": {
    "job_schema": "jianying-job/v2",
    "published_draft": "/Users/me/Movies/JianyingPro/User Data/Projects/com.lveditor.draft/Rust Canary 20261020",
    "draft_digest": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    "approval_id": "user-approval-20261020-macos-canary"
  },
  "source_evidence": {
    "manifest": "provenance/MACOS_CANARY_20261020.sources.json",
    "source_attribution_status": "verified"
  },
  "evidence_levels": {
    "cold_reopen": "passed",
    "continuous_playback": "passed",
    "native_export": "passed",
    "agent_visual_review": "passed",
    "human_review": "passed"
  },
  "observations": {
    "cold_reopen": "完全退出后重新启动，同名草稿仍含 6 秒视频、音频与 3 条字幕",
    "continuous_playback": "播放头从 00:00:00:xx 推进到结尾，画面与字幕依次变化",
    "native_export": "剪映界面显示导出成功；关闭云备份，仅写本地目录"
  },
  "native_output": {
    "video": {
      "path": "/Users/me/canary-jianying-20261020/native.mov",
      "sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
      "size_bytes": 5365921,
      "duration_seconds": 6.0,
      "width": 1280,
      "height": 720,
      "fps": 30,
      "video_codec": "h264",
      "audio_codec": "aac",
      "audio_channels": 2,
      "audio_sample_rate": 44100,
      "black_frames_detected": false,
      "export_observation": "剪映导出对话框显示导出成功"
    },
    "contact_sheet": {
      "path": "/Users/me/canary-jianying-20261020/contact-sheet.png",
      "sha256": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
    }
  }
}
```

`draft_digest`：对发布后的草稿目录做稳定摘要。可以用
`shasum -a 256 draft_content.json draft_info.json draft_meta_info.json`，把三行按文件名排序后
再哈希；或者直接记录 `sha256(sum of the three file digests)`。**不要**用目录 mtime。

> 上面记录里的哈希、字节数、时长都是**示例值**。必须用实际 `shasum` / `ffprobe` 输出替换；
> `runtime acceptance verify` 会重新计算导出制品、联系表和流信息，任何不一致都会 fail closed。

跑校验（工作目录：`$CLI`）：

```sh
"$JY" --json runtime acceptance verify \
  "$CLI/provenance/MACOS_CANARY_20261020.json" \
  --ffprobe-cmd "$(command -v ffprobe)" \
  --require-human-review
```

```powershell
& $JY --json runtime acceptance verify "$CLI\provenance\MACOS_CANARY_20261020.json" --ffprobe-cmd (Get-Command ffprobe).Source --require-human-review
```

期望：

```json
{
  "ok": true,
  "data": {
    "schema": "jianying-runtime-acceptance-verification/v1",
    "result": "passed",
    "case_id": "MACOS-CANARY-20261020",
    "scenario": "macos cold reopen + playback + native export",
    "highest_evidence_level": "native-export",
    "human_review": "passed",
    "editor": {
      "name": "VideoFusion-macOS",
      "version": "11.5.13239",
      "build": "11.6.0-beta1",
      "runtime_identity": "sha256:20cdaeb576fed9cdf85fc1858336a90291173d0ab03ccd0c225abe580ceae428"
    },
    "cli": { "version": "1.6.31", "release_ref": "v1.6.31" },
    "artifact": {
      "path": "/Users/me/canary-jianying-20261020/native.mov",
      "sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
      "size_bytes": 5365921
    },
    "evidence_sha256": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    "checks": [
      "runtime_identity",
      "released_cli_identity",
      "draft_digest",
      "approval_id",
      "source_attribution",
      "cold_reopen_observation",
      "continuous_playback_observation",
      "native_export_observation",
      "output_sha256",
      "output_size",
      "contact_sheet_sha256",
      "video_stream",
      "audio_stream"
    ]
  }
}
```

> `runtime acceptance verify` 会重新计算导出制品、联系表、来源清单摘要，并用 `ffprobe`
> 复核流；它**不会**重新计算编辑器/CLI 二进制身份，也不会验证 GitHub Release。
> 声明的身份必须靠 `shasum` + `runtime discover` 的真实输出支撑，不能手填假值。

### 3.9 DMG 布局重生成（craft #39）

工作目录：`$CRAFT`

先确认当前 DS_Store 仍是旧卷名。下面只读检查**不需要** `ds_store` 库：

```sh
python3 - "$CRAFT/packaging/macos/dmg/dmg-layout.DS_Store" <<'PY'
import sys
raw = open(sys.argv[1], "rb").read()
print("bytes:", len(raw))
print("stale FilmCraft (ascii or utf16be):",
      b"FilmCraft" in raw or "FilmCraft".encode("utf-16-be") in raw)
print("JianyingCraft hits:",
      raw.count(b"JianyingCraft") + raw.count("JianyingCraft".encode("utf-16-be")))
PY
python3 packaging/macos/dmg/rename_layout_entry.py --check
```

期望（2026-10-20 实测）：`bytes: 16388`；`stale FilmCraft ...: True`；
`JianyingCraft hits` 至少 1（图标记录键已经是 `JianyingCraft.app`）；
`rename_layout_entry.py --check` 输出 `up to date (JianyingCraft.app)` 且退出 0。
旧卷名在 `icvp` 的 `backgroundImageAlias` 里（ASCII 卷名、UTF-16 卷名、Carbon 路径、
POSIX 路径共 4 处），所以 `--check` 通过**不代表**背景别名已经修好。

安装依赖并重生成：

```sh
python3 -m pip install 'pillow>=12' ds_store==1.3.3 mac_alias==2.2.3
resvg --version
python3 packaging/macos/dmg/generate.py
```

```powershell
python -m pip install "pillow>=12" ds_store==1.3.3 mac_alias==2.2.3
resvg --version
python packaging/macos/dmg/generate.py
```

`generate.py` 成功时**不打印任何东西**，直接改写：

- `packaging/macos/dmg/background.tiff`
- `packaging/macos/dmg/dmg-layout.DS_Store`

验证（工作目录：`$CRAFT`）：

```sh
python3 packaging/macos/dmg/rename_layout_entry.py --check

python3 - "$CRAFT/packaging/macos/dmg/dmg-layout.DS_Store" <<'PY'
import sys
raw = open(sys.argv[1], "rb").read()
print("bytes:", len(raw))
print("stale FilmCraft:", b"FilmCraft" in raw or "FilmCraft".encode("utf-16-be") in raw)
print("JianyingCraft hits:",
      raw.count(b"JianyingCraft") + raw.count("JianyingCraft".encode("utf-16-be")))
PY

python3 - "$CRAFT/packaging/macos/dmg/background.tiff" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1])
print("pages:", im.n_frames)
im.seek(0)
print("1x:", im.size, im.info.get("dpi"))
im.seek(1)
print("2x:", im.size, im.info.get("dpi"))
PY

shasum -a 256 packaging/macos/dmg/background.tiff packaging/macos/dmg/dmg-layout.DS_Store
```

期望：

| 检查 | 期望 |
|---|---|
| `rename_layout_entry.py --check` | 退出 0，`up to date (JianyingCraft.app)` |
| DS_Store 原始字节 | `stale FilmCraft: False`；`JianyingCraft hits` ≥ 4 |
| `background.tiff` | 2 页；1x `(660, 400)` / `(72, 72)`；2x `(1320, 800)` / `(144, 144)` |
| `generate.py` 重复运行 | 两个文件的 SHA-256 不变（脚本承诺逐字节可复现） |

可复现性检查：

```sh
shasum -a 256 packaging/macos/dmg/background.tiff packaging/macos/dmg/dmg-layout.DS_Store > /tmp/dmg-before.sha
python3 packaging/macos/dmg/generate.py
shasum -a 256 packaging/macos/dmg/background.tiff packaging/macos/dmg/dmg-layout.DS_Store > /tmp/dmg-after.sha
diff /tmp/dmg-before.sha /tmp/dmg-after.sha
```

`diff` 无输出、退出 0 才算逐字节可复现。

> `generate.py` 只写文件，不做 Finder 可视化验证。要证明 DMG 打开后背景真的显示，
> 还得用 `packaging/macos/package.sh` 打一个 DMG、挂载、在 Finder 里看窗口；
> 这一步需要 macOS GUI，且不属于脚本的职责。**不要**把"文件里没有 FilmCraft"冒充
> "Finder 显示正确"。

## 4. 期望结果（汇总）

| 检查 | 期望 |
|---|---|
| `runtime discover` | 记录到版本与 `executable_identity.sha256`；`automatic_routing: false`、`support_status: unverified` |
| `runtime probe` | `supported: true`，权限三项均为 true，`editor_running: false` |
| `project quickstart` | `data.ok: true`，`added.video/audio: true`，`added.captions: 3`，`lint.errors: 0` |
| `project verify` | `data.ok: true`，`issues: []` |
| 冷重开 | 完全退出后重新打开，同名草稿内容仍在 |
| 播放 | 播放头从 0 推进到 6 秒，画面/字幕变化 |
| `ffmpeg -v error -i native.mov -f null -` | 退出 0，无错误输出 |
| `runtime acceptance verify` | `result: "passed"`，`highest_evidence_level: "native-export"` |
| `generate.py` | 退出 0；`background.tiff` 2 页 HiDPI；DS_Store 无 `FilmCraft` 字节 |
| `rename_layout_entry.py --check` | 退出 0 |

## 5. 证据怎么留

| 文件 | 位置 | 说明 |
|---|---|---|
| `MACOS_CANARY_20261020.json` | `jianying-cli/provenance/` | 主验收记录，`schema: jianying-video-acceptance/v1` |
| `MACOS_CANARY_20261020.sources.json` | `jianying-cli/provenance/` | 来源清单（合成素材的 license 与 SHA-256） |
| `MACOS_CANARY_20261020.verify.json` | `jianying-cli/provenance/` | `runtime acceptance verify` 的完整输出（重定向保存） |
| `MACOS_DMG_LAYOUT_20261020.json` | `jianying-cli/provenance/` | DS_Store 重生成前后 SHA、依赖版本、`--check` 退出码 |
| `native.mov` / `contact-sheet.png` | 仓库外（`$CANARY`） | 媒体制品，不提交；记录路径与 SHA-256 |

`MACOS_DMG_LAYOUT_20261020.json` 最小形状：

```json
{
  "schema": "jianying-dmg-layout-regeneration/v1",
  "observed_at": "2026-10-20T15:30:00+08:00",
  "platform": "macos",
  "dependencies": {
    "python": "3.12.14",
    "resvg": "0.45.1",
    "ds_store": "1.3.3",
    "mac_alias": "2.2.3",
    "pillow": "12.3.0"
  },
  "before": {
    "ds_store_sha256": "1111111111111111111111111111111111111111111111111111111111111111",
    "ds_store_bytes": 16388,
    "stale_filmcraft": true
  },
  "after": {
    "ds_store_sha256": "2222222222222222222222222222222222222222222222222222222222222222",
    "ds_store_bytes": 16388,
    "stale_filmcraft": false,
    "background_tiff_sha256": "3333333333333333333333333333333333333333333333333333333333333333",
    "background_pages": 2,
    "background_1x": [660, 400],
    "background_2x": [1320, 800]
  },
  "checks": {
    "rename_layout_entry_check_exit": 0,
    "second_generate_identical": true,
    "finder_visual_verified": false
  },
  "note": "只证明 DSDB 结构与背景别名；未构建/挂载 DMG 做 Finder 可视化验证。"
}
```

标准证据索引（可选）：再写一份 `provenance/MACOS_CANARY_20261020.evidence.json`，
用 `schemas/evidence-record-v1.schema.json` 的 `jianying-evidence-record/v1`，
`level: "native-export"`，`artifacts` 指向上面的文件。它只做索引，不替代主记录。

## 6. 失败时先看这 5 种

| 现象 | 含义 | 处理 |
|---|---|---|
| `cli_release_identity_mismatch: release_ref must equal v<version>` | 记录里的 `release_ref` 与 `cli.version` 不一致 | 改成实际版本，例如 `1.6.31` / `v1.6.31` |
| `human_review_required: expected passed, found pending` | 还没做人工审查 | 真看过冷重开/播放/导出后再把 `evidence_levels.human_review` 改成 `passed`；不要先改后看 |
| `output_sha256_mismatch` / `output_size_mismatch` | 导出文件被替换或记录填错 | 重新 `shasum`，用同一个文件；不要改记录去迁就旧哈希 |
| `ffprobe_missing_audio_stream` / `audio_sample_rate_mismatch` / `duration_mismatch` | 导出没有音频、采样率/时长与记录不符 | 用 `ffprobe` 实际输出填记录；检查导出设置是否关了音频 |
| `resvg not found` 或 `No module named 'ds_store'` | DMG 依赖缺失 | `brew install resvg`；`python3 -m pip install 'pillow>=12' ds_store==1.3.3 mac_alias==2.2.3` |

其他值得记录的失败：

- `error: the 2x page isn't 144 dpi (needs Pillow 12 or later)`：Pillow 太旧，升级到 12+。
- `draft metadata path draft_fold_path is invalid`：复制草稿到新路径导致；重建草稿，不要复制。
- 剪映打开草稿后黑屏/缺素材：先看 `project verify` 和 `ffprobe`，离线素材不会被自动修复。
- 剪映保存后 `job run edit` 报 `unsupported_draft_encoding`：这是已知的应用私有编码，
  不要绕过；它正是 9.3"已有草稿编辑"仍不完成的证据。
- `render native --plan` 报 `capability render.native is not available`：预期行为；
  本 runbook 的原生导出走 GUI，不走 `render native`。

## 7. 本文不证明什么

- **不关闭 9.7**。9.7 要求完整 parity、来源、差分和真实剪映门禁；本文只做 macOS
  单版本、单草稿的冷重开/播放/导出。
- **不证明 Rust native adapter 可用**。GUI 导出是人点出来的，`render.native` 仍是
  `external_dependency`；不要把 GUI 结果写成 adapter 证据。
- **不证明所有 macOS 版本/架构**。只覆盖实际运行的 `sw_vers` 与剪映版本；arm64 与 x64
  需要分别记录。
- **不证明已有草稿可无损编辑**。应用保存后的不透明元数据会让 `job edit` 失败；
  本文只证明"Rust 新建草稿 + GUI 导出"这条链路。
- **不证明 Finder 里的 DMG 背景正确**。`generate.py` 只写 DS_Store 和 TIFF；
  真实验证需要打 DMG、挂载、看窗口。
- **`runtime acceptance verify` 不是签名**。它重新计算制品和流，但信任记录里声明的
  编辑器/CLI 身份；身份要靠 `shasum` 与 `runtime discover` 的真实输出支撑。
