//! 产品字标与名称。
//!
//! 派生说明：上游 FilmCraft 使用 ArtCraft 商标字标（曾位于 `docs/brand/`，第三方商标、按许可仅限上游使用）。
//! JianyingCraft 作为派生品**不沿用该商标**，且已按上游 `LICENSE-brand.txt` 的要求删除
//! `docs/brand/`；此处改为用文字绘制产品名。正式 logo 资产属后续任务（见 docs/derivation/）。

/// 产品显示名。UI 中出现产品名时统一引用此常量。
pub const PRODUCT_NAME: &str = "JianyingCraft";

/// 以文字绘制产品字标，返回绘制区域。
///
/// `height` 是字标高度（点）；`dark` 表示深色 UI（用亮色文字）。
/// 签名与上游 `paint_wordmark` 一致，调用点无需改动。
pub fn paint_wordmark(ui: &egui::Ui, left_center: egui::Pos2, height: f32, dark: bool) -> Option<egui::Rect> {
    let color = if dark { egui::Color32::from_rgb(0xf2, 0xf4, 0xf8) } else { egui::Color32::from_rgb(0x18, 0x1c, 0x24) };
    let job = egui::text::LayoutJob::simple_singleline(PRODUCT_NAME.to_owned(), egui::FontId::proportional(height), color);
    let galley = ui.painter().layout_job(job);
    let size = galley.size();
    let rect = egui::Rect::from_min_size(egui::pos2(left_center.x, left_center.y - size.y / 2.0), size);
    ui.painter().galley(rect.min, galley, color);
    Some(rect)
}
