# 剪映专业版功能与界面参考（本地实测）

> 取证对象：本机安装的 **剪映专业版 11.6.0.14552**
> 安装路径：`C:\Users\hnxyh\AppData\Local\JianyingPro\Apps\11.6.0.14552`
> 取证方式：安装目录清点 + gettext 翻译目录（`Resources/po/zh-Hans.po`）+ 用户提供的界面截图 + REA 6.7.0 静态分析
> 用途：**只作功能与交互参考**。不复制其资源、代码、文案；不读写 `.json` 私有草稿格式；不接账号/云/支付。

## 1. 证据与规模

| 证据 | 实测值 |
|---|---|
| 主程序 | `JianyingPro.exe`（Qt6 + CEF：`Qt6Quick.dll`、`libcef.dll`、`resources.pak`） |
| 编辑内核 | `VECreator.dll`（272 MB）、`videoeditor.dll`（72 MB）、`vebun.exe`（108 MB） |
| AI/推理 | `onnxruntime*.dll`、`Resources/models`、`Resources/canvas_agent`（含 `chinese-clip-vit` ONNX） |
| 界面文案 | `Resources/po/zh-Hans.po`：**msgstr 18,093 条**，其中 ≤8 字的候选标签 **6,777 条** |
| 功能模块 | `Resources/` 下 **60+ 个模块目录**（见 §3） |
| REA | `rea-agents@6.7.0`；`doctor` 显示各客户端 MCP 未注册（故只用 CLI，未执行 `rea setup`） |

> 重要：下表计数是**文案命中数**，不是功能数。同一个功能会有十几条提示文案
> （"AI补帧"/"AI补帧完成"/"AI补帧已取消"…），因此只能用来判断"这块有多重"。

## 2. 文案分桶（zh-Hans.po 实测）

| 板块 | 命中 | 代表性文案 |
|---|---|---|
| 素材/媒体 | 1046 | 已离线、AI素材、Pro素材、%1个素材待替换 |
| 音频 | 532 | AI 音乐、AI音效、10秒音频即刻克隆、响度 |
| 文本/字幕 | 841 | AI 字幕识别、AI 图搜字体、AI 文字成片、花字 |
| 贴纸/图形 | 126 | AI 贴纸、AIGC贴纸、{X}个贴纸 |
| 特效 | 212 | AI特效生成中、90s特效、%1个滤镜 |
| 转场 | 54 | AI 转场生成中、撤销 AI 转场效果 |
| 调色 | 108 | HSL、log色轮、LUT、AI 智能调色 |
| 蒙版/抠像 | 134 | 从时间轴向右跟踪、编辑跟踪平面、智能抠像 |
| 变速/动画 | 198 | AI补帧、曲线变速、仅对慢速片段补帧 |
| 模板/成片 | 398 | AI模板、AI生成文字模板、剪同款 |
| 数字人/配音 | 340 | %1个数字人正在渲染、5秒克隆专属音色、TTS |
| 导出/发布 | 1039 | %1 导出成功、%1次免费导出、码率/分辨率 |
| AI 能力 | 1342 | AI积分规则、智能裁剪、自动重绘 |
| 协作/云 | 818 | 个人云空间、草稿可备份、团队协作 |

## 3. 模块目录（`Resources/`，节选）

`canvas_agent`（AI 画布）、`digital_human`（数字人）、`tts`（本地音色）、`figure`、
`SceneEditDetection`（场景切分）、`lvop_intelligent_crop`（智能裁剪）、`export_publish`（发布）、
`image_h5_smart_lyrics` / `smart_text` / `smart_voiceover_script*`（歌词/文字/口播稿）、
`image_h5_material_publish` / `material_shop`（素材上架/商店）、`CloudMaterial`（云素材）、
`Chroma` / `Chroma2`（色度抠像）、`Curves` / `HSL` / `PrimaryWheel` / `DefaultAdjustBundle`（调色）、
`mattingBlend*` / `rule_adjust_mask`（抠像/蒙版）、`MixMode` / `lut` / `Font` / `template_sticker`。

界面层：主编辑器是 **Qt6 + CEF 混合**（不是纯 Web），因此没有可直接读的 QML/JS UI 源码；
可稳定取证的是**文案目录**与**模块清单**，不是控件树。

## 4. 界面结构（截图 + 文案交叉验证）

| 区域 | 内容 |
|---|---|
| 顶部 | 13 个一级 Tab：素材/音频/文本/贴纸/特效/转场/字幕/智能包装/滤镜/调节/模板/数字人/插件 |
| 左栏 | 导入、素材、子草稿、我的、AI 生成、云素材、官方素材、即梦 AI |
| 中部 | 预览 + 时间帧显示 + 播放控制 |
| 右栏 | 未选中片段时是**草稿参数**（名称/保存位置/导出路径/导入方式/自由层级/素材代理/比例/分辨率/帧率） |
| 底部 | 时间轴 + 工具条（撤销/分割/定格/倒放/镜像/旋转/裁剪/速度/音量/降噪/变声 + 吸附/主轨磁吸/联动） |
| 空态 | "素材拖拽到这里，开始你的大作吧~" |

## 5. 与 JianyingCraft 命令面的对照（实测 id）

| 剪映能力 | 我们的命令面 | 状态 |
|---|---|---|
| 字幕识别 | `transcript.generate`（Whisper）、`transcript.createCaptions` | ✅ 已有（14 条 transcript.*） |
| 字幕样式/校时/导出 | `captions.setStyle` / `captions.setTimes` / `captions.export`（25 条） | ✅ |
| 文本/花字/贴纸 | `graphics.*`（83 条） | ✅ |
| 特效 | `effects.*`（17 条，含关键帧） | ✅ |
| 转场 | `sequence.applyVideoTransition` / `applyAudioTransition` | ✅ |
| 调色 | `sequence.colorSettings`、`project.matteColor`、`file.newColorMatte`、Lumetri 面板 | ⚠️ 有基础，缺 HSL/色轮级命令 |
| 蒙版/跟踪 | `masks.*`（12 条，含 `masks.track`） | ✅ |
| 变速/补帧 | `clip.speedDuration`（`interpolation: opticalFlow`） | ✅（光流补帧） |
| 音频修复/降噪/闪避 | `essentialSound.*`（9 条） | ✅ |
| 变声 | 无 pitch/voice-change 命令 | ❌ 缺口 |
| 数字人 | 无 | ❌ 不做（云端） |
| 模板/剪同款 | 无 | ⏳ 待做（本地模板） |
| AI 生成（图/视频/扩图） | 无 | ❌ 不做（云端） |
| 云素材/团队/支付 | 无 | ❌ 不做 |
| 代理 | 有 proxy 能力（`proxy` 相关 1 条 + 引擎作业） | ⚠️ UI 未暴露 |

## 6. 本轮由参考得出的两个修正

1. **技能写错了**：`jianyingcraft-captions` 原写"引擎没有内建 ASR"，实际有 `transcript.generate`
   （Whisper）+ `transcript.createCaptions`。已改为"引擎内转写优先，外部 SRT 兜底"，
   并要求只在真的执行过 `transcript.generate` 时才声称做过识别。
2. **优先级调整**：剪映的重量在"素材生态 + AI 生成 + 云/支付"（约 3200 条文案），
   这部分我们**不做**；真正该补的本地能力只有：变声（pitch）、本地模板、代理 UI。

## 7. 复现命令

```powershell
# 文案规模
$po = "$env:LOCALAPPDATA\JianyingPro\Apps\11.6.0.14552\Resources\po\zh-Hans.po"
(Select-String -Path $po -Pattern '^msgstr ' ).Count

# 模块清单
Get-ChildItem "$env:LOCALAPPDATA\JianyingPro\Apps\11.6.0.14552\Resources" -Directory | Select-Object Name

# REA（只读；不执行 setup，不改任何客户端配置）
npx -y rea-agents@latest doctor
npx -y rea-agents@latest analyze "<安装目录>" --format md --token-limit 4000
```

REA 的 `analyze` 会输出逐文件进度（本次 1431 个文件），完整 JSON 达 171 MB，**未纳入仓库**；
它的价值是确认"安装目录里有哪些 JS/配置模块"，功能判断以 PO 文案与模块清单为准。

## 8. 边界

- 不复制剪映的任何资源、图标、字体、文案原文；本文只记录"有哪些能力"。
- 不实现 `.json` 私有草稿格式的读写，不接剪映账号/云/支付。
- 参考的是**功能集合与交互习惯**，实现是我们自己的命令面（702 条，见 `target/commands.json`）。
