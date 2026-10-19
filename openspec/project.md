# JianyingCraft 项目约定

- 产品：JianyingCraft —— 独立的智能剪辑软件，交互取向借鉴短视频剪辑工具，工程格式与引擎自持。
- 技术基座：派生自 storytold/filmcraft（MIT OR Apache-2.0），保留 upstream remote 以合并上游更新。
- 规格事实源：`openspec/changes/`。行为变更先改 spec 与测试，再改实现。
- 命名：内部 crate 名保留 `filmcraft-*`（上游契约，降低合并成本）；所有用户可见名称用 `JianyingCraft`。
