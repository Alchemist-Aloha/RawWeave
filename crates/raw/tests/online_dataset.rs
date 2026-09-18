use std::path::PathBuf;

use rawweave_raw::RawloaderDecoder;

fn dataset_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/images/raw")
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
