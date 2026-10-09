//! RAW foundations and decoder adapters.
//!
//! The public decoder boundary deliberately does not expose vendor types. `RawlerDecoder`
//! extracts sensor data through dnglab's pure-Rust decoder; development stays in graph nodes.
//! Real camera files cover decoder integration; deterministic fixtures validate algorithms.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

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

impl Orientation {
    /// Stable lower-case identifier used by control-value graph nodes.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::HorizontalFlip => "horizontal-flip",
            Self::Rotate180 => "rotate-180",
            Self::VerticalFlip => "vertical-flip",
            Self::Transpose => "transpose",
            Self::Rotate90 => "rotate-90",
            Self::Transverse => "transverse",
            Self::Rotate270 => "rotate-270",
            Self::Unknown => "unknown",
        }
    }
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

impl From<rawler::Orientation> for Orientation {
    fn from(value: rawler::Orientation) -> Self {
        match value {
            rawler::Orientation::Normal => Self::Normal,
            rawler::Orientation::HorizontalFlip => Self::HorizontalFlip,
            rawler::Orientation::Rotate180 => Self::Rotate180,
            rawler::Orientation::VerticalFlip => Self::VerticalFlip,
            rawler::Orientation::Transpose => Self::Transpose,
            rawler::Orientation::Rotate90 => Self::Rotate90,
            rawler::Orientation::Transverse => Self::Transverse,
            rawler::Orientation::Rotate270 => Self::Rotate270,
            rawler::Orientation::Unknown => Self::Unknown,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CfaPattern {
    width: u32,
    height: u32,
    colors: Vec<CfaColor>,
}

#[derive(Deserialize)]
struct CfaPatternDto {
    width: u32,
    height: u32,
    colors: Vec<CfaColor>,
}

impl<'de> Deserialize<'de> for CfaPattern {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let dto = CfaPatternDto::deserialize(deserializer)?;
        Self::new(dto.width, dto.height, dto.colors).map_err(serde::de::Error::custom)
    }
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
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Mosaic {
    dimensions: Dimensions,
    samples: Arc<Vec<f32>>,
    bit_depth: u8,
    cfa: CfaPattern,
    orientation: Orientation,
}

#[derive(Deserialize)]
struct MosaicDto {
    dimensions: Dimensions,
    samples: Vec<f32>,
    bit_depth: u8,
    cfa: CfaPattern,
    orientation: Orientation,
}

impl<'de> Deserialize<'de> for Mosaic {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let dto = MosaicDto::deserialize(deserializer)?;
        Self::new(
            dto.dimensions,
            dto.samples,
            dto.bit_depth,
            dto.cfa,
            dto.orientation,
        )
        .map_err(serde::de::Error::custom)
    }
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
        if dimensions.width == 0 || dimensions.height == 0 {
            return Err(RawError::InvalidDimensions(dimensions));
        }
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
            samples: Arc::new(samples),
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

/// An optional embedded preview with its source MIME type when known.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddedPreview {
    bytes: Option<Vec<u8>>,
    mime_type: Option<String>,
}

/// Compatibility name for graph boundaries that model optional byte payloads.
pub type OptionalBytes = EmbeddedPreview;

impl EmbeddedPreview {
    /// Construct a preview value. `None` bytes is an explicit unavailable preview.
    pub fn new<M>(bytes: Option<Vec<u8>>, mime_type: Option<M>) -> Self
    where
        M: Into<String>,
    {
        Self {
            bytes,
            mime_type: mime_type.map(Into::into),
        }
    }

    /// Construct an unavailable preview value.
    pub fn unavailable() -> Self {
        Self::default()
    }

    /// Construct a preview and infer a common MIME type from its signature.
    pub fn from_bytes(bytes: Option<Vec<u8>>) -> Self {
        let mime_type = bytes.as_deref().and_then(preview_mime_type);
        Self { bytes, mime_type }
    }

    /// Preview bytes when a vendor or EXIF source supplied them.
    pub fn bytes(&self) -> Option<&[u8]> {
        self.bytes.as_deref()
    }

    /// MIME type supplied by the source or inferred from the byte signature.
    pub fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }
}

impl From<Option<Vec<u8>>> for EmbeddedPreview {
    fn from(bytes: Option<Vec<u8>>) -> Self {
        Self::from_bytes(bytes)
    }
}

fn preview_mime_type(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg".to_owned())
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png".to_owned())
    } else {
        None
    }
}

/// Camera color profile metadata.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CameraProfile {
    /// Camera manufacturer.
    pub make: String,
    /// Camera model.
    pub model: String,
    /// Original vendor matrix mapping XYZ values into camera channels.
    ///
    /// This is retained for provenance and serialization compatibility. Processing must use
    /// [`CameraProfile::camera_to_xyz`], never this source-direction matrix.
    pub xyz_to_camera: [[f32; 3]; 4],
    /// Validated 3x3 matrix mapping camera RGB channels into D65-referenced XYZ.
    pub camera_to_xyz: [[f32; 3]; 3],
}

#[derive(Deserialize)]
struct CameraProfileDto {
    make: String,
    model: String,
    xyz_to_camera: [[f32; 3]; 4],
    #[serde(default)]
    camera_to_xyz: Option<[[f32; 3]; 3]>,
}

impl<'de> Deserialize<'de> for CameraProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let dto = CameraProfileDto::deserialize(deserializer)?;
        validate_xyz_to_camera_channels(dto.xyz_to_camera).map_err(serde::de::Error::custom)?;
        let camera_to_xyz = match dto.camera_to_xyz {
            Some(matrix) => {
                invert_3x3(matrix, "camera-to-XYZ color matrix")
                    .map_err(serde::de::Error::custom)?;
                matrix
            }
            None => invert_camera_matrix(dto.xyz_to_camera).map_err(serde::de::Error::custom)?,
        };
        Ok(Self {
            make: dto.make,
            model: dto.model,
            xyz_to_camera: dto.xyz_to_camera,
            camera_to_xyz,
        })
    }
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
            camera_to_xyz: identity_camera_to_xyz(),
        }
    }

    /// Build a profile from a decoder's XYZ-to-camera matrix.
    pub fn from_xyz_to_camera(
        make: impl Into<String>,
        model: impl Into<String>,
        xyz_to_camera: [[f32; 3]; 4],
    ) -> Result<Self, RawError> {
        validate_xyz_to_camera_channels(xyz_to_camera)?;
        let camera_to_xyz = invert_camera_matrix(xyz_to_camera)?;
        Ok(Self {
            make: make.into(),
            model: model.into(),
            xyz_to_camera,
            camera_to_xyz,
        })
    }

    /// Build a profile from an already adapted camera-to-XYZ matrix.
    pub fn from_camera_to_xyz(
        make: impl Into<String>,
        model: impl Into<String>,
        camera_to_xyz: [[f32; 3]; 3],
    ) -> Result<Self, RawError> {
        let xyz_to_camera = invert_3x3(camera_to_xyz, "camera color matrix")?;
        Ok(Self {
            make: make.into(),
            model: model.into(),
            xyz_to_camera: [
                xyz_to_camera[0],
                xyz_to_camera[1],
                xyz_to_camera[2],
                [0.0; 3],
            ],
            camera_to_xyz,
        })
    }

    /// Whether this profile intentionally performs no camera calibration.
    pub fn is_identity(&self) -> bool {
        self.camera_to_xyz == identity_camera_to_xyz()
    }
}

fn identity_camera_to_xyz() -> [[f32; 3]; 3] {
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
}

fn invert_camera_matrix(xyz_to_camera: [[f32; 3]; 4]) -> Result<[[f32; 3]; 3], RawError> {
    invert_3x3(
        [xyz_to_camera[0], xyz_to_camera[1], xyz_to_camera[2]],
        "camera color matrix",
    )
}

fn validate_xyz_to_camera_channels(xyz_to_camera: [[f32; 3]; 4]) -> Result<(), RawError> {
    if xyz_to_camera[3].iter().any(|value| *value != 0.0) {
        return Err(RawError::UnsupportedData(
            "camera profile contains an unsupported fourth channel".to_owned(),
        ));
    }
    Ok(())
}

fn invert_3x3(matrix: [[f32; 3]; 3], label: &str) -> Result<[[f32; 3]; 3], RawError> {
    if matrix.iter().flatten().any(|value| !value.is_finite()) {
        return Err(RawError::InvalidCameraMatrix(format!(
            "{label} contains a non-finite value"
        )));
    }
    let scale = matrix
        .iter()
        .flatten()
        .copied()
        .map(f32::abs)
        .fold(0.0_f32, f32::max);
    if scale == 0.0 {
        return Err(RawError::InvalidCameraMatrix(format!(
            "{label} is singular"
        )));
    }
    let [a, b, c] = matrix[0];
    let [d, e, f] = matrix[1];
    let [g, h, i] = matrix[2];
    let determinant = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    let threshold = 1e-6_f32 * scale.powi(3);
    if !determinant.is_finite() || determinant.abs() <= threshold {
        return Err(RawError::InvalidCameraMatrix(format!(
            "{label} is singular or ill-conditioned"
        )));
    }
    let inverse_determinant = 1.0 / determinant;
    let inverse = [
        [
            (e * i - f * h) * inverse_determinant,
            (c * h - b * i) * inverse_determinant,
            (b * f - c * e) * inverse_determinant,
        ],
        [
            (f * g - d * i) * inverse_determinant,
            (a * i - c * g) * inverse_determinant,
            (c * d - a * f) * inverse_determinant,
        ],
        [
            (d * h - e * g) * inverse_determinant,
            (b * g - a * h) * inverse_determinant,
            (a * e - b * d) * inverse_determinant,
        ],
    ];
    if inverse.iter().flatten().any(|value| !value.is_finite()) {
        return Err(RawError::InvalidCameraMatrix(format!(
            "{label} produced a non-finite inverse"
        )));
    }
    Ok(inverse)
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
    /// Source of the calibration coefficients.
    #[serde(default)]
    pub provenance: LensProfileProvenance,
}

impl LensProfile {
    /// Create a no-op lens profile.
    pub fn identity(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            radial_distortion: [0.0; 3],
            tangential_distortion: [0.0; 2],
            vignette: [0.0; 3],
            provenance: LensProfileProvenance::Unavailable,
        }
    }

    /// Create an explicit unavailable identity profile.
    pub fn unavailable(name: impl Into<String>) -> Self {
        Self::identity(name)
    }

    /// Create a calibrated profile with coefficients from a trusted provider.
    pub fn calibrated(
        name: impl Into<String>,
        radial_distortion: [f32; 3],
        tangential_distortion: [f32; 2],
        vignette: [f32; 3],
    ) -> Self {
        Self {
            name: name.into(),
            radial_distortion,
            tangential_distortion,
            vignette,
            provenance: LensProfileProvenance::BuiltInCalibrated,
        }
    }

    /// Create a calibrated profile supplied by an external provider.
    pub fn external(
        name: impl Into<String>,
        radial_distortion: [f32; 3],
        tangential_distortion: [f32; 2],
        vignette: [f32; 3],
    ) -> Self {
        Self {
            name: name.into(),
            radial_distortion,
            tangential_distortion,
            vignette,
            provenance: LensProfileProvenance::External,
        }
    }

    /// Profile provenance.
    pub const fn provenance(&self) -> LensProfileProvenance {
        self.provenance
    }

    /// Whether this profile would change image coordinates.
    pub fn is_identity(&self) -> bool {
        self.radial_distortion == [0.0; 3]
            && self.tangential_distortion == [0.0; 2]
            && self.vignette == [0.0; 3]
    }
}

/// Provenance for a lens profile, including an explicit unavailable state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LensProfileProvenance {
    /// The coefficients came from the built-in calibrated table.
    BuiltInCalibrated,
    /// The coefficients came from an injected or external provider.
    External,
    /// No calibrated correction is available for this lens.
    #[default]
    Unavailable,
}

/// Provider boundary for camera/lens calibration data.
pub trait LensProfileProvider: Send + Sync {
    /// Return a calibrated profile for the supplied camera metadata, if known.
    fn profile_for(&self, camera: &CameraMetadata) -> Option<LensProfile>;
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct LensProfileKey {
    make: String,
    model: String,
    lens: String,
}

/// Small deterministic registry used by the production RAW adapter.
#[derive(Clone, Debug, Default)]
pub struct LensProfileRegistry {
    profiles: BTreeMap<LensProfileKey, LensProfile>,
}

impl LensProfileRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create the built-in calibrated table.
    ///
    /// The RawWeave Test / Test Camera / Test Lens entry is a deterministic fixture. Production
    /// lenses are intentionally absent until their coefficients have a trusted source.
    pub fn built_in() -> Self {
        let mut registry = Self::new();
        registry.register(
            "RawWeave",
            "Test Camera",
            "Test Lens",
            LensProfile::calibrated(
                "RawWeave Test / Test Camera / Test Lens",
                [0.012, -0.001, 0.0],
                [0.0005, -0.0003],
                [-0.08, 0.01, 0.0],
            ),
        );
        registry
    }

    /// Register or replace one normalized make/model/lens entry.
    pub fn register(&mut self, make: &str, model: &str, lens: &str, profile: LensProfile) {
        self.profiles.insert(
            LensProfileKey {
                make: normalize_profile_key(make),
                model: normalize_profile_key(model),
                lens: normalize_profile_key(lens),
            },
            profile,
        );
    }

    /// Look up a profile by make, model, and lens using normalized keys.
    pub fn lookup(&self, make: &str, model: &str, lens: &str) -> Option<&LensProfile> {
        self.profiles.get(&LensProfileKey {
            make: normalize_profile_key(make),
            model: normalize_profile_key(model),
            lens: normalize_profile_key(lens),
        })
    }
}

impl LensProfileProvider for LensProfileRegistry {
    fn profile_for(&self, camera: &CameraMetadata) -> Option<LensProfile> {
        self.lookup(
            &camera.make,
            &camera.model,
            camera.lens.as_deref().unwrap_or_default(),
        )
        .cloned()
    }
}

fn normalize_profile_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// A decoded RAW frame and all metadata needed by the initial RAW graph stages.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RawFrame {
    mosaic: Mosaic,
    black_levels: [f32; 4],
    white_levels: [f32; 4],
    camera: CameraMetadata,
    profile: CameraProfile,
    lens_profile: Option<LensProfile>,
    embedded_preview: EmbeddedPreview,
    exif: ExifMetadata,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum EmbeddedPreviewPayload {
    Current(EmbeddedPreview),
    Legacy(Option<Vec<u8>>),
}

#[derive(Deserialize)]
struct RawFrameDto {
    mosaic: Mosaic,
    black_levels: [f32; 4],
    white_levels: [f32; 4],
    camera: CameraMetadata,
    profile: CameraProfile,
    #[serde(default)]
    lens_profile: Option<LensProfile>,
    #[serde(default)]
    embedded_preview: Option<EmbeddedPreviewPayload>,
    #[serde(default)]
    exif: ExifMetadata,
}

impl<'de> Deserialize<'de> for RawFrame {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let dto = RawFrameDto::deserialize(deserializer)?;
        let embedded_preview = match dto.embedded_preview {
            Some(EmbeddedPreviewPayload::Current(preview)) => preview,
            Some(EmbeddedPreviewPayload::Legacy(bytes)) => EmbeddedPreview::from_bytes(bytes),
            None => EmbeddedPreview::unavailable(),
        };
        Self::new(
            dto.mosaic,
            dto.black_levels,
            dto.white_levels,
            dto.camera,
            dto.profile,
            dto.lens_profile,
            embedded_preview,
            dto.exif,
        )
        .map_err(serde::de::Error::custom)
    }
}

impl RawFrame {
    /// Construct a validated RAW frame.
    #[allow(clippy::too_many_arguments)]
    pub fn new<P>(
        mosaic: Mosaic,
        black_levels: [f32; 4],
        white_levels: [f32; 4],
        camera: CameraMetadata,
        profile: CameraProfile,
        lens_profile: Option<LensProfile>,
        embedded_preview: P,
        exif: ExifMetadata,
    ) -> Result<Self, RawError>
    where
        P: Into<EmbeddedPreview>,
    {
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
            embedded_preview: embedded_preview.into(),
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

    /// Embedded preview, including explicit unavailable state and MIME metadata.
    pub fn embedded_preview(&self) -> &EmbeddedPreview {
        &self.embedded_preview
    }

    /// Embedded preview bytes when available.
    pub fn embedded_preview_bytes(&self) -> Option<&[u8]> {
        self.embedded_preview.bytes()
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
    /// A RAW image has a zero dimension.
    #[error("invalid RAW dimensions {0:?}")]
    InvalidDimensions(Dimensions),
    /// Input bytes exceed the configured decoder limit.
    #[error("RAW input is too large: {actual} bytes exceeds limit {max}")]
    InputTooLarge { actual: usize, max: usize },
    /// Decoded dimensions exceed the configured decoder limit.
    #[error("RAW dimensions {dimensions:?} exceed limit {max_width}x{max_height}")]
    DimensionTooLarge {
        dimensions: Dimensions,
        max_width: u32,
        max_height: u32,
    },
    /// Decoded pixels exceed the configured decoder limit.
    #[error("RAW pixel count {actual} exceeds limit {max}")]
    PixelCountTooLarge { actual: usize, max: usize },
    /// Decoded samples exceed the configured decoder limit.
    #[error("RAW sample count {actual} exceeds limit {max}")]
    SampleCountTooLarge { actual: usize, max: usize },
    /// A recognized RAW container did not expose dimensions safely before decode.
    #[error("RAW header dimensions are unavailable for {format}; refusing untrusted decode")]
    HeaderDimensionsUnavailable { format: &'static str },
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
    /// A camera color matrix cannot be inverted safely.
    #[error("invalid camera color matrix: {0}")]
    InvalidCameraMatrix(String),
    /// EXIF metadata could not be parsed by the pure-Rust EXIF reader.
    #[error("EXIF metadata could not be parsed: {0}")]
    Exif(String),
}

/// Conservative resource limits applied before and after vendor decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawDecodeLimits {
    /// Maximum encoded input size accepted by the adapter.
    pub max_input_bytes: usize,
    /// Maximum number of decoded scalar samples accepted.
    pub max_samples: usize,
    /// Maximum number of decoded pixels accepted.
    pub max_pixels: usize,
    /// Maximum decoded image width.
    pub max_width: u32,
    /// Maximum decoded image height.
    pub max_height: u32,
}

impl Default for RawDecodeLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 128 * 1024 * 1024,
            max_samples: 64 * 1024 * 1024,
            max_pixels: 64 * 1024 * 1024,
            max_width: 32_768,
            max_height: 32_768,
        }
    }
}

impl RawDecodeLimits {
    fn validate_input_size(self, actual: usize) -> Result<(), RawError> {
        if actual > self.max_input_bytes {
            return Err(RawError::InputTooLarge {
                actual,
                max: self.max_input_bytes,
            });
        }
        Ok(())
    }

    fn validate_dimensions(self, dimensions: Dimensions) -> Result<usize, RawError> {
        if dimensions.width == 0 || dimensions.height == 0 {
            return Err(RawError::InvalidDimensions(dimensions));
        }
        if dimensions.width > self.max_width || dimensions.height > self.max_height {
            return Err(RawError::DimensionTooLarge {
                dimensions,
                max_width: self.max_width,
                max_height: self.max_height,
            });
        }
        let pixels = dimensions
            .pixel_count()
            .map_err(|_| RawError::DimensionsOverflow)?;
        if pixels > self.max_pixels {
            return Err(RawError::PixelCountTooLarge {
                actual: pixels,
                max: self.max_pixels,
            });
        }
        Ok(pixels)
    }

    fn validate_sample_count(self, actual: usize) -> Result<(), RawError> {
        if actual > self.max_samples {
            return Err(RawError::SampleCountTooLarge {
                actual,
                max: self.max_samples,
            });
        }
        Ok(())
    }

    fn validate_predecode(
        self,
        dimensions: Dimensions,
        samples_per_pixel: usize,
    ) -> Result<(), RawError> {
        let pixels = self.validate_dimensions(dimensions)?;
        let samples = pixels
            .checked_mul(samples_per_pixel)
            .ok_or(RawError::DimensionsOverflow)?;
        self.validate_sample_count(samples)
    }
}

/// Decoder boundary used by graph nodes and test fixtures.
pub trait RawDecoder: Send + Sync {
    /// Decode one complete file buffer into a validated frame.
    fn decode(&self, input: &[u8]) -> Result<RawFrame, RawError>;

    /// Decode one file while allowing adapters to enforce filesystem-level limits before reading.
    fn decode_file(&self, path: &std::path::Path) -> Result<RawFrame, RawError> {
        let input = std::fs::read(path).map_err(|error| RawError::Decoder(error.to_string()))?;
        self.decode(&input)
    }
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

/// Adapter around dnglab's `rawler` decoder, including compressed RAF and CR3.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawlerDecoder {
    limits: RawDecodeLimits,
}

/// Compatibility spelling for callers that use the library's capitalized name.
pub type RawLoaderDecoder = RawlerDecoder;
/// Legacy adapter name retained for source compatibility.
pub type RawloaderDecoder = RawlerDecoder;
/// Compatibility spelling for callers that refer to this type as an adapter.
pub type RawloaderAdapter = RawlerDecoder;

impl RawlerDecoder {
    /// Construct a decoder with explicit resource limits.
    pub const fn with_limits(limits: RawDecodeLimits) -> Self {
        Self { limits }
    }

    /// Resource limits used by this decoder.
    pub const fn limits(&self) -> RawDecodeLimits {
        self.limits
    }

    /// Decode bytes with rawler after applying encoded and declared-dimension limits.
    pub fn decode_bytes(&self, input: &[u8]) -> Result<RawFrame, RawError> {
        <Self as RawDecoder>::decode(self, input)
    }

    /// Decode bytes with rawler.
    pub fn decode(&self, input: &[u8]) -> Result<RawFrame, RawError> {
        self.decode_bytes(input)
    }

    /// Decode a file through rawler after checking its filesystem size.
    pub fn decode_file(&self, path: impl AsRef<std::path::Path>) -> Result<RawFrame, RawError> {
        let path = path.as_ref();
        let metadata = std::fs::metadata(path)
            .map_err(|error| RawError::Decoder(format!("failed to inspect RAW file: {error}")))?;
        let file_size = usize::try_from(metadata.len()).map_err(|_| RawError::InputTooLarge {
            actual: usize::MAX,
            max: self.limits.max_input_bytes,
        })?;
        self.limits.validate_input_size(file_size)?;
        let bytes = std::fs::read(path).map_err(|error| RawError::Decoder(error.to_string()))?;
        self.decode_bytes(&bytes)
    }
}

impl RawDecoder for RawlerDecoder {
    fn decode(&self, input: &[u8]) -> Result<RawFrame, RawError> {
        self.limits.validate_input_size(input.len())?;
        if let Some((dimensions, samples_per_pixel)) = predecode_dimensions(input)? {
            self.limits
                .validate_predecode(dimensions, samples_per_pixel)?;
        }
        catch_unwind(AssertUnwindSafe(|| {
            let source = rawler::rawsource::RawSource::new_from_slice(input);
            let params = rawler::decoders::RawDecodeParams::default();
            let decoder = rawler::get_decoder(&source)
                .map_err(|error| RawError::Decoder(error.to_string()))?;
            // Dummy decoding reads the actual sensor layout without allocating pixel data.
            // TIFF EXIF dimensions can describe only the embedded JPEG, not the sensor.
            let header = match catch_unwind(AssertUnwindSafe(|| {
                decoder.raw_image(&source, &params, true)
            })) {
                Ok(result) => result.map_err(|error| RawError::Decoder(error.to_string()))?,
                Err(_) if has_prefix(input, b"FUJIFILM") => {
                    // rawler 0.8 reads uninitialized pixels in rotated Super CCD dummy decoding.
                    // Keep the established legacy adapter until upstream fixes this preflight.
                    let decoded = rawloader::decode(&mut Cursor::new(input))
                        .map_err(|error| RawError::Decoder(error.to_string()))?;
                    return adapt_rawloader_image(decoded, input, self.limits);
                }
                Err(_) => {
                    return Err(RawError::Decoder(
                        "rawler panicked during sensor preflight".to_owned(),
                    ));
                }
            };
            self.limits.validate_predecode(
                Dimensions::new(
                    u32::try_from(header.width).map_err(|_| RawError::DimensionsOverflow)?,
                    u32::try_from(header.height).map_err(|_| RawError::DimensionsOverflow)?,
                ),
                header.cpp,
            )?;
            let decoded = decoder
                .raw_image(&source, &params, false)
                .map_err(|error| RawError::Decoder(error.to_string()))?;
            let metadata = decoder.raw_metadata(&source, &params).ok();
            adapt_rawler_image(decoded, metadata, input, self.limits)
        }))
        .map_err(|_| RawError::Decoder("rawler panicked while parsing input".to_owned()))?
    }

    fn decode_file(&self, path: &std::path::Path) -> Result<RawFrame, RawError> {
        RawlerDecoder::decode_file(self, path)
    }
}

fn predecode_dimensions(input: &[u8]) -> Result<Option<(Dimensions, usize)>, RawError> {
    if has_prefix(input, b"ARRI") {
        return probe_arri_dimensions(input).map(Some);
    }
    if is_mrw(input) {
        return probe_mrw_dimensions(input).map(Some);
    }
    if has_prefix(input, b"FOVb") {
        return probe_x3f_dimensions(input).map(Some);
    }
    if has_prefix(input, b"FUJIFILM") {
        return probe_raf_dimensions(input).map(Some);
    }
    if is_ciff(input) {
        return Err(RawError::HeaderDimensionsUnavailable { format: "CIFF" });
    }
    if is_tiff_family(input) {
        return declared_dimensions(input)
            .map(|dimensions| (dimensions, 1))
            .ok_or(RawError::HeaderDimensionsUnavailable { format: "TIFF" })
            .map(Some);
    }
    Ok(declared_dimensions(input).map(|dimensions| (dimensions, 1)))
}

fn probe_arri_dimensions(input: &[u8]) -> Result<(Dimensions, usize), RawError> {
    let width =
        read_le_u32(input, 20).ok_or(RawError::HeaderDimensionsUnavailable { format: "ARRI" })?;
    let height =
        read_le_u32(input, 24).ok_or(RawError::HeaderDimensionsUnavailable { format: "ARRI" })?;
    Ok((Dimensions::new(width, height), 1))
}

fn probe_mrw_dimensions(input: &[u8]) -> Result<(Dimensions, usize), RawError> {
    let data_offset = read_be_u32(input, 4)
        .and_then(|offset| usize::try_from(offset).ok())
        .and_then(|offset| offset.checked_add(8))
        .ok_or(RawError::HeaderDimensionsUnavailable { format: "MRW" })?;
    let scan_end = data_offset.min(input.len());
    if scan_end < 8 {
        return Err(RawError::HeaderDimensionsUnavailable { format: "MRW" });
    }

    let mut cursor = 8_usize;
    while cursor.checked_add(8).is_some_and(|end| end <= scan_end) {
        let tag = read_be_u32(input, cursor)
            .ok_or(RawError::HeaderDimensionsUnavailable { format: "MRW" })?;
        let length = read_be_u32(input, cursor + 4)
            .and_then(|length| usize::try_from(length).ok())
            .ok_or(RawError::HeaderDimensionsUnavailable { format: "MRW" })?;

        if tag == 0x0050_5244 {
            let dimensions_end = cursor
                .checked_add(20)
                .ok_or(RawError::HeaderDimensionsUnavailable { format: "MRW" })?;
            if dimensions_end <= scan_end {
                let height = read_be_u16(input, cursor + 16)
                    .ok_or(RawError::HeaderDimensionsUnavailable { format: "MRW" })?;
                let width = read_be_u16(input, cursor + 18)
                    .ok_or(RawError::HeaderDimensionsUnavailable { format: "MRW" })?;
                return Ok((Dimensions::new(u32::from(width), u32::from(height)), 1));
            }
            return Err(RawError::HeaderDimensionsUnavailable { format: "MRW" });
        }

        let next = cursor
            .checked_add(8)
            .and_then(|offset| offset.checked_add(length))
            .ok_or(RawError::HeaderDimensionsUnavailable { format: "MRW" })?;
        if next > scan_end || next <= cursor {
            return Err(RawError::HeaderDimensionsUnavailable { format: "MRW" });
        }
        cursor = next;
    }

    Err(RawError::HeaderDimensionsUnavailable { format: "MRW" })
}

fn probe_x3f_dimensions(input: &[u8]) -> Result<(Dimensions, usize), RawError> {
    let footer_offset = input
        .len()
        .checked_sub(4)
        .and_then(|offset| read_le_u32(input, offset))
        .and_then(|offset| usize::try_from(offset).ok())
        .ok_or(RawError::HeaderDimensionsUnavailable { format: "X3F" })?;
    let directory_header_end = footer_offset
        .checked_add(12)
        .ok_or(RawError::HeaderDimensionsUnavailable { format: "X3F" })?;
    if directory_header_end > input.len()
        || read_le_u32(input, footer_offset + 4).is_none_or(|version| version < 0x0002_0000)
    {
        return Err(RawError::HeaderDimensionsUnavailable { format: "X3F" });
    }
    let entries = read_le_u32(input, footer_offset + 8)
        .and_then(|entries| usize::try_from(entries).ok())
        .ok_or(RawError::HeaderDimensionsUnavailable { format: "X3F" })?;
    let available_entries = (input.len() - directory_header_end) / 12;
    if entries > available_entries {
        return Err(RawError::HeaderDimensionsUnavailable { format: "X3F" });
    }

    for index in 0..entries {
        let entry_offset = footer_offset
            .checked_add(12)
            .and_then(|offset| {
                index
                    .checked_mul(12)
                    .and_then(|delta| offset.checked_add(delta))
            })
            .ok_or(RawError::HeaderDimensionsUnavailable { format: "X3F" })?;
        let image_offset = read_le_u32(input, entry_offset)
            .and_then(|offset| usize::try_from(offset).ok())
            .ok_or(RawError::HeaderDimensionsUnavailable { format: "X3F" })?;
        let name_end = entry_offset
            .checked_add(12)
            .ok_or(RawError::HeaderDimensionsUnavailable { format: "X3F" })?;
        let name_start = entry_offset
            .checked_add(8)
            .ok_or(RawError::HeaderDimensionsUnavailable { format: "X3F" })?;
        let name = input
            .get(name_start..name_end)
            .ok_or(RawError::HeaderDimensionsUnavailable { format: "X3F" })?;
        if name != b"IMA2" {
            continue;
        }
        let image_end = image_offset
            .checked_add(24)
            .ok_or(RawError::HeaderDimensionsUnavailable { format: "X3F" })?;
        if image_end > input.len() {
            return Err(RawError::HeaderDimensionsUnavailable { format: "X3F" });
        }
        let image_type = read_le_u32(input, image_offset + 8)
            .ok_or(RawError::HeaderDimensionsUnavailable { format: "X3F" })?;
        if image_type != 1 && image_type != 3 {
            continue;
        }
        let width = read_le_u32(input, image_offset + 16)
            .ok_or(RawError::HeaderDimensionsUnavailable { format: "X3F" })?;
        let height = read_le_u32(input, image_offset + 20)
            .ok_or(RawError::HeaderDimensionsUnavailable { format: "X3F" })?;
        return Ok((Dimensions::new(width, height), 3));
    }

    Err(RawError::HeaderDimensionsUnavailable { format: "X3F" })
}

fn probe_raf_dimensions(input: &[u8]) -> Result<(Dimensions, usize), RawError> {
    const RAF_DIMENSIONS_TAG: u16 = 0x0100;
    const RAF_MAX_DIRECTORY_ENTRIES: usize = 4000;

    let unavailable = || RawError::HeaderDimensionsUnavailable { format: "RAF" };
    let directory_offset = read_be_u32(input, 92)
        .and_then(|offset| usize::try_from(offset).ok())
        .ok_or_else(unavailable)?;
    let entry_count = read_be_u32(input, directory_offset)
        .and_then(|count| usize::try_from(count).ok())
        .filter(|count| *count <= RAF_MAX_DIRECTORY_ENTRIES)
        .ok_or_else(unavailable)?;

    let mut entry_offset = directory_offset.checked_add(4).ok_or_else(unavailable)?;
    for _ in 0..entry_count {
        let tag = read_be_u16(input, entry_offset).ok_or_else(unavailable)?;
        let length = read_be_u16(input, entry_offset.checked_add(2).ok_or_else(unavailable)?)
            .ok_or_else(unavailable)?;
        if tag == RAF_DIMENSIONS_TAG && length < 4 {
            return Err(unavailable());
        }
        let next_offset = entry_offset
            .checked_add(4)
            .and_then(|offset| offset.checked_add(usize::from(length)))
            .ok_or_else(unavailable)?;
        if next_offset > input.len() || next_offset <= entry_offset {
            return Err(unavailable());
        }

        if tag == RAF_DIMENSIONS_TAG {
            let data_offset = entry_offset.checked_add(4).ok_or_else(unavailable)?;
            let data_end = data_offset.checked_add(4).ok_or_else(unavailable)?;
            let data = input.get(data_offset..data_end).ok_or_else(unavailable)?;
            let height = read_be_u16(data, 0).ok_or_else(unavailable)?;
            let width = read_be_u16(data, 2).ok_or_else(unavailable)?;
            return Ok((Dimensions::new(u32::from(width), u32::from(height)), 1));
        }

        entry_offset = next_offset;
    }

    Err(unavailable())
}

fn has_prefix(input: &[u8], prefix: &[u8]) -> bool {
    input.get(..prefix.len()) == Some(prefix)
}

fn is_mrw(input: &[u8]) -> bool {
    read_be_u32(input, 0) == Some(0x004d_524d)
}

fn is_ciff(input: &[u8]) -> bool {
    input.get(6..14) == Some(b"HEAPCCDR")
}

fn is_tiff_family(input: &[u8]) -> bool {
    input.get(..2) == Some(b"II") || input.get(..2) == Some(b"MM")
}

fn read_be_u16(input: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    let bytes: [u8; 2] = input.get(offset..end)?.try_into().ok()?;
    Some(u16::from_be_bytes(bytes))
}

fn read_le_u32(input: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let bytes: [u8; 4] = input.get(offset..end)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn read_be_u32(input: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let bytes: [u8; 4] = input.get(offset..end)?.try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}

fn declared_dimensions(input: &[u8]) -> Option<Dimensions> {
    catch_unwind(AssertUnwindSafe(|| {
        parse_exif_metadata(input).ok()?.camera.dimensions
    }))
    .ok()
    .flatten()
}

fn adapt_rawloader_image(
    image: rawloader::RawImage,
    input: &[u8],
    limits: RawDecodeLimits,
) -> Result<RawFrame, RawError> {
    if image.cpp != 1 {
        return Err(RawError::UnsupportedData(format!(
            "rawloader returned {} components per pixel",
            image.cpp
        )));
    }
    let dimensions = Dimensions::new(
        u32::try_from(image.width).map_err(|_| RawError::DimensionsOverflow)?,
        u32::try_from(image.height).map_err(|_| RawError::DimensionsOverflow)?,
    );
    let pixel_count = limits.validate_dimensions(dimensions)?;
    let sample_count = match &image.data {
        rawloader::RawImageData::Integer(data) => data.len(),
        rawloader::RawImageData::Float(data) => data.len(),
    };
    limits.validate_sample_count(sample_count)?;
    if sample_count != pixel_count {
        return Err(RawError::SampleCountMismatch {
            dimensions,
            expected: pixel_count,
            actual: sample_count,
        });
    }
    let cfa_width = u32::try_from(image.cfa.width).map_err(|_| RawError::DimensionsOverflow)?;
    let cfa_height = u32::try_from(image.cfa.height).map_err(|_| RawError::DimensionsOverflow)?;
    if cfa_width == 0 || cfa_height == 0 {
        return Err(RawError::UnsupportedData(
            "rawloader returned an invalid CFA".to_owned(),
        ));
    }
    let raw_cfa = &image.cfa;
    let cfa_colors: Vec<_> = (0..cfa_height)
        .flat_map(|y| {
            (0..cfa_width).map(move |x| match raw_cfa.color_at(y as usize, x as usize) {
                0 => CfaColor::Red,
                1 => CfaColor::Green,
                2 => CfaColor::Blue,
                3 => CfaColor::Extra,
                _ => CfaColor::Unknown,
            })
        })
        .collect();
    if cfa_colors
        .iter()
        .any(|color| matches!(color, CfaColor::Extra | CfaColor::Unknown))
    {
        return Err(RawError::UnsupportedData(
            "rawloader returned a CFA with unsupported RGBE or unknown channels".to_owned(),
        ));
    }
    let cfa = CfaPattern::new(cfa_width, cfa_height, cfa_colors)?;
    let samples = match image.data {
        rawloader::RawImageData::Integer(data) => data.into_iter().map(f32::from).collect(),
        rawloader::RawImageData::Float(data) => data,
    };
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
    let lens_profile = camera.lens.as_deref().map(|lens| {
        LensProfileRegistry::built_in()
            .profile_for(&camera)
            .unwrap_or_else(|| LensProfile::unavailable(lens))
    });
    let embedded_preview =
        EmbeddedPreview::from_bytes(parsed.and_then(|metadata| metadata.embedded_preview));
    let profile = CameraProfile::from_xyz_to_camera(
        camera.make.clone(),
        camera.model.clone(),
        image.xyz_to_cam,
    )?;
    RawFrame::new(
        mosaic,
        image.blacklevels.map(f32::from),
        white_levels,
        camera,
        profile,
        lens_profile,
        embedded_preview,
        exif,
    )
}

fn adapt_rawler_image(
    image: rawler::RawImage,
    metadata: Option<rawler::decoders::RawMetadata>,
    input: &[u8],
    limits: RawDecodeLimits,
) -> Result<RawFrame, RawError> {
    if image.cpp != 1 {
        return Err(RawError::UnsupportedData(format!(
            "rawler returned {} components per pixel",
            image.cpp
        )));
    }
    let dimensions = Dimensions::new(
        u32::try_from(image.width).map_err(|_| RawError::DimensionsOverflow)?,
        u32::try_from(image.height).map_err(|_| RawError::DimensionsOverflow)?,
    );
    let pixel_count = limits.validate_dimensions(dimensions)?;
    let sample_count = match &image.data {
        rawler::RawImageData::Integer(data) => data.len(),
        rawler::RawImageData::Float(data) => data.len(),
    };
    limits.validate_sample_count(sample_count)?;
    if sample_count != pixel_count {
        return Err(RawError::SampleCountMismatch {
            dimensions,
            expected: pixel_count,
            actual: sample_count,
        });
    }
    let raw_cfa = match &image.photometric {
        rawler::rawimage::RawPhotometricInterpretation::Cfa(config) => &config.cfa,
        _ => {
            return Err(RawError::UnsupportedData(
                "expected a sensor CFA mosaic".to_owned(),
            ));
        }
    };
    let cfa_width = u32::try_from(raw_cfa.width).map_err(|_| RawError::DimensionsOverflow)?;
    let cfa_height = u32::try_from(raw_cfa.height).map_err(|_| RawError::DimensionsOverflow)?;
    if cfa_width == 0 || cfa_height == 0 {
        return Err(RawError::UnsupportedData(
            "rawler returned an invalid CFA".to_owned(),
        ));
    }
    let cfa_colors: Vec<_> = (0..cfa_height)
        .flat_map(|y| {
            (0..cfa_width).map(move |x| match raw_cfa.color_at(y as usize, x as usize) {
                0 => CfaColor::Red,
                1 => CfaColor::Green,
                2 => CfaColor::Blue,
                3 => CfaColor::Extra,
                _ => CfaColor::Unknown,
            })
        })
        .collect();
    if cfa_colors
        .iter()
        .any(|color| matches!(color, CfaColor::Extra | CfaColor::Unknown))
    {
        return Err(RawError::UnsupportedData(
            "rawler returned a CFA with unsupported RGBE or unknown channels".to_owned(),
        ));
    }
    let cfa = CfaPattern::new(cfa_width, cfa_height, cfa_colors)?;
    let black_levels = adapt_black_levels(&image.blacklevel, &cfa)?;
    let white_levels = match image.whitelevel.0.as_slice() {
        [level] => [*level as f32; 4],
        [r, g, b] | [r, g, b, _] => [*r as f32, *g as f32, *b as f32, *g as f32],
        _ => return Err(RawError::UnsupportedData("invalid white levels".to_owned())),
    };
    // Prefer daylight calibration, with a deterministic fallback to another supplied illuminant.
    let matrix = image
        .color_matrix
        .get(&rawler::imgop::xyz::Illuminant::D65)
        .or_else(|| {
            image
                .color_matrix
                .iter()
                .min_by_key(|(illuminant, _)| **illuminant as u32)
                .map(|(_, matrix)| matrix)
        })
        .ok_or_else(|| {
            RawError::InvalidCameraMatrix("decoder supplied no color matrix".to_owned())
        })?;
    let matrix: [f32; 9] = matrix.as_slice().try_into().map_err(|_| {
        RawError::UnsupportedData("expected a three-channel camera matrix".to_owned())
    })?;
    let xyz_to_camera = [
        [matrix[0], matrix[1], matrix[2]],
        [matrix[3], matrix[4], matrix[5]],
        [matrix[6], matrix[7], matrix[8]],
        [0.0; 3],
    ];
    let orientation = metadata
        .as_ref()
        .and_then(|metadata| metadata.exif.orientation)
        .and_then(|value| parse_orientation(u32::from(value)))
        .unwrap_or_else(|| image.orientation.into());
    let samples = match image.data {
        rawler::RawImageData::Integer(data) => data.into_iter().map(f32::from).collect(),
        rawler::RawImageData::Float(data) => data,
    };
    let bit_depth = infer_bit_depth(white_levels);
    let mosaic = Mosaic::new(dimensions, samples, bit_depth, cfa, orientation)?;
    let vendor_exif = metadata.map(|metadata| metadata.exif).unwrap_or_default();
    let camera = CameraMetadata {
        make: image.make.clone(),
        model: image.model.clone(),
        lens: vendor_exif.lens_model,
        iso: vendor_exif
            .iso_speed
            .or(vendor_exif.iso_speed_ratings.map(u32::from)),
        aperture: vendor_exif
            .fnumber
            .map(|value| value.as_f32())
            .filter(|value| value.is_finite()),
        shutter_seconds: vendor_exif
            .exposure_time
            .map(|value| value.as_f32())
            .filter(|value| value.is_finite()),
        focal_length_mm: vendor_exif
            .focal_length
            .map(|value| value.as_f32())
            .filter(|value| value.is_finite()),
        capture_time: vendor_exif.date_time_original.map(normalize_capture_time),
        orientation,
        dimensions: Some(dimensions),
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
    let lens_profile = camera.lens.as_deref().map(|lens| {
        LensProfileRegistry::built_in()
            .profile_for(&camera)
            .unwrap_or_else(|| LensProfile::unavailable(lens))
    });
    let embedded_preview =
        EmbeddedPreview::from_bytes(parsed.and_then(|metadata| metadata.embedded_preview));
    let profile = CameraProfile::from_xyz_to_camera(
        camera.make.clone(),
        camera.model.clone(),
        xyz_to_camera,
    )?;
    RawFrame::new(
        mosaic,
        black_levels,
        white_levels,
        camera,
        profile,
        lens_profile,
        embedded_preview,
        exif,
    )
}

// The frame schema stores RGB channel levels; rawler supplies a spatial repeat pattern.
// ponytail: average the two green sites; use spatial black levels if per-site correction is needed.
fn adapt_black_levels(
    level: &rawler::rawimage::BlackLevel,
    cfa: &CfaPattern,
) -> Result<[f32; 4], RawError> {
    let expected = level
        .width
        .checked_mul(level.height)
        .ok_or(RawError::DimensionsOverflow)?;
    if level.cpp != 1 || expected == 0 || level.levels.len() != expected {
        return Err(RawError::UnsupportedData(
            "invalid black level pattern".to_owned(),
        ));
    }
    if expected == 1 {
        return Ok([level.levels[0].as_f32(); 4]);
    }
    if level.width != cfa.width() as usize || level.height != cfa.height() as usize {
        return Err(RawError::UnsupportedData(
            "black level repeat differs from CFA".to_owned(),
        ));
    }
    let mut sums = [0.0; 4];
    let mut counts = [0_u32; 4];
    for (value, color) in level.levels.iter().zip(cfa.colors()) {
        let channel = match color {
            CfaColor::Red => 0,
            CfaColor::Green => 1,
            CfaColor::Blue => 2,
            _ => {
                return Err(RawError::UnsupportedData(
                    "unsupported black level channel".to_owned(),
                ));
            }
        };
        sums[channel] += value.as_f32();
        counts[channel] += 1;
    }
    for channel in 0..3 {
        if counts[channel] == 0 {
            return Err(RawError::UnsupportedData(
                "missing black level channel".to_owned(),
            ));
        }
        sums[channel] /= counts[channel] as f32;
    }
    sums[3] = sums[1];
    Ok(sums)
}

#[cfg(test)]
mod decoder_adaptation_tests {
    use super::*;

    #[test]
    fn spatial_black_levels_follow_cfa_colors_not_site_order() {
        let cfa = CfaPattern::new(
            2,
            2,
            vec![
                CfaColor::Green,
                CfaColor::Blue,
                CfaColor::Red,
                CfaColor::Green,
            ],
        )
        .unwrap();
        let level = rawler::rawimage::BlackLevel::new(&[20_u16, 30, 10, 24], 2, 2, 1);
        assert_eq!(
            adapt_black_levels(&level, &cfa).unwrap(),
            [10.0, 22.0, 30.0, 22.0]
        );
        assert_eq!(
            adapt_black_levels(&rawler::rawimage::BlackLevel::new(&[7_u16], 1, 1, 1), &cfa)
                .unwrap(),
            [7.0; 4]
        );
        let invalid = rawler::rawimage::BlackLevel {
            levels: vec![],
            ..level
        };
        assert!(adapt_black_levels(&invalid, &cfa).is_err());
    }
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
        let error = RawloaderDecoder::default()
            .decode(b"not a RAW file")
            .unwrap_err();
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
