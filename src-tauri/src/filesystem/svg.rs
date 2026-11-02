//! SVG thumbnails rendered in Rust (resvg), for consumers that cannot use the WebView's own SVG
//! renderer: the phone over remote control (`thumb.get`) and cached SFTP copies.
//!
//! Security: an SVG can reference other files (`<image href="/elsewhere/photo.png">`, relative
//! hrefs, `file://`). Rendering those would copy pixels of a file outside the folder the user shared
//! (remote-control rule 2) into the thumbnail sent to the phone. Every non-`data:` reference is
//! refused, `resources_dir` is unset, and usvg already forbids `<image>` inside embedded SVGs.

use std::sync::{Arc, OnceLock};

use resvg::{tiny_skia, usvg};

// Bigger sources are not worth parsing for a thumbnail (and bound parse time on hostile files).
const SVG_MAX_BYTES: u64 = 5 * 1024 * 1024;

const ERROR_TOO_LARGE: &str = "SVG too large for a thumbnail";
const ERROR_EMPTY: &str = "SVG has no drawable size";

// Render `path` to fit within `size` x `size`, on white: thumbnails are cached as JPEG (no alpha).
pub fn svg_thumbnail(path: &str, size: u32) -> Result<image::DynamicImage, String> {
    let length = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if length > SVG_MAX_BYTES {
        return Err(ERROR_TOO_LARGE.to_string());
    }
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    render(&data, size)
}

fn render(data: &[u8], size: u32) -> Result<image::DynamicImage, String> {
    let options = usvg::Options {
        resources_dir: None,
        fontdb: system_fonts(),
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(data, &options).map_err(|e| e.to_string())?;
    let source = tree.size();
    let edge = size.max(1) as f32;
    let scale = (edge / source.width()).min(edge / source.height());
    if !scale.is_finite() || scale <= 0.0 {
        return Err(ERROR_EMPTY.to_string());
    }
    let width = ((source.width() * scale).round() as u32).clamp(1, size.max(1));
    let height = ((source.height() * scale).round() as u32).clamp(1, size.max(1));
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or(ERROR_EMPTY)?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // Fully opaque after the white fill, so premultiplied RGBA equals straight RGBA.
    image::RgbaImage::from_raw(width, height, pixmap.take())
        .map(image::DynamicImage::ImageRgba8)
        .ok_or_else(|| ERROR_EMPTY.to_string())
}

// System fonts for <text>, scanned once per process (the scan is slow; renders are frequent).
fn system_fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut database = usvg::fontdb::Database::new();
            database.load_system_fonts();
            Arc::new(database)
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn center(image: &image::DynamicImage) -> [u8; 4] {
        let rgba = image.to_rgba8();
        rgba.get_pixel(rgba.width() / 2, rgba.height() / 2).0
    }

    #[test]
    fn renders_to_fit_keeping_aspect_ratio() {
        // `##` delimiters: the color's `"#` would otherwise end the raw string.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
            <rect width="200" height="100" fill="#ff0000"/></svg>"##;
        let image = render(svg, 64).unwrap();
        assert_eq!((image.width(), image.height()), (64, 32));
        assert_eq!(center(&image), [255, 0, 0, 255]);
    }

    #[test]
    fn transparent_areas_become_white() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#;
        assert_eq!(center(&render(svg, 32).unwrap()), [255, 255, 255, 255]);
    }

    #[test]
    fn never_loads_files_referenced_by_path() {
        // A red PNG outside "the shared folder"; the SVG tries to pull it in by absolute path and
        // as file:// URL. Both must be ignored, leaving the white background.
        let dir = std::env::temp_dir().join(format!("sfb-svg-href-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("secret.png");
        image::RgbImage::from_pixel(8, 8, image::Rgb([255, 0, 0]))
            .save(&png)
            .unwrap();
        let png = png.to_string_lossy();
        for href in [png.to_string(), format!("file://{png}")] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="8" height="8">
                <image width="8" height="8" href="{href}" xlink:href="{href}"/></svg>"#
            );
            assert_eq!(center(&render(svg.as_bytes(), 16).unwrap()), [255, 255, 255, 255], "{href}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_garbage() {
        assert!(render(b"not an svg", 32).is_err());
    }
}
