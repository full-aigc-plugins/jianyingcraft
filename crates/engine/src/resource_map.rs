//! 剪映「按编号选型」到引擎转场 effect id 的**手工验证子集**映射。
//!
//! 剪映导出的草稿里，转场资源带一个 19 位编号（`resourceId`），它对应剪映自己的转场素材；
//! 引擎能渲染的是 [`filmcraft_project::vtransition`] 里的转场定义（`cross_dissolve` 等）。
//! 本模块只收录**语义已逐条人工核对过**的编号，用它把 `resourceId` 解析成引擎 effect id，
//! 让 `effects.apply` 和 `sequence.applyVideoTransition` 能在只给编号时真正执行。
//!
//! **这不是完整映射，也不允许被当成完整映射使用**：
//!
//! - 表中没有的编号一律报错（见 [`unmapped_effect_message`] / [`unmapped_transition_message`]），
//!   绝不按名称猜测、绝不回退到默认转场——猜错会悄悄改变用户片子的观感；
//! - 音频过渡不在本表范围内，只给 `resourceId` 时明确报「音频过渡暂无映射」；
//! - 同时给 `effect` 和 `resourceId` 时以 `effect` 为准，编号只作为实例元数据写入。
//!
//! 扩充方式见 `docs/action-plan-mapping.md`：先人工核对语义，再加一行，并补测试。

/// 已人工核对的映射表：`(剪映 resourceId, 剪映名称, 引擎 effect id)`。
///
/// 只有语义明确、能在 [`filmcraft_project::vtransition::find_transition`] 里查到的转场才允许
/// 出现在这里；一行一个编号，重复编号视为错误（由测试保证）。
pub static RESOURCE_TRANSITIONS: &[(&str, &str, &str)] = &[
    ("6724845717472416269", "叠化", "cross_dissolve"),
    ("6724845376098013708", "闪白", "dip_to_white"),
    ("6724239388189921806", "闪黑", "dip_to_black"),
    ("6724226861666144779", "推近", "push"),
    ("6747979085894390279", "翻页", "page_turn"),
];

/// 剪映编号 `resource_id` 对应的引擎转场 effect id；未收录返回 `None`。
///
/// 返回静态字符串引用，调用方按需 `to_string()`；`None` **必须**变成可读错误，不能猜。
pub fn engine_transition_for(resource_id: &str) -> Option<&'static str> {
    let id = resource_id.trim();
    RESOURCE_TRANSITIONS.iter().find(|(rid, _, _)| *rid == id).map(|(_, _, engine)| *engine)
}

/// 剪映编号对应的剪映名称（诊断与文档用）；未收录返回 `None`。
pub fn jianying_name(resource_id: &str) -> Option<&'static str> {
    let id = resource_id.trim();
    RESOURCE_TRANSITIONS.iter().find(|(rid, _, _)| *rid == id).map(|(_, name, _)| *name)
}

/// 已人工核对、已映射的编号条数。
pub fn mapped_count() -> usize {
    RESOURCE_TRANSITIONS.len()
}

/// 未映射 `resourceId` 的统一错误文案（命令层包成参数错误）。
pub fn unmapped_effect_message(resource_id: &str) -> String {
    unmapped_message("效果", resource_id)
}

/// 未映射 `resourceId` 的转场错误文案（命令层包成参数错误）。
pub fn unmapped_transition_message(resource_id: &str) -> String {
    unmapped_message("转场", resource_id)
}

/// 音频过渡不参与本次映射时的错误文案：既说清原因，也给出已映射条数与文档位置。
pub fn audio_unmapped_message(resource_id: &str) -> String {
    format!("音频过渡暂无映射：resourceId {resource_id} 只覆盖视频转场；已映射 {} 条，见 docs/action-plan-mapping.md", mapped_count())
}

/// 「已映射 N 条」的统一句式，避免每处错误文案各写一遍计数。
fn unmapped_message(kind: &str, resource_id: &str) -> String {
    format!("resourceId {resource_id} 还没有映射到引擎{kind}；已映射 {} 条，见 docs/action-plan-mapping.md", mapped_count())
}
