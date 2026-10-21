//! `file.importJianyingResources`: scan a Jianying resource tree (audio / image / effect folders)
//! and import only the media — the template packages are counted, never imported — with a bound on
//! depth and file size, and an empty result (not an error) for a folder that does not exist.
//!
//! Sticker folders and filter folders are wired in too: a sticker that is a picture becomes
//! ordinary image media, and `.cube` / `.3dl` looks go into the project LUT library through
//! `lut.import`. A dry run writes nothing (no item, no LUT).

use std::path::{Path, PathBuf};

use filmcraft_project::{ItemId, ItemKind, MediaRef};
use serde_json::{Value, json};

use crate::Session;
use crate::jianying_resources::{Category, ImportKind, Limits, classify, import_kind, scan_with};
use crate::media_test_util::tmp_dir;

fn write_wav(path: &Path) {
    std::fs::write(path, crate::voiceover::write_wav_f32(&vec![0.0; 4_800], 48_000)).unwrap();
}

fn write_png(path: &Path) {
    image::RgbaImage::from_pixel(16, 9, image::Rgba([10, 90, 200, 255])).save(path).unwrap();
}

/// A minimal `.cube` that `filmcraft_color::Lut::parse_cube` accepts: `LUT_3D_SIZE 2` and its
/// eight rows (red fastest, green then blue).
const MINIMAL_CUBE: &str = "TITLE \"minimal\"\nLUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";

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

/// `root/sticker/star.png`, `root/filter/look.cube`, `root/filter/junk.txt`: a picture sticker, a
/// look the LUT library takes, and a filter file that is neither.
fn sticker_and_filter_fixture(name: &str) -> PathBuf {
    let root = tmp_dir(name);
    for dir in ["sticker", "filter"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    write_png(&root.join("sticker").join("star.png"));
    std::fs::write(root.join("filter").join("look.cube"), MINIMAL_CUBE).unwrap();
    std::fs::write(root.join("filter").join("junk.txt"), "not a look").unwrap();
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
    // …and how each of those is used: a sticker picture and media are imported, a look goes to the
    // LUT library, a sticker package and a non-LUT filter file are report-only.
    assert_eq!(import_kind(Category::Sticker, Path::new("C:/jianying/sticker/star.png")), ImportKind::Media);
    assert_eq!(import_kind(Category::Sticker, Path::new("C:/jianying/sticker/star.webp")), ImportKind::Media);
    assert_eq!(import_kind(Category::Sticker, Path::new("C:/jianying/sticker/star.json")), ImportKind::CountOnly);
    assert_eq!(import_kind(Category::Image, Path::new("C:/jianying/image/still.png")), ImportKind::Media);
    assert_eq!(import_kind(Category::Audio, Path::new("C:/jianying/audio/tone.wav")), ImportKind::Media);
    assert_eq!(import_kind(Category::Filter, Path::new("C:/jianying/filter/warm.cube")), ImportKind::Lut);
    assert_eq!(import_kind(Category::Filter, Path::new("C:/jianying/filter/warm.3dl")), ImportKind::Lut);
    assert_eq!(import_kind(Category::Filter, Path::new("C:/jianying/filter/warm.look")), ImportKind::CountOnly);
    assert_eq!(import_kind(Category::Effect, Path::new("C:/jianying/effect/whoosh.json")), ImportKind::CountOnly);
}

#[test]
fn a_dry_run_imports_neither_sticker_pictures_nor_luts() {
    let root = sticker_and_filter_fixture("jianying-sticker-dry-run");
    let mut s = Session::default();
    let items = s.project.items.len();
    let r = run(&mut s, json!({"root": root.to_string_lossy(), "dryRun": true}));
    assert_eq!(r["scanned"], 3, "{r}");
    assert_eq!(r["byCategory"]["sticker"], 1, "the sticker is still counted: {r}");
    assert_eq!(r["byCategory"]["filter"], 2, "the look and the junk file: {r}");
    assert_eq!(r["imported"], json!([]), "a dry run imports no media");
    assert_eq!(r["luts"], json!([]), "a dry run imports no LUT");
    assert_eq!(r["importedLuts"], 0);
    assert_eq!(s.project.items.len(), items, "the project is untouched");
    assert!(s.project.luts.is_empty(), "the LUT library is untouched");
}

#[test]
fn a_sticker_picture_becomes_media_and_a_cube_fills_the_lut_library() {
    let root = sticker_and_filter_fixture("jianying-sticker-filter");
    let mut s = Session::default();
    let items = s.project.items.len();
    let r = run(&mut s, json!({"root": root.to_string_lossy()}));
    // The sticker folder's picture is imported as ordinary image media.
    let ids: Vec<ItemId> = r["imported"].as_array().unwrap().iter().map(|v| ItemId(v.as_u64().unwrap())).collect();
    assert_eq!(ids.len(), 1, "the sticker picture: {r}");
    assert_eq!(s.project.items.len(), items + 1);
    assert!(path_of(&s, ids[0]).ends_with("star.png"), "{}", path_of(&s, ids[0]));
    assert!(s.project.item(ids[0]).is_some_and(|i| i.as_media().is_some()));
    // The look is a project LUT, visible through `lut.list`.
    let luts = r["luts"].as_array().unwrap();
    assert_eq!(luts.len(), 1, "{r}");
    assert_eq!(r["importedLuts"], 1);
    let lut_id = luts[0].as_str().expect("a LUT id");
    let list = s.execute("lut.list", json!({})).unwrap();
    let lib = list["library"].as_array().unwrap();
    assert!(lib.iter().any(|l| l["id"] == json!(lut_id) && l["name"] == "look"), "{list}");
    assert!(lib.iter().any(|l| l["source"].as_str().is_some_and(|p| p.ends_with("look.cube"))), "{list}");
    assert_eq!(s.project.luts.len(), 1);
    // The junk filter file is counted and reported, never imported.
    let note = r["unsupported"].as_array().unwrap().iter().find(|u| u["category"] == "filter").expect("a filter note");
    assert_eq!(note["count"], 1, "only the .txt, not the .cube: {note}");
    assert!(r["skipped"].as_array().unwrap().iter().all(|s| !s["path"].as_str().is_some_and(|p| p.ends_with("junk.txt"))), "{r}");
    // The sticker folder needs no manual step: its only file was imported.
    assert!(r["unsupported"].as_array().unwrap().iter().all(|u| u["category"] != "sticker"), "{r}");
}

#[test]
fn rescanning_reuses_the_lut_and_does_not_readd_the_sticker() {
    let root = sticker_and_filter_fixture("jianying-sticker-twice");
    let mut s = Session::default();
    let first = run(&mut s, json!({"root": root.to_string_lossy()}));
    let lut_id = first["luts"][0].as_str().expect("the imported LUT").to_string();
    let items = s.project.items.len();
    let second = run(&mut s, json!({"root": root.to_string_lossy()}));
    assert_eq!(second["imported"], json!([]), "the sticker is already in the project: {second}");
    assert_eq!(second["luts"], json!([lut_id]), "the existing library entry is reported, not duplicated: {second}");
    assert_eq!(s.project.luts.len(), 1, "the library did not grow");
    assert_eq!(s.project.items.len(), items, "nothing was added twice");
}

#[test]
fn a_lut_that_does_not_parse_is_skipped_and_the_good_ones_still_import() {
    let root = tmp_dir("jianying-broken-lut");
    for dir in ["sticker", "filter"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    write_png(&root.join("sticker").join("star.png"));
    std::fs::write(root.join("filter").join("look.cube"), MINIMAL_CUBE).unwrap();
    std::fs::write(root.join("filter").join("bad.cube"), "LUT_3D_SIZE 2\n0 0 0\n").unwrap();
    let mut s = Session::default();
    let r = run(&mut s, json!({"root": root.to_string_lossy()}));
    assert_eq!(r["luts"].as_array().unwrap().len(), 1, "the good look still imports: {r}");
    let bad =
        r["skipped"].as_array().unwrap().iter().find(|s| s["path"].as_str().is_some_and(|p| p.ends_with("bad.cube"))).expect("the broken look is reported");
    assert!(!bad["reason"].as_str().unwrap_or("").is_empty(), "{bad}");
    assert_eq!(s.project.luts.len(), 1, "the broken look was not added");
    assert_eq!(r["imported"].as_array().unwrap().len(), 1, "the sticker was still imported: {r}");
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
