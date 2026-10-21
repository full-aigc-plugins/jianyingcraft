# 三宿主安装验收：ZCode / Codex / Kimi

> 状态：**流程文档，尚未执行**。本文给出可复制的安装、MCP 调用和取证步骤；
> 三张截图和 JSON 由操作者实际跑完后产生。没有实际截图之前，任何宿主都保持 NOT_RUN。

## 1. 这条能证明什么

**验收命题**：`jianyingcraft` 插件能在 **ZCode / Codex / Kimi** 三个真实宿主里安装、
被发现、启动 MCP server、让 Agent 调用工具，并把可见编辑器面板显示给用户；
每个宿主留下"安装 + MCP 调用 + 画面"三样证据。

关闭的 openspec 任务：

| 仓库 | 位置 | 任务 |
|---|---|---|
| `jianyingcraft` | `openspec/changes/derive-from-filmcraft/tasks.md` 任务 **#499** | `ZCode / Codex / Kimi 实机验收（安装 + MCP 调用 + 画面三段证据）` |
| `jianyingcraft-plugin` | `README.md`「当前状态」 | 四宿主实机验收由 `NOT_RUN` 变为有证据（是否改 `supportedPluginHosts` 是后续发布决策） |

**判定标准（每个宿主单独判定）**：

1. 宿主里能看到插件 `jianyingcraft 0.1.0`，且安装诊断无错误；
2. 新会话里 MCP server `jianyingcraft-editor` 已连接，至少成功调用
   `engine_diagnose`、`open_editor`、`engine_cli_list`、`command_list`、`command_run`、`screenshot`；
3. `open_editor` 返回的 `editorUrl` 在宿主面板/浏览器里真的打开，
   `editor_status.ready` 变为 `true`，`screenshot` 返回 PNG；
4. 留一张能看清"宿主 + 面板"的截图，以及一份 JSON 记录。

> `command_run` / `screenshot` 走**可见面板**；`engine_cli_run` 走**宿主侧子进程**。
> 两者不能互相冒充：`engine_cli_run` 成功不能证明面板已经打开。

## 2. 前置条件与逐项检查

| 依赖 | 要求 | 检查命令 | 期望 |
|---|---|---|---|
| Node.js | 24+（adapter 的 `target: node24`） | `node --version` | `v24.x` 或更高 |
| npm | 随 Node | `npm --version` | 有版本 |
| 宿主 CLI | 至少一个已安装 | `codex --version` / `zcode --version` / `kimi --version` | 有版本；记录到证据 |
| 插件源 | `jianyingcraft-plugin` 同级仓 | `Test-Path ..\jianyingcraft-plugin\ui\dist\adapter.mjs` | `True` |
| CLI 锁 | `ui/cli.lock.json` | `Get-Content ..\jianyingcraft-plugin\ui\cli.lock.json -Raw` | `resolvedVersion: 1.6.31` |
| 工作区 | 一个绝对路径，插件只写其内部 | `New-Item -ItemType Directory -Force $WS` | 存在 |
| 宿主账号 | 已登录、能新建任务/安装本地插件 | 打开宿主确认 | 不是只读/访客模式 |
| 网络 | 首次运行可能下载 1.6.31 CLI；离线时用 `JIANYINGCRAFT_CLI_BIN` | 见 §2.1 | — |

**本机实测基线（2026-10-20）**：Node `v24.14.1`、npm `11.11.0`、Kimi `0.27.0`；
Codex/ZCode 未安装。插件测试 **46 tests / 44 pass / 2 skip / 0 fail**（skip 是
`CHROME_PATH` 和 `JIANYINGCRAFT_WASM_JS` 未设置，不是失败）。

> 没有 Codex/ZCode 的机器上，不能把"manifest 结构正确"写成"宿主验收通过"。
> 每个宿主必须单独跑、单独记录。

### 2.1 可选：固定使用本机 CLI，避免下载

```powershell
$env:JIANYINGCRAFT_CLI_BIN = "E:\workspaces\workspace-agent-skills\full-aigc-plugins-repositories\jianying-cli\target\debug\jianying.exe"
$env:JIANYINGCRAFT_CLI_UPDATE = "off"
```

```sh
export JIANYINGCRAFT_CLI_BIN="$HOME/workspaces/full-aigc-plugins-repositories/jianying-cli/target/debug/jianying"
export JIANYINGCRAFT_CLI_UPDATE=off
```

设置后 adapter 会跳过下载，直接校验/使用该二进制；版本不匹配会报可诊断错误，不会静默降级。

### 2.2 构建并跑基线测试

工作目录：`<ABS_PLUGIN>/ui`

```powershell
$PLUGIN = "E:\workspaces\workspace-agent-skills\full-aigc-plugins-repositories\jianyingcraft-plugin"
Push-Location (Join-Path $PLUGIN "ui")
npm ci
npm run build
Get-ChildItem .\dist | Select-Object Name,Length
Pop-Location
node --test (Get-ChildItem (Join-Path $PLUGIN "ui\tests") -Filter "*.test.mjs" | ForEach-Object FullName)
```

```sh
PLUGIN="$HOME/workspaces/full-aigc-plugins-repositories/jianyingcraft-plugin"
cd "$PLUGIN/ui"
npm ci
npm run build
ls -l dist
node --test tests/*.test.mjs
```

期望：`npm run build` 输出 `jianyingcraft-dist/v1` 的 manifest；`node --test` 是
`tests 46 / pass 44 / fail 0 / skipped 2`。`e2e.test.mjs` 单独跑时是 **2 passed**，
它会真的拉起 `dist/adapter.mjs`、真的调 CLI、真的落草稿。

> `npm ci` 若报 lockfile 与 `package.json` 不一致，改用 `npm install` 并记录
> `package-lock.json` 的变化；不要用 `--force` 掩盖。离线且 `node_modules/` 已存在时，
> 可以跳过 `npm ci` 直接 `npm run build`，但要在证据里写明"未做干净安装"。
> `dist/` 是构建产物，只用于本次安装，不要把它当成源码提交。

## 3. 步骤

三个宿主的共同前提：

- 用**绝对路径**指向插件目录与 adapter；
- 一个宿主只开一个面板（单活页；第二个会 `panel_busy`）；
- MCP server 命令统一是 `node <ABS_PLUGIN>/ui/dist/adapter.mjs`，工作目录 `<ABS_PLUGIN>`；
- 工作区通过 `open_editor.workspace` 传入，或设 `CRAFT_WORKSPACE_ROOT`（绝对路径）。

### 3.1 Codex

> `codex` 子命令在本机未安装，下面标了 `# unverified` 的行必须在目标机先用
> `codex plugin --help` 确认。市场名以 `codex plugin marketplace list` 输出为准。

```powershell
$PLUGIN = "E:\workspaces\workspace-agent-skills\full-aigc-plugins-repositories\jianyingcraft-plugin"
codex --version
codex plugin --help

# unverified — check with --help
codex plugin marketplace add "$PLUGIN" --json
# marketplace 名可能是 jianyingcraft 或 jianyingcraft-plugin；用 list 确认后替换 <marketplace>
codex plugin marketplace list --json
# unverified — check with --help
codex plugin add "jianyingcraft@<marketplace>" --json
# unverified — check with --help
codex plugin list --json
```

```sh
PLUGIN="$HOME/workspaces/full-aigc-plugins-repositories/jianyingcraft-plugin"
codex --version
codex plugin --help
# unverified — check with --help
codex plugin marketplace add "$PLUGIN" --json
codex plugin marketplace list --json
# unverified — check with --help
codex plugin add "jianyingcraft@<marketplace>" --json
# unverified — check with --help
codex plugin list --json
```

如果你不想走市场，按目标机 `codex mcp --help` 给出的格式注册一个 stdio server：
命令 `node`，参数指向 `<ABS_PLUGIN>/ui/dist/adapter.mjs`，工作目录 `<ABS_PLUGIN>`。
不要手改 `.codex-plugin/mcp.json`——它是插件清单，不是用户级配置。

期望：`plugin list` 里出现 `jianyingcraft 0.1.0`，没有 diagnostics。然后新建一个 Codex
任务，按 §3.4 调用工具。

### 3.2 ZCode

> ZCode 0.16.9 的插件管理器命令在姊妹仓 `jianying-draft-plugin` 的
> `docs/rust-cli-migration/HOST_ACCEPTANCE_STATUS.md` 有真实记录：`zcode plugins marketplace add` /
> `zcode plugins install` / `zcode plugins list`。本机未安装 ZCode，仍需在目标机用
> `zcode plugins --help` 确认。

```powershell
$PLUGIN = "E:\workspaces\workspace-agent-skills\full-aigc-plugins-repositories\jianyingcraft-plugin"
zcode --version
zcode plugins --help

# unverified — check with --help
zcode plugins marketplace add "$PLUGIN" --scope user --json
zcode plugins marketplace list --json
# unverified — check with --help
zcode plugins install "jianyingcraft@<marketplace>" --scope user --json
zcode plugins list --json
```

```sh
PLUGIN="$HOME/workspaces/full-aigc-plugins-repositories/jianyingcraft-plugin"
zcode --version
zcode plugins --help
# unverified — check with --help
zcode plugins marketplace add "$PLUGIN" --scope user --json
zcode plugins marketplace list --json
# unverified — check with --help
zcode plugins install "jianyingcraft@<marketplace>" --scope user --json
zcode plugins list --json
```

期望：`plugins list --json` 里 `jianyingcraft` 的 `state` 正常、`diagnostics` 为空。
如果报 `plugin_hook_invalid`，先检查 `.zcode-plugin/plugin.json` 是否重复声明了
`hooks/hooks.json`（ZCode 会自动发现一次）；本插件当前没有 hooks 目录，出现这个错误
说明用了别的插件清单。

ZCode 的 CSP 需要放行 WASM：确认 `.zcode-plugin/panel-meta.json` 被宿主读取：

```json
{
  "zcode/csp": {
    "wasmUnsafeEval": true
  }
}
```

### 3.3 Kimi

Kimi 0.27.0 的 `kimi --help` **没有** `plugin` 子命令（本机实测），所以不能照搬
ZCode/Codex 的市场安装。已知可行的做法（Blender MCP 在 Kimi 0.43.1 上的记录）是：
把插件放进 `~/.kimi-code/plugins/`，在 `~/.kimi-code/mcp.json` 追加 MCP server，
再用 `--skills-dir` 或 `extra_skill_dirs` 加载技能。

```powershell
$PLUGIN = "E:\workspaces\workspace-agent-skills\full-aigc-plugins-repositories\jianyingcraft-plugin"
$KIMI = Join-Path $env:USERPROFILE ".kimi-code"
kimi --version
kimi doctor

New-Item -ItemType Directory -Force (Join-Path $KIMI "plugins") | Out-Null
Copy-Item -Recurse -Force $PLUGIN (Join-Path $KIMI "plugins\jianyingcraft")
```

```sh
PLUGIN="$HOME/workspaces/full-aigc-plugins-repositories/jianyingcraft-plugin"
KIMI="$HOME/.kimi-code"
kimi --version
kimi doctor
mkdir -p "$KIMI/plugins"
cp -R "$PLUGIN" "$KIMI/plugins/jianyingcraft"
```

追加 MCP 配置（先备份；只追加 `jianyingcraft-editor`，不要覆盖已有 server）：

```powershell
$mcpPath = Join-Path $KIMI "mcp.json"
$pluginRoot = Join-Path $KIMI "plugins\jianyingcraft"
if (Test-Path $mcpPath) { Copy-Item $mcpPath "$mcpPath.bak" }
@'
import json, os, sys
p, root = sys.argv[1], sys.argv[2]
d = json.load(open(p, encoding="utf-8")) if os.path.exists(p) else {}
d.setdefault("mcpServers", {})["jianyingcraft-editor"] = {
    "command": "node",
    "args": [root.replace("\\", "/") + "/ui/dist/adapter.mjs"],
    "cwd": root.replace("\\", "/")
}
with open(p, "w", encoding="utf-8", newline="\n") as f:
    json.dump(d, f, ensure_ascii=False, indent=2)
    f.write("\n")
print("updated", p)
'@ | python - $mcpPath $pluginRoot
```

```sh
python3 - "$KIMI/mcp.json" "$KIMI/plugins/jianyingcraft" <<'PY'
import json, os, shutil, sys
p, root = sys.argv[1], sys.argv[2]
d = json.load(open(p, encoding="utf-8")) if os.path.exists(p) else {}
if os.path.exists(p):
    shutil.copy2(p, p + ".bak")
d.setdefault("mcpServers", {})["jianyingcraft-editor"] = {
    "command": "node",
    "args": [root + "/ui/dist/adapter.mjs"],
    "cwd": root,
}
with open(p, "w", encoding="utf-8", newline="\n") as f:
    json.dump(d, f, ensure_ascii=False, indent=2)
    f.write("\n")
print("updated", p)
PY
```

启动新会话时显式带技能目录（`--skills-dir` 是 `kimi --help` 里已存在的选项）：

```sh
kimi --skills-dir "$KIMI/plugins/jianyingcraft/skills"
```

```powershell
kimi --skills-dir (Join-Path $KIMI "plugins\jianyingcraft\skills")
```

再跑一次 `kimi doctor`。期望：配置校验通过；新会话里能看到 `jianyingcraft-editor` 的工具。
**如果看不到**，不要改结论为通过：记录 `kimi --version`、`mcp.json` 路径和 doctor 输出，
把 Kimi 标成 `blocked`。

> `kimi doctor` 在本机只校验 `config.toml` / `tui.toml`，不校验 MCP。MCP 是否被加载，
> 必须在**新会话**里实际调用一次工具才能确认。

### 3.4 三个宿主共用的 MCP 验收调用

在宿主的新任务里让 Agent 依次调用（不是 shell 命令；参数按宿主 UI 填写）：

| # | 工具 | 参数 | 期望 |
|---|---|---|---|
| 1 | `engine_diagnose` | `{}` | `supported: true`，`lockedVersion: "1.6.31"`，`cached.ok: true`，`platform` 与宿主平台一致 |
| 2 | `open_editor` | `{"workspace": "<ABS_WORKSPACE>"}` | `editor: "jianyingcraft"`，`workspace` 是绝对路径，`editorUrl` 形如 `http://127.0.0.1:<port>/<64 hex>/app/`，`ready: false` |
| 3 | 人工 | 打开 `editorUrl`（宿主面板或浏览器） | 页面出现 JianyingCraft 编辑器；轮询几次 `editor_status` 后 `ready: true` |
| 4 | `engine_cli_list` | `{}` | `schema: "jianying-command-catalog/v1"`，`catalogueTotal >= 150`，`returned == catalogueTotal` |
| 5 | `command_list` | `{"filter": "sequence"}` | 可见引擎的命令目录，包含 `sequence` 相关 id |
| 6 | `command_run` | `{"id": "sequence.inspect"}` | 结构化结果；面板 revision 推进（可用 `editor_status` 对比） |
| 7 | `screenshot` | `{}` | 返回 PNG（`content` 里的 `type: "image"`，或 `structuredContent` 里的路径；以实际工具返回为准）；长度 > 0，文件头 `\x89PNG` |
| 8 | `list_files` | `{}` | 工作区文件列表；可选 |

`open_editor` 返回的 `ready` 在面板连上之前必须是 `false`；如果它直接返回 `true`，
说明状态自述有问题，不能作为通过证据。

`engine_diagnose` 的实测形状（来自 `ui/tests/e2e.test.mjs` 的断言）：

```json
{
  "lockedVersion": "1.6.31",
  "platform": "win32-x64",
  "supported": true,
  "cached": {
    "ok": true,
    "sha256": "fdda1e76dd0a4d63faafee5fd808e89f44186e392f50fb3a5b4787de61956980"
  }
}
```

macOS 上 `platform` 是 `darwin-arm64` 或 `darwin-x64`，`cached.sha256` 对应
`ui/cli.lock.json` 里该平台的值。

`command_run` 只在面板心跳新鲜时可用；没开面板会得到
`Open the editor panel`（15 秒心跳超时）。这属于预期边界，不是"工具坏了"。

### 3.5 截图（每个宿主三选二，至少一张带宿主）

| 文件 | 怎么拍 | 必须看清 |
|---|---|---|
| `<host>-install.png` | 宿主插件列表/管理页 | 插件名 `jianyingcraft`、版本 `0.1.0`、无错误标记 |
| `<host>-panel.png` | 用 `screenshot` 工具保存的 PNG | 编辑器面板、时间线、素材区；能对应到刚才的 `open_editor` |
| `<host>-window.png` | 系统截图（macOS `Cmd+Shift+4`；Windows `Win+Shift+S`） | 宿主窗口 + 面板同框；宿主名称/版本 |

三张图放在：

`docs/runbooks/evidence/host-acceptance/`

命名：`<host>-install-20261020.png`、`<host>-panel-20261020.png`、`<host>-window-20261020.png`。
这些图从宿主机截取后复制到本仓证据目录；不要提交含个人账号/隐私的截图。

## 4. 期望结果（汇总）

| 检查 | Codex | ZCode | Kimi |
|---|---|---|---|
| 宿主版本可读 | `codex --version` 有输出 | `zcode --version` 有输出 | `kimi --version` 有输出 |
| 插件被列出 | `jianyingcraft 0.1.0` | `jianyingcraft 0.1.0`，diagnostics 空 | 新会话能看到 `jianyingcraft-editor` 工具 |
| MCP 工具调用 | `engine_diagnose` 等 6 项成功 | 同左 | 同左 |
| 面板 ready | `editor_status.ready: true` | 同左 | 同左 |
| 截图 | install/panel/window | 同左 | 同左 |
| 证据 JSON | `result: "passed"` | 同左 | 同左 |

基线测试期望（与本机一致）：

```text
tests 46
pass 44
fail 0
skipped 2
```

## 5. 证据怎么留

每个宿主一份 JSON，放在
`docs/runbooks/evidence/host-acceptance/<host>-20261020.json`：

```json
{
  "schema": "jianyingcraft-host-acceptance/v1",
  "host": "zcode",
  "host_version": "0.16.9",
  "plugin": {
    "name": "jianyingcraft",
    "version": "0.1.0",
    "source": "local checkout",
    "adapter_sha256": "1111111111111111111111111111111111111111111111111111111111111111"
  },
  "install": {
    "method": "zcode plugins install",
    "marketplace": "jianyingcraft",
    "diagnostics": []
  },
  "mcp": {
    "server": "jianyingcraft-editor",
    "tools_total": 17,
    "tools_public": 14,
    "engine_version": "1.6.31",
    "engine_cli_catalogue_total_at_least": 150,
    "visible_command_total": 701,
    "calls": [
      "engine_diagnose",
      "open_editor",
      "engine_cli_list",
      "command_list",
      "command_run",
      "screenshot"
    ]
  },
  "panel": {
    "ready": true,
    "editor_url": "http://127.0.0.1:45678/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef/app/",
    "screenshot_sha256": "2222222222222222222222222222222222222222222222222222222222222222"
  },
  "screenshots": {
    "install": "docs/runbooks/evidence/host-acceptance/zcode-install-20261020.png",
    "panel": "docs/runbooks/evidence/host-acceptance/zcode-panel-20261020.png",
    "window": "docs/runbooks/evidence/host-acceptance/zcode-window-20261020.png"
  },
  "result": "passed",
  "observed_at": "2026-10-20T15:30:00+08:00",
  "notes": "面板打开后 ready 才为 true；未执行 13 个技能中的任何一个完整创作流程"
}
```

`adapter_sha256` 用 `shasum -a 256 ui/dist/adapter.mjs` 或
`Get-FileHash -Algorithm SHA256 ui\dist\adapter.mjs`。`screenshot_sha256` 用同样方法算。

**不要**把 `result` 写成 `passed` 除非三样证据齐全。缺截图或 MCP 调用失败时写
`failed` 或 `blocked`，并在 `notes` 写清原因。

> 示例里的哈希、端口、token、版本都是占位；必须用实际 `Get-FileHash` / `shasum`、
> 浏览器地址栏里的真实 URL 和宿主实际版本替换。

## 6. 失败时先看这 5 种

| 现象 | 含义 | 处理 |
|---|---|---|
| `Cannot find module '@modelcontextprotocol/sdk'` 或 `MCP error -32000: Connection closed` | `dist/` 没构建或 `node_modules` 缺失；adapter 启动即退出 | 在 `ui/` 跑 `npm ci && npm run build`；手工跑 `node ui/dist/adapter.mjs` 看 stderr；确认 `dist/config.json` 存在 |
| `Call open_editor first` | 未打开工作区就调 `engine_cli_*` | 先 `open_editor {workspace}`；工作区必须是绝对路径 |
| `Open the editor panel`（`screenshot` / `command_run`） | 面板没连上，心跳超 15 秒 | 打开 `editorUrl`；检查宿主 CSP（ZCode 的 `wasmUnsafeEval`）；确认没有第二个面板占着单活页 |
| `Save and close this instance` / `already has an active page` / `panel_busy` | 第二次 `open_editor` 或第二个面板 | 关闭旧面板/会话；重启宿主；一个宿主只开一个实例 |
| `${CLAUDE_PLUGIN_ROOT}` 原样出现 / `ENOENT ... adapter.mjs` | 宿主不展开占位符，或路径不是绝对路径 | 把 MCP 配置里的 `${CLAUDE_PLUGIN_ROOT}` 换成绝对插件路径；`cwd` 也写绝对路径 |

Kimi 专属：

- `kimi` 没有 `plugin` 子命令：这是 0.27.0 的事实，不是安装错误；走 §3.3 的手工安装。
- `kimi doctor` 通过但新会话没有 MCP 工具：MCP 没被加载；记录版本并标 `blocked`，
  不要用 `doctor` 的通过冒充 MCP 通过。

## 7. 本文不证明什么

- **不证明 13 个技能都能跑**。本文只做安装 + MCP 冒烟 + 面板可见；技能实跑见
  `skill-reruns.md`，两者不能互相替代。
- **不证明创作正确性**。`screenshot` 只能证明"画面出来了"，不证明剪得对、导出对。
- **不证明其他宿主版本**。只覆盖记录下来的 ZCode/Codex/Kimi 版本；升级后要重跑。
- **不自动发布或提升支持级别**。`supportedPluginHosts`、市场发布、CI 门禁是另外的决策；
  三份 JSON 先作为评审输入。也**不自动改写**插件的 `runtime/host-acceptance.json`——
  那需要维护者复核三份证据后再更新。
- **不证明宿主内嵌面板与浏览器打开等价**。如果只是在浏览器里打开 `editorUrl`，
  要在 `notes` 写明；真正的宿主内嵌验收需要 MCP Apps/宿主面板里的截图。

## 8. Kimi Code CLI（2026-10-22 实测通过）

```powershell
# 1) 安装/确认（Moonshot 官方 Kimi Code CLI，不是 npm 上的同名占位包）
kimi --version        # 0.27.0
# 2) 插件仓里放项目级 MCP 配置：jianyingcraft-plugin/.kimi-code/mcp.json
#    {"mcpServers":{"jianyingcraft-editor":{"command":"node","args":["ui/dist/adapter.mjs"]}}}
# 3) 从插件仓根目录做一次非交互调用
cd jianyingcraft-plugin
kimi -p "你接入了 jianyingcraft-editor MCP。列出全部工具名，并调用 command_run 执行 sequence.inspect。"
```

实测结果：列出 **17 个工具**，`command_run sequence.inspect` 返回演示工程的
`{"audio":[…],"video":[…]}`；会话 `session_bbaf4039-3153-4d2d-9bdc-ad0bf6e54ce4`。
`-p` 不能与 `--yolo`/`--auto` 同用（CLI 会直接报错），非交互模式下按提示逐条允许即可。

画面证据由 `node --test tests/panel-live.test.mjs tests/panel.test.mjs`（`CHROME_PATH` 指向 Chrome）
生成到 `runtime/panel-evidence/editor-page.png`，显示「引擎已加载 / 面板桥就绪 · 701 命令」。
注意：这是 headless Chrome 打开的面板页，不是 Kimi 自带的 webview（Kimi CLI 没有 GUI）。
证据文件：`docs/runbooks/evidence/host-acceptance/KIMI_20261022.json`。