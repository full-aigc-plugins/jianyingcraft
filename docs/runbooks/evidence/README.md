# Runbook 证据

本目录存放**尚未提交**的验收证据模板与操作者实际产出的 JSON。规则：

- 每份 JSON 必须有 `schema` 字段（例如 `jianyingcraft-skill-rerun/v1`、
  `jianyingcraft-host-acceptance/v1`）；跨仓的 CLI 证据优先放
  `../jianying-cli/provenance/`，遵循那边的 `jianying-evidence-record/v1` 分级。
- 只写**实际观察到的值**。SHA-256、字节数、词数、时长、版本都是实测；不要用占位值
  冒充通过。
- 媒体文件（MOV/MP4/WAV/PNG 截图）默认不提交；证据里写仓库外绝对路径 + SHA-256。
  宿主截图除外——它们是三宿主验收的必要证据，放
  `docs/runbooks/evidence/host-acceptance/`，并确认不含账号、令牌、个人路径。
- 一个 runbook 一份主证据；需要附加原始命令输出时，用同名 `.raw.json` / `.log`，
  不要改写主证据。
- `result` 只能是 `passed` / `failed` / `blocked`。缺一样必要证据就是 `blocked`，
  不是 `passed`。

建议的最小骨架：

```json
{
  "schema": "jianyingcraft-runbook-evidence/v1",
  "runbook": "skill-reruns",
  "observed_at": "2026-10-20T15:30:00+08:00",
  "host": {
    "os": "windows",
    "version": "10.0.26100"
  },
  "result": "blocked",
  "artifacts": {
    "command_log": "docs/runbooks/evidence/skill-reruns-20261020.log"
  },
  "notes": "缺少 ffprobe 与 Whisper 模型；未执行"
}
```
