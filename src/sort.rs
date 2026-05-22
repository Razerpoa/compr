use std::path::Path;
use rayon::prelude::*;
use crate::classify::EntryKind;

/// Sorting strategy for pack entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    /// Folder-grouped: parent dir → Image before Video → filename (default).
    Folder,
    /// Color-grouped: sort images globally by dominant HSV hue → sat → val,
    /// then videos at the end.  Visually similar images cluster together,
    /// improving ZSTD LDM cross-entry matches.
    Color,
}

impl std::str::FromStr for SortMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "folder" => Ok(SortMode::Folder),
            "color"  => Ok(SortMode::Color),
            other    => Err(format!(
                "unknown sort mode '{}'; valid options: folder, color", other
            )),
        }
    }
}

/// Sort a list of `(DirEntry, EntryKind)` pairs in-place using `mode`.
///
/// `Color` mode pre-computes a dominant-color key for every image via a
/// fast 16×16 thumbnail mean, then sorts globally by HSV (hue → sat → val).
/// Videos always trail images.
pub fn sort_entries(
    entries: &mut Vec<(walkdir::DirEntry, EntryKind)>,
    mode: SortMode,
) {
    match mode {
        SortMode::Folder => {
            entries.sort_by(|(a, ak), (b, bk)| {
                // Video first (uncompressed), then Image (compressed)
                let ka = if *ak == EntryKind::Video { 0 } else { 1 };
                let kb = if *bk == EntryKind::Video { 0 } else { 1 };
                ka.cmp(&kb)
                    .then_with(|| a.path().parent().cmp(&b.path().parent()))
                    .then_with(|| a.file_name().cmp(b.file_name()))
            });
        }

        SortMode::Color => {
            // 1. Pre-compute color key for every entry in parallel.
            //    This is the expensive step (thumbnail decode per image);
            //    rayon spreads it across all available cores.
            let keys: Vec<(u8, u8, u8)> = entries.par_iter().map(|(e, k)| {
                if *k == EntryKind::Image {
                    color_key(e.path())
                } else {
                    // Sentinel: videos sort after all images.
                    (u8::MAX, u8::MAX, u8::MAX)
                }
            }).collect();

            // 2. Build a sorted index permutation.
            let mut indices: Vec<usize> = (0..entries.len()).collect();
            indices.sort_by(|&a, &b| {
                let ka = keys[a];
                let kb = keys[b];
                // is_video flag (true = video)
                let va = ka == (u8::MAX, u8::MAX, u8::MAX);
                let vb = kb == (u8::MAX, u8::MAX, u8::MAX);
                // Video first (uncompressed)
                vb.cmp(&va)
                    // hue → saturation → value
                    .then_with(|| ka.cmp(&kb))
                    // tie-break by filename
                    .then_with(|| entries[a].0.file_name().cmp(entries[b].0.file_name()))
            });

            // 3. Permute entries in-place using Option<T> to move without Clone.
            let mut optioned: Vec<Option<(walkdir::DirEntry, EntryKind)>> =
                entries.drain(..).map(Some).collect();
            for i in indices {
                entries.push(optioned[i].take().unwrap());
            }
        }
    }
}

/// Compute a sort key from the dominant color of an image.
///
/// Loads a 16×16 thumbnail, averages RGB, then converts to HSV.
/// Returns `(hue_0_255, sat_0_255, val_0_255)`.
pub fn color_key(path: &Path) -> (u8, u8, u8) {
    let img = match image::open(path) {
        Ok(i) => i,
        Err(_) => return (0, 0, 0), // fallback: treat as black
    };
    let thumb = img.thumbnail(16, 16).to_rgb8();
    let n = thumb.pixels().count() as u64;
    if n == 0 { return (0, 0, 0); }

    let (rs, gs, bs) = thumb.pixels().fold((0u64, 0u64, 0u64), |(r, g, b), p| {
        (r + p[0] as u64, g + p[1] as u64, b + p[2] as u64)
    });
    rgb_to_hsv_key((rs / n) as u8, (gs / n) as u8, (bs / n) as u8)
}

/// Convert mean RGB → `(hue_256, sat_256, val_256)` sort key.
fn rgb_to_hsv_key(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;

    let cmax = rf.max(gf).max(bf);
    let cmin = rf.min(gf).min(bf);
    let delta = cmax - cmin;

    let val = (cmax * 255.0) as u8;

    if delta < 1e-6 || cmax < 1e-6 {
        // Achromatic — group greys/blacks/whites together at hue 0
        return (0, 0, val);
    }

    let sat = ((delta / cmax) * 255.0) as u8;

    let hue_deg = if (cmax - rf).abs() < 1e-6 {
        60.0 * (((gf - bf) / delta) % 6.0)
    } else if (cmax - gf).abs() < 1e-6 {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };
    let hue_deg = if hue_deg < 0.0 { hue_deg + 360.0 } else { hue_deg };
    let hue = ((hue_deg / 360.0) * 255.0) as u8;

    (hue, sat, val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_to_hsv_red() {
        let (h, s, v) = rgb_to_hsv_key(255, 0, 0);
        assert_eq!(v, 255);
        assert_eq!(s, 255);
        assert_eq!(h, 0); // red = 0°
    }

    #[test]
    fn test_rgb_to_hsv_green() {
        let (h, s, v) = rgb_to_hsv_key(0, 255, 0);
        assert_eq!(v, 255);
        assert_eq!(s, 255);
        // green ≈ 120° → 120/360*255 ≈ 85
        assert!((h as i32 - 85).abs() <= 2, "green hue={}", h);
    }

    #[test]
    fn test_rgb_to_hsv_grey() {
        let (h, s, _v) = rgb_to_hsv_key(128, 128, 128);
        assert_eq!(h, 0);
        assert_eq!(s, 0);
    }

    #[test]
    fn test_sort_mode_from_str() {
        assert_eq!("folder".parse::<SortMode>().unwrap(), SortMode::Folder);
        assert_eq!("color".parse::<SortMode>().unwrap(), SortMode::Color);
        assert_eq!("COLOR".parse::<SortMode>().unwrap(), SortMode::Color);
        assert!("xyz".parse::<SortMode>().is_err());
    }
}
