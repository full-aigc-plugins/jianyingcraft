# 变声听感验收（真人听、真剪映）

> 状态：**流程文档，尚未执行**。本文只写可复制的步骤、可核对的输出和证据落点，
> 不声称任何一次听感验收已经发生。

## 1. 这条能证明什么

**验收命题**：用 `jianying timeline audio-effect` + `timeline audio-effect-param` 生成的
`category_id:"timbre"` 音色素材，在真实剪映里**听起来**是否就是目录里的那个预设；
以及草稿协议里的 `sub_type` 到底该写 `1`（场景音效）还是 `2`（VCVoiceChange）。

关闭的 openspec 任务：

| 仓库 | 位置 | 任务 |
|---|---|---|
| `jianyingcraft` | `openspec/changes/derive-from-filmcraft/tasks.md` §3「仍缺：变声（pitch）无对应命令」块 | 真机听感验收；`timeline.audio_effect_params` 目前保持 **partial**，等这一条 |
| `jianying-cli` | `src/timeline_ops.rs` `audio_effect` / `audio_effect_param` | `category_id:"timbre"` 的 `sub_type` 取值（当前代码固定写 `1`） |

**判定标准**（三条同时满足才算通过）：

1. 同一段人声、同一个音色预设、只差 `sub_type` 的两份草稿，都能在剪映里正常打开和播放；
2. 至少一名评审在盲听（不知道哪份是 1、哪份是 2）后，能指出哪一份更接近剪映内置预设的试听效果；
3. 结论能落到一个明确动作：把 `timbre` 的默认 `sub_type` 改成 `2`，或保留 `1` 并写明理由。

> 结构测试只能证明字段写对了，**不能**证明剪映按 VC 变声渲染。听感是主观证据，
> 所以本文要求盲听和原始观察记录，不把"能打开"冒充"听起来对"。

## 2. 前置条件与逐项检查

| 依赖 | 要求 | 检查命令 | 期望 |
|---|---|---|---|
| 剪映专业版 | Windows 或 macOS，已安装且能正常打开草稿 | `jianying runtime discover --json` | `state` 为 `found_unverified` 或 `not_installed` 以外；记录 `version` 与 `executable_identity.sha256` |
| `jianying` CLI | 本仓 `target/debug/jianying.exe`（或 release 版） | `jianying --version` | `jianying 1.7.0`（以实际为准） |
| `ffprobe` | `media probe` / `quickstart --audio` 必需 | `ffprobe -version` | 第一行含 `ffprobe version` |
| Python | 仅用于改 `sub_type` 和校验 JSON | `python --version` | 3.10+ |
| 测试人声 | 5–10 秒、干净、无背景音乐；A/B 必须同一文件 | `ffprobe -v error -show_entries format=duration -of default=nw=1 <voice>` | 时长 ≥ 5 秒 |
| 耳机/监听 | 同一副耳机、同一音量；建议关闭空间音频 | 人工确认 | — |
| 剪映进程 | 所有写草稿的命令都要求剪映完全退出 | `tasklist /FI "IMAGENAME eq JianyingPro.exe"`（Windows）/ `pgrep -fl VideoFusion-macOS`（macOS） | 无输出 |

**环境变量（PowerShell，先执行一次）**

```powershell
$CRAFT = "E:\workspaces\workspace-agent-skills\full-aigc-plugins-repositories\jianyingcraft"
$CLI   = "E:\workspaces\workspace-agent-skills\full-aigc-plugins-repositories\jianying-cli"
$JY    = Join-Path $CLI "target\debug\jianying.exe"
$WS    = Join-Path $env:TEMP "jy-voice-listening"
New-Item -ItemType Directory -Force -Path $WS | Out-Null
& $JY --version
```

```sh
CRAFT="$HOME/workspaces/full-aigc-plugins-repositories/jianyingcraft"
CLI="$HOME/workspaces/full-aigc-plugins-repositories/jianying-cli"
JY="$CLI/target/debug/jianying"
WS="${TMPDIR:-/tmp}/jy-voice-listening"
mkdir -p "$WS"
"$JY" --version
```

> **关于 `JIANYING_CLI_PROCESS_LIST`**：只在跑 `--help` / 只读检查时把它指向一个空文件，
> 避免主机的 `tasklist` 卡住。**真正写草稿时不要设置它**——它会绕过"剪映正在运行就拒绝写入"
> 的保护，可能让剪映覆盖你刚改的文件。

```powershell
# 只读/--help 检查时可用；真实写入前 Remove-Item Env:JIANYING_CLI_PROCESS_LIST
$empty = Join-Path $env:TEMP "jy-empty-processes.txt"
New-Item -ItemType File -Force -Path $empty | Out-Null
$env:JIANYING_CLI_PROCESS_LIST = $empty
```

```sh
# 只读/--help 检查时可用；真实写入前 unset JIANYING_CLI_PROCESS_LIST
empty="${TMPDIR:-/tmp}/jy-empty-processes.txt"
: > "$empty"
export JIANYING_CLI_PROCESS_LIST="$empty"
```

## 3. 步骤

### 3.1 记录环境（只读）

工作目录：`$CLI`

```powershell
& $JY doctor --json
& $JY runtime discover --json
& $JY capabilities --json > "$WS\capabilities.json"
```

```sh
"$JY" doctor --json
"$JY" runtime discover --json
"$JY" capabilities --json > "$WS/capabilities.json"
```

期望（本机实测，2026-10-20，Windows）：

```json
{
  "ok": true,
  "data": {
    "version": "1.7.0",
    "plan_schema": "jianying-cli-plan/v1",
    "draft_roots": [
      {
        "namespace": "jianying",
        "path": "<redacted-path>/com.lveditor.draft",
        "exists": true
      }
    ],
    "editors_running": [],
    "ffprobe": null,
    "ffmpeg": null
  }
}
```

`ffprobe` 为 `null` 就说明本机没装 ffprobe，后面 `quickstart --audio` 会直接失败。

### 3.2 确认预设名和参数个数

工作目录：`$CLI`

```powershell
& $JY media catalog --domain tone_effects --search "大叔" --json
```

```sh
"$JY" media catalog --domain tone_effects --search "大叔" --json
```

期望（实测）：

```json
{
  "ok": true,
  "data": {
    "results": [
      {
        "name": "大叔",
        "vip": false,
        "resource_id": "7020344898033291790",
        "effect_id": "2672760",
        "md5": "2509bbd71e127b04a29f52a54e82c53c",
        "params": [
          { "name": "音调", "default": 0.834, "min": 0.0, "max": 1.0 },
          { "name": "音色", "default": 1.0, "min": 0.0, "max": 1.0 }
        ]
      }
    ],
    "count": 1
  }
}
```

`--params` 必须给**和 `params` 等长**的数组，取值用剪映 UI 的 0–100 标度；
落盘时除以 100 存进 `audio_adjust_params[].value`。

### 3.3 建 A 稿（`sub_type` 保持 CLI 默认的 1）

工作目录：`$WS`。**`--out` 和 `--audio` 都用绝对路径**——相对路径会让素材被写成
`draft\draft\assets\...`，随后 `project verify` 报 `local material file does not exist`。

```powershell
$VOICE = Join-Path $WS "voice.wav"   # 换成你自己的 5–10 秒人声
$DRAFT_A = Join-Path $WS "draft-a"
& $JY project quickstart --out $DRAFT_A "Voice A (sub_type 1)" --audio $VOICE --json
& $JY project verify $DRAFT_A --json

$segments = & $JY timeline segments $DRAFT_A --json | ConvertFrom-Json
$SEG_A = $segments.data[0].id
& $JY timeline audio-effect $DRAFT_A $SEG_A "大叔" --json
& $JY timeline audio-effect-param $DRAFT_A $SEG_A "大叔" --params "[83.4,100]" --json
& $JY timeline get $DRAFT_A $SEG_A --json
```

```sh
VOICE="$WS/voice.wav"
DRAFT_A="$WS/draft-a"
"$JY" project quickstart --out "$DRAFT_A" "Voice A (sub_type 1)" --audio "$VOICE" --json
"$JY" project verify "$DRAFT_A" --json
SEG_A=$("$JY" timeline segments "$DRAFT_A" --json | python3 -c "import json,sys; print(json.load(sys.stdin)['data'][0]['id'])")
"$JY" timeline audio-effect "$DRAFT_A" "$SEG_A" "大叔" --json
"$JY" timeline audio-effect-param "$DRAFT_A" "$SEG_A" "大叔" --params "[83.4,100]" --json
"$JY" timeline get "$DRAFT_A" "$SEG_A" --json
```

期望输出（实测，ID 会不同）：

```json
{
  "ok": true,
  "data": {
    "ok": true,
    "id": "96224c636e5646c68f63b7076ce3b279",
    "name": "大叔",
    "material_id": "0a754bc27ff442dd8e2bdcdd0353531e",
    "applied": true,
    "category_id": "timbre"
  }
}
```

```json
{
  "ok": true,
  "data": {
    "ok": true,
    "id": "96224c636e5646c68f63b7076ce3b279",
    "name": "大叔",
    "material_id": "0a754bc27ff442dd8e2bdcdd0353531e",
    "params": ["音调", "音色"],
    "old_values": [0.834, 1.0],
    "values": [0.8340000000000001, 1.0]
  }
}
```

再次挂同一个预设应为幂等：

```json
{
  "ok": true,
  "data": {
    "ok": true,
    "id": "1422948d9d444535ba8ea5717d589db8",
    "name": "大叔",
    "material_id": "35e9d359c77a46299e9f5cce85627d59",
    "applied": false,
    "reason": "already-applied"
  }
}
```

### 3.4 建 B 稿（把 `timbre` 的 `sub_type` 改成 2）

**不要复制 A 稿目录**：`draft_meta_info.json` 里的 `draft_fold_path` 绑定原路径，
复制后 `project verify` 会报 `draft metadata path draft_fold_path is invalid`。
从零再建一份，只改 `sub_type`。

```powershell
$DRAFT_B = Join-Path $WS "draft-b"
& $JY project quickstart --out $DRAFT_B "Voice B (sub_type 2)" --audio $VOICE --json
$segments = & $JY timeline segments $DRAFT_B --json | ConvertFrom-Json
$SEG_B = $segments.data[0].id
& $JY timeline audio-effect $DRAFT_B $SEG_B "大叔" --json
& $JY timeline audio-effect-param $DRAFT_B $SEG_B "大叔" --params "[83.4,100]" --json

@'
import json, os, sys
root = sys.argv[1]
n = 0
for name in ("draft_content.json", "draft_info.json"):
    p = os.path.join(root, name)
    d = json.load(open(p, encoding="utf-8"))
    for fx in d.get("materials", {}).get("audio_effects", []):
        if fx.get("category_id") == "timbre":
            fx["sub_type"] = 2
            n += 1
    with open(p, "w", encoding="utf-8", newline="\n") as f:
        json.dump(d, f, ensure_ascii=False, indent=2)
print("patched entries:", n)
'@ | python - $DRAFT_B

& $JY project verify $DRAFT_B --json
```

```sh
DRAFT_B="$WS/draft-b"
"$JY" project quickstart --out "$DRAFT_B" "Voice B (sub_type 2)" --audio "$VOICE" --json
SEG_B=$("$JY" timeline segments "$DRAFT_B" --json | python3 -c "import json,sys; print(json.load(sys.stdin)['data'][0]['id'])")
"$JY" timeline audio-effect "$DRAFT_B" "$SEG_B" "大叔" --json
"$JY" timeline audio-effect-param "$DRAFT_B" "$SEG_B" "大叔" --params "[83.4,100]" --json
python3 - "$DRAFT_B" <<'PY'
import json, os, sys
root = sys.argv[1]
n = 0
for name in ("draft_content.json", "draft_info.json"):
    p = os.path.join(root, name)
    d = json.load(open(p, encoding="utf-8"))
    for fx in d.get("materials", {}).get("audio_effects", []):
        if fx.get("category_id") == "timbre":
            fx["sub_type"] = 2
            n += 1
    with open(p, "w", encoding="utf-8", newline="\n") as f:
        json.dump(d, f, ensure_ascii=False, indent=2)
print("patched entries:", n)
PY
"$JY" project verify "$DRAFT_B" --json
```

> **必须同时改 `draft_content.json` 和 `draft_info.json`。** 两份是字节镜像；
> 只改一份会得到 `draft_content.json and draft_info.json must be byte-identical mirrors`。
> 两份都改完后 `project verify` 应恢复 `data.ok: true`。

确认两份草稿的唯一结构差异：

```powershell
@'
import json, sys, hashlib
for label, path in (("A", sys.argv[1]), ("B", sys.argv[2])):
    raw = open(path, "rb").read()
    d = json.loads(raw)
    fx = [x for x in d["materials"]["audio_effects"] if x.get("category_id") == "timbre"]
    print(label, "sha256=" + hashlib.sha256(raw).hexdigest(),
          "sub_type=" + str([x.get("sub_type") for x in fx]),
          "values=" + str([[p.get("value") for p in x.get("audio_adjust_params", [])] for x in fx]))
'@ | python - "$DRAFT_A\draft_content.json" "$DRAFT_B\draft_content.json"
```

期望：A 的 `sub_type` 是 `[1]`，B 是 `[2]`，`values` 都是 `[[0.8340000000000001, 1.0]]`。
`sha256` 必然不同；这正是唯一需要听感来裁决的差异。

### 3.5 发布到剪映草稿库

工作目录：`$CLI`。先找到真实草稿根，再把两份草稿发布进去；发布前确认剪映已退出。

```powershell
& $JY store directories --json
$DRAFT_ROOT = "<store directories 里 namespace=jianying 且 exists=true 的 path>"
& $JY store publish $DRAFT_A --root $DRAFT_ROOT --json
& $JY store publish $DRAFT_B --root $DRAFT_ROOT --json
& $JY store list --root $DRAFT_ROOT --json
```

```sh
"$JY" store directories --json
DRAFT_ROOT="<store directories 里 namespace=jianying 且 exists=true 的 path>"
"$JY" store publish "$DRAFT_A" --root "$DRAFT_ROOT" --json
"$JY" store publish "$DRAFT_B" --root "$DRAFT_ROOT" --json
"$JY" store list --root "$DRAFT_ROOT" --json
```

期望：

```json
{
  "ok": true,
  "data": {
    "status": "published",
    "name": "Voice A (sub_type 1)",
    "draft": "<DRAFT_ROOT>/Voice A (sub_type 1)",
    "store_key": "all_draft_store",
    "editor_running": []
  }
}
```

### 3.6 盲听（人工）

1. 打开剪映，草稿列表里应出现 `Voice A (sub_type 1)` 与 `Voice B (sub_type 2)`。
2. 先不要看文件名里的 `sub_type`；让第二个人把两份草稿随机改名为 `X` / `Y`，或由脚本随机决定播放顺序。
3. 对同一段人声，分别试听：
   - 草稿 A 的原声（先把音效禁用/旁路，作为基准）；
   - 草稿 A 的「大叔」；
   - 草稿 B 的「大叔」；
   - 剪映内置「音色 → 大叔」的官方试听（如果版本提供）。
4. 每份至少听 3 遍，记录：
   - 音调是否明显下移、是否像"大叔"；
   - 共振峰/音色是否改变，还是只像加了 EQ/混响；
   - 有没有机器人感、双声、吞字、爆音；
   - 与官方试听的接近程度（1–5 分）。
5. 揭盲，把分数和判断写进证据 JSON。

> 建议顺序：基准 → A → B → 官方试听 → B → A，减少顺序偏差。
> 不要在同一段音频上反复切换超过 6 次，听觉疲劳会让结论偏向"听不出差别"。

### 3.7 判定与动作

| 盲听结果 | 结论 | 动作 |
|---|---|---|
| B 明显更像官方预设；A 只像普通音效或几乎无变化 | `timbre` 应写 `sub_type: 2` | 改 `src/timeline_ops.rs` 的 `audio_effect`：按 `category_id` 决定 `sub_type`；补 `timeline_cli_contract` 断言；重跑本节 A/B |
| A、B 听不出差别，或剪映对两者渲染一致 | 保留 `sub_type: 1` | 在证据里写明"该版本/该预设下无差异"；不改代码 |
| A、B 都不像官方预设，或 B 被剪映拒绝/静音 | 协议层无法用 `sub_type` 表达 | 不改默认值；把现象和版本记入证据，升级为协议问题 |
| B 能播放但出现爆音/吞字 | `sub_type: 2` 需要额外字段 | 记录缺失字段；不要只改 `sub_type` 就宣称通过 |

**能力状态**：本条通过后，`timeline.audio_effect_params` 才具备从 `partial` 提升的证据；
提升本身是代码/能力清单变更，不是本 runbook 自动完成的。

## 4. 期望结果（汇总）

| 检查 | 期望 |
|---|---|
| `project verify $DRAFT_A` / `$DRAFT_B` | `data.ok: true`，`data.issues: []` |
| A 的 `materials.audio_effects[*].sub_type` | `1` |
| B 的 `materials.audio_effects[*].sub_type` | `2` |
| `category_id` / `category_name` | `timbre` / `音色` |
| `audio_adjust_params` | 2 项，`value` 为 `0.8340000000000001` 与 `1.0` |
| `store publish` | `data.status: "published"`，`editor_running: []` |
| 盲听 | 至少一名评审给出可复述的差异判断，并揭盲 |

> `project verify` 即使结构失败也返回**退出码 0**。必须解析 `data.ok`，不能只看 `$LASTEXITCODE`。

```powershell
$v = & $JY project verify $DRAFT_B --json | ConvertFrom-Json
if (-not $v.data.ok) { throw ("verify failed: " + ($v.data.issues -join "; ")) }
```

```sh
"$JY" project verify "$DRAFT_B" --json > "$WS/verify-b.json"
python3 -c "import json,sys; d=json.load(open('$WS/verify-b.json')); sys.exit(0 if d['data']['ok'] else 1)"
```

## 5. 证据怎么留

**主证据文件**（放在 CLI 仓，遵循 `provenance/` 的 `schema` 约定）：

`provenance/VOICE_CHANGE_LISTENING_20261020.json`

```json
{
  "schema": "jianying-voice-change-listening/v1",
  "observed_at": "2026-10-20T15:30:00+08:00",
  "evidence_level": "playback",
  "platform": "windows",
  "editor": {
    "product": "JianYing Pro",
    "version": "11.6.0.14552",
    "executable_sha256": "4611aaa8ce00efc6afb41384d3f0313206d403a9bb1a1f83d6b6514cd2e5025a"
  },
  "cli": {
    "version": "1.7.0",
    "binary_sha256": "0000000000000000000000000000000000000000000000000000000000000000"
  },
  "preset": {
    "name": "大叔",
    "resource_id": "7020344898033291790",
    "params": { "音调": 83.4, "音色": 100.0 }
  },
  "draft_a": {
    "path": "C:/Users/me/AppData/Local/Temp/jy-voice-listening/draft-a",
    "content_sha256": "1111111111111111111111111111111111111111111111111111111111111111",
    "sub_type": 1
  },
  "draft_b": {
    "path": "C:/Users/me/AppData/Local/Temp/jy-voice-listening/draft-b",
    "content_sha256": "2222222222222222222222222222222222222222222222222222222222222222",
    "sub_type": 2
  },
  "listen": {
    "blind": true,
    "order": ["A", "B", "official", "B", "A"],
    "repeats": 3,
    "headphones": "wired, no spatial audio",
    "volume": "same for both drafts"
  },
  "observations": {
    "a_vs_official": "A 只像普通 EQ，音调下移不明显；与官方试听差距大",
    "b_vs_official": "B 的音调与共振峰更接近官方试听，第 3 遍仍能分辨",
    "artifacts": "B 无爆音/吞字；A 无异常"
  },
  "decision": {
    "affects_timbre_sub_type": true,
    "decided_sub_type": 2,
    "rationale": "盲听后 B 更接近官方预设；A 与 B 可稳定区分",
    "follow_up": "修改 audio_effect 的 sub_type 选择并补测试；本证据只覆盖该版本与大叔预设"
  },
  "reviewer": {
    "name": "（评审人）",
    "role": "human listening reviewer"
  }
}
```

同一目录可选再放一份标准记录 `provenance/VOICE_CHANGE_LISTENING_20261020.evidence.json`，
使用 `schemas/evidence-record-v1.schema.json` 的 `jianying-evidence-record/v1`，把上面的文件
放进 `artifacts`。两份都保留时，标准记录只做索引，不重复主观结论。

**原始材料**（不要提交进仓库）：

- 测试人声原文件；
- 两份草稿目录的打包（可选）；
- 盲听顺序的随机种子/记录；
- 如做了客观对比，保留 `ffmpeg astats` / 频谱图输出。

## 6. 失败时先看这 5 种

| 现象 | 含义 | 处理 |
|---|---|---|
| `ffprobe not found on PATH (brew install ffmpeg)` | 没有 ffprobe，`quickstart --audio` 无法测时长 | 安装 ffmpeg；`jianying doctor --json` 的 `ffprobe` 不再是 `null` |
| `local material file does not exist: ...\draft\draft\assets\...` | `--out` 或 `--audio` 用了相对路径，素材路径双前缀 | 全部改绝对路径，从零重建草稿 |
| `editor is running (...); close it before mutating drafts` | 剪映没退干净 | 完全退出剪映；确认 `tasklist`/`pgrep` 无主进程；不要用 `--force` 绕过 |
| `draft_content.json and draft_info.json must be byte-identical mirrors` | 只改了一份镜像 | 两份都改；改完 `project verify` 必须 `data.ok: true` |
| `unknown audio effect "..."` / `declares 2 parameter(s) ...; got 1 value(s)` / `must be a finite number in [0, 100]` | 预设名或 `--params` 不匹配 | 用 `media catalog --domain tone_effects --search <名>` 重新取 `params`；数组长度和顺序必须一致 |

其他值得记录的失败：

- `draft metadata path draft_fold_path is invalid`：复制草稿到新路径导致；不要复制，重建。
- 剪映打开后看不到素材/黑屏：先 `project verify` 与 `ffprobe` 检查素材是否在线；离线素材不会自动修复。
- B 被剪映拒绝或静音：记录剪映版本与预设，不要改协议去"猜"。

## 7. 本文不证明什么

- **不证明听感是客观事实**。它是一名或几名评审在特定耳机、音量、版本下的主观判断；
  换人、换设备、换剪映版本都可能不同。
- **不证明所有音色预设**。只覆盖实际听过的预设（本文示例是「大叔」）；其余 56 个预设
  需要各自或抽样验收。
- **不证明 AI 音色克隆/任意变调**。剪映的云端渲染只写 `production_path`，草稿里没有数值
  pitch 字段；这类能力不在协议命令面内。
- **不证明 macOS/Windows 一致**。两个平台的剪映版本和音频栈不同，必须分别记录。
- **不自动提升 capability**。`timeline.audio_effect_params` 从 `partial` 到 `supported`
  需要代码、测试和能力清单一起改；本文只提供判定依据。
