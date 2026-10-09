use std::path::PathBuf;

use rawweave_raw::{RawDecodeLimits, RawError, RawlerDecoder, RawloaderDecoder};

fn dataset_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/images/raw")
}

#[test]
fn compressed_raf_and_canon_cr2_cr3_decode_sensor_mosaics() {
    for (filename, make, cfa_size) in [
        ("fujifilm-x-t4-lossless.raf", "FUJIFILM", 6),
        ("canon-eos-kiss-f.cr2", "Canon", 2),
        ("canon-eos-1dx-iii-craw.cr3", "Canon", 2),
        ("canon-eos-1dx-iii-raw.cr3", "Canon", 2),
    ] {
        let frame = RawloaderDecoder::default()
            .decode_file(dataset_root().join(filename))
            .unwrap_or_else(|error| panic!("{filename}: {error}"));
        assert_eq!(frame.camera().make, make);
        assert_eq!(frame.mosaic().cfa().width(), cfa_size);
        assert_eq!(frame.mosaic().cfa().height(), cfa_size);
        assert!(frame.mosaic().samples().iter().any(|sample| *sample > 0.0));
        assert_eq!(
            frame.mosaic().samples().len(),
            frame.sensor_dimensions().pixel_count().unwrap()
        );
    }
}

#[test]
fn cr3_sensor_limits_are_checked_before_pixel_decompression() {
    let limits = RawDecodeLimits {
        max_width: 64,
        max_height: 64,
        ..RawDecodeLimits::default()
    };
    for filename in ["canon-eos-1dx-iii-raw.cr3", "canon-eos-1dx-iii-craw.cr3"] {
        let error = RawlerDecoder::with_limits(limits)
            .decode_file(dataset_root().join(filename))
            .unwrap_err();
        assert!(
            matches!(error, RawError::DimensionTooLarge { .. }),
            "{error}"
        );
    }
}

#[test]
fn downloaded_cc0_raw_dataset_decodes_with_the_production_adapter() {
    let decoder = RawloaderDecoder::default();
    let files = [
        "nikon-d70s-12bit-lossy.nef",
        "sony-ilce-7s-14bit-compressed.arw",
        "fujifilm-finepix-s5000.raf",
    ];

    for filename in files {
        let path = dataset_root().join(filename);
        let frame = decoder
            .decode_file(&path)
            .unwrap_or_else(|error| panic!("{} failed to decode: {error}", path.display()));
        assert!(
            frame.sensor_dimensions().width > 0,
            "{filename} has zero width"
        );
        assert!(
            frame.sensor_dimensions().height > 0,
            "{filename} has zero height"
        );
    }
}
