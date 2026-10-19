//! 面板桥所调用的三个宿主导入 `__craftRead`、`__craftWrite` 与 `__craftScreenshot` 的策略
//! （`docs/derivation/03-panel-bridge.md` §3/§4）。
//!
//! 本模块刻意与平台无关：不使用 `web_sys`、`wasm-bindgen`，也不使用 JavaScript。它承载那些必须由
//! 测试*证明*、而不能只靠声明的规则：
//!
//! - 每个路径都会被解析（`realpath`），且必须留在工作区根目录内；`..`、符号链接逃逸、UNC 路径和
//!   Windows 盘符相对路径都会以错误码 `path_outside_workspace` 拒绝；
//! - 一次导入最多 [`MAX_IMPORT_FILES`] 个文件 / [`MAX_IMPORT_BYTES`] 字节；单个产物最多
//!   [`MAX_ARTIFACT_BYTES`] 字节；
//! - 写入先落到目标旁的临时文件，再重命名到位，因此被拒绝或失败的写入会让原文件逐字节保持不变，
//!   而覆盖写入会报告它替换掉的路径；
//! - 没有截图能力的宿主返回 `None`，即 [`Screenshot::Unsupported`]，而不是错误。
//!
//! 基于文件系统的参考实现（[`Workspace`]）只在宿主上编译：面板运行在浏览器中，真正的 I/O 由*宿主*
//! 执行（`__craftRead` / `__craftWrite`）。`tests/host_policy.rs` 按路径包含本文件（与
//! `tests/recovery_policy.rs` 的做法相同），并针对真实的临时目录执行该实现，因此即使 crate 的其余
//! 部分是 `wasm32` 专用，下面的规则仍能在宿主上受测。wasm 胶水层（`host.rs`）复用这些类型以及可移植
//! 的路径/配额检查，并对任何无法验证的情况失败关闭。

use std::fmt;
use std::path::{Component, Path};

/// 一次导入最多包含这么多个文件（§4.2）。
pub const MAX_IMPORT_FILES: usize = 256;
/// 一次导入的字节总数上限（§4.2）。
pub const MAX_IMPORT_BYTES: u64 = 128 * 1024 * 1024;
/// 单个写出产物的字节上限（§4.2）。
pub const MAX_ARTIFACT_BYTES: u64 = 128 * 1024 * 1024;
/// 宿主自身的保存对话框把单次保存限制在这个字节数（§4.2）。面板不强制该限制（对话框属于宿主）；
/// 记录在此，是为了让 §4 的所有数值集中在一处。
pub const MAX_SAVE_DIALOG_BYTES: u64 = 50 * 1024 * 1024;
/// 出现 [`Screenshot::Unsupported`] 时调用方退化的目标（§3）。
pub const SCREENSHOT_FALLBACK: &str = "render_frame";

/// [`HostError::Quota`] 中记录的、被超出的配额。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quota {
    /// 一次导入的文件数超过 [`MAX_IMPORT_FILES`]。
    ImportFiles,
    /// 一次导入的字节数超过 [`MAX_IMPORT_BYTES`]（单个文件超限也属于此类）。
    ImportBytes,
    /// 单个写出产物的字节数超过 [`MAX_ARTIFACT_BYTES`]。
    ArtifactBytes,
}

impl Quota {
    /// 稳定的机器可读名称，与 `error.code` 并列上报为 `error.quota`。
    pub fn code(self) -> &'static str {
        match self {
            Self::ImportFiles => "import_files",
            Self::ImportBytes => "import_bytes",
            Self::ArtifactBytes => "artifact_bytes",
        }
    }

    /// 该配额对应的上限值。
    pub fn limit(self) -> u64 {
        match self {
            Self::ImportFiles => MAX_IMPORT_FILES as u64,
            Self::ImportBytes => MAX_IMPORT_BYTES,
            Self::ArtifactBytes => MAX_ARTIFACT_BYTES,
        }
    }
}

/// 宿主导入失败的原因。[`HostError::code`] 是面板与宿主在错误对象中上报的稳定字符串（§3）：
/// 越界永远是 `path_outside_workspace`，绝不会退化成某个回退路径。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostError {
    /// 请求的路径解析后落在工作区根目录之外（T4）。
    OutsideWorkspace { path: String },
    /// 将超出 §4.2 的某项配额。
    Quota { quota: Quota, requested: u64 },
    /// 该路径根本无法命名一个工作区文件（空、含 NUL、根目录本身……）。
    InvalidPath { path: String, reason: &'static str },
    /// 文件系统（或宿主）失败。
    Io { operation: &'static str, path: String, detail: String },
}

impl HostError {
    /// 结果/错误对象使用的稳定错误码（§3）。
    pub fn code(&self) -> &'static str {
        match self {
            Self::OutsideWorkspace { .. } => "path_outside_workspace",
            Self::Quota { .. } => "quota_exceeded",
            Self::InvalidPath { .. } => "invalid_path",
            Self::Io { .. } => "io_error",
        }
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutsideWorkspace { path } => write!(f, "path_outside_workspace: {path}"),
            Self::Quota { quota, requested } => {
                write!(f, "quota_exceeded ({}): requested {requested}, limit {}", quota.code(), quota.limit())
            }
            Self::InvalidPath { path, reason } => write!(f, "invalid_path: {path}: {reason}"),
            Self::Io { operation, path, detail } => write!(f, "io_error: {operation} {path}: {detail}"),
        }
    }
}

impl std::error::Error for HostError {}

#[cfg(not(target_arch = "wasm32"))]
fn io(operation: &'static str, path: impl fmt::Display, detail: impl fmt::Display) -> HostError {
    HostError::Io { operation, path: path.to_string(), detail: detail.to_string() }
}

/// 在触碰文件系统之前，拒绝任何根本不可能命名工作区文件的路径（§4.1）。
///
/// 在此拒绝的情况：
/// - 空路径，或含 NUL 字节的路径；
/// - UNC / 设备路径（`\\server\share`、`\\?\C:`、`//server/share`）：它们命名的是另一台机器
///   或某个设备，而不是工作区里的文件；
/// - Windows 盘符相对路径（`C:foo`，区别于 `C:\foo`）：它相对于该盘的当前目录解析，从构造上
///   就落在工作区之外；
/// - 任何 `..` 组件 —— 直接拒绝而不做规范化，这样包含性检查就不会被路径拼接算术击破。
///
/// 盘符绝对路径（`C:\workspace\a.png`）*不*在这里拒绝：[`Workspace::resolve`] 仍须对它们做
/// canonicalise 与包含性检查。wasm 胶水层也使用本函数，因此面板无法自证的路径在到达宿主之前
/// 就会被拒绝。
pub fn check_syntax(path: &str) -> Result<(), HostError> {
    if path.is_empty() {
        return Err(HostError::InvalidPath { path: path.to_string(), reason: "empty path" });
    }
    if path.contains('\0') {
        return Err(HostError::InvalidPath { path: path.to_string(), reason: "NUL byte" });
    }
    if path.starts_with("\\\\") || path.starts_with("//") {
        return Err(HostError::OutsideWorkspace { path: path.to_string() });
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes.len() == 2 || !matches!(bytes[2], b'\\' | b'/')) {
        return Err(HostError::OutsideWorkspace { path: path.to_string() });
    }
    if Path::new(path).components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(HostError::OutsideWorkspace { path: path.to_string() });
    }
    Ok(())
}

/// 比较路径组件；在 Windows 上大小写不敏感（`C:\Temp` 与 `c:\temp` 是同一目录）。
#[cfg(not(target_arch = "wasm32"))]
fn component_eq(a: Component<'_>, b: Component<'_>) -> bool {
    #[cfg(windows)]
    {
        a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

/// 判断 `path` 是 `root` 本身还是位于其下。两者都应是真实路径，因此不会残留需要解析的 `..`
/// 或符号链接组件。
#[cfg(not(target_arch = "wasm32"))]
fn is_within(root: &Path, path: &Path) -> bool {
    let mut root = root.components();
    for component in path.components() {
        match root.next() {
            Some(expected) if component_eq(expected, component) => {}
            Some(_) => return false,
            // `path` 比 `root` 更深：根目录的每个组件都已匹配。
            None => return true,
        }
    }
    // `path` 已结束：它等于根目录（相同），或是根目录的祖先（不在其内）。
    root.next().is_none()
}

/// 一次成功写入的结果（§4.3）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteOutcome {
    /// 实际写出的工作区相对路径，以 `/` 分隔，使每个宿主的上报方式一致。
    pub path: String,
    /// 写出的字节数。
    pub bytes: u64,
    /// 既有文件被替换掉的路径；写入新建了文件（或宿主无法判断）时为 `None`。
    pub overwritten: Option<String>,
}

/// 一次导入已接纳的文件与字节（§4.2）。第一个越过限制的文件/计数会被拒绝，批次保持不变。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImportBatch {
    files: u64,
    bytes: u64,
}

impl ImportBatch {
    /// 目前已接纳的文件数。
    pub fn files(&self) -> u64 {
        self.files
    }

    /// 目前已接纳的字节数。
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// 再接纳一个 `size` 字节的文件；超限则拒绝，且不改变批次。
    pub fn admit(&mut self, size: u64) -> Result<(), HostError> {
        let files = self.files.saturating_add(1);
        let bytes = self.bytes.saturating_add(size);
        if files > MAX_IMPORT_FILES as u64 {
            return Err(HostError::Quota { quota: Quota::ImportFiles, requested: files });
        }
        if bytes > MAX_IMPORT_BYTES {
            return Err(HostError::Quota { quota: Quota::ImportBytes, requested: bytes });
        }
        self.files = files;
        self.bytes = bytes;
        Ok(())
    }
}

/// 宿主对 `__craftScreenshot` 的应答（§3）。`Unsupported` 表示宿主没有截图能力（或应答 `null`）；
/// 调用方退化为 [`SCREENSHOT_FALLBACK`]。它不算错误。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screenshot {
    /// 无截图能力：退化为 `render_frame`。
    Unsupported,
    /// 面板/窗口的 PNG 字节。
    Png(Vec<u8>),
}

impl Screenshot {
    /// 把宿主的应答映射为 [`Screenshot`]：`None`（全局量缺失、`null` 或 `undefined`）即
    /// `Unsupported`，绝不是错误。
    pub fn from_png(png: Option<Vec<u8>>) -> Self {
        png.map_or(Self::Unsupported, Self::Png)
    }

    /// 取出 PNG；宿主不支持截图时为 `None`。
    pub fn png(self) -> Option<Vec<u8>> {
        match self {
            Self::Unsupported => None,
            Self::Png(png) => Some(png),
        }
    }

    /// 调用方是否必须退化为 [`SCREENSHOT_FALLBACK`]。
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported)
    }
}

/// 仅在宿主端编译的参考实现：用 `realpath` 解析出的工作区根目录，提供读取、原子写入与导入计数。
/// 面板的浏览器构建不会链接它（那里的文件系统归宿主所有）；`tests/host_policy.rs` 会针对真实
/// 目录执行它。
#[cfg(not(target_arch = "wasm32"))]
mod fs_impl {
    use std::ffi::{OsStr, OsString};
    use std::fs::{File, OpenOptions};
    use std::io::{self, Read, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{HostError, ImportBatch, MAX_ARTIFACT_BYTES, MAX_IMPORT_BYTES, Quota, WriteOutcome, check_syntax, is_within};

    /// 已用 `realpath` 解析过的工作区根目录。
    #[derive(Clone, Debug)]
    pub struct Workspace {
        root: PathBuf,
    }

    /// 通过 [`Workspace::resolve`] 校验的路径。
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Resolved {
        absolute: PathBuf,
        relative: PathBuf,
        existed: bool,
    }

    impl Resolved {
        /// 实际操作所用的真实绝对路径。
        pub fn absolute(&self) -> &Path {
            &self.absolute
        }

        /// 相对于工作区根目录的路径。
        pub fn relative(&self) -> &Path {
            &self.relative
        }

        /// 解析时目标是否已存在（即这是一次覆盖写入）。
        pub fn existed(&self) -> bool {
            self.existed
        }

        /// 结果对象中上报的、以 `/` 分隔的路径。
        pub fn report_path(&self) -> String {
            report_path(&self.relative)
        }
    }

    /// 从工作区内读到的字节，以及这些字节的来源路径。
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ReadFile {
        path: String,
        bytes: Vec<u8>,
    }

    impl ReadFile {
        /// 字节来源的、以 `/` 分隔的工作区路径。
        pub fn path(&self) -> &str {
            &self.path
        }

        /// 文件的字节。
        pub fn bytes(&self) -> &[u8] {
            &self.bytes
        }

        /// 取出字节。
        pub fn into_bytes(self) -> Vec<u8> {
            self.bytes
        }
    }

    fn report_path(relative: &Path) -> String {
        let mut out = String::new();
        for component in relative.components() {
            if !out.is_empty() {
                out.push('/');
            }
            out.push_str(&component.as_os_str().to_string_lossy());
        }
        out
    }

    impl Workspace {
        /// 打开一个工作区根目录，先用 `realpath` 解析它。
        pub fn open(root: impl AsRef<Path>) -> Result<Self, HostError> {
            let root = root.as_ref();
            let root = std::fs::canonicalize(root).map_err(|e| super::io("open workspace", root.display(), e))?;
            if !root.is_dir() {
                return Err(HostError::Io { operation: "open workspace", path: root.display().to_string(), detail: "not a directory".into() });
            }
            Ok(Self { root })
        }

        /// 工作区根目录的真实路径。
        pub fn root(&self) -> &Path {
            &self.root
        }

        /// 解析面板给出的路径，并证明它位于工作区内（§4.1）。
        pub fn resolve(&self, path: &str) -> Result<Resolved, HostError> {
            self.resolve_with(path, &mut |candidate| std::fs::canonicalize(candidate))
        }

        /// [`Workspace::resolve`] 的变体，realpath 步骤由调用方提供。测试在创建符号链接需要特权的
        /// 平台上借助它验证包含性检查；自行解析路径的原生宿主也可以使用它。
        pub fn resolve_with(&self, path: &str, realpath: &mut dyn FnMut(&Path) -> io::Result<PathBuf>) -> Result<Resolved, HostError> {
            check_syntax(path)?;
            let given = Path::new(path);
            let candidate = if given.is_absolute() { given.to_path_buf() } else { self.root.join(given) };
            let (absolute, existed) = self.real_path(&candidate, realpath)?;
            if !is_within(&self.root, &absolute) {
                return Err(HostError::OutsideWorkspace { path: path.to_string() });
            }
            let relative = absolute.strip_prefix(&self.root).map_err(|_| HostError::OutsideWorkspace { path: path.to_string() })?.to_path_buf();
            Ok(Resolved { absolute, relative, existed })
        }

        fn real_path(&self, candidate: &Path, realpath: &mut dyn FnMut(&Path) -> io::Result<PathBuf>) -> Result<(PathBuf, bool), HostError> {
            match realpath(candidate) {
                Ok(real) => Ok((real, true)),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    // 目标可能是新文件。改为解析它将要所在的目录：指向根目录之外的符号链接父目录
                    // 会在这里被拦住。
                    let name =
                        candidate.file_name().ok_or_else(|| HostError::InvalidPath { path: candidate.display().to_string(), reason: "not a file path" })?;
                    let parent =
                        candidate.parent().ok_or_else(|| HostError::InvalidPath { path: candidate.display().to_string(), reason: "not a file path" })?;
                    let parent = realpath(parent).map_err(|e| super::io("resolve parent", parent.display(), e))?;
                    Ok((parent.join(name), false))
                }
                Err(e) => Err(super::io("resolve", candidate.display(), e)),
            }
        }

        /// 读取工作区内的一个文件；解析后落在工作区外的路径是错误，绝不会回退到其他位置。
        pub fn read(&self, path: &str) -> Result<ReadFile, HostError> {
            let resolved = self.resolve(path)?;
            let file = File::open(resolved.absolute()).map_err(|e| super::io("read", resolved.absolute().display(), e))?;
            // 分配量与结果都要设上限：最多只多读一个字节，用来判断是否超限。
            let mut limited = file.take(MAX_IMPORT_BYTES + 1);
            let mut bytes = Vec::new();
            limited.read_to_end(&mut bytes).map_err(|e| super::io("read", resolved.absolute().display(), e))?;
            if bytes.len() as u64 > MAX_IMPORT_BYTES {
                return Err(HostError::Quota { quota: Quota::ImportBytes, requested: bytes.len() as u64 });
            }
            Ok(ReadFile { path: resolved.report_path(), bytes })
        }

        /// 读取一次导入中的一个文件，并在 `batch` 中计入（§4.2）。
        pub fn read_import(&self, batch: &mut ImportBatch, path: &str) -> Result<ReadFile, HostError> {
            let file = self.read(path)?;
            batch.admit(file.bytes().len() as u64)?;
            Ok(file)
        }

        /// 原子地写出一份产物：先写目标同目录下的临时文件，再重命名。被拒绝或失败的写入不会改动
        /// 任何既有文件（§4.3），覆盖写入则报告被替换的路径。
        pub fn write(&self, path: &str, bytes: &[u8]) -> Result<WriteOutcome, HostError> {
            let len = bytes.len() as u64;
            if len > MAX_ARTIFACT_BYTES {
                return Err(HostError::Quota { quota: Quota::ArtifactBytes, requested: len });
            }
            let resolved = self.resolve(path)?;
            let report = resolved.report_path();
            if resolved.relative().as_os_str().is_empty() {
                return Err(HostError::InvalidPath { path: path.to_string(), reason: "the workspace root is a directory, not an artifact" });
            }
            if resolved.absolute().is_dir() {
                return Err(HostError::Io { operation: "write", path: report, detail: "is a directory".into() });
            }
            let parent = resolved.absolute().parent().ok_or_else(|| HostError::InvalidPath { path: path.to_string(), reason: "no parent directory" })?;
            let name = resolved.absolute().file_name().ok_or_else(|| HostError::InvalidPath { path: path.to_string(), reason: "not a file path" })?;
            let (temp_path, temp) = create_temp(parent, name)?;
            // 本代码块结束时临时文件即关闭：Windows 无法重命名（或删除）仍处于打开状态的文件。
            let written = {
                let mut file = temp;
                file.write_all(bytes).and_then(|()| file.sync_all())
            };
            if let Err(e) = written {
                let _ = std::fs::remove_file(&temp_path);
                return Err(super::io("write", &report, e));
            }
            if let Err(e) = std::fs::rename(&temp_path, resolved.absolute()) {
                let _ = std::fs::remove_file(&temp_path);
                return Err(super::io("rename", &report, e));
            }
            let overwritten = resolved.existed().then(|| report.clone());
            Ok(WriteOutcome { path: report, bytes: len, overwritten })
        }
    }

    /// 在 `dir` 中新建临时文件，文件名带上 `name` 以便识别残留；用 `create_new` 创建，使两个
    /// 写入者无法共用同一个名字。
    fn create_temp(dir: &Path, name: &OsStr) -> Result<(PathBuf, File), HostError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let pid = std::process::id();
        for _ in 0..32 {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let mut file_name = OsString::from(".");
            file_name.push(name);
            file_name.push(format!(".craft-{pid}-{n}.tmp"));
            let path = dir.join(file_name);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok((path, file)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(super::io("create temporary file", path.display(), e)),
            }
        }
        Err(HostError::Io { operation: "create temporary file", path: dir.display().to_string(), detail: "no unique temporary name after 32 attempts".into() })
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(unused_imports)] // `tests/host_policy.rs` 以私有方式包含本模块，并使用其中一个子集。
pub use fs_impl::{ReadFile, Resolved, Workspace};
