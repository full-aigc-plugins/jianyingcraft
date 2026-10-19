//! The host-import policy (§3/§4 of `docs/derivation/03-panel-bridge.md`) runs natively here even
//! though the rest of the crate is `wasm32`-only, exactly as `tests/recovery_policy.rs` does: the
//! module is pure `std` (no `web_sys`), so the rules the wasm glue relies on are proved by real file
//! operations rather than by type-checking.
//!
//! T4: `../`, an absolute path outside the root, a link that resolves outside and a UNC path are all
//! refused with `path_outside_workspace`, and nothing outside the workspace is read or written.
//! §4.2/§4.3: over-quota writes are refused before anything is touched (the previous bytes are still
//! there), and a successful overwrite reports the path it replaced.

#[path = "../src/host_policy.rs"]
#[allow(dead_code)] // the tests exercise a subset of the policy's surface
mod host_policy;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use host_policy::{HostError, ImportBatch, MAX_ARTIFACT_BYTES, MAX_IMPORT_BYTES, MAX_IMPORT_FILES, Quota, Screenshot, Workspace};

/// A temporary directory removed on drop. `tempfile` is not a dependency of this crate, and the
/// workspace has no shared fixture crate for this, so the fixture stays here, in `std`.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("jianyingcraft-host-policy-{}-{label}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(&path).expect("create the temporary directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    /// Entry names in the directory, sorted, to prove a rejected write left no temporary file.
    fn entries(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.path)
            .expect("read the temporary directory")
            .map(|e| e.expect("directory entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn assert_code(error: &HostError, code: &str) {
    assert_eq!(error.code(), code, "expected {code}, got {error}");
}

/// T4: `../` is refused for reads and writes, and the file it aimed at is untouched.
#[test]
fn parent_dir_escape_is_rejected_for_reads_and_writes() {
    let workspace_dir = TempDir::new("ws-parent");
    let outside = TempDir::new("outside-parent");
    fs::write(outside.path().join("secret.txt"), b"secret").expect("write the outside file");
    let ws = Workspace::open(workspace_dir.path()).expect("open the workspace");

    let escaping = format!("../{}/secret.txt", outside.path().file_name().expect("outside name").to_string_lossy());
    assert_code(&ws.read(&escaping).unwrap_err(), "path_outside_workspace");
    assert_code(&ws.write(&escaping, b"overwritten").unwrap_err(), "path_outside_workspace");
    assert_eq!(fs::read(outside.path().join("secret.txt")).expect("read the outside file"), b"secret");
}

/// T4: an absolute path outside the root is refused, even though it exists and is readable.
#[test]
fn absolute_path_outside_the_root_is_rejected() {
    let workspace_dir = TempDir::new("ws-absolute");
    let outside = TempDir::new("outside-absolute");
    let secret = outside.path().join("secret.txt");
    fs::write(&secret, b"secret").expect("write the outside file");
    let ws = Workspace::open(workspace_dir.path()).expect("open the workspace");
    let absolute = secret.to_string_lossy().into_owned();

    assert_code(&ws.read(&absolute).unwrap_err(), "path_outside_workspace");
    assert_code(&ws.write(&absolute, b"overwritten").unwrap_err(), "path_outside_workspace");
    assert_eq!(fs::read(&secret).expect("read the outside file"), b"secret");
}

/// The check is containment, not a refusal of absolute paths: one inside the root works.
#[test]
fn absolute_path_inside_the_root_is_allowed() {
    let workspace_dir = TempDir::new("ws-inside");
    let inside = workspace_dir.path().join("inside.txt");
    fs::write(&inside, b"inside").expect("write the inside file");
    let ws = Workspace::open(workspace_dir.path()).expect("open the workspace");

    let file = ws.read(&inside.to_string_lossy()).expect("read an absolute path inside the root");
    assert_eq!(file.path(), "inside.txt");
    assert_eq!(file.bytes(), b"inside");
}

/// T4: UNC/device paths and drive-relative Windows paths name something else and are refused.
#[test]
fn unc_drive_relative_and_nul_paths_are_rejected() {
    let workspace_dir = TempDir::new("ws-syntax");
    let ws = Workspace::open(workspace_dir.path()).expect("open the workspace");

    for path in [r"\\server\share\secret.txt", r"\\?\C:\Windows\win.ini", "//server/share/secret.txt", "C:secret.txt"] {
        assert_code(&ws.read(path).unwrap_err(), "path_outside_workspace");
    }
    assert_code(&ws.read("a\0b").unwrap_err(), "invalid_path");
    assert_code(&ws.write("", b"x").unwrap_err(), "invalid_path");
}

/// T4: a link inside the workspace whose target is outside is refused. Windows needs no privilege
/// for a junction (a reparse point `realpath` resolves just like a symlink), so the test does not
/// silently disappear there; if neither can be created, it says so instead of passing quietly.
#[test]
#[ignore = "needs a directory symlink or junction; run with `cargo test -p filmcraft-web --test host_policy -- --ignored`"]
fn a_link_pointing_outside_the_workspace_is_rejected() {
    let workspace_dir = TempDir::new("ws-link");
    let outside = TempDir::new("outside-link");
    fs::write(outside.path().join("secret.txt"), b"secret").expect("write the outside file");
    let link = workspace_dir.path().join("escape");
    // 用 `#[ignore]` 显式声明依赖，而不是创建失败就静默 return：后者在 CI 上会把"没跑"
    // 记成"通过"。真跑的时候建不出链接就是环境不满足，应当失败并说明原因。
    assert!(link_to(outside.path(), &link), "this test needs a directory symlink or junction; run it where they are allowed");
    let ws = Workspace::open(workspace_dir.path()).expect("open the workspace");
    let escaped = "escape/secret.txt";

    assert_code(&ws.read(escaped).unwrap_err(), "path_outside_workspace");
    assert_code(&ws.write(escaped, b"overwritten").unwrap_err(), "path_outside_workspace");
    assert_eq!(fs::read(outside.path().join("secret.txt")).expect("read the outside file"), b"secret");
}

/// The containment check itself, independent of whether this platform lets the test create a link:
/// a realpath that leaves the root is refused even for a path that was written inside it.
#[test]
fn a_resolved_path_that_leaves_the_root_is_rejected() {
    let workspace_dir = TempDir::new("ws-realpath");
    let ws = Workspace::open(workspace_dir.path()).expect("open the workspace");
    let outside = std::env::temp_dir().join("jianyingcraft-definitely-outside.txt");
    let mut fake_realpath = |_: &Path| -> std::io::Result<PathBuf> { Ok(outside.clone()) };

    assert_code(&ws.resolve_with("inside.txt", &mut fake_realpath).unwrap_err(), "path_outside_workspace");
}

/// §4.2/§4.3: a write over the artifact quota is refused before a temporary file exists, so the
/// previous bytes are exactly what they were and no half-written artifact is left behind.
#[test]
fn a_write_over_the_artifact_quota_leaves_the_previous_file_untouched() {
    let workspace_dir = TempDir::new("ws-quota");
    let ws = Workspace::open(workspace_dir.path()).expect("open the workspace");
    let original = b"original bytes";
    ws.write("notes.txt", original).expect("write the original");

    let too_big = vec![0u8; MAX_ARTIFACT_BYTES as usize + 1];
    match ws.write("notes.txt", &too_big).unwrap_err() {
        HostError::Quota { quota: Quota::ArtifactBytes, .. } => {}
        other => panic!("expected the artifact quota, got {other}"),
    }
    assert_eq!(ws.read("notes.txt").expect("read the original back").bytes(), original);
    assert_eq!(workspace_dir.entries(), ["notes.txt"], "a rejected write must not leave a temporary file");
}

/// §4.3: an overwrite succeeds and reports the path it replaced; a fresh write reports none.
#[test]
fn an_overwrite_reports_the_path_it_replaced() {
    let workspace_dir = TempDir::new("ws-overwrite");
    let ws = Workspace::open(workspace_dir.path()).expect("open the workspace");

    let first = ws.write("notes.txt", b"first").expect("write the first version");
    assert_eq!(first.path, "notes.txt");
    assert_eq!(first.bytes, 5);
    assert_eq!(first.overwritten, None);

    let second = ws.write("notes.txt", b"second").expect("overwrite it");
    assert_eq!(second.path, "notes.txt");
    assert_eq!(second.bytes, 6);
    assert_eq!(second.overwritten.as_deref(), Some("notes.txt"));
    assert_eq!(ws.read("notes.txt").expect("read the second version").bytes(), b"second");
    assert_eq!(workspace_dir.entries(), ["notes.txt"], "the rename must leave no temporary file");
}

/// Paths are reported workspace-relative with `/`, whichever separator the host uses, and one import
/// accounts for each file it reads.
#[test]
fn reads_and_writes_report_workspace_relative_paths_and_are_accounted_for() {
    let workspace_dir = TempDir::new("ws-nested");
    fs::create_dir(workspace_dir.path().join("media")).expect("create a subdirectory");
    let ws = Workspace::open(workspace_dir.path()).expect("open the workspace");

    ws.write("media/a.png", b"png").expect("write into a subdirectory");
    let file = ws.read("media/a.png").expect("read it back");
    assert_eq!(file.path(), "media/a.png");
    assert_eq!(file.bytes(), b"png");

    let mut batch = ImportBatch::default();
    ws.read_import(&mut batch, "media/a.png").expect("read it as part of an import");
    assert_eq!((batch.files(), batch.bytes()), (1, 3));
}

/// §4.2: one import admits at most 256 files and 128 MiB, and a rejected file is not counted.
#[test]
fn an_import_admits_at_most_256_files_and_128_mib() {
    let mut batch = ImportBatch::default();
    for _ in 0..MAX_IMPORT_FILES {
        batch.admit(1).expect("within the file limit");
    }
    assert_eq!(batch.files(), MAX_IMPORT_FILES as u64);
    match batch.admit(1).unwrap_err() {
        HostError::Quota { quota: Quota::ImportFiles, requested } => assert_eq!(requested, MAX_IMPORT_FILES as u64 + 1),
        other => panic!("expected the import file quota, got {other}"),
    }
    assert_eq!(batch.files(), MAX_IMPORT_FILES as u64, "a rejected file is not admitted");

    let mut batch = ImportBatch::default();
    batch.admit(MAX_IMPORT_BYTES).expect("exactly at the byte limit");
    match batch.admit(1).unwrap_err() {
        HostError::Quota { quota: Quota::ImportBytes, .. } => {}
        other => panic!("expected the import byte quota, got {other}"),
    }
    assert_eq!(batch.bytes(), MAX_IMPORT_BYTES, "a rejected file is not admitted");
}

/// §3: a host without screenshots answers `None`; that is a normal outcome, not an error.
#[test]
fn an_unsupported_screenshot_is_none_not_an_error() {
    let unsupported = Screenshot::from_png(None);
    assert!(unsupported.is_unsupported());
    assert_eq!(unsupported.png(), None);

    let png = vec![0x89, b'P', b'N', b'G'];
    assert_eq!(Screenshot::from_png(Some(png.clone())).png(), Some(png));
}

/// Create a directory link `link` -> `target`. Unix symlinks always work; Windows tries a symlink
/// (needs Developer Mode) and falls back to a junction, which needs no privilege.
fn link_to(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_dir(target, link).is_ok() {
            return true;
        }
        std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
}
