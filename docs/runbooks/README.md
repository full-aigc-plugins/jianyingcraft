# 可执行验收 runbook（Runbooks）

> 这四个文件把"因为没人在这台机器上跑过而一直空着"的 openspec 任务，变成可复制的
> 操作步骤：命令、期望输出、证据落点、失败排查和**不证明什么**。
>
> 它们都是**流程**，不是执行记录。没有实际产物之前，不要勾选对应任务，也不要把
> "命令存在"写成"验收通过"。

## 四个 runbook

| Runbook | 关闭/推进的任务 | 一句话 |
|---|---|---|
| [`voice-change-listening.md`](voice-change-listening.md) | craft `tasks.md` §3 变声块；`timeline.audio_effect_params` 的 partial | 在真剪映里盲听 `sub_type:1` vs `2` 的「音色」草稿，决定 `category_id:"timbre"` 该写哪个值 |
| [`macos-canary.md`](macos-canary.md) | CLI **9.3**；craft **#39**（**不关闭 9.7**） | macOS 冷重开 + 播放 + GUI 原生导出，并重生成 DMG 的 `DS_Store`/`background.tiff` |
| [`three-host-install.md`](three-host-install.md) | craft **#499** | 在 ZCode / Codex / Kimi 安装插件、调 MCP 工具、留三张截图 |
| [`skill-reruns.md`](skill-reruns.md) | craft **#478 / #484 / #490** | ASR、批量混剪 + 外部 ffprobe、图文成片 + 中文配音的真实产物 |

## 共同约定

- **一律用绝对路径**。`jianying project quickstart --out` 用相对路径时，素材会被写成
  `draft\draft\assets\...`，随后 `project verify` 报 `local material file does not exist`。
- **一律加 `--json`**，并解析 `data.ok`。`project verify` / `project quickstart` 即使
  `data.ok:false` 也可能返回退出码 0；只看 `$LASTEXITCODE` 会误判。
- **`JIANYING_CLI_PROCESS_LIST` 只用于 `--help` / 只读检查**。指向空文件会绕过
  "剪映正在运行就拒绝写草稿"的保护；真实写入前必须 `unset` / `Remove-Item Env:`。
- **不把 GUI 导出冒充 Rust native adapter**。`render.native` 仍是
  `external_dependency`；`render proxy` 是 ffmpeg 预览，不是剪映原生导出。
- **不提交媒体**。`native.mov`、`contact-sheet.png`、渲染出的 MP4、Whisper 模型都留在
  仓库外；证据 JSON 只写绝对路径 + SHA-256。
- **证据 JSON 必须有 `schema` 字段**。CLI 仓用 `provenance/` 与
  `schemas/evidence-record-v1.schema.json` 的分级；craft 侧的新证据放在
  [`evidence/`](evidence/)，每份自带 schema id。

## 证据放哪

| 验收 | 证据文件 | 仓库 |
|---|---|---|
| 变声听感 | `provenance/VOICE_CHANGE_LISTENING_<YYYYMMDD>.json` | `jianying-cli` |
| macOS canary | `provenance/MACOS_CANARY_<version>.json` + `.sources.json` + `.verify.json` | `jianying-cli` |
| DMG 重生成 | `provenance/MACOS_DMG_LAYOUT_<YYYYMMDD>.json` | `jianying-cli` |
| 三宿主 | `docs/runbooks/evidence/host-acceptance/<host>-<YYYYMMDD>.json` + 3 张 PNG | `jianyingcraft` |
| 技能实跑 | `docs/runbooks/evidence/SKILL_RERUNS_<YYYYMMDD>.json` | `jianyingcraft` |
| CLI ASR（可选） | `provenance/ASR_RUN_<YYYYMMDD>.json` | `jianying-cli` |

## 怎么用

1. 先读对应 runbook 的 §1「这条能证明什么」和 §7「本文不证明什么」，确认你要的结论
   确实由这次运行覆盖。
2. 按 §2 逐项打勾；缺一个依赖就停下来装，不要"先跑跑看"。
3. 按 §3 复制命令；每步先看 §4 的期望，不要等最后才发现中间一步的 `data.ok` 是 false。
4. 按 §5 写证据 JSON，SHA-256 用实际文件算，不要填占位值。
5. 失败时先查 §6 的表；表里没有的失败，把原始 stderr 和版本写进证据，再决定是否算
   `blocked`。

## 当前状态（诚实清单）

| Runbook | 是否已执行 | 说明 |
|---|---|---|
| 变声听感 | **未执行** | 需要真剪映 + 人耳；本机只有结构测试 |
| macOS canary | **未执行** | 需要 macOS + 剪映；本机是 Windows |
| 三宿主 | **未执行** | 本机只有 Kimi 0.27.0；Codex/ZCode 未安装 |
| 技能实跑 | **部分未执行** | ASR 需要 Whisper 模型；ffprobe 缺失；中文 TTS 只用系统音色跑过命令面 |
