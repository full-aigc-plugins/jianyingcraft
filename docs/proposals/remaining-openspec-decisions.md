# 剩余 openspec 条目的决策说明（2026-10-20）

> 这份文档不修改任何规格、不实现任何功能：它把"还需要人拍板或换机器"的条目摊开，
> 每条给出可选项、成本、风险与建议，便于一次性决定。
>
> 现状：`derive-from-filmcraft` 110 done / 4 todo；`unify-rust-jianying-engine` 109 / 4；
> `support-jianying-editions-and-official-assets` 27 / 6。
> 本机已能自动化的部分已全部跑通（见 `docs/derivation/11-acceptance-report.md` §11–§13 与
> `docs/runbooks/evidence/`）。

## 1. #84 一键"短视频工程"命令

**现状**：`file.newSequence` + 参数已经能建出竖屏工程（`new_sequence_offers_aspect_presets`
覆盖入口渲染），但"短视频工程"作为**一条命令**不存在。上一轮刻意没有新增命令，以遵守
"不改命令面"的约束。

**为什么需要你拍板**：新增 engine 命令会同时扩：命令目录（701 → 702）、capability manifest、
CLI `describe`/`commands` 输出、MCP 工具面（如果暴露）、i18n 词条、以及一条永久维护成本。

**三个选项**

| 方案 | 内容 | 成本 | 风险 |
|---|---|---|---|
| A. 不加命令（推荐） | 保持"新建序列 + 预设"两步；文档里把竖屏短视频写成一条配方（recipe） | 0 | 无；用户/智能体需要两步而不是一步 |
| B. 加 `project.quickShortVideo` | 一条命令建出 1080×1920/30 + 可选默认字幕轨 + 可选 TTS 旁白 | ~1 天（含测试/目录/capability/文档） | 命令面永久 +1；参数设计一旦发布难改 |
| C. 加通用"配方"命令 | `project.applyRecipe {"recipe":"short-video","params":{...}}`，配方是数据不是命令 | ~2–3 天 | 抽象更重；配方 schema 需要版本化 |

**我的建议**：A。理由：现在两个技能（图文成片、批量混剪）已经用"命令序列"完成同样的事，
再加一条复合命令只会让"能用命令组合"和"必须用复合命令"两条路并存。若你希望降低使用门槛，
选 C 比 B 更值——配方可扩展且不逐个占命令 id。

## 2. #522 craft 原生中文音色

**现状**：`crates/tts-text/src/phonemize.rs` 是**英文专用**（ARPAbet → Kokoro 符号表，
CMUdict 词典 + 拼写规则）；`tts.voices` 只有 en-US（2 个内置 + 9 个 Kokoro 神经音色）。
输入中文会报 `the script has nothing to say`。上一轮已验证：英文神经音色 `kokoro-heart`
经 whisper.cpp 回读**逐字一致**，所以模型侧没问题，缺的是中文前端。

**三个选项**

| 方案 | 内容 | 成本 | 风险 |
|---|---|---|---|
| A. 系统语音（推荐） | 加一个 `platform` provider：Windows SAPI / macOS AVSpeech，中文走系统音色 | 3–5 天（含能力标记、失败降级、测试） | 依赖宿主 OS 语音包；不可用于 wasm 构建；音色由系统决定 |
| B. 自研中文 G2P | 分词 + 拼音 + 声调 + 变调 + 多音字，映射到 Kokoro 中文音素集 | 2–4 周，且需引入拼音词典（pinyin-data MIT / jieba MIT）与 zh 音色 | 多音字与变调错误会直接听错；词典体积；长期维护 |
| C. 只做英文（现状） | 中文场景用 CLI（`jianying media tts --provider system-windows`）绕过 | 0 | craft 自身仍不支持中文配音 |

**我的建议**：A。它能让剪映式"中文旁白"在今天可用，且与既有 CLI 能力一致；等中文配音
成为核心卖点再评估 B。

## 3. #533 三个交互型技能能否判定为"不可脚本化"

**现状**：`verify-skills` 已校验它们的 44 个命令 id 都存在；但这三个技能（编辑器/使用指南/
场景手册）本身是操作说明，没有独立的命令流可端到端断言。

**需要你判定**：是否接受"命令 id 全部有效 + 文档审阅"作为这三条的验收口径。
- 接受 → 我按此口径勾选并写明证据边界。
- 不接受 → 需要定义可脚本化的替代产物（例如把每个技能改写成 N 条可执行的 recipe）。

## 4. #535 ZCode / Codex / Kimi 三宿主实机验收

**现状**：`docs/runbooks/three-host-install.md` 已写好安装、MCP 调用、截图取证三步，
其中 codex/zcode 的命令在本机未安装、标注为 `unverified`。

**需要的输入**：一台干净主机（或允许我在本机安装这三个宿主）。装完后按 runbook 执行，
证据按 `docs/runbooks/evidence/host-acceptance/` 的命名落盘。

## 5. CLI 侧剩下的 10 条

| 变更 | 条目 | 卡点 |
|---|---|---|
| unify | 4.12 pyJYD partial → supported | 需要真剪映差分证据 |
| unify | 8.9 首个支持 Runtime Profile | 需要宿主机 canary（冷重开/播放/导出） |
| unify | 9.3 macOS 剪映 canary | 需要 macOS |
| unify | 9.7 完整 parity/来源/差分/真机门禁 | 依赖上面三条 |
| editions | 4.1–4.5 官方素材权益 | 需要专业账号与可下载的官方素材 |

这些没有"再装一个工具"的空间了：本机已备好 ffprobe、whisper、神经音色、
`runtime discover/probe` 可用，差的是真机与账号。

## 6. 一句话结论

**本机能自动验证的 openspec 条目已经清零。** 剩下的每一项都要么需要一次产品决策
（#84、#522、#533），要么需要一台我没有的机器或账号（#535、9.3、9.7、editions）。
按上面"建议"列执行，我可以立刻继续：#84 选 A 则直接勾选并补文档；#522 选 A 我出一份
platform provider 的实施方案；#533 你给定口径我照此收尾。
