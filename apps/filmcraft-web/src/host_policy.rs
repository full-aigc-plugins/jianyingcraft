//! Policy for the three host imports the panel bridge calls: `__craftRead`, `__craftWrite` and
//! `__craftScreenshot` (§3/§4 of `docs/derivation/03-panel-bridge.md`).
//!
//! This module is deliberately platform-agnostic: no `web_sys`, no `wasm-bindgen`, no JavaScript. It
//! holds the rules that must be *proved* by tests, not merely declared:
//!
//! - every path is resolved (`realpath`) and must stay inside the workspace root; `..`, symlink
//!   escapes, UNC paths and drive-relative Windows paths are rejected with the error code
//!   `path_outside_workspace`;
//! - one import carries at most [`MAX_IMPORT_FILES`] files / [`MAX_IMPORT_BYTES`]; one artifact is at
//!   most [`MAX_ARTIFACT_BYTES`];
//! - a write goes to a temporary file next to its target and is renamed into place, so a rejected or
//!   failed write leaves the previous file byte-for-byte intact, and an overwrite reports the path it
//!   replaced;
//! - a host without screenshots answers `None`, which is [`Screenshot::Unsupported`] and not an error.
//!
//! The filesystem-backed reference implementation ([`Workspace`]) is compiled on the host only: the
//! panel runs in a browser, where the *host* performs the real I/O (`__craftRead` / `__craftWrite`).
//! `tests/host_policy.rs` includes this file by path (the same way `tests/recovery_policy.rs` does)
//! and executes that implementation against real temporary directories, so the rules below are
//! tested on the host even though the rest of the crate is `wasm32`-only. The wasm glue (`host.rs`)
//! reuses these types and the portable path/quota checks, and fails closed on anything it cannot
//! verify.

use std::fmt;
use std::path::{Component, Path};

/// One import carries at most this many files (§4.2).
pub const MAX_IMPORT_FILES: usize = 256;
/// One import carries at most this many bytes in total (§4.2).
pub const MAX_IMPORT_BYTES: u64 = 128 * 1024 * 1024;
/// One written artifact is at most this many bytes (§4.2).
pub const MAX_ARTIFACT_BYTES: u64 = 128 * 1024 * 1024;
/// The host's own save dialog caps a save at this many bytes (§4.2). The panel does not enforce it
/// (the dialog belongs to the host); it is recorded here so all the §4 numbers live together.
pub const MAX_SAVE_DIALOG_BYTES: u64 = 50 * 1024 * 1024;
/// What a caller falls back to when [`Screenshot::Unsupported`] (§3).
pub const SCREENSHOT_FALLBACK: &str = "render_frame";

/// The quota a [`HostError::Quota`] exceeded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quota {
    /// More than [`MAX_IMPORT_FILES`] files in one import.
    ImportFiles,
    /// More than [`MAX_IMPORT_BYTES`] bytes in one import (also a single file over the limit).
    ImportBytes,
    /// More than [`MAX_ARTIFACT_BYTES`] bytes in one written artifact.
    ArtifactBytes,
}

impl Quota {
    /// Stable machine-readable name, reported as `error.quota` next to `error.code`.
    pub fn code(self) -> &'static str {
        match self {
            Self::ImportFiles => "import_files",
            Self::ImportBytes => "import_bytes",
            Self::ArtifactBytes => "artifact_bytes",
        }
    }

    /// The limit this quota names.
    pub fn limit(self) -> u64 {
        match self {
            Self::ImportFiles => MAX_IMPORT_FILES as u64,
            Self::ImportBytes => MAX_IMPORT_BYTES,
            Self::ArtifactBytes => MAX_ARTIFACT_BYTES,
        }
    }
}

/// Why a host import failed. [`HostError::code`] is the stable string the panel and the host report
/// in the error object (§3): an escape is always `path_outside_workspace`, never a fallback path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostError {
    /// The requested path resolves outside the workspace root (T4).
    OutsideWorkspace { path: String },
    /// A §4.2 quota would be exceeded.
    Quota { quota: Quota, requested: u64 },
    /// The path cannot name a workspace file at all (empty, NUL, the root itself…).
    InvalidPath { path: String, reason: &'static str },
    /// The filesystem (or the host) failed.
    Io { operation: &'static str, path: String, detail: String },
}

impl HostError {
    /// Stable error code for the result/error object (§3).
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

/// Reject every path that can never name a workspace file, before touching the filesystem (§4.1).
///
/// Rejected here:
/// - an empty path or one containing a NUL byte;
/// - a UNC / device path (`\\server\share`, `\\?\C:`, `//server/share`): it names another machine
///   or a device, not a file in the workspace;
/// - a drive-relative Windows path (`C:foo`, as opposed to `C:\foo`): it resolves against that
///   drive's current directory, which is outside the workspace by construction;
/// - any `..` component — rejected outright rather than normalised, so the containment check can not
///   be defeated by joined-path arithmetic.
///
/// Drive-absolute paths (`C:\workspace\a.png`) are *not* rejected here: [`Workspace::resolve`] still
/// has to canonicalise and containment-check them. The wasm glue uses this function too, so a path
/// the panel can not justify is refused before it reaches the host.
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

/// Compare path components, case-insensitively on Windows (where `C:\Temp` and `c:\temp` are the
/// same directory).
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

/// Whether `path` is `root` itself or below it. Both are expected to be real paths, so no `..` or
/// symlink component is left to resolve.
#[cfg(not(target_arch = "wasm32"))]
fn is_within(root: &Path, path: &Path) -> bool {
    let mut root = root.components();
    for component in path.components() {
        match root.next() {
            Some(expected) if component_eq(expected, component) => {}
            Some(_) => return false,
            // `path` is deeper than `root`: every root component matched.
            None => return true,
        }
    }
    // `path` ended: it is the root (equal) or one of its ancestors (not inside).
    root.next().is_none()
}

/// What a successful write did (§4.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteOutcome {
    /// Workspace-relative path written, `/`-separated, so every host reports it the same way.
    pub path: String,
    /// Bytes written.
    pub bytes: u64,
    /// The path whose existing file was replaced; `None` when the write created a new file (or the
    /// host could not tell).
    pub overwritten: Option<String>,
}

/// Files and bytes admitted by one import (§4.2). The first file/count over a limit is rejected and
/// the batch is left unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImportBatch {
    files: u64,
    bytes: u64,
}

impl ImportBatch {
    /// Files admitted so far.
    pub fn files(&self) -> u64 {
        self.files
    }

    /// Bytes admitted so far.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Admit one more file of `size` bytes, or reject it without changing the batch.
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

/// The host's answer to `__craftScreenshot` (§3). `Unsupported` means the host has no screenshot
/// support (or answered `null`); the caller degrades to [`SCREENSHOT_FALLBACK`]. It is not an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screenshot {
    /// No screenshot support: degrade to `render_frame`.
    Unsupported,
    /// PNG bytes of the panel/window.
    Png(Vec<u8>),
}

impl Screenshot {
    /// Map the host's reply into a [`Screenshot`]: `None` (an absent global, `null` or `undefined`)
    /// is `Unsupported`, never an error.
    pub fn from_png(png: Option<Vec<u8>>) -> Self {
        png.map_or(Self::Unsupported, Self::Png)
    }

    /// The PNG, or `None` when the host does not support screenshots.
    pub fn png(self) -> Option<Vec<u8>> {
        match self {
            Self::Unsupported => None,
            Self::Png(png) => Some(png),
        }
    }

    /// Whether the caller has to fall back to [`SCREENSHOT_FALLBACK`].
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported)
    }
}

/// The host-only reference implementation: a workspace root resolved with `realpath`, with reads,
/// atomic writes and import accounting. The panel's browser build does not link it (the host owns
/// the file system there); `tests/host_policy.rs` executes it against real directories.
#[cfg(not(target_arch = "wasm32"))]
mod fs_impl {
    use std::ffi::{OsStr, OsString};
    use std::fs::{File, OpenOptions};
    use std::io::{self, Read, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{HostError, ImportBatch, MAX_ARTIFACT_BYTES, MAX_IMPORT_BYTES, Quota, WriteOutcome, check_syntax, is_within};

    /// A workspace root that has been resolved with `realpath`.
    #[derive(Clone, Debug)]
    pub struct Workspace {
        root: PathBuf,
    }

    /// A path that passed [`Workspace::resolve`].
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Resolved {
        absolute: PathBuf,
        relative: PathBuf,
        existed: bool,
    }

    impl Resolved {
        /// The real, absolute path to operate on.
        pub fn absolute(&self) -> &Path {
            &self.absolute
        }

        /// The path relative to the workspace root.
        pub fn relative(&self) -> &Path {
            &self.relative
        }

        /// Whether the target already existed (an overwrite) when it was resolved.
        pub fn existed(&self) -> bool {
            self.existed
        }

        /// The `/`-separated path reported in result objects.
        pub fn report_path(&self) -> String {
            report_path(&self.relative)
        }
    }

    /// Bytes read from inside the workspace, plus the path they came from.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ReadFile {
        path: String,
        bytes: Vec<u8>,
    }

    impl ReadFile {
        /// The `/`-separated workspace path the bytes came from.
        pub fn path(&self) -> &str {
            &self.path
        }

        /// The file's bytes.
        pub fn bytes(&self) -> &[u8] {
            &self.bytes
        }

        /// Take the bytes.
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
        /// Open a workspace root, resolving it with `realpath` first.
        pub fn open(root: impl AsRef<Path>) -> Result<Self, HostError> {
            let root = root.as_ref();
            let root = std::fs::canonicalize(root).map_err(|e| super::io("open workspace", root.display(), e))?;
            if !root.is_dir() {
                return Err(HostError::Io { operation: "open workspace", path: root.display().to_string(), detail: "not a directory".into() });
            }
            Ok(Self { root })
        }

        /// The real path of the workspace root.
        pub fn root(&self) -> &Path {
            &self.root
        }

        /// Resolve a panel-supplied path and prove it is inside the workspace (§4.1).
        pub fn resolve(&self, path: &str) -> Result<Resolved, HostError> {
            self.resolve_with(path, &mut |candidate| std::fs::canonicalize(candidate))
        }

        /// [`Workspace::resolve`] with the realpath step supplied by the caller. Tests use it to
        /// prove the containment check on platforms where creating a symlink needs privileges; a
        /// native host that resolves paths itself can use it too.
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
                    // The target may be new. Resolve the directory it will live in instead: a
                    // symlinked parent that points outside the root is caught here.
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

        /// Read a file inside the workspace; a path that resolves outside is an error, never a
        /// fallback to another location.
        pub fn read(&self, path: &str) -> Result<ReadFile, HostError> {
            let resolved = self.resolve(path)?;
            let file = File::open(resolved.absolute()).map_err(|e| super::io("read", resolved.absolute().display(), e))?;
            // Cap the allocation as well as the result: at most one byte over the quota is read.
            let mut limited = file.take(MAX_IMPORT_BYTES + 1);
            let mut bytes = Vec::new();
            limited.read_to_end(&mut bytes).map_err(|e| super::io("read", resolved.absolute().display(), e))?;
            if bytes.len() as u64 > MAX_IMPORT_BYTES {
                return Err(HostError::Quota { quota: Quota::ImportBytes, requested: bytes.len() as u64 });
            }
            Ok(ReadFile { path: resolved.report_path(), bytes })
        }

        /// Read one file of an import and account for it in `batch` (§4.2).
        pub fn read_import(&self, batch: &mut ImportBatch, path: &str) -> Result<ReadFile, HostError> {
            let file = self.read(path)?;
            batch.admit(file.bytes().len() as u64)?;
            Ok(file)
        }

        /// Write an artifact atomically: a temporary file in the target's directory, then a rename.
        /// A rejected or failed write leaves any existing file untouched (§4.3), and an overwrite
        /// reports the path it replaced.
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
            // The temporary file is closed when this block ends: Windows cannot rename (or remove)
            // a file that is still open.
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

    /// A new temporary file in `dir`, named after `name` so a leftover is identifiable, created with
    /// `create_new` so two writers can not share one name.
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
#[allow(unused_imports)] // `tests/host_policy.rs` includes this module privately and uses a subset.
pub use fs_impl::{ReadFile, Resolved, Workspace};
