//! File ▸ Import From ▸ Jianying Resources (`file.importJianyingResources`): one-click import of
//! the resources the Jianying desktop app already downloaded onto this machine.
//!
//! JianyingCraft neither bundles nor redistributes any Jianying resource. The user already has
//! these files locally, so this command only reads them: it walks the resource folders (the
//! Windows locations below, the folders a user configured, or the `root` the caller names) to a
//! bounded depth, classifies every file by its folder names and extension, and imports what can
//! become real project material:
//!
//! - audio / image / video files, through the same import path as File ▸ Import;
//! - pictures a sticker folder holds — a sticker *is* a picture, so it becomes ordinary image
//!   media; a sticker folder's `.json` packages are not pictures and stay counted only;
//! - `.cube` / `.3dl` looks, through [`crate::color`]'s `lut.import`, into the project LUT library.
//!
//! The other kinds — text, effects, transitions, captions, non-picture stickers and non-LUT filter
//! files — are counted and reported only: they are template packages rather than media, so they
//! need the user's own manual import or are not supported yet. They are never copied or rewritten.
//!
//! Safety: symbolic links are never followed (a link in the tree cannot pull files in from outside
//! it), the walk is bounded ([`DEPTH_LIMIT`] folder levels, [`FILE_LIMIT`] files,
//! [`FILE_SIZE_LIMIT`] bytes per file), a missing folder or an empty tree is an empty result rather
//! than an error, and nothing outside the project state is written.
//!
//! Signature: `file.importJianyingResources { "root": str?, "dryRun": bool = false,
//! "categories": [str]? }`. Without `root` the Windows folders
//! `%LOCALAPPDATA%\JianyingPro\User Data`, `%LOCALAPPDATA%\JianyingPro\Resources` and
//! `%APPDATA%\JianyingPro` are probed, plus the folders a user configured in
//! `<settings folder>/jianying-resources.json` (`{"roots": ["D:\\…"]}`, also `root` /
//! `directories`) or in `JIANYINGCRAFT_JIANYING_ROOT`; folders that do not exist are skipped
//! without an error. `categories` limits the walk to the named categories (an empty list or none =
//! every category).
//!
//! Result: `{"scanned": n, "imported": [itemId], "luts": [lutId], "importedLuts": n, "byCategory":
//! {"audio": n, …}, "skipped": [{"path": str, "reason": str}], "root": str?, "roots": [str],
//! "dryRun": bool, "truncated": bool, "unsupported": [{"category": str, "count": n, "message":
//! str}], "warnings": [str]}`. `imported` lists the project items the run added (stickers as
//! pictures included); `luts` lists the LUT library ids the run resolved, `importedLuts` their
//! number (a LUT whose text the project already has is reused, so its id is reported and the
//! library does not grow). `scanned` counts the files in the requested categories; `byCategory`
//! always has one entry per category, and `unsupported` counts only the files that were *not*
//! imported (so `byCategory.sticker` can be 2 while `unsupported` reports 1).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use filmcraft_project::{ItemId, ItemKind, MediaRef};
use serde_json::{Value, json};

use crate::commands::{bad, bool_p};
use crate::{EngineError, Result, Session};

/// Command id (registered in [`crate::commands`]).
pub const COMMAND: &str = "file.importJianyingResources";

/// How many folder levels below a root are scanned (a root itself is level 0).
pub const DEPTH_LIMIT: usize = 4;
/// How many files one scan looks at before it stops and reports `truncated`.
pub const FILE_LIMIT: usize = 20_000;
/// Files larger than this are reported as skipped instead of imported (2 GiB).
pub const FILE_SIZE_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

/// A user-configured folder list beside the settings file.
const CONFIG_FILE: &str = "jianying-resources.json";
/// A folder given by the environment (one path), handy for tests and for unusual installs.
const ENV_ROOT: &str = "JIANYINGCRAFT_JIANYING_ROOT";

/// What a scanned file is, by folder name and extension. Only the first three are media the
/// project can hold; the rest are template packages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    Audio,
    Image,
    Video,
    Text,
    Sticker,
    Effect,
    Transition,
    Caption,
    Filter,
    Other,
}

impl Category {
    /// Every category, in report order.
    pub const ALL: [Category; 10] = [
        Category::Audio,
        Category::Image,
        Category::Video,
        Category::Text,
        Category::Sticker,
        Category::Effect,
        Category::Transition,
        Category::Caption,
        Category::Filter,
        Category::Other,
    ];

    /// The JSON key in `byCategory`, and the name `categories` takes.
    pub const fn key(self) -> &'static str {
        match self {
            Category::Audio => "audio",
            Category::Image => "image",
            Category::Video => "video",
            Category::Text => "text",
            Category::Sticker => "sticker",
            Category::Effect => "effect",
            Category::Transition => "transition",
            Category::Caption => "caption",
            Category::Filter => "filter",
            Category::Other => "other",
        }
    }

    /// A category named by a caller (case-insensitive).
    pub fn from_key(name: &str) -> Option<Self> {
        let name = name.trim();
        Self::ALL.into_iter().find(|c| c.key().eq_ignore_ascii_case(name))
    }

    /// The categories whose files are project media (audio / image / video). Stickers are only
    /// media when the file is a picture, and filters only become a LUT: see [`import_kind`].
    pub const fn importable(self) -> bool {
        matches!(self, Category::Audio | Category::Image | Category::Video)
    }
}

/// What one scanned file becomes: project media, a project LUT, or a report entry only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportKind {
    /// Imported as project media (audio / image / video, or a sticker that is a picture).
    Media,
    /// Imported into the project LUT library through `lut.import` (`.cube` / `.3dl`).
    Lut,
    /// Counted and reported in `unsupported`; never imported.
    CountOnly,
}

/// How the file at `path`, classified as `category`, is used. A sticker folder holds a picture
/// (import it as image media) beside `.json` packages (report only); a filter folder holds
/// `.cube` / `.3dl` looks (import into the LUT library) beside everything else (report only).
pub fn import_kind(category: Category, path: &Path) -> ImportKind {
    let ext = extension(path);
    if category == Category::Sticker {
        // Stickers that are pictures go in as ordinary image media; `.json` / other packages don't.
        return if media_family(&ext) == Some(Category::Image) { ImportKind::Media } else { ImportKind::CountOnly };
    }
    if category.importable() {
        return ImportKind::Media;
    }
    if category == Category::Filter && LUT_EXTS.contains(&ext.as_str()) { ImportKind::Lut } else { ImportKind::CountOnly }
}

/// Audio formats Jianying downloads that our importer may not read yet: still audio, so the user
/// sees an honest "not supported" skip rather than a silent drop.
const AUDIO_EXTRAS: &[&str] = &["wma", "mka", "amr", "ape", "caf"];
const CAPTION_EXTS: &[&str] = &["srt", "ass", "ssa", "vtt", "sbv", "lrc"];
const TEXT_EXTS: &[&str] = &["txt", "rtf", "md", "doc", "docx", "pdf"];
const FILTER_EXTS: &[&str] = &["cube", "lut", "3dl", "look"];
/// The filter files `lut.import` can read (`filmcraft_color::LutFormat::from_path`).
const LUT_EXTS: &[&str] = &["cube", "3dl"];

/// Folder names (matched anywhere in a name, case-insensitive) and what they hold. Media hints
/// come first so that `audio_effects` is audio; the nearest folder wins in [`dir_hint`].
const DIR_HINTS: &[(&str, Category)] = &[
    ("audio", Category::Audio),
    ("music", Category::Audio),
    ("sound", Category::Audio),
    ("音频", Category::Audio),
    ("音乐", Category::Audio),
    ("音效", Category::Audio),
    ("image", Category::Image),
    ("picture", Category::Image),
    ("photo", Category::Image),
    ("图片", Category::Image),
    ("图像", Category::Image),
    ("video", Category::Video),
    ("movie", Category::Video),
    ("视频", Category::Video),
    ("caption", Category::Caption),
    ("subtitle", Category::Caption),
    ("字幕", Category::Caption),
    ("transition", Category::Transition),
    ("转场", Category::Transition),
    ("sticker", Category::Sticker),
    ("贴纸", Category::Sticker),
    ("effect", Category::Effect),
    ("特效", Category::Effect),
    ("filter", Category::Filter),
    ("滤镜", Category::Filter),
    ("text", Category::Text),
    ("title", Category::Text),
    ("文本", Category::Text),
];

/// What folder name `name` suggests.
fn hint_of_name(name: &str) -> Option<Category> {
    let lower = name.to_lowercase();
    DIR_HINTS.iter().find(|(needle, _)| lower.contains(needle)).map(|(_, c)| *c)
}

/// The nearest folder of `path` (up to and including `root`) whose name says what it holds.
fn dir_hint(root: &Path, path: &Path) -> Option<Category> {
    if !path.starts_with(root) {
        return path.parent().and_then(|d| d.file_name()).and_then(|n| n.to_str()).and_then(hint_of_name);
    }
    let mut cur = path.parent();
    while let Some(dir) = cur {
        if let Some(c) = dir.file_name().and_then(|n| n.to_str()).and_then(hint_of_name) {
            return Some(c);
        }
        if dir == root {
            break;
        }
        cur = dir.parent();
    }
    None
}

fn extension(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// The media family an extension belongs to, if it is one our importer can hold (or a Jianying
/// audio variant we recognise and then report as unsupported at import time).
fn media_family(ext: &str) -> Option<Category> {
    if filmcraft_media::AUDIO_EXTENSIONS.contains(&ext) || AUDIO_EXTRAS.contains(&ext) {
        Some(Category::Audio)
    } else if filmcraft_media::VIDEO_EXTENSIONS.contains(&ext) {
        Some(Category::Video)
    } else if filmcraft_media::STILL_EXTENSIONS.contains(&ext) {
        Some(Category::Image)
    } else {
        None
    }
}

fn non_media_ext(ext: &str) -> Option<Category> {
    if CAPTION_EXTS.contains(&ext) {
        Some(Category::Caption)
    } else if TEXT_EXTS.contains(&ext) {
        Some(Category::Text)
    } else if FILTER_EXTS.contains(&ext) {
        Some(Category::Filter)
    } else {
        None
    }
}

/// Classify `path` inside `root`: a media extension decides audio / image / video (a picture in a
/// sticker folder is classified as a sticker, though — [`import_kind`] then imports it as image
/// media), and otherwise the nearest folder name decides, then a caption / text / filter extension.
/// JSON and other packages are only recognised through their folder (`effect`, `transition`,
/// `text`…), because a bare `.json` says nothing about what it is.
pub fn classify(root: &Path, path: &Path) -> Category {
    let ext = extension(path);
    let hint = dir_hint(root, path);
    match media_family(&ext) {
        Some(Category::Image) if hint == Some(Category::Sticker) => Category::Sticker,
        Some(family) => family,
        None => hint.or_else(|| non_media_ext(&ext)).unwrap_or(Category::Other),
    }
}

/// The walk bounds: tests tighten them instead of writing multi-gigabyte files.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub depth: usize,
    pub files: usize,
    pub file_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { depth: DEPTH_LIMIT, files: FILE_LIMIT, file_bytes: FILE_SIZE_LIMIT }
    }
}

/// One file that was counted but not imported, and why.
#[derive(Clone, Debug)]
pub struct Skipped {
    pub path: String,
    pub reason: String,
}

/// What a scan found. `counts` has an entry only for categories that occurred.
#[derive(Clone, Debug, Default)]
pub struct Scan {
    pub limits: Limits,
    pub roots: Vec<String>,
    /// Files looked at (the [`Limits::files`] budget), including those the caller filtered out.
    pub looked_at: usize,
    pub truncated: bool,
    pub counts: BTreeMap<Category, usize>,
    /// Files counted but not imported (the per-category part of `unsupported`).
    pub counted_only: BTreeMap<Category, usize>,
    /// Media files to import, in walk order (sticker pictures included).
    pub media: Vec<PathBuf>,
    /// `.cube` / `.3dl` looks to import into the LUT library, in walk order.
    pub luts: Vec<PathBuf>,
    pub skipped: Vec<Skipped>,
    pub warnings: Vec<String>,
}

impl Scan {
    /// Files in the requested categories (the reported `scanned`).
    pub fn scanned(&self) -> usize {
        self.counts.values().sum()
    }

    fn skip(&mut self, path: &Path, reason: impl Into<String>) {
        self.skipped.push(Skipped { path: path.to_string_lossy().into_owned(), reason: reason.into() });
    }
}

/// Scan `roots` for resource files, keeping only `filter` categories (empty = all).
pub fn scan(roots: &[PathBuf], filter: &[Category]) -> Scan {
    scan_with(roots, filter, Limits::default())
}

/// [`scan`] with explicit walk bounds.
pub fn scan_with(roots: &[PathBuf], filter: &[Category], limits: Limits) -> Scan {
    let mut out = Scan { limits, ..Default::default() };
    for root in roots {
        let canonical = match std::fs::canonicalize(root) {
            Ok(c) => c,
            Err(e) => {
                out.warnings.push(format!("{}: {e}", root.display()));
                continue;
            }
        };
        if !canonical.is_dir() {
            out.warnings.push(format!("{}: not a folder", root.display()));
            continue;
        }
        out.roots.push(root.display().to_string());
        walk(&canonical, &canonical, 0, filter, &mut out);
    }
    out
}

/// Walk one folder. `dir` is always a canonical path under `root`; links are never followed, so
/// the canonical check is a second line of defence rather than the only one.
fn walk(root: &Path, dir: &Path, depth: usize, filter: &[Category], out: &mut Scan) {
    if out.looked_at >= out.limits.files {
        out.truncated = true;
        return;
    }
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) => {
            out.warnings.push(format!("{}: {e}", dir.display()));
            return;
        }
    };
    // read_dir order is the file system's; sort so a run is repeatable and reports are stable.
    let mut entries = Vec::new();
    for entry in read {
        match entry {
            Ok(e) => entries.push(e),
            Err(e) => out.warnings.push(format!("{}: {e}", dir.display())),
        }
    }
    entries.sort_by_key(|a| a.file_name());
    for entry in entries {
        if out.looked_at >= out.limits.files {
            out.truncated = true;
            return;
        }
        let path = entry.path();
        let kind = match entry.file_type() {
            Ok(k) => k,
            Err(e) => {
                out.warnings.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        // A symbolic link inside the tree could point anywhere: report it, never follow it.
        if kind.is_symlink() {
            out.skip(&path, "symbolic link, not followed");
            continue;
        }
        if kind.is_dir() {
            if depth >= out.limits.depth {
                continue;
            }
            match std::fs::canonicalize(&path) {
                Ok(next) if next.starts_with(root) => walk(root, &next, depth + 1, filter, out),
                Ok(_) => out.skip(&path, "outside the scanned folder after resolving links"),
                Err(e) => out.warnings.push(format!("{}: {e}", path.display())),
            }
            continue;
        }
        if !kind.is_file() {
            continue;
        }
        out.looked_at += 1;
        let size = match entry.metadata() {
            Ok(m) => m.len(),
            Err(e) => {
                out.warnings.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        if size > out.limits.file_bytes {
            out.skip(&path, format!("larger than {} bytes", out.limits.file_bytes));
            continue;
        }
        let category = classify(root, &path);
        if !filter.is_empty() && !filter.contains(&category) {
            continue;
        }
        *out.counts.entry(category).or_insert(0) += 1;
        match import_kind(category, &path) {
            ImportKind::Media => out.media.push(path),
            ImportKind::Lut => out.luts.push(path),
            ImportKind::CountOnly => *out.counted_only.entry(category).or_insert(0) += 1,
        }
    }
}

/// Folders to probe when the caller names none: the Windows locations Jianying uses, then what
/// the user configured. Directories that do not exist are dropped silently.
pub fn discover_roots(s: &Session) -> (Vec<PathBuf>, Vec<String>) {
    let mut roots = Vec::new();
    #[cfg(windows)]
    {
        for (var, rel) in [("LOCALAPPDATA", r"JianyingPro\User Data"), ("LOCALAPPDATA", r"JianyingPro\Resources"), ("APPDATA", r"JianyingPro")] {
            if let Some(base) = std::env::var_os(var) {
                push_root(&mut roots, PathBuf::from(base).join(rel));
            }
        }
    }
    let (configured, warnings) = configured_roots(s);
    for root in configured {
        push_root(&mut roots, root);
    }
    roots.retain(|p| p.is_dir());
    (roots, warnings)
}

/// The user's own folders: `JIANYINGCRAFT_JIANYING_ROOT` and
/// `<folder of the settings file>/jianying-resources.json`.
fn configured_roots(s: &Session) -> (Vec<PathBuf>, Vec<String>) {
    let mut roots = Vec::new();
    let mut warnings = Vec::new();
    if let Some(v) = std::env::var_os(ENV_ROOT) {
        let v = v.to_string_lossy().into_owned();
        if !v.trim().is_empty() {
            roots.push(PathBuf::from(v));
        }
    }
    let Some(folder) = s.prefs_path.as_ref().and_then(|p| p.parent()) else { return (roots, warnings) };
    let file = folder.join(CONFIG_FILE);
    let bytes = match std::fs::read(&file) {
        Ok(b) => b,
        Err(_) => return (roots, warnings),
    };
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(v) => {
            for key in ["root", "roots", "directories"] {
                match v.get(key) {
                    Some(Value::String(p)) => roots.push(PathBuf::from(p)),
                    Some(Value::Array(a)) => roots.extend(a.iter().filter_map(Value::as_str).map(PathBuf::from)),
                    _ => {}
                }
            }
        }
        Err(e) => warnings.push(format!("{}: {e}", file.display())),
    }
    (roots, warnings)
}

fn push_root(roots: &mut Vec<PathBuf>, root: PathBuf) {
    if !roots.iter().any(|p| same_path(p, &root)) {
        roots.push(root);
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    path_key(a) == path_key(b)
}

/// A path's comparison key (Windows paths are case-insensitive and may mix separators).
fn path_key(path: &Path) -> String {
    let s = path.to_string_lossy();
    #[cfg(windows)]
    {
        s.replace('/', "\\").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        s.into_owned()
    }
}

/// The paths of the project's file media (offline ones too: re-importing them would duplicate).
fn project_paths(project: &filmcraft_project::Project) -> HashSet<String> {
    project
        .items
        .values()
        .filter_map(|item| match &item.kind {
            ItemKind::Media(media) => match &media.media {
                MediaRef::File { path } => Some(path_key(Path::new(path))),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// Import one media file through the same path as File ▸ Import (the host's reader when it has one,
/// otherwise the bytes). Nothing is copied.
fn import_media(s: &mut Session, path: &Path) -> Result<ItemId> {
    let p = path.to_string_lossy().into_owned();
    if !filmcraft_media::is_importable(path) {
        return Err(EngineError::Other("the importer does not read this format yet".into()));
    }
    if s.services.reader(&p).is_some() {
        return crate::commands::import_streamed(s, &p, None).map_err(|e| EngineError::Other(format!("{p}: {e}")));
    }
    let bytes = s.services.read_file(&p).map_err(|e| EngineError::Other(format!("{p}: {e}")))?;
    crate::commands::import_bytes(s, &p, bytes.into(), None).map_err(|e| EngineError::Other(format!("{p}: {e}")))
}

/// Import one look into the project LUT library through the command the LUT panel itself uses
/// (`lut.import`). `lut.import` reuses an entry whose text the library already holds, so the id
/// comes back either way and rescanning a folder never grows the library.
fn import_lut(s: &mut Session, path: &Path) -> Result<String> {
    let p = path.to_string_lossy().into_owned();
    let r = s.execute("lut.import", json!({ "path": p })).map_err(|e| EngineError::Other(format!("{p}: {e}")))?;
    r.get("id").and_then(Value::as_str).map(str::to_string).ok_or_else(|| EngineError::Other(format!("{p}: lut.import returned no id")))
}

/// `categories` param: the categories to walk; absent, null or empty means all.
fn categories_p(p: &Value, cmd: &str) -> Result<Vec<Category>> {
    let Some(value) = p.get("categories") else { return Ok(Vec::new()) };
    if value.is_null() {
        return Ok(Vec::new());
    }
    let Some(list) = value.as_array() else {
        return Err(bad(cmd, "`categories` must be a list of category names"));
    };
    let mut out: Vec<Category> = Vec::new();
    for entry in list {
        let Some(name) = entry.as_str() else {
            return Err(bad(cmd, "`categories` must name categories with strings"));
        };
        let Some(c) = Category::from_key(name) else {
            let keys: Vec<&str> = Category::ALL.iter().map(|c| c.key()).collect();
            return Err(bad(cmd, format!("unknown category `{name}` (use one of: {})", keys.join(", "))));
        };
        if !out.contains(&c) {
            out.push(c);
        }
    }
    Ok(out)
}

/// `file.importJianyingResources`: scan the user's Jianying folders and import their media, their
/// sticker pictures and their LUT looks.
pub fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let dry_run = bool_p(p, "dryRun").unwrap_or(false);
    let filter = categories_p(p, COMMAND)?;
    let mut warnings = Vec::new();
    let explicit = match p.get("root") {
        Some(Value::Null) => None,
        Some(Value::String(r)) => {
            if r.trim().is_empty() {
                return Err(bad(COMMAND, "`root` must be a folder path"));
            }
            Some(PathBuf::from(r))
        }
        Some(_) => return Err(bad(COMMAND, "`root` must be a folder path")),
        None => None,
    };
    let given = match &explicit {
        Some(root) => vec![root.clone()],
        None => {
            let (roots, notes) = discover_roots(s);
            warnings.extend(notes);
            roots
        }
    };
    if given.is_empty() {
        warnings.push("no Jianying resource folder was found; pass `root` to point at one".into());
    }
    let mut found = scan(&given, &filter);
    warnings.append(&mut found.warnings);

    let mut imported: Vec<u64> = Vec::new();
    let mut luts: Vec<String> = Vec::new();
    let mut skipped = std::mem::take(&mut found.skipped);
    if !dry_run {
        let mut known = project_paths(&s.project);
        for path in &found.media {
            if !known.insert(path_key(path)) {
                skipped.push(Skipped { path: path.to_string_lossy().into_owned(), reason: "already in the project".into() });
                continue;
            }
            match import_media(s, path) {
                Ok(id) => imported.push(id.0),
                Err(e) => skipped.push(Skipped { path: path.to_string_lossy().into_owned(), reason: e.to_string() }),
            }
        }
        // Looks go into the LUT library, one at a time: a file `lut.import` refuses is reported
        // and the rest still import. A LUT the library already holds comes back with its id, but
        // is listed once per run.
        for path in &found.luts {
            match import_lut(s, path) {
                Ok(id) if !luts.contains(&id) => luts.push(id),
                Ok(_) => {}
                Err(e) => skipped.push(Skipped { path: path.to_string_lossy().into_owned(), reason: e.to_string() }),
            }
        }
    }

    let scanned = found.scanned();
    let mut by_category = serde_json::Map::new();
    let mut unsupported = Vec::new();
    for c in Category::ALL {
        let n = found.counts.get(&c).copied().unwrap_or(0);
        by_category.insert(c.key().to_string(), json!(n));
        // Only the files that were not imported are unsupported (a sticker folder's pictures are).
        let only = found.counted_only.get(&c).copied().unwrap_or(0);
        if only > 0 {
            unsupported.push(json!({
                "category": c.key(),
                "count": only,
                "message": "这类资源需要用户手动导入/暂不支持自动导入",
            }));
        }
    }
    let root = match &explicit {
        Some(root) => json!(root.display().to_string()),
        None => found.roots.first().map(|r| json!(r)).unwrap_or(Value::Null),
    };
    let skipped: Vec<Value> = skipped.iter().map(|s| json!({"path": s.path, "reason": s.reason})).collect();
    Ok(json!({
        "scanned": scanned,
        "imported": imported,
        "luts": luts,
        "importedLuts": luts.len(),
        "byCategory": Value::Object(by_category),
        "skipped": skipped,
        "root": root,
        "roots": found.roots,
        "dryRun": dry_run,
        "truncated": found.truncated,
        "unsupported": unsupported,
        "warnings": warnings,
    }))
}
