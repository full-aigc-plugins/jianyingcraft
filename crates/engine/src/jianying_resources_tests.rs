//! `file.importJianyingResources`: scan a Jianying resource tree (audio / image / effect folders)
//! and import only the media — the template packages are counted, never imported — with a bound on
//! depth and file size, and an empty result (not an error) for a folder that does not exist.

use std::path::{Path, PathBuf};

use filmcraft_project::{ItemId, ItemKind, MediaRef};
use serde_json::{Value, json};

use crate::Session;
use crate::jianying_resources::{Category, Limits, classify, scan_with};
use crate::media_test_util::tmp_dir;

fn write_wav(path: &Path) {
    std::fs::write(path, crate::voiceover::write_wav_f32(&vec![0.0; 4_800], 48_000)).unwrap();
}

fn write_png(path: &Path) {
    image::RgbaImage::from_pixel(16, 9, image::Rgba([10, 90, 200, 255])).save(path).unwrap();
}

/// `root/audio/tone.wav`, `root/image/still.png`, `root/effect/whoosh.json`: the three-layer
/// structure the feature is for.
fn fixture(name: &str) -> PathBuf {
    let root = tmp_dir(name);
    for dir in ["audio", "image", "effect"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    write_wav(&root.join("audio").join("tone.wav"));
    write_png(&root.join("image").join("still.png"));
    std::fs::write(root.join("effect").join("whoosh.json"), br#"{"name":"whoosh"}"#).unwrap();
    root
}

fn run(s: &mut Session, params: Value) -> Value {
    s.execute("file.importJianyingResources", params).unwrap()
}

fn path_of(s: &Session, id: ItemId) -> String {
    match &s.project.item(id).expect("the imported item exists").kind {
        ItemKind::Media(media) => match &media.media {
            MediaRef::File { path } => path.clone(),
            _ => panic!("the imported item is not a file"),
        },
        _ => panic!("the imported item is not media"),
    }
}

#[test]
fn dry_run_counts_audio_image_and_effect_without_touching_the_project() {
    let root = fixture("jianying-dry-run");
    let mut s = Session::default();
    let items = s.project.items.len();
    let r = run(&mut s, json!({"root": root.to_string_lossy(), "dryRun": true}));
    assert_eq!(r["scanned"], 3, "{r}");
    assert_eq!(r["byCategory"]["audio"], 1);
    assert_eq!(r["byCategory"]["image"], 1);
    assert_eq!(r["byCategory"]["video"], 0);
    assert_eq!(r["byCategory"]["effect"], 1);
    assert_eq!(r["imported"].as_array().unwrap().len(), 0, "a dry run imports nothing");
    assert_eq!(s.project.items.len(), items, "the project is untouched");
    assert_eq!(r["root"], json!(root.to_string_lossy()));
    assert_eq!(r["roots"], json!([root.to_string_lossy()]));
    // The effect is reported as a category that needs the user's own import.
    let note = r["unsupported"].as_array().unwrap().iter().find(|u| u["category"] == "effect").expect("an effect note");
    assert_eq!(note["count"], 1);
    assert!(note["message"].as_str().unwrap().contains("手动导入"), "{note}");
}

#[test]
fn importing_adds_only_the_media_categories_to_the_project() {
    let root = fixture("jianying-import");
    let mut s = Session::default();
    let items = s.project.items.len();
    let r = run(&mut s, json!({"root": root.to_string_lossy()}));
    assert_eq!(r["dryRun"], false);
    let ids: Vec<ItemId> = r["imported"].as_array().unwrap().iter().map(|v| ItemId(v.as_u64().unwrap())).collect();
    assert_eq!(ids.len(), 2, "audio and image, not the effect: {r}");
    assert_eq!(s.project.items.len(), items + 2);
    let paths: Vec<String> = ids.iter().map(|id| path_of(&s, *id)).collect();
    assert!(paths.iter().any(|p| p.ends_with("tone.wav")), "{paths:?}");
    assert!(paths.iter().any(|p| p.ends_with("still.png")), "{paths:?}");
    assert!(!paths.iter().any(|p| p.ends_with(".json")), "the effect was not imported: {paths:?}");
    assert!(s.project.item(ids[0]).is_some_and(|i| i.as_media().is_some()));
}

#[test]
fn a_second_import_does_not_duplicate_the_items() {
    let root = fixture("jianying-twice");
    let mut s = Session::default();
    let first = run(&mut s, json!({"root": root.to_string_lossy()}));
    assert_eq!(first["imported"].as_array().unwrap().len(), 2);
    let items = s.project.items.len();
    let second = run(&mut s, json!({"root": root.to_string_lossy()}));
    assert_eq!(second["scanned"], 3, "the scan still sees the files");
    assert_eq!(second["imported"].as_array().unwrap().len(), 0);
    assert_eq!(s.project.items.len(), items, "nothing was added twice");
    let already = second["skipped"].as_array().unwrap().iter().filter(|s| s["reason"] == "already in the project").count();
    assert_eq!(already, 2, "{second}");
}

#[test]
fn a_missing_root_is_an_empty_result_not_an_error() {
    let root = tmp_dir("jianying-missing").join("does-not-exist");
    let mut s = Session::default();
    let r = run(&mut s, json!({"root": root.to_string_lossy()}));
    assert_eq!(r["scanned"], 0);
    assert_eq!(r["imported"], json!([]));
    assert_eq!(r["roots"], json!([]), "nothing was scanned");
    assert_eq!(r["root"], json!(root.to_string_lossy()), "the folder the caller named is echoed");
    assert!(!r["warnings"].as_array().unwrap().is_empty(), "and the caller is told why: {r}");
    for c in Category::ALL {
        assert_eq!(r["byCategory"][c.key()], 0, "{}", c.key());
    }
}

#[test]
fn an_empty_folder_scans_zero() {
    let root = tmp_dir("jianying-empty");
    let mut s = Session::default();
    let r = run(&mut s, json!({"root": root.to_string_lossy()}));
    assert_eq!(r["scanned"], 0);
    assert!(r["warnings"].as_array().unwrap().is_empty(), "{r}");
}

#[test]
fn categories_restrict_the_walk() {
    let root = fixture("jianying-filter");
    let mut s = Session::default();
    let r = run(&mut s, json!({"root": root.to_string_lossy(), "dryRun": true, "categories": ["audio"]}));
    assert_eq!(r["scanned"], 1, "{r}");
    assert_eq!(r["byCategory"]["audio"], 1);
    assert_eq!(r["byCategory"]["effect"], 0, "the effect was not counted");
    assert_eq!(r["byCategory"]["image"], 0, "the image was not counted");
    assert!(r["unsupported"].as_array().unwrap().is_empty());
    let e = s.execute("file.importJianyingResources", json!({"categories": ["pictures"]})).unwrap_err().to_string();
    assert!(e.contains("unknown category"), "{e}");
}

#[test]
fn depth_and_file_size_are_bounded() {
    let root = tmp_dir("jianying-limits");
    // Allowed: the file sits in a folder at depth 4.
    std::fs::create_dir_all(root.join("audio/shallow/a/b")).unwrap();
    write_wav(&root.join("audio/shallow/a/b/ok.wav"));
    // Refused: the file sits in a folder at depth 5.
    std::fs::create_dir_all(root.join("audio/deep/a/b/c")).unwrap();
    write_wav(&root.join("audio/deep/a/b/c/deep.wav"));
    let s = scan_with(std::slice::from_ref(&root), &[], Limits::default());
    let paths: Vec<String> = s.media.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    assert!(paths.iter().any(|p| p.ends_with("ok.wav")), "{paths:?}");
    assert!(!paths.iter().any(|p| p.ends_with("deep.wav")), "{paths:?}");
    // A size limit far below the file: skipped with a reason instead of read.
    let small = scan_with(std::slice::from_ref(&root), &[], Limits { file_bytes: 8, ..Default::default() });
    assert_eq!(small.scanned(), 0);
    assert_eq!(small.media.len(), 0);
    assert!(small.skipped.iter().all(|s| s.reason.contains("larger than")), "{:?}", small.skipped);
    // The file budget stops the walk and says so (a second folder is left unread).
    let busy = fixture("jianying-budget");
    let one = scan_with(std::slice::from_ref(&busy), &[], Limits { files: 1, ..Default::default() });
    assert_eq!(one.scanned(), 1);
    assert!(one.truncated, "the caller is told the walk was cut short");
}

#[test]
fn a_picture_in_a_sticker_folder_is_a_sticker() {
    let root = Path::new("C:/jianying");
    assert_eq!(classify(root, Path::new("C:/jianying/sticker/star.png")), Category::Sticker);
    assert_eq!(classify(root, Path::new("C:/jianying/audio_effects/tone.wav")), Category::Audio);
    assert_eq!(classify(root, Path::new("C:/jianying/image/still.png")), Category::Image);
    assert_eq!(classify(root, Path::new("C:/jianying/effect/whoosh.json")), Category::Effect);
    assert_eq!(classify(root, Path::new("C:/jianying/transition/wipe.json")), Category::Transition);
    assert_eq!(classify(root, Path::new("C:/jianying/caption/talk.srt")), Category::Caption);
    assert_eq!(classify(root, Path::new("C:/jianying/filter/warm.cube")), Category::Filter);
    assert_eq!(classify(root, Path::new("C:/jianying/misc/shot.mp4")), Category::Video);
    assert_eq!(classify(root, Path::new("C:/jianying/misc/readme.txt")), Category::Text);
    assert_eq!(classify(root, Path::new("C:/jianying/misc/draft.json")), Category::Other);
}

/// A link inside the tree must not pull in files from outside it.
#[cfg(unix)]
#[test]
fn a_symbolic_link_is_reported_and_not_followed() {
    let root = tmp_dir("jianying-symlink");
    let outside = tmp_dir("jianying-outside");
    write_wav(&outside.join("secret.wav"));
    std::os::unix::fs::symlink(&outside, root.join("audio")).unwrap();
    let mut s = Session::default();
    let r = run(&mut s, json!({"root": root.to_string_lossy()}));
    assert_eq!(r["scanned"], 0, "{r}");
    assert_eq!(r["imported"], json!([]));
    assert!(r["skipped"].as_array().unwrap().iter().any(|s| s["reason"].as_str().unwrap_or("").contains("symbolic link")), "{r}");
}
