# jianyingcraft-product 规格增量

## ADDED Requirements

### Requirement: 派生可追溯
JianyingCraft SHALL 由 filmcraft 派生，且派生关系可追溯到具体上游提交。

#### Scenario: 上游可合并
- **WHEN** 需要吸收上游 filmcraft 的修复
- **THEN** 仓库 MUST 保留指向 storytold/filmcraft 的 `upstream` remote，且本地改动不得改写上游历史

#### Scenario: 来源声明
- **WHEN** 交付派生代码
- **THEN** `docs/derivation/` MUST 说明派生自哪个版本与提交、改了什么、许可义务如何满足

### Requirement: 产品名一致
所有**用户可见**的产品名 SHALL 为 JianyingCraft。

#### Scenario: 显示名
- **WHEN** 用户打开桌面窗口、Web 页或崩溃对话框
- **THEN** 显示名 MUST 为 JianyingCraft，不得显示 FilmCraft

#### Scenario: 内部标识不强制改名
- **WHEN** 检视 crate 名、模块路径与文件格式扩展名
- **THEN** 允许保留 `filmcraft-*` 与 `.fcproj`，但 MUST 在派生文档中说明理由

### Requirement: 商标合规
派生品 SHALL NOT 使用第三方商标资产。

#### Scenario: 字标
- **WHEN** UI 绘制产品字标
- **THEN** MUST 使用文字绘制或自有资产，不得加载 ArtCraft 商标图片

#### Scenario: 上游链接诚实
- **WHEN** Help 菜单包含指向上游社区/仓库的链接
- **THEN** 标签 MUST 注明其为上游（upstream）页面，不得标注成本产品的自有页面

### Requirement: 上游能力保留
派生 SHALL 保留上游已实现的媒体能力与命令面。

#### Scenario: 命令面
- **WHEN** 打开派生后的引擎
- **THEN** 上游 165 条引擎命令 MUST 全部仍可用

#### Scenario: 导入导出
- **WHEN** 使用上游支持的容器与编码导入或导出
- **THEN** 行为 MUST 与派生基线的上游版本一致，不得因品牌改造退化
