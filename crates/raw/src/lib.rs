//! RAW foundations and decoder adapters.
//!
//! The public decoder boundary deliberately does not expose `rawloader` types. This keeps
//! graph nodes and projects independent of the vendor decoder selected for a build. The
//! `RawloaderDecoder` below is an adapter and is compile-tested with invalid input; the
//! deterministic corpus is the validation source for RAW algorithms in this crate.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};

use exif::{In, Reader, Tag, Value};
use rawweave_image::Dimensions;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The orientation recorded by the camera metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Orientation {
    /// No rotation or flip is required.
    #[default]
    Normal,
    /// Mirror around the vertical axis.
    HorizontalFlip,
    /// Rotate 180 degrees.
    Rotate180,
    /// Mirror around the horizontal axis.
    VerticalFlip,
    /// Transpose across the top-left to bottom-right diagonal.
    Transpose,
    /// Rotate 90 degrees clockwise.
    Rotate90,
    /// Transverse reflection.
    Transverse,
    /// Rotate 270 degrees clockwise.
    Rotate270,
    /// The source did not provide a usable orientation.
    Unknown,
}

impl From<rawloader::Orientation> for Orientation {
    fn from(value: rawloader::Orientation) -> Self {
        match value {
            rawloader::Orientation::Normal => Self::Normal,
            rawloader::Orientation::HorizontalFlip => Self::HorizontalFlip,
            rawloader::Orientation::Rotate180 => Self::Rotate180,
            rawloader::Orientation::VerticalFlip => Self::VerticalFlip,
            rawloader::Orientation::Transpose => Self::Transpose,
            rawloader::Orientation::Rotate90 => Self::Rotate90,
            rawloader::Orientation::Transverse => Self::Transverse,
            rawloader::Orientation::Rotate270 => Self::Rotate270,
            rawloader::Orientation::Unknown => Self::Unknown,
        }
    }
}

/// A color in a camera's repeating color filter array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CfaColor {
    /// Red photosite.
    Red,
    /// Green photosite.
    Green,
    /// Blue photosite.
    Blue,
    /// A fourth channel used by some sensors.
    Extra,
    /// An unknown or monochrome photosite.
    Unknown,
}

/// A validated repeating color filter array pattern.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CfaPattern {
    width: u32,
    height: u32,
    colors: Vec<CfaColor>,
}

impl CfaPattern {
    /// Construct a repeating CFA pattern from row-major colors.
    pub fn new(width: u32, height: u32, colors: Vec<CfaColor>) -> Result<Self, RawError> {
        if width == 0 || height == 0 {
            return Err(RawError::InvalidCfa {
                width,
                height,
                colors: colors.len(),
            });
        }
        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or(RawError::DimensionsOverflow)?;
        if colors.len() != expected {
            return Err(RawError::InvalidCfa {
                width,
                height,
                colors: colors.len(),
            });
        }
        Ok(Self {
            width,
            height,
            colors,
        })
    }

    /// Width of the repeating pattern.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height of the repeating pattern.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Row-major pattern colors.
    pub fn colors(&self) -> &[CfaColor] {
        &self.colors
    }

    /// Return the color at a sensor coordinate, repeating the pattern as needed.
    pub fn color_at(&self, x: u32, y: u32) -> Option<CfaColor> {
        let x = (x % self.width) as usize;
        let y = (y % self.height) as usize;
        self.colors.get(y * self.width as usize + x).copied()
    }

    /// Return a pattern shifted to a cropped sensor origin.
    pub fn shifted(&self, x: u32, y: u32) -> Self {
        let colors = (0..self.height)
            .flat_map(|row| {
                (0..self.width).map(move |column| {
                    self.color_at(column + x, row + y)
                        .unwrap_or(CfaColor::Unknown)
                })
            })
            .collect();
        Self {
            width: self.width,
            height: self.height,
            colors,
        }
    }
}

/// Sensor mosaic samples and their layout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mosaic {
    dimensions: Dimensions,
    samples: Vec<f32>,
    bit_depth: u8,
    cfa: CfaPattern,
    orientation: Orientation,
}

impl Mosaic {
    /// Construct a validated sensor mosaic.
    pub fn new(
        dimensions: Dimensions,
        samples: Vec<f32>,
        bit_depth: u8,
        cfa: CfaPattern,
        orientation: Orientation,
    ) -> Result<Self, RawError> {
        if bit_depth == 0 || bit_depth > 32 {
            return Err(RawError::InvalidBitDepth(bit_depth));
        }
        let expected = dimensions
            .pixel_count()
            .map_err(|_| RawError::DimensionsOverflow)?;
        if samples.len() != expected {
            return Err(RawError::SampleCountMismatch {
                dimensions,
                expected,
                actual: samples.len(),
            });
        }
        if let Some(index) = samples.iter().position(|sample| !sample.is_finite()) {
            return Err(RawError::NonFiniteSample(index));
        }
        Ok(Self {
            dimensions,
            samples,
            bit_depth,
            cfa,
            orientation,
        })
    }

    /// Sensor dimensions.
    pub const fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    /// Raw samples in row-major order.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// Bit depth advertised by the decoder.
    pub const fn bit_depth(&self) -> u8 {
        self.bit_depth
    }

    /// Repeating CFA layout.
    pub fn cfa(&self) -> &CfaPattern {
        &self.cfa
    }

    /// Source orientation metadata.
    pub const fn orientation(&self) -> Orientation {
        self.orientation
    }

    /// Sample at a sensor coordinate.
    pub fn sample(&self, x: u32, y: u32) -> Option<f32> {
        if x >= self.dimensions.width || y >= self.dimensions.height {
            return None;
        }
        self.samples
            .get(y as usize * self.dimensions.width as usize + x as usize)
            .copied()
    }

    /// Apply a deterministic operation to every sample and validate the result.
    pub fn map_samples(
        &self,
        mut map: impl FnMut(usize, f32, CfaColor) -> f32,
    ) -> Result<Self, RawError> {
        let samples = self
            .samples
            .iter()
            .enumerate()
            .map(|(index, sample)| {
                let x = index as u32 % self.dimensions.width;
                let y = index as u32 / self.dimensions.width;
                map(
                    index,
                    *sample,
                    self.cfa.color_at(x, y).unwrap_or(CfaColor::Unknown),
                )
            })
            .collect();
        Self::new(
            self.dimensions,
            samples,
            self.bit_depth,
            self.cfa.clone(),
            self.orientation,
        )
    }

    /// Create a copy with a different sample buffer after validation.
    pub fn with_samples(&self, samples: Vec<f32>) -> Result<Self, RawError> {
        Self::new(
            self.dimensions,
            samples,
            self.bit_depth,
            self.cfa.clone(),
            self.orientation,
        )
    }
}

/// Minimum camera capture metadata exposed to graph nodes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CameraMetadata {
    /// Camera manufacturer.
    pub make: String,
    /// Camera model.
    pub model: String,
    /// Lens name, when available.
    pub lens: Option<String>,
    /// Sensor sensitivity.
    pub iso: Option<u32>,
    /// Aperture in f-stops.
    pub aperture: Option<f32>,
    /// Exposure time in seconds.
    pub shutter_seconds: Option<f32>,
    /// Focal length in millimeters.
    pub focal_length_mm: Option<f32>,
    /// Capture time in an ISO-8601-compatible string.
    pub capture_time: Option<String>,
    /// Orientation recorded by the camera.
    pub orientation: Orientation,
    /// Pixel dimensions recorded by the camera metadata, when present.
    #[serde(default)]
    pub dimensions: Option<Dimensions>,
}

/// Structured EXIF data retained for later metadata/control nodes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExifMetadata {
    /// Additional decoded tags not covered by the common fields.
    pub tags: BTreeMap<String, String>,
}

/// Metadata parsed from an input container before a RAW decoder is selected.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ParsedExifMetadata {
    /// Common camera metadata parsed from EXIF.
    pub camera: CameraMetadata,
    /// All decoded EXIF fields retained for later nodes.
    pub exif: ExifMetadata,
    /// JPEG or TIFF thumbnail bytes, when the EXIF container contains one.
    pub embedded_preview: Option<Vec<u8>>,
}

/// Camera color profile metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraProfile {
    /// Camera manufacturer.
    pub make: String,
    /// Camera model.
    pub model: String,
    /// Matrix mapping XYZ-like values into camera channels, where supplied.
    pub xyz_to_camera: [[f32; 3]; 4],
}

impl CameraProfile {
    /// Create an identity-like profile for a camera without a calibration matrix.
    pub fn identity(make: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            make: make.into(),
            model: model.into(),
            xyz_to_camera: [
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 0.0],
            ],
        }
    }
}

/// Lens correction profile metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LensProfile {
    /// Human-readable lens name.
    pub name: String,
    /// Radial distortion coefficients, if known.
    pub radial_distortion: [f32; 3],
    /// Tangential distortion coefficients, if known.
    pub tangential_distortion: [f32; 2],
    /// Radial vignette correction coefficients.
    #[serde(default)]
    pub vignette: [f32; 3],
}

impl LensProfile {
    /// Create a no-op lens profile.
    pub fn identity(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            radial_distortion: [0.0; 3],
            tangential_distortion: [0.0; 2],
            vignette: [0.0; 3],
        }
    }

    /// Whether this profile would change image coordinates.
    pub fn is_identity(&self) -> bool {
        self.radial_distortion == [0.0; 3]
            && self.tangential_distortion == [0.0; 2]
            && self.vignette == [0.0; 3]
    }
}

/// A decoded RAW frame and all metadata needed by the initial RAW graph stages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RawFrame {
    mosaic: Mosaic,
    black_levels: [f32; 4],
    white_levels: [f32; 4],
    camera: CameraMetadata,
    profile: CameraProfile,
    lens_profile: Option<LensProfile>,
    embedded_preview: Option<Vec<u8>>,
    exif: ExifMetadata,
}

impl RawFrame {
    /// Construct a validated RAW frame.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mosaic: Mosaic,
        black_levels: [f32; 4],
        white_levels: [f32; 4],
        camera: CameraMetadata,
        profile: CameraProfile,
        lens_profile: Option<LensProfile>,
        embedded_preview: Option<Vec<u8>>,
        exif: ExifMetadata,
    ) -> Result<Self, RawError> {
        if let Some(index) = black_levels
            .iter()
            .chain(white_levels.iter())
            .position(|level| !level.is_finite())
        {
            return Err(RawError::NonFiniteLevel(index));
        }
        if black_levels
            .iter()
            .zip(white_levels.iter())
            .any(|(black, white)| black > white)
        {
            return Err(RawError::BlackLevelAboveWhiteLevel);
        }
        Ok(Self {
            mosaic,
            black_levels,
            white_levels,
            camera,
            profile,
            lens_profile,
            embedded_preview,
            exif,
        })
    }

    /// Sensor dimensions.
    pub fn sensor_dimensions(&self) -> Dimensions {
        self.mosaic.dimensions()
    }

    /// Sensor mosaic.
    pub fn mosaic(&self) -> &Mosaic {
        &self.mosaic
    }

    /// Per-channel black levels in RGBA/extra order.
    pub fn black_levels(&self) -> &[f32; 4] {
        &self.black_levels
    }

    /// Per-channel white levels in RGBA/extra order.
    pub fn white_levels(&self) -> &[f32; 4] {
        &self.white_levels
    }

    /// Common camera metadata.
    pub fn camera(&self) -> &CameraMetadata {
        &self.camera
    }

    /// Camera color profile.
    pub fn profile(&self) -> &CameraProfile {
        &self.profile
    }

    /// Optional lens profile.
    pub fn lens_profile(&self) -> Option<&LensProfile> {
        self.lens_profile.as_ref()
    }

    /// Optional embedded preview bytes.
    pub fn embedded_preview(&self) -> Option<&[u8]> {
        self.embedded_preview.as_deref()
    }

    /// Structured EXIF metadata.
    pub fn exif(&self) -> &ExifMetadata {
        &self.exif
    }

    /// Replace the mosaic while retaining frame metadata and updated levels.
    pub fn with_mosaic(&self, mosaic: Mosaic) -> Result<Self, RawError> {
        Self::new(
            mosaic,
            self.black_levels,
            self.white_levels,
            self.camera.clone(),
            self.profile.clone(),
            self.lens_profile.clone(),
            self.embedded_preview.clone(),
            self.exif.clone(),
        )
    }

    /// Replace black and white levels while retaining the decoded pixels.
    pub fn with_levels(
        &self,
        black_levels: [f32; 4],
        white_levels: [f32; 4],
    ) -> Result<Self, RawError> {
        Self::new(
            self.mosaic.clone(),
            black_levels,
            white_levels,
            self.camera.clone(),
            self.profile.clone(),
            self.lens_profile.clone(),
            self.embedded_preview.clone(),
            self.exif.clone(),
        )
    }
}

/// Errors raised while validating or adapting a RAW frame.
#[derive(Debug, Error, PartialEq)]
pub enum RawError {
    /// Dimensions could not be represented as a pixel count.
    #[error("RAW dimensions overflow")]
    DimensionsOverflow,
    /// A CFA has an invalid size or sample count.
    #[error("invalid CFA {width}x{height} with {colors} colors")]
    InvalidCfa {
        width: u32,
        height: u32,
        colors: usize,
    },
    /// A sample buffer does not match the sensor dimensions.
    #[error("RAW mosaic requires {expected} samples for {dimensions:?}, got {actual}")]
    SampleCountMismatch {
        dimensions: Dimensions,
        expected: usize,
        actual: usize,
    },
    /// Bit depths outside the supported range are rejected.
    #[error("invalid RAW bit depth {0}")]
    InvalidBitDepth(u8),
    /// A sample was not finite.
    #[error("RAW sample at index {0} is not finite")]
    NonFiniteSample(usize),
    /// A level was not finite.
    #[error("RAW level at index {0} is not finite")]
    NonFiniteLevel(usize),
    /// The decoder reported an impossible level range.
    #[error("RAW black level is above its white level")]
    BlackLevelAboveWhiteLevel,
    /// A vendor decoder failed.
    #[error("RAW decoder failed: {0}")]
    Decoder(String),
    /// A vendor decoder returned data this abstraction cannot represent.
    #[error("unsupported RAW decoder data: {0}")]
    UnsupportedData(String),
    /// EXIF metadata could not be parsed by the pure-Rust EXIF reader.
    #[error("EXIF metadata could not be parsed: {0}")]
    Exif(String),
}

/// Decoder boundary used by graph nodes and test fixtures.
pub trait RawDecoder: Send + Sync {
    /// Decode one complete file buffer into a validated frame.
    fn decode(&self, input: &[u8]) -> Result<RawFrame, RawError>;
}

/// A deterministic decoder used by tests and algorithm development.
#[derive(Clone, Debug)]
pub struct DeterministicDecoder {
    frame: RawFrame,
}

impl DeterministicDecoder {
    /// Return the same frame for every input. Input bytes are intentionally ignored.
    pub fn new(frame: RawFrame) -> Self {
        Self { frame }
    }

    /// Access the fixture frame.
    pub fn frame(&self) -> &RawFrame {
        &self.frame
    }
}

impl RawDecoder for DeterministicDecoder {
    fn decode(&self, _input: &[u8]) -> Result<RawFrame, RawError> {
        Ok(self.frame.clone())
    }
}

/// Parse EXIF metadata and embedded preview bytes from a supported image container.
///
/// This helper intentionally accepts TIFF/EXIF bytes independently of RAW decoding so metadata
/// tests and future decoders can share the same pure-Rust parser. A RAW adapter may ignore the
/// error when its container has no parseable EXIF block.
pub fn parse_exif_metadata(input: &[u8]) -> Result<ParsedExifMetadata, RawError> {
    let exif = Reader::new()
        .read_from_container(&mut Cursor::new(input))
        .map_err(|error| RawError::Exif(error.to_string()))?;
    let mut tags = BTreeMap::new();
    for field in exif.fields() {
        tags.insert(
            format!("{}@{}", field.tag, field.ifd_num),
            field.display_value().to_string(),
        );
    }

    let make = exif_text(&exif, Tag::Make).unwrap_or_default();
    let model = exif_text(&exif, Tag::Model).unwrap_or_default();
    let lens = exif_text(&exif, Tag::LensModel);
    let dimensions = exif_dimensions(&exif);
    let orientation = exif
        .get_field(Tag::Orientation, In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .and_then(parse_orientation);
    let capture_time = exif_text(&exif, Tag::DateTimeOriginal)
        .or_else(|| exif_text(&exif, Tag::DateTime))
        .map(normalize_capture_time);
    let camera = CameraMetadata {
        make,
        model,
        lens,
        iso: exif
            .get_field(Tag::PhotographicSensitivity, In::PRIMARY)
            .and_then(|field| field.value.get_uint(0)),
        aperture: exif_number(&exif, Tag::FNumber),
        shutter_seconds: exif_number(&exif, Tag::ExposureTime),
        focal_length_mm: exif_number(&exif, Tag::FocalLength),
        capture_time,
        orientation: orientation.unwrap_or_default(),
        dimensions,
    };
    Ok(ParsedExifMetadata {
        camera,
        exif: ExifMetadata { tags },
        embedded_preview: exif_thumbnail(&exif),
    })
}

fn exif_text(exif: &exif::Exif, tag: Tag) -> Option<String> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    let bytes = match &field.value {
        Value::Ascii(values) => values.first()?.as_slice(),
        Value::Byte(values) => values.as_slice(),
        Value::Undefined(values, _) => values.as_slice(),
        _ => return Some(field.display_value().to_string()),
    };
    let text = std::str::from_utf8(bytes).ok()?.trim_matches('\0').trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn exif_number(exif: &exif::Exif, tag: Tag) -> Option<f32> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    let number = match &field.value {
        Value::Rational(values) => values.first()?.to_f32(),
        Value::SRational(values) => values.first()?.to_f32(),
        Value::Float(values) => *values.first()?,
        Value::Double(values) => *values.first()? as f32,
        _ => field.value.get_uint(0)? as f32,
    };
    number.is_finite().then_some(number)
}

fn exif_dimensions(exif: &exif::Exif) -> Option<Dimensions> {
    let width = exif
        .get_field(Tag::ImageWidth, In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .or_else(|| {
            exif.get_field(Tag::PixelXDimension, In::PRIMARY)
                .and_then(|field| field.value.get_uint(0))
        })?;
    let height = exif
        .get_field(Tag::ImageLength, In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .or_else(|| {
            exif.get_field(Tag::PixelYDimension, In::PRIMARY)
                .and_then(|field| field.value.get_uint(0))
        })?;
    Some(Dimensions::new(width, height))
}

fn parse_orientation(value: u32) -> Option<Orientation> {
    Some(match value {
        1 => Orientation::Normal,
        2 => Orientation::HorizontalFlip,
        3 => Orientation::Rotate180,
        4 => Orientation::VerticalFlip,
        5 => Orientation::Transpose,
        6 => Orientation::Rotate90,
        7 => Orientation::Transverse,
        8 => Orientation::Rotate270,
        _ => return None,
    })
}

fn normalize_capture_time(value: String) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 19 && bytes[4] == b':' && bytes[7] == b':' && bytes[10] == b' ' {
        format!(
            "{}-{}-{}T{}",
            &value[0..4],
            &value[5..7],
            &value[8..10],
            &value[11..]
        )
    } else {
        value
    }
}

fn exif_thumbnail(exif: &exif::Exif) -> Option<Vec<u8>> {
    let jpeg_offset = exif
        .get_field(Tag::JPEGInterchangeFormat, In::THUMBNAIL)
        .and_then(|field| field.value.get_uint(0));
    let jpeg_length = exif
        .get_field(Tag::JPEGInterchangeFormatLength, In::THUMBNAIL)
        .and_then(|field| field.value.get_uint(0));
    if let Some(bytes) = jpeg_offset
        .zip(jpeg_length)
        .and_then(|(offset, length)| exif_slice(exif.buf(), offset, length))
    {
        return (!bytes.is_empty()).then(|| bytes.to_vec());
    }

    let offsets = exif
        .get_field(Tag::StripOffsets, In::THUMBNAIL)
        .and_then(|field| field.value.iter_uint())
        .map(|values| values.collect::<Vec<_>>())?;
    let lengths = exif
        .get_field(Tag::StripByteCounts, In::THUMBNAIL)
        .and_then(|field| field.value.iter_uint())
        .map(|values| values.collect::<Vec<_>>())?;
    let mut preview = Vec::new();
    for (offset, length) in offsets.into_iter().zip(lengths) {
        preview.extend_from_slice(exif_slice(exif.buf(), offset, length)?);
    }
    (!preview.is_empty()).then_some(preview)
}

fn exif_slice(bytes: &[u8], offset: u32, length: u32) -> Option<&[u8]> {
    let start = usize::try_from(offset).ok()?;
    let end = start.checked_add(usize::try_from(length).ok()?)?;
    bytes.get(start..end)
}

/// Adapter around the selected `rawloader` vendor decoder.
///
/// This is compile coverage for the vendor boundary. The deterministic corpus, rather than
/// downloaded camera files, validates the RAW algorithms in this repository.
#[derive(Clone, Copy, Debug, Default)]
pub struct RawloaderDecoder;

/// Compatibility spelling for callers that use the library's capitalized name.
pub type RawLoaderDecoder = RawloaderDecoder;
/// Compatibility spelling for callers that refer to this type as an adapter.
pub type RawloaderAdapter = RawloaderDecoder;

impl RawloaderDecoder {
    /// Decode bytes with rawloader.
    pub fn decode(&self, input: &[u8]) -> Result<RawFrame, RawError> {
        <Self as RawDecoder>::decode(self, input)
    }

    /// Decode a file through rawloader.
    pub fn decode_file(&self, path: impl AsRef<std::path::Path>) -> Result<RawFrame, RawError> {
        let bytes = std::fs::read(path).map_err(|error| RawError::Decoder(error.to_string()))?;
        self.decode(&bytes)
    }
}

impl RawDecoder for RawloaderDecoder {
    fn decode(&self, input: &[u8]) -> Result<RawFrame, RawError> {
        let decoded = catch_unwind(AssertUnwindSafe(|| {
            rawloader::decode(&mut Cursor::new(input))
        }))
        .map_err(|_| RawError::Decoder("rawloader panicked while parsing input".to_owned()))?
        .map_err(|error| RawError::Decoder(error.to_string()))?;
        adapt_rawloader_image(decoded, input)
    }
}

fn adapt_rawloader_image(image: rawloader::RawImage, input: &[u8]) -> Result<RawFrame, RawError> {
    if image.cpp != 1 {
        return Err(RawError::UnsupportedData(format!(
            "rawloader returned {} components per pixel",
            image.cpp
        )));
    }
    let samples = match image.data {
        rawloader::RawImageData::Integer(data) => data.into_iter().map(f32::from).collect(),
        rawloader::RawImageData::Float(data) => data,
    };
    let dimensions = Dimensions::new(
        u32::try_from(image.width).map_err(|_| RawError::DimensionsOverflow)?,
        u32::try_from(image.height).map_err(|_| RawError::DimensionsOverflow)?,
    );
    let cfa_width = u32::try_from(image.cfa.width).map_err(|_| RawError::DimensionsOverflow)?;
    let cfa_height = u32::try_from(image.cfa.height).map_err(|_| RawError::DimensionsOverflow)?;
    let cfa_colors = if cfa_width == 0 || cfa_height == 0 {
        vec![CfaColor::Unknown]
    } else {
        let raw_cfa = &image.cfa;
        (0..cfa_height)
            .flat_map(|y| {
                (0..cfa_width).map(move |x| match raw_cfa.color_at(y as usize, x as usize) {
                    0 => CfaColor::Red,
                    1 => CfaColor::Green,
                    2 => CfaColor::Blue,
                    3 => CfaColor::Extra,
                    _ => CfaColor::Unknown,
                })
            })
            .collect()
    };
    let (cfa_width, cfa_height) = if cfa_width == 0 || cfa_height == 0 {
        (1, 1)
    } else {
        (cfa_width, cfa_height)
    };
    let cfa = CfaPattern::new(cfa_width, cfa_height, cfa_colors)?;
    let white_levels = image.whitelevels.map(f32::from);
    let bit_depth = infer_bit_depth(white_levels);
    let mosaic = Mosaic::new(
        dimensions,
        samples,
        bit_depth,
        cfa,
        image.orientation.into(),
    )?;
    let camera = CameraMetadata {
        make: image.make.clone(),
        model: image.model.clone(),
        orientation: image.orientation.into(),
        dimensions: Some(dimensions),
        ..CameraMetadata::default()
    };
    let parsed = parse_exif_metadata(input).ok();
    let camera = merge_camera_metadata(camera, parsed.as_ref().map(|metadata| &metadata.camera));
    let mut tags = parsed
        .as_ref()
        .map(|metadata| metadata.exif.tags.clone())
        .unwrap_or_default();
    tags.insert("clean_make".to_owned(), image.clean_make.clone());
    tags.insert("clean_model".to_owned(), image.clean_model.clone());
    tags.insert("cpp".to_owned(), image.cpp.to_string());
    let exif = ExifMetadata { tags };
    let lens_profile = camera.lens.clone().map(LensProfile::identity);
    let embedded_preview = parsed.and_then(|metadata| metadata.embedded_preview);
    RawFrame::new(
        mosaic,
        image.blacklevels.map(f32::from),
        white_levels,
        camera,
        CameraProfile {
            make: image.make,
            model: image.model,
            xyz_to_camera: image.xyz_to_cam,
        },
        lens_profile,
        embedded_preview,
        exif,
    )
}

fn merge_camera_metadata(
    mut base: CameraMetadata,
    parsed: Option<&CameraMetadata>,
) -> CameraMetadata {
    let Some(parsed) = parsed else {
        return base;
    };
    if !parsed.make.is_empty() {
        base.make = parsed.make.clone();
    }
    if !parsed.model.is_empty() {
        base.model = parsed.model.clone();
    }
    if parsed.lens.is_some() {
        base.lens = parsed.lens.clone();
    }
    if parsed.iso.is_some() {
        base.iso = parsed.iso;
    }
    if parsed.aperture.is_some() {
        base.aperture = parsed.aperture;
    }
    if parsed.shutter_seconds.is_some() {
        base.shutter_seconds = parsed.shutter_seconds;
    }
    if parsed.focal_length_mm.is_some() {
        base.focal_length_mm = parsed.focal_length_mm;
    }
    if parsed.capture_time.is_some() {
        base.capture_time = parsed.capture_time.clone();
    }
    if parsed.dimensions.is_some() {
        base.dimensions = parsed.dimensions;
    }
    if parsed.orientation != Orientation::Normal {
        base.orientation = parsed.orientation;
    }
    base
}

fn infer_bit_depth(white_levels: [f32; 4]) -> u8 {
    let white = white_levels
        .iter()
        .copied()
        .filter(|value| value.is_finite() && *value >= 1.0)
        .fold(1.0_f32, f32::max);
    white.log2().ceil().clamp(1.0, 32.0) as u8
}

struct FixtureSpec {
    dimensions: Dimensions,
    bit_depth: u8,
    orientation: Orientation,
    cfa: CfaPattern,
    black_levels: [f32; 4],
    white_levels: [f32; 4],
    make: String,
    model: String,
    clipped_highlights: bool,
}

impl FixtureSpec {
    fn new(
        dimensions: Dimensions,
        bit_depth: u8,
        orientation: Orientation,
        cfa: CfaPattern,
    ) -> Self {
        Self {
            dimensions,
            bit_depth,
            orientation,
            cfa,
            black_levels: [16.0; 4],
            white_levels: [((1_u32 << bit_depth.min(16)) - 1) as f32; 4],
            make: "RawWeave Test".to_owned(),
            model: format!("Fixture {bit_depth}-bit"),
            clipped_highlights: false,
        }
    }

    fn levels(mut self, black_levels: [f32; 4], white_levels: [f32; 4]) -> Self {
        self.black_levels = black_levels;
        self.white_levels = white_levels;
        self
    }

    fn camera(mut self, make: &str, model: &str) -> Self {
        self.make = make.to_owned();
        self.model = model.to_owned();
        self
    }

    fn with_clipped_highlights(mut self) -> Self {
        self.clipped_highlights = true;
        self
    }

    fn build(self) -> RawFrame {
        let pixel_count = self
            .dimensions
            .pixel_count()
            .expect("fixture dimensions are small");
        let max = ((1_u32 << self.bit_depth.min(16)) - 1) as f32;
        let mut samples: Vec<f32> = (0..pixel_count)
            .map(|index| (index as f32 * 37.0 + 11.0) % max)
            .collect();
        if self.clipped_highlights {
            samples[0] = self.white_levels[0] + 250.0;
            samples[pixel_count - 1] = self.white_levels[2] + 500.0;
        }
        let mosaic = Mosaic::new(
            self.dimensions,
            samples,
            self.bit_depth,
            self.cfa,
            self.orientation,
        )
        .expect("fixture mosaic is valid");
        RawFrame::new(
            mosaic,
            self.black_levels,
            self.white_levels,
            CameraMetadata {
                make: self.make.clone(),
                model: self.model.clone(),
                orientation: self.orientation,
                dimensions: Some(self.dimensions),
                ..CameraMetadata::default()
            },
            CameraProfile::identity(self.make, self.model),
            None,
            None,
            ExifMetadata::default(),
        )
        .expect("fixture frame is valid")
    }
}

/// Deterministic fixtures covering the RAW cases needed before camera files are available.
pub struct DeterministicCorpus;

impl DeterministicCorpus {
    /// A small 12-bit Canon Bayer frame.
    pub fn bayer_12_bit() -> RawFrame {
        FixtureSpec::new(
            Dimensions::new(4, 2),
            12,
            Orientation::Normal,
            bayer_pattern(CfaColor::Red, CfaColor::Blue),
        )
        .levels([16.0, 20.0, 24.0, 28.0], [4095.0; 4])
        .camera("Canon", "EOS R5")
        .build()
    }

    /// A 14-bit Fujifilm X-Trans frame with a true 6x6 sensor extent.
    pub fn xtrans_14_bit() -> RawFrame {
        FixtureSpec::new(
            Dimensions::new(6, 6),
            14,
            Orientation::Rotate180,
            xtrans_pattern(),
        )
        .levels([32.0, 36.0, 40.0, 44.0], [16383.0; 4])
        .camera("Fujifilm", "X-T5")
        .build()
    }

    /// A 16-bit Sony Bayer frame with a 90-degree orientation tag.
    pub fn rotated_16_bit() -> RawFrame {
        FixtureSpec::new(
            Dimensions::new(2, 3),
            16,
            Orientation::Rotate90,
            bayer_pattern(CfaColor::Blue, CfaColor::Red),
        )
        .levels([256.0, 260.0, 264.0, 268.0], [65535.0; 4])
        .camera("Sony", "ILCE-7RM5")
        .build()
    }

    /// A 12-bit Olympus Bayer frame with intentional samples above white level.
    pub fn clipped_highlights() -> RawFrame {
        FixtureSpec::new(
            Dimensions::new(4, 4),
            12,
            Orientation::Rotate270,
            bayer_pattern(CfaColor::Green, CfaColor::Red),
        )
        .levels(
            [128.0, 132.0, 136.0, 140.0],
            [4000.0, 4095.0, 4050.0, 4095.0],
        )
        .camera("OM Digital Solutions", "OM-1")
        .with_clipped_highlights()
        .build()
    }

    /// Return all deterministic fixtures in a stable order.
    pub fn all() -> Vec<RawFrame> {
        vec![
            Self::bayer_12_bit(),
            Self::xtrans_14_bit(),
            Self::rotated_16_bit(),
            Self::clipped_highlights(),
        ]
    }

    /// Compatibility alias for callers that refer to the fixtures as a corpus.
    pub fn fixtures() -> Vec<RawFrame> {
        Self::all()
    }
}

fn bayer_pattern(first: CfaColor, fourth: CfaColor) -> CfaPattern {
    CfaPattern::new(2, 2, vec![first, CfaColor::Green, CfaColor::Green, fourth])
        .expect("fixture CFA is valid")
}

fn xtrans_pattern() -> CfaPattern {
    CfaPattern::new(
        6,
        6,
        vec![
            CfaColor::Green,
            CfaColor::Green,
            CfaColor::Red,
            CfaColor::Green,
            CfaColor::Green,
            CfaColor::Red,
            CfaColor::Green,
            CfaColor::Green,
            CfaColor::Blue,
            CfaColor::Green,
            CfaColor::Green,
            CfaColor::Blue,
            CfaColor::Blue,
            CfaColor::Blue,
            CfaColor::Green,
            CfaColor::Blue,
            CfaColor::Blue,
            CfaColor::Green,
            CfaColor::Green,
            CfaColor::Green,
            CfaColor::Blue,
            CfaColor::Green,
            CfaColor::Green,
            CfaColor::Blue,
            CfaColor::Red,
            CfaColor::Green,
            CfaColor::Green,
            CfaColor::Red,
            CfaColor::Green,
            CfaColor::Green,
            CfaColor::Green,
            CfaColor::Green,
            CfaColor::Blue,
            CfaColor::Green,
            CfaColor::Green,
            CfaColor::Blue,
        ],
    )
    .expect("fixture CFA is valid")
}

#[cfg(test)]
mod tests {
    use super::{CfaColor, CfaPattern, DeterministicCorpus, Mosaic, RawDecoder, RawloaderDecoder};
    use rawweave_image::Dimensions;

    #[test]
    fn deterministic_corpus_covers_cfa_bit_depth_and_orientation() {
        let bayer = DeterministicCorpus::bayer_12_bit();
        assert_eq!(bayer.mosaic().bit_depth(), 12);
        assert_eq!(bayer.mosaic().cfa().color_at(1, 1), Some(CfaColor::Blue));

        let xtrans = DeterministicCorpus::xtrans_14_bit();
        assert_eq!(xtrans.mosaic().cfa().width(), 6);
        assert_eq!(xtrans.mosaic().bit_depth(), 14);
        assert_eq!(xtrans.camera().orientation, super::Orientation::Rotate180);

        let rotated = DeterministicCorpus::rotated_16_bit();
        assert_eq!(rotated.mosaic().bit_depth(), 16);
        assert_eq!(rotated.mosaic().orientation(), super::Orientation::Rotate90);
    }

    #[test]
    fn rawloader_adapter_returns_an_error_for_invalid_data() {
        let error = RawloaderDecoder.decode(b"not a RAW file").unwrap_err();
        assert!(error.to_string().contains("decoder"));
    }

    #[test]
    fn mosaic_rejects_wrong_sample_count() {
        let cfa = CfaPattern::new(2, 2, vec![CfaColor::Red; 4]).unwrap();
        let error = Mosaic::new(
            Dimensions::new(2, 2),
            vec![0.0],
            12,
            cfa,
            Default::default(),
        )
        .unwrap_err();
        assert!(matches!(error, super::RawError::SampleCountMismatch { .. }));
    }

    #[test]
    fn deterministic_decoder_implements_the_decoder_boundary() {
        let decoder = super::DeterministicDecoder::new(DeterministicCorpus::bayer_12_bit());
        assert_eq!(
            decoder.decode(b"one").unwrap(),
            decoder.decode(b"two").unwrap()
        );
    }
}
