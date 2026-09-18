//! RAW foundations and decoder adapters.
//!
//! The public decoder boundary deliberately does not expose `rawloader` types. This keeps
//! graph nodes and projects independent of the vendor decoder selected for a build. The
//! `RawloaderDecoder` below is an adapter and is compile-tested with invalid input; the
//! deterministic corpus is the validation source for RAW algorithms in this crate.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};

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
}

/// Structured EXIF data retained for later metadata/control nodes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExifMetadata {
    /// Additional decoded tags not covered by the common fields.
    pub tags: BTreeMap<String, String>,
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
        adapt_rawloader_image(decoded)
    }
}

fn adapt_rawloader_image(image: rawloader::RawImage) -> Result<RawFrame, RawError> {
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
        ..CameraMetadata::default()
    };
    let mut tags = BTreeMap::new();
    tags.insert("clean_make".to_owned(), image.clean_make.clone());
    tags.insert("clean_model".to_owned(), image.clean_model.clone());
    tags.insert("cpp".to_owned(), image.cpp.to_string());
    let exif = ExifMetadata { tags };
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
        None,
        None,
        exif,
    )
}

fn infer_bit_depth(white_levels: [f32; 4]) -> u8 {
    let white = white_levels
        .iter()
        .copied()
        .filter(|value| value.is_finite() && *value >= 1.0)
        .fold(1.0_f32, f32::max);
    white.log2().ceil().clamp(1.0, 32.0) as u8
}

/// Deterministic fixtures covering the RAW cases needed before camera files are available.
pub struct DeterministicCorpus;

impl DeterministicCorpus {
    /// A small 12-bit Bayer frame.
    pub fn bayer_12_bit() -> RawFrame {
        frame_fixture(
            Dimensions::new(4, 2),
            12,
            Orientation::Normal,
            CfaPattern::new(
                2,
                2,
                vec![
                    CfaColor::Red,
                    CfaColor::Green,
                    CfaColor::Green,
                    CfaColor::Blue,
                ],
            )
            .expect("fixture CFA is valid"),
        )
    }

    /// A small 14-bit 6x6 multi-CFA frame.
    pub fn xtrans_14_bit() -> RawFrame {
        let colors = [
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
        ];
        frame_fixture(
            Dimensions::new(6, 2),
            14,
            Orientation::Rotate180,
            CfaPattern::new(6, 6, colors.to_vec()).expect("fixture CFA is valid"),
        )
    }

    /// A 16-bit frame with a rotated orientation tag.
    pub fn rotated_16_bit() -> RawFrame {
        frame_fixture(
            Dimensions::new(2, 3),
            16,
            Orientation::Rotate90,
            CfaPattern::new(
                2,
                2,
                vec![
                    CfaColor::Red,
                    CfaColor::Green,
                    CfaColor::Green,
                    CfaColor::Blue,
                ],
            )
            .expect("fixture CFA is valid"),
        )
    }
}

fn frame_fixture(
    dimensions: Dimensions,
    bit_depth: u8,
    orientation: Orientation,
    cfa: CfaPattern,
) -> RawFrame {
    let pixel_count = dimensions
        .pixel_count()
        .expect("fixture dimensions are small");
    let max = ((1_u32 << bit_depth.min(16)) - 1) as f32;
    let samples = (0..pixel_count)
        .map(|index| (index as f32 * 37.0 + 11.0) % max)
        .collect();
    let mosaic = Mosaic::new(dimensions, samples, bit_depth, cfa, orientation)
        .expect("fixture mosaic is valid");
    RawFrame::new(
        mosaic,
        [16.0; 4],
        [max; 4],
        CameraMetadata {
            make: "RawWeave Test".to_owned(),
            model: format!("Fixture {bit_depth}-bit"),
            orientation,
            ..CameraMetadata::default()
        },
        CameraProfile::identity("RawWeave Test", format!("Fixture {bit_depth}-bit")),
        None,
        None,
        ExifMetadata::default(),
    )
    .expect("fixture frame is valid")
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
