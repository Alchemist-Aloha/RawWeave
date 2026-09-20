use rawweave_batch::{
    MAX_ORDINARY_ENCODED_BYTES, MAX_ORDINARY_IMAGE_EDGE, MAX_ORDINARY_IMAGE_PIXELS,
    OrdinaryDecodeError, decode_ordinary_bytes,
};
use std::io::Cursor;

fn declared_png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut bytes), 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[0, 0, 0, 255]).unwrap();
        writer.finish().unwrap();
    }
    bytes[16..20].copy_from_slice(&width.to_be_bytes());
    bytes[20..24].copy_from_slice(&height.to_be_bytes());
    let crc = png_crc32(&bytes[12..29]);
    bytes[29..33].copy_from_slice(&crc.to_be_bytes());
    bytes
}

fn png_crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 0 {
                crc >> 1
            } else {
                (crc >> 1) ^ 0xedb8_8320
            };
        }
    }
    !crc
}

#[test]
fn ordinary_decode_rejects_declared_dimensions_before_decoding_pixels() {
    let error = decode_ordinary_bytes(&declared_png(MAX_ORDINARY_IMAGE_EDGE + 1, 1))
        .expect_err("declared oversized image must be rejected");

    assert!(
        matches!(
            error,
            OrdinaryDecodeError::DimensionsTooLarge { .. }
                | OrdinaryDecodeError::PixelCountTooLarge { .. }
                | OrdinaryDecodeError::Rgba32fAllocationTooLarge { .. }
        ),
        "unexpected decode error: {error:?}"
    );
}

#[test]
fn ordinary_decode_rejects_declared_pixel_count_before_decoding_pixels() {
    let width = MAX_ORDINARY_IMAGE_EDGE.min(4096);
    let height = (MAX_ORDINARY_IMAGE_PIXELS / u64::from(width) + 1) as u32;
    let error = decode_ordinary_bytes(&declared_png(width, height))
        .expect_err("declared oversized pixel count must be rejected");

    assert!(
        matches!(
            error,
            OrdinaryDecodeError::PixelCountTooLarge { .. }
                | OrdinaryDecodeError::Rgba32fAllocationTooLarge { .. }
        ),
        "unexpected decode error: {error:?}"
    );
}

#[test]
fn ordinary_decode_rejects_rgba32f_allocations_before_decoding_pixels() {
    let error = decode_ordinary_bytes(&declared_png(3000, 3000))
        .expect_err("declared RGBA32F allocation must be rejected");

    assert!(
        matches!(error, OrdinaryDecodeError::Rgba32fAllocationTooLarge { .. }),
        "unexpected decode error: {error:?}"
    );
}

#[test]
fn ordinary_decode_rejects_encoded_payloads_above_the_byte_bound() {
    let bytes = vec![0_u8; MAX_ORDINARY_ENCODED_BYTES + 1];
    let error = decode_ordinary_bytes(&bytes).expect_err("encoded payload must be bounded");

    assert!(matches!(
        error,
        OrdinaryDecodeError::EncodedBytesTooLarge { .. }
    ));
}
