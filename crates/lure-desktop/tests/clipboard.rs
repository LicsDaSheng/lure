use gpui::{ClipboardEntry, ClipboardItem, Image, ImageFormat};
use lure_desktop::clipboard::paste_text;

#[test]
fn text_paste_does_not_create_files() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        paste_text(&ClipboardItem::new_string("你好\n世界".into()), root.path()).unwrap(),
        Some("你好\n世界".into())
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn image_is_saved_with_absolute_unique_path_and_original_bytes() {
    let root = tempfile::tempdir().unwrap();
    let item = ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, vec![1, 2, 3]));
    let first = paste_text(&item, root.path()).unwrap().unwrap();
    let second = paste_text(&item, root.path()).unwrap().unwrap();
    assert_ne!(first, second);
    assert!(std::path::Path::new(&first).is_absolute());
    assert_eq!(std::path::Path::new(&first).extension().unwrap(), "png");
    assert_eq!(std::fs::read(first).unwrap(), vec![1, 2, 3]);
}

#[test]
fn image_takes_priority_over_text_and_preserves_format() {
    let root = tempfile::tempdir().unwrap();
    let mut item = ClipboardItem::new_string("描述".into());
    item.entries.push(ClipboardEntry::Image(Image::from_bytes(
        ImageFormat::Tiff,
        vec![42],
    )));
    let path = paste_text(&item, root.path()).unwrap().unwrap();
    assert_eq!(std::path::Path::new(&path).extension().unwrap(), "tiff");
    assert_eq!(std::fs::read(path).unwrap(), vec![42]);
}

#[test]
fn write_failure_is_reported_without_text_fallback() {
    let root = tempfile::tempdir().unwrap();
    let item = ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, vec![1]));
    assert!(paste_text(&item, &root.path().join("missing")).is_err());
}

#[test]
fn empty_clipboard_does_not_paste() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        paste_text(&ClipboardItem { entries: vec![] }, root.path()).unwrap(),
        None
    );
}
