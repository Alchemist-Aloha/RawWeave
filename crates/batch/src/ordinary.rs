use std::fs::File;
use std::io::{BufReader, Cursor, Read};
use std::mem::size_of;
use std::path::Path;

use rawweave_image::{ColorDomain, Image, PixelFormat};
use thiserror::Error;

/// Maximum encoded ordinary-image payload accepted by the shared decoder.
pub const MAX_ORDINARY_ENCODED_BYTES: usize = 64 * 1024 * 1024;
/// Maximum width or height accepted by the shared decoder.
pub const MAX_ORDINARY_IMAGE_EDGE: u32 = 16_384;
/// Maximum number of decoded ordinary-image pixels.
pub const MAX_ORDINARY_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;
/// Maximum allocation reserved for the final RGBA32F image buffer.
pub const MAX_ORDINARY_RGBA32F_BYTES: usize = 128 * 1024 * 1024;

const RGBA32F_BYTES_PER_PIXEL: usize = size_of::<[f32; 4]>();

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OrdinaryDecodeError {
    #[error("encoded ordinary image is {actual} bytes, exceeding the {limit}-byte limit")]
    EncodedBytesTooLarge { actual: usize, limit: usize },
    #[error("ordinary image dimensions {width}x{height} exceed the {limit}-pixel edge limit")]
    DimensionsTooLarge { width: u32, height: u32, limit: u32 },
    #[error(
        "ordinary image dimensions {width}x{height} contain {pixels} pixels, exceeding the {limit}-pixel limit"
    )]
    PixelCountTooLarge {
        width: u32,
        height: u32,
        pixels: u64,
        limit: u64,
    },
    #[error(
        "ordinary image dimensions {width}x{height} require {bytes} RGBA32F bytes, exceeding the {limit}-byte allocation limit"
    )]
    Rgba32fAllocationTooLarge {
        width: u32,
        height: u32,
        bytes: usize,
        limit: usize,
    },
    #[error("could not identify ordinary image format: {0}")]
    Format(String),
    #[error("could not inspect ordinary image header: {0}")]
    Header(String),
    #[error("could not decode ordinary image: {0}")]
    Decode(String),
    #[error("could not read ordinary image: {0}")]
    Io(String),
    #[error("decoded ordinary image dimensions changed from {expected:?} to {actual:?}")]
    DimensionsChanged {
        expected: (u32, u32),
        actual: (u32, u32),
    },
    #[error("decoded ordinary image pixel buffer has the wrong length")]
    PixelBufferLength,
    #[error("could not create RawWeave image: {0}")]
    Image(String),
}

/// Decode an ordinary image after bounded encoded-size and header validation.
///
/// The header is inspected before `decode` is called so declared dimensions,
/// pixel count, and the checked RGBA32F allocation budget reject hostile input
/// without allocating the final image buffer.
pub fn decode_ordinary_bytes(bytes: &[u8]) -> Result<image::DynamicImage, OrdinaryDecodeError> {
    validate_encoded_size(bytes.len())?;

    let mut header_reader = image_reader(bytes)?;
    header_reader.limits(image::Limits::no_limits());
    let dimensions = header_reader
        .into_dimensions()
        .map_err(|error| OrdinaryDecodeError::Header(error.to_string()))?;
    let _pixel_count = validate_dimensions(dimensions.0, dimensions.1)?;

    let mut reader = image_reader(bytes)?;
    reader.limits(decoder_limits());
    reader
        .decode()
        .map_err(|error| OrdinaryDecodeError::Decode(error.to_string()))
}

/// Read and decode an ordinary image into RawWeave's bounded RGBA32F image.
pub fn decode_ordinary_file(path: &Path) -> Result<Image, OrdinaryDecodeError> {
    let bytes = read_bounded_file(path)?;
    let decoded = decode_ordinary_bytes(&bytes)?;
    dynamic_image_to_rawweave(decoded)
}

/// Validate the allocation implied by an ordinary image header.
pub fn validate_dimensions(width: u32, height: u32) -> Result<usize, OrdinaryDecodeError> {
    if width > MAX_ORDINARY_IMAGE_EDGE || height > MAX_ORDINARY_IMAGE_EDGE {
        return Err(OrdinaryDecodeError::DimensionsTooLarge {
            width,
            height,
            limit: MAX_ORDINARY_IMAGE_EDGE,
        });
    }

    let pixels = u64::from(width).checked_mul(u64::from(height)).ok_or(
        OrdinaryDecodeError::PixelCountTooLarge {
            width,
            height,
            pixels: u64::MAX,
            limit: MAX_ORDINARY_IMAGE_PIXELS,
        },
    )?;
    if pixels > MAX_ORDINARY_IMAGE_PIXELS {
        return Err(OrdinaryDecodeError::PixelCountTooLarge {
            width,
            height,
            pixels,
            limit: MAX_ORDINARY_IMAGE_PIXELS,
        });
    }

    let pixels_as_usize =
        usize::try_from(pixels).map_err(|_| OrdinaryDecodeError::Rgba32fAllocationTooLarge {
            width,
            height,
            bytes: usize::MAX,
            limit: MAX_ORDINARY_RGBA32F_BYTES,
        })?;
    let rgba32f_bytes = pixels_as_usize.checked_mul(RGBA32F_BYTES_PER_PIXEL).ok_or(
        OrdinaryDecodeError::Rgba32fAllocationTooLarge {
            width,
            height,
            bytes: usize::MAX,
            limit: MAX_ORDINARY_RGBA32F_BYTES,
        },
    )?;
    if rgba32f_bytes > MAX_ORDINARY_RGBA32F_BYTES {
        return Err(OrdinaryDecodeError::Rgba32fAllocationTooLarge {
            width,
            height,
            bytes: rgba32f_bytes,
            limit: MAX_ORDINARY_RGBA32F_BYTES,
        });
    }
    Ok(pixels_as_usize)
}

fn validate_encoded_size(actual: usize) -> Result<(), OrdinaryDecodeError> {
    if actual > MAX_ORDINARY_ENCODED_BYTES {
        return Err(OrdinaryDecodeError::EncodedBytesTooLarge {
            actual,
            limit: MAX_ORDINARY_ENCODED_BYTES,
        });
    }
    Ok(())
}

fn image_reader(
    bytes: &[u8],
) -> Result<image::ImageReader<BufReader<Cursor<&[u8]>>>, OrdinaryDecodeError> {
    image::ImageReader::new(BufReader::new(Cursor::new(bytes)))
        .with_guessed_format()
        .map_err(|error| OrdinaryDecodeError::Format(error.to_string()))
}

fn decoder_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_ORDINARY_IMAGE_EDGE);
    limits.max_image_height = Some(MAX_ORDINARY_IMAGE_EDGE);
    limits.max_alloc = Some(MAX_ORDINARY_RGBA32F_BYTES as u64);
    limits
}

fn read_bounded_file(path: &Path) -> Result<Vec<u8>, OrdinaryDecodeError> {
    let file = File::open(path).map_err(|error| OrdinaryDecodeError::Io(error.to_string()))?;
    let mut bytes = Vec::new();
    let capacity = MAX_ORDINARY_ENCODED_BYTES.checked_add(1).ok_or(
        OrdinaryDecodeError::EncodedBytesTooLarge {
            actual: usize::MAX,
            limit: MAX_ORDINARY_ENCODED_BYTES,
        },
    )?;
    let mut reader = file.take(capacity as u64);
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| OrdinaryDecodeError::Io(error.to_string()))?;
    validate_encoded_size(bytes.len())?;
    Ok(bytes)
}

fn dynamic_image_to_rawweave(decoded: image::DynamicImage) -> Result<Image, OrdinaryDecodeError> {
    let expected = (decoded.width(), decoded.height());
    let pixel_count = validate_dimensions(expected.0, expected.1)?;
    let rgba = decoded.to_rgba32f();
    if (rgba.width(), rgba.height()) != expected {
        return Err(OrdinaryDecodeError::DimensionsChanged {
            expected,
            actual: (rgba.width(), rgba.height()),
        });
    }
    let mut pixels = Vec::with_capacity(pixel_count);
    pixels.extend(rgba.pixels().map(|pixel| pixel.0));
    if pixels.len() != pixel_count {
        return Err(OrdinaryDecodeError::PixelBufferLength);
    }
    Image::from_pixels_with_metadata(
        expected.0,
        expected.1,
        pixels,
        PixelFormat::Rgba32Float,
        ColorDomain::Srgb,
    )
    .map_err(|error| OrdinaryDecodeError::Image(error.to_string()))
}
