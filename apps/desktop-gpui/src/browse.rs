//! Image browsing: folder scanning and bounded thumbnails.
//!
//! Decoding stays in the Rust image/RAW boundaries; this module only lists a
//! folder and produces one small display raster per file for the contact sheet.

use crate::PreviewFrame;
use rawweave_image::{Dimensions, Image};
use rawweave_raw::{RawDecodeLimits, RawloaderDecoder};
use std::path::{Path, PathBuf};

/// Ordinary image extensions the editor can open.
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "tif", "tiff", "webp", "bmp", "gif", "avif", "jxl",
];
/// RAW extensions the decoder boundary claims.
pub const RAW_EXTENSIONS: &[&str] = &[
    "3fr", "arw", "cr2", "cr3", "crw", "dng", "erf", "fff", "iiq", "kdc", "mef", "mos", "mrw",
    "nef", "nrw", "orf", "pef", "raf", "raw", "rw2", "rwl", "srw", "x3f",
];
/// A folder larger than this is listed only in part, and says so.
pub const MAX_BROWSE_ENTRIES: usize = 2000;
/// Long-edge ceiling for one contact-sheet thumbnail.
pub const THUMBNAIL_EDGE: u32 = 320;

/// One listed file. `code` is what the sheet prints about it, never user data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowseEntry {
    pub path: PathBuf,
    pub name: String,
    pub raw: bool,
    pub code: String,
    /// Encoded size, printed when the file has no dimensions to report.
    pub bytes: u64,
}

/// The result of listing a folder. `truncated` is reported, not hidden.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FolderScan {
    pub entries: Vec<BrowseEntry>,
    pub truncated: bool,
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
}

/// Whether the decoder boundary claims this file as RAW.
pub fn is_raw(path: &Path) -> bool {
    extension(path).is_some_and(|ext| RAW_EXTENSIONS.contains(&ext.as_str()))
}

fn is_ordinary(path: &Path) -> bool {
    extension(path).is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.as_str()))
}

/// Whether browsing and the file dialog should offer this path.
pub fn is_supported(path: &Path) -> bool {
    is_raw(path) || is_ordinary(path)
}

/// The uppercase code printed beside a browsed file.
pub fn media_code(path: &Path) -> Option<String> {
    let ext = extension(path)?;
    (is_raw(path) || is_ordinary(path)).then(|| ext.to_uppercase())
}

/// List the supported images directly inside one folder, sorted by name.
///
/// Subfolders are not walked: the sheet shows what this folder holds, and a
/// bounded count keeps a huge directory from building an unbounded element tree.
pub fn scan_folder(dir: &Path) -> Result<FolderScan, String> {
    let reader = std::fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let mut scan = FolderScan::default();
    for entry in reader {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !path.is_file() || !is_supported(&path) {
            continue;
        }
        if scan.entries.len() >= MAX_BROWSE_ENTRIES {
            scan.truncated = true;
            break;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let raw = is_raw(&path);
        let code = media_code(&path).unwrap_or_default();
        scan.entries.push(BrowseEntry {
            path,
            name,
            raw,
            code,
            bytes: metadata.len(),
        });
    }
    scan.entries.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    Ok(scan)
}

/// A measured file size for the ledger row, in the units a person reads.
pub fn byte_label(bytes: u64) -> String {
    const KB: u64 = 1000;
    const MB: u64 = 1000 * KB;
    const GB: u64 = 1000 * MB;
    match bytes {
        b if b >= GB => format!("{:.1} GB", b as f64 / GB as f64),
        b if b >= MB => format!("{:.1} MB", b as f64 / MB as f64),
        b if b >= KB => format!("{:.0} KB", b as f64 / KB as f64),
        b => format!("{b} B"),
    }
}

/// The coarsest mip whose longest edge fits inside `edge`.
pub fn mip_for(dimensions: Dimensions, edge: u32) -> u8 {
    (0..=6)
        .find(|mip| dimensions.width.max(dimensions.height).div_ceil(1 << mip) <= edge)
        .unwrap_or(6)
}

/// Decode one bounded display thumbnail for the contact sheet.
///
/// RAW files use their embedded JPEG preview when the container has one: the
/// sheet is a contact sheet, not a development, so the sensor mosaic is not
/// demosaiced here.
pub fn thumbnail(path: &Path) -> Result<PreviewFrame, String> {
    let image = if is_raw(path) {
        embedded_preview_image(path)?
    } else {
        rawweave_batch::decode_ordinary_file(path).map_err(|error| error.to_string())?
    };
    let full = image.dimensions();
    let mip = mip_for(full, THUMBNAIL_EDGE);
    let selected = if mip == 0 {
        image
    } else {
        image.sample_mip(mip).map_err(|error| error.to_string())?
    };
    crate::image_display_frame(&selected, full)
}

fn embedded_preview_image(path: &Path) -> Result<Image, String> {
    let bytes = crate::bounded_read(path, RawDecodeLimits::default().max_input_bytes)?;
    let frame = RawloaderDecoder::default()
        .decode(&bytes)
        .map_err(|error| error.to_string())?;
    let preview = frame
        .embedded_preview_bytes()
        .ok_or("RAW file has no embedded preview to show in the contact sheet")?;
    let decoded =
        rawweave_batch::decode_ordinary_bytes(preview).map_err(|error| error.to_string())?;
    rawweave_batch::decode_ordinary_dynamic(decoded).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"not an image").unwrap();
        path
    }

    #[test]
    fn scan_lists_only_supported_files_sorted_and_ignores_directories() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "B.TIF");
        write(dir.path(), "a.nef");
        write(dir.path(), "notes.txt");
        std::fs::create_dir(dir.path().join("nested.png")).unwrap();
        let scan = scan_folder(dir.path()).unwrap();
        assert!(!scan.truncated);
        assert_eq!(
            scan.entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["a.nef", "B.TIF"]
        );
        assert!(scan.entries[0].raw);
        assert_eq!(scan.entries[0].code, "NEF");
        assert_eq!(scan.entries[1].code, "TIF");
        assert_eq!(
            scan.entries[0].bytes,
            b"not an image".len() as u64,
            "the ledger reports the real size"
        );
        assert_eq!(byte_label(980), "980 B");
        assert_eq!(byte_label(12_400), "12 KB");
        assert_eq!(byte_label(24_500_000), "24.5 MB");
        assert_eq!(byte_label(4_200_000_000), "4.2 GB");
        assert!(scan_folder(&dir.path().join("missing")).is_err());
    }

    #[test]
    fn extension_claims_and_media_codes_stay_in_one_place() {
        assert!(is_raw(Path::new("/a/a.RW2")));
        assert!(is_supported(Path::new("x.JPEG")));
        assert!(!is_supported(Path::new("x.psd")));
        assert_eq!(media_code(Path::new("x.cr3")).as_deref(), Some("CR3"));
        assert_eq!(media_code(Path::new("x.psd")), None);
        assert_eq!(media_code(Path::new("noextension")), None);
    }

    #[test]
    fn thumbnails_bound_the_long_edge_and_report_unreadable_files() {
        assert_eq!(mip_for(Dimensions::new(6000, 4000), THUMBNAIL_EDGE), 5);
        assert_eq!(mip_for(Dimensions::new(320, 200), THUMBNAIL_EDGE), 0);
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "broken.jpg");
        assert!(thumbnail(&path).is_err());
    }

    // Encodes a real file, so it needs the feature that links the image codec.
    #[cfg(feature = "native")]
    #[test]
    fn thumbnail_reduces_a_real_ordinary_image_to_a_display_raster() {
        let dir = tempfile::tempdir().unwrap();
        let source =
            rawweave_image::Image::from_pixels(64, 32, vec![[0.25, 0.5, 0.75, 1.0]; 64 * 32])
                .unwrap();
        let path = dir.path().join("wide.png");
        let mut encoded = std::fs::File::create(&path).unwrap();
        let mut rgba = image::RgbaImage::new(64, 32);
        for pixel in rgba.pixels_mut() {
            *pixel = image::Rgba([64, 128, 191, 255]);
        }
        rgba.write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        drop(encoded);
        let frame = thumbnail(&path).unwrap();
        assert_eq!(frame.dimensions, Dimensions::new(64, 32));
        assert_eq!(frame.full_dimensions, source.dimensions());
        assert_eq!(frame.bgra.len(), 64 * 32 * 4);
        assert_eq!(&frame.bgra[..4], &[191, 128, 64, 255]);
    }
}
