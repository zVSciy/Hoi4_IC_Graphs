//! Saving graphs to disk.

pub mod csv;
pub mod html;

use std::{
    io,
    path::{Path, PathBuf},
};

use eframe::egui::{ColorImage, Rect};

/// Crops `region` (in points) out of a window screenshot and writes it as PNG.
pub fn save_png(
    screenshot: &ColorImage,
    region: Rect,
    pixels_per_point: f32,
    path: &Path,
) -> io::Result<()> {
    let cropped = crop(screenshot, region, pixels_per_point)
        .ok_or_else(|| io::Error::other("graph area is outside the window"))?;
    let [width, height] = cropped.size;
    let rgba: Vec<u8> = cropped
        .pixels
        .iter()
        .flat_map(|c| c.to_srgba_unmultiplied())
        .collect();
    image::save_buffer(
        path,
        &rgba,
        width as u32,
        height as u32,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(io::Error::other)
}

/// Like [`ColorImage::region`], but clamped to the image instead of panicking
/// when the area extends past it (e.g. the window shrank since the last frame).
fn crop(image: &ColorImage, region: Rect, pixels_per_point: f32) -> Option<ColorImage> {
    let [width, height] = image.size;
    let px = |v: f32, max: usize| ((v * pixels_per_point).max(0.0) as usize).min(max);
    let (x0, x1) = (px(region.min.x, width), px(region.max.x, width));
    let (y0, y1) = (px(region.min.y, height), px(region.max.y, height));
    (x1 > x0 && y1 > y0).then(|| image.region_by_pixels([x0, y0], [x1 - x0, y1 - y0]))
}

/// Writes an HTML graph to a fresh file in the temp directory and opens it in
/// the default browser. Returns the file so the caller can delete it later.
pub fn open_in_browser(html: &str, stem: &str) -> io::Result<PathBuf> {
    let path =
        std::env::temp_dir().join(format!("hoi4_analyzer_{}_{stem}.html", std::process::id()));
    std::fs::write(&path, html)?;
    open::that_detached(&path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Color32, pos2};

    #[test]
    fn crop_clamps_to_image() {
        let image = ColorImage::new([100, 50], vec![Color32::RED; 100 * 50]);
        let inside = crop(
            &image,
            Rect::from_min_max(pos2(10.0, 10.0), pos2(20.0, 30.0)),
            1.0,
        )
        .unwrap();
        assert_eq!(inside.size, [10, 20]);
        let scaled = crop(
            &image,
            Rect::from_min_max(pos2(10.0, 10.0), pos2(20.0, 20.0)),
            2.0,
        )
        .unwrap();
        assert_eq!(scaled.size, [20, 20]);
        let overflow = crop(
            &image,
            Rect::from_min_max(pos2(-5.0, 40.0), pos2(500.0, 500.0)),
            1.0,
        )
        .unwrap();
        assert_eq!(overflow.size, [100, 10]);
        assert!(
            crop(
                &image,
                Rect::from_min_max(pos2(200.0, 0.0), pos2(300.0, 10.0)),
                1.0
            )
            .is_none()
        );
    }
}
