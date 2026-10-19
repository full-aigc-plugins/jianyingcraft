//! 测试辅助：UI 快照目录的解析（给 `tests/*_ui.rs` 用，不属于公开 API）。
//!
//! 快照测试通过 `FILMCRAFT_UI_SNAPSHOT_DIR` 打开截图。此前每个测试文件各自
//! `std::env::var_os(..).map(PathBuf::from)`，相对路径会按**进程 CWD** 解析：在
//! `crates/ui-egui` 里执行 `cargo test` 就会写出 `crates/ui-egui/crates/ui-egui/target/...`
//! 这种嵌套垃圾目录。这里统一把相对路径按**工作区根目录**解析，并从一处读取变量。

use std::path::{Path, PathBuf};

/// 打开 UI 截图的环境变量名。
pub const SNAPSHOT_DIR_ENV: &str = "FILMCRAFT_UI_SNAPSHOT_DIR";

/// 快照输出目录；变量未设置时返回 `None`（测试照常跑，只是不做 wgpu 离屏渲染）。
///
/// 相对路径按工作区根目录（`crates/ui-egui` 的上两级）解析，而不是进程 CWD，因此从仓库根
/// 或从 `crates/ui-egui` 里跑测试都会写到同一处。
pub fn snapshot_dir() -> Option<PathBuf> {
    std::env::var_os(SNAPSHOT_DIR_ENV).map(PathBuf::from).map(|dir| resolve_snapshot_dir(&dir))
}

/// 把配置的快照目录解析成绝对路径：绝对路径原样返回，相对路径挂到工作区根目录下。
///
/// [`snapshot_dir`] 的纯函数部分，便于单测验证"相对路径不再跟随 CWD"。
pub fn resolve_snapshot_dir(dir: &Path) -> PathBuf {
    if dir.is_absolute() { dir.to_path_buf() } else { workspace_root().join(dir) }
}

/// 编译期记录的工作区根目录：`CARGO_MANIFEST_DIR` 是 `crates/ui-egui`，往上两级即仓库根。
///
/// 取不到父目录时退回 `"."`（此时 `resolve_snapshot_dir` 仍是确定行为，不会 panic）。
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{resolve_snapshot_dir, workspace_root};

    /// 绝对路径原样保留。
    #[test]
    fn an_absolute_snapshot_dir_is_kept() {
        let dir = workspace_root().join("target/ui-snapshots");
        assert_eq!(resolve_snapshot_dir(&dir), dir);
    }

    /// 相对路径挂到工作区根目录下，而不是进程 CWD：这是 `crates/ui-egui/crates/ui-egui/...`
    /// 嵌套目录的回归测试。
    #[test]
    fn a_relative_snapshot_dir_is_rooted_at_the_workspace() {
        let root = workspace_root();
        assert!(root.join("Cargo.toml").is_file(), "workspace root should be the repository root: {}", root.display());
        let resolved = resolve_snapshot_dir(Path::new("target/ui-snapshots"));
        assert_eq!(resolved, root.join("target").join("ui-snapshots"));
        // 旧实现按 CWD 解析，在 `crates/ui-egui` 里跑就会多套一层。
        assert_ne!(resolved, root.join("crates").join("ui-egui").join("target").join("ui-snapshots"));
    }
}
