use std::path::Path;
use anyhow::{Context, Result};

/// Load an image from path, convert to RGB, planarize to RRR...GGG...BBB... byte layout.
///
/// Returns (width, height, planar_bytes).
/// Planar format maximizes ZSTD LDM cross-image matches (all R-planes adjacent in stream).
pub fn load_planar(path: &Path) -> Result<(u32, u32, Vec<u8>)> {
    let img = image::open(path)
        .with_context(|| format!("Open image {:?}", path))?;
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let pixels = rgb.into_raw(); // Vec<u8> in RGBRGB order
    let total = (w as usize) * (h as usize);

    let mut planar = Vec::with_capacity(total * 3);

    // R plane: indices 0, 3, 6, 9...
    for i in (0..total * 3).step_by(3) {
        planar.push(pixels[i]);
    }
    // G plane: indices 1, 4, 7, 10...
    for i in (1..total * 3).step_by(3) {
        planar.push(pixels[i]);
    }
    // B plane: indices 2, 5, 8, 11...
    for i in (2..total * 3).step_by(3) {
        planar.push(pixels[i]);
    }

    Ok((w, h, planar))
}

/// De-planarize RRR...GGG...BBB... bytes back into an RGB image and save as PNG.
pub fn save_planar(path: &Path, width: u32, height: u32, data: &[u8]) -> Result<()> {
    let total = (width as usize) * (height as usize);
    anyhow::ensure!(
        data.len() == total * 3,
        "Data size mismatch for {width}x{height}: got {} bytes, expected {}",
        data.len(),
        total * 3
    );

    let mut rgb = vec![0u8; total * 3];

    // R plane starts at 0
    for (i, &v) in data[..total].iter().enumerate() {
        rgb[i * 3] = v;
    }
    // G plane starts at total
    for (i, &v) in data[total..total * 2].iter().enumerate() {
        rgb[i * 3 + 1] = v;
    }
    // B plane starts at total * 2
    for (i, &v) in data[total * 2..].iter().enumerate() {
        rgb[i * 3 + 2] = v;
    }

    let img = image::RgbImage::from_raw(width, height, rgb)
        .context("Failed to create RgbImage from raw data")?;

    // Save as PNG (replace original extension)
    let png_path = path.with_extension("png");
    img.save(&png_path)
        .with_context(|| format!("Save PNG {:?}", png_path))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_planar_roundtrip() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("test.png");

        // Create a 4x4 test image with unique pixel values
        let mut img = image::RgbImage::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                img.put_pixel(x, y, image::Rgb([(x * 64) as u8, (y * 64) as u8, 128]));
            }
        }
        img.save(&src).unwrap();

        let (w, h, planar) = load_planar(&src).unwrap();
        assert_eq!(w, 4);
        assert_eq!(h, 4);
        assert_eq!(planar.len(), 4 * 4 * 3);

        let dst = dir.path().join("out.png");
        save_planar(&dst, w, h, &planar).unwrap();

        let restored = image::open(&dst).unwrap().to_rgb8();
        assert_eq!(restored.dimensions(), (4, 4));
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(restored.get_pixel(x, y), img.get_pixel(x, y),
                    "Pixel mismatch at ({x},{y})");
            }
        }
    }

    #[test]
    fn test_grayscale_auto_rgb() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("gray.png");

        // 8x8 grayscale image
        let gray = image::GrayImage::from_raw(8, 8, vec![128u8; 64]).unwrap();
        gray.save(&src).unwrap();

        let (w, h, planar) = load_planar(&src).unwrap();
        assert_eq!(w, 8);
        assert_eq!(h, 8);
        assert_eq!(planar.len(), 8 * 8 * 3);
        // All R, G, B should be 128
        assert!(planar.iter().all(|&b| b == 128));
    }

    #[test]
    fn test_bad_data_size_rejected() {
        let dir = TempDir::new().unwrap();
        let dst = dir.path().join("out.png");
        let result = save_planar(&dst, 10, 10, &[0u8; 100]); // 10*10*3 = 300, not 100
        assert!(result.is_err());
    }
}
