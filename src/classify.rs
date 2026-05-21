use std::path::Path;

/// Known image extensions supported by the `image` crate (lowercase).
const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp",
    "tiff", "tif", "pnm", "pbm", "pgm", "ppm",
    "dds", "tga", "ico", "avif", "qoi",
    "xbm", "xpm",
];

/// Known video extensions (lowercase).
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm",
    "m4v", "mpg", "mpeg", "3gp", "ogv", "ts", "mts", "m2ts", "vob",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryKind {
    Image,
    Video,
}

/// Classify a file entry using extension-based matching, falling back to
/// `image::image_dimensions()` (header-only, no full decode) for unknown extensions.
pub fn classify(path: &Path) -> Option<EntryKind> {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        return Some(EntryKind::Image);
    }
    if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        return Some(EntryKind::Video);
    }

    // Fallback: try lightweight dimension probe for unknown extensions
    if image::image_dimensions(path).is_ok() {
        return Some(EntryKind::Image);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_png_is_image() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("t.png");
        let rgb = image::RgbImage::new(1, 1);
        rgb.save(&p).unwrap();
        assert_eq!(classify(&p), Some(EntryKind::Image));
    }

    #[test]
    fn test_jpg_is_image() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("t.jpg");
        let rgb = image::RgbImage::new(1, 1);
        rgb.save(&p).unwrap();
        assert_eq!(classify(&p), Some(EntryKind::Image));
    }

    #[test]
    fn test_mp4_is_video() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("t.mp4");
        fs::write(&p, b"").unwrap();
        assert_eq!(classify(&p), Some(EntryKind::Video));
    }

    #[test]
    fn test_txt_is_unsupported() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("t.txt");
        fs::write(&p, b"hello").unwrap();
        assert_eq!(classify(&p), None);
    }

    #[test]
    fn test_webp_is_image() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("t.webp");
        let rgb = image::RgbImage::new(1, 1);
        rgb.save(&p).unwrap();
        assert_eq!(classify(&p), Some(EntryKind::Image));
    }

    #[test]
    fn test_uppercase_extension() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("t.MP4");
        fs::write(&p, b"").unwrap();
        assert_eq!(classify(&p), Some(EntryKind::Video));
    }
}
