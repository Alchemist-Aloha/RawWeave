//! Scene-linear and display color foundations.
//!
//! Color conversion lives here, not in the viewer. The initial implementation provides a
//! serializable scene-linear RGB buffer, a display RGB buffer, and a small transform boundary
//! that can later be backed by OCIO or LittleCMS without changing graph value types.

use std::sync::Arc;

use rawweave_image::Dimensions;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Named working spaces supported by the initial color boundary.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WorkingSpace {
    /// Linear RGB values expressed in the camera sensor's native primaries.
    CameraNative,
    /// Linearized sRGB primaries.
    #[default]
    Srgb,
    /// Display-P3 primaries.
    DisplayP3,
    /// ProPhoto RGB primaries.
    ProPhoto,
    /// Rec. 2020 primaries.
    Rec2020,
    /// A future or externally supplied color space.
    Custom(String),
}

/// Errors raised while constructing or transforming color buffers.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ColorError {
    /// The pixel buffer length does not match the declared dimensions.
    #[error("color dimensions {width}x{height} require {expected} pixels, got {actual}")]
    PixelCountMismatch {
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
        /// Required pixel count.
        expected: usize,
        /// Actual pixel count.
        actual: usize,
    },
    /// The requested dimensions overflow the platform pixel-count type.
    #[error("color dimensions overflow")]
    DimensionsOverflow,
    /// Reduced RGB buffers must retain a valid full-size coordinate grid.
    #[error("invalid preview sampling geometry")]
    InvalidPreviewSampling,
    /// A color sample must be finite.
    #[error("color sample at pixel {pixel}, channel {channel} is not finite")]
    NonFiniteSample { pixel: usize, channel: usize },
    /// A conversion needs a working-space definition that is not available here.
    #[error("unsupported working-space conversion from {from:?} to {to:?}")]
    UnsupportedWorkingSpace {
        /// Source working space.
        from: WorkingSpace,
        /// Destination working space.
        to: WorkingSpace,
    },
    /// A native color backend is represented but not linked into this build.
    #[error("color backend {backend:?} is unavailable: {operation}")]
    BackendUnavailable {
        /// Backend that would perform the operation.
        backend: ColorTransformBackend,
        /// Operation that could not be performed.
        operation: String,
    },
}

fn validate_pixels(dimensions: Dimensions, pixels: &[[f32; 3]]) -> Result<(), ColorError> {
    let expected = dimensions
        .pixel_count()
        .map_err(|_| ColorError::DimensionsOverflow)?;
    if pixels.len() != expected {
        return Err(ColorError::PixelCountMismatch {
            width: dimensions.width,
            height: dimensions.height,
            expected,
            actual: pixels.len(),
        });
    }
    for (pixel, values) in pixels.iter().enumerate() {
        for (channel, value) in values.iter().enumerate() {
            if !value.is_finite() {
                return Err(ColorError::NonFiniteSample { pixel, channel });
            }
        }
    }
    Ok(())
}

/// A display-preview raster sampled on the full image's origin-anchored grid.
/// Mip zero/full-quality buffers retain their legacy wire representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PreviewSampling {
    pub full_dimensions: Dimensions,
    pub mip: u8,
}

impl PreviewSampling {
    pub fn dimensions(self) -> Result<Dimensions, ColorError> {
        if self.mip == 0
            || self.mip > 6
            || self.full_dimensions.width == 0
            || self.full_dimensions.height == 0
        {
            return Err(ColorError::InvalidPreviewSampling);
        }
        let scale = 1_u32 << self.mip;
        Ok(Dimensions::new(
            self.full_dimensions.width.div_ceil(scale),
            self.full_dimensions.height.div_ceil(scale),
        ))
    }
}

fn validate_sampling(
    dimensions: Dimensions,
    sampling: Option<PreviewSampling>,
) -> Result<(), ColorError> {
    if let Some(sampling) = sampling
        && sampling.dimensions()? != dimensions
    {
        return Err(ColorError::InvalidPreviewSampling);
    }
    Ok(())
}

/// Scene-referred linear RGB pixels. Values are intentionally not clipped to 0..1.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SceneLinearRGB {
    dimensions: Dimensions,
    pixels: Arc<Vec<[f32; 3]>>,
    working_space: WorkingSpace,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sampling: Option<PreviewSampling>,
}

#[derive(Deserialize)]
struct SceneLinearRGBWire {
    dimensions: Dimensions,
    pixels: Vec<[f32; 3]>,
    working_space: WorkingSpace,
    #[serde(default)]
    sampling: Option<PreviewSampling>,
}

impl<'de> Deserialize<'de> for SceneLinearRGB {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = SceneLinearRGBWire::deserialize(deserializer)?;
        Self::new(wire.dimensions, wire.pixels, wire.working_space)
            .and_then(|scene| scene.with_sampling(wire.sampling))
            .map_err(serde::de::Error::custom)
    }
}

impl SceneLinearRGB {
    /// Construct a scene-linear buffer in an explicit working space.
    pub fn new(
        dimensions: Dimensions,
        pixels: Vec<[f32; 3]>,
        working_space: WorkingSpace,
    ) -> Result<Self, ColorError> {
        validate_pixels(dimensions, &pixels)?;
        Ok(Self {
            dimensions,
            pixels: Arc::new(pixels),
            working_space,
            sampling: None,
        })
    }

    pub const fn sampling(&self) -> Option<PreviewSampling> {
        self.sampling
    }

    pub fn with_sampling(mut self, sampling: Option<PreviewSampling>) -> Result<Self, ColorError> {
        validate_sampling(self.dimensions, sampling)?;
        self.sampling = sampling;
        Ok(self)
    }

    /// Construct an sRGB scene-linear buffer.
    pub fn from_pixels(width: u32, height: u32, pixels: Vec<[f32; 3]>) -> Result<Self, ColorError> {
        Self::new(Dimensions::new(width, height), pixels, WorkingSpace::Srgb)
    }

    /// Buffer dimensions.
    pub const fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    /// Scene working space.
    pub fn working_space(&self) -> WorkingSpace {
        self.working_space.clone()
    }

    /// Scene pixels in row-major order.
    pub fn pixels(&self) -> &[[f32; 3]] {
        &self.pixels
    }

    /// Pixel at a local coordinate.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[f32; 3]> {
        if x >= self.dimensions.width || y >= self.dimensions.height {
            return None;
        }
        self.pixels
            .get(y as usize * self.dimensions.width as usize + x as usize)
            .copied()
    }

    /// Apply a per-pixel operation while retaining the scene working space.
    pub fn map_pixels(
        &self,
        mut map: impl FnMut([f32; 3]) -> [f32; 3],
    ) -> Result<Self, ColorError> {
        Self::new(
            self.dimensions,
            self.pixels.iter().copied().map(&mut map).collect(),
            self.working_space.clone(),
        )
        .and_then(|scene| scene.with_sampling(self.sampling))
    }

    /// Share the immutable buffer in another declared working space without changing samples.
    pub fn with_working_space(&self, working_space: WorkingSpace) -> Self {
        Self {
            dimensions: self.dimensions,
            pixels: self.pixels.clone(),
            working_space,
            sampling: self.sampling,
        }
    }
}

/// Display-referred RGB pixels. Display values are normally clipped to 0..1.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DisplayRGB {
    dimensions: Dimensions,
    pixels: Arc<Vec<[f32; 3]>>,
    working_space: WorkingSpace,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sampling: Option<PreviewSampling>,
}

#[derive(Deserialize)]
struct DisplayRGBWire {
    dimensions: Dimensions,
    pixels: Vec<[f32; 3]>,
    working_space: WorkingSpace,
    #[serde(default)]
    sampling: Option<PreviewSampling>,
}

impl<'de> Deserialize<'de> for DisplayRGB {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = DisplayRGBWire::deserialize(deserializer)?;
        Self::new(wire.dimensions, wire.pixels, wire.working_space)
            .and_then(|display| display.with_sampling(wire.sampling))
            .map_err(serde::de::Error::custom)
    }
}

impl DisplayRGB {
    /// Construct a display buffer in an explicit display space.
    pub fn new(
        dimensions: Dimensions,
        pixels: Vec<[f32; 3]>,
        working_space: WorkingSpace,
    ) -> Result<Self, ColorError> {
        validate_pixels(dimensions, &pixels)?;
        Ok(Self {
            dimensions,
            pixels: Arc::new(pixels),
            working_space,
            sampling: None,
        })
    }

    /// Buffer dimensions.
    pub const fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    pub const fn sampling(&self) -> Option<PreviewSampling> {
        self.sampling
    }

    pub fn with_sampling(mut self, sampling: Option<PreviewSampling>) -> Result<Self, ColorError> {
        validate_sampling(self.dimensions, sampling)?;
        self.sampling = sampling;
        Ok(self)
    }

    /// Display working space.
    pub fn working_space(&self) -> WorkingSpace {
        self.working_space.clone()
    }

    /// Display pixels in row-major order.
    pub fn pixels(&self) -> &[[f32; 3]] {
        &self.pixels
    }

    /// Pixel at a local coordinate.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[f32; 3]> {
        if x >= self.dimensions.width || y >= self.dimensions.height {
            return None;
        }
        self.pixels
            .get(y as usize * self.dimensions.width as usize + x as usize)
            .copied()
    }
}

/// Transform scene-linear RGB into display RGB.
pub trait DisplayTransform: Send + Sync {
    /// Apply the transform without mutating the scene buffer.
    fn transform(&self, scene: &SceneLinearRGB) -> Result<DisplayRGB, ColorError>;

    /// Human-readable transform name for node metadata and diagnostics.
    fn name(&self) -> &'static str;
}

/// A basic sRGB transfer-function display transform.
#[derive(Clone, Copy, Debug, Default)]
pub struct SrgbDisplayTransform;

impl DisplayTransform for SrgbDisplayTransform {
    fn transform(&self, scene: &SceneLinearRGB) -> Result<DisplayRGB, ColorError> {
        let srgb_scene = MatrixWorkingSpaceTransform::new(WorkingSpace::Srgb).transform(scene)?;
        let workers = std::thread::available_parallelism().map_or(1, usize::from);
        let pixels = encode_display_pixels(srgb_scene.pixels(), workers);
        DisplayRGB::new(srgb_scene.dimensions(), pixels, WorkingSpace::Srgb)
            .and_then(|display| display.with_sampling(scene.sampling()))
    }

    fn name(&self) -> &'static str {
        "sRGB display transfer"
    }
}

/// A no-op scene-to-scene transform used as the default camera/lens boundary.
#[derive(Clone, Copy, Debug, Default)]
pub struct IdentitySceneTransform;

/// Transform scene-linear data while retaining its representation.
pub trait SceneTransform: Send + Sync {
    /// Apply a scene transform.
    fn transform(&self, scene: &SceneLinearRGB) -> Result<SceneLinearRGB, ColorError>;

    /// Human-readable transform name.
    fn name(&self) -> &'static str;
}

impl SceneTransform for IdentitySceneTransform {
    fn transform(&self, scene: &SceneLinearRGB) -> Result<SceneLinearRGB, ColorError> {
        Ok(scene.clone())
    }

    fn name(&self) -> &'static str {
        "identity scene transform"
    }
}

/// A named color backend, kept separate from the scene graph value types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorTransformBackend {
    /// OpenColorIO, when the application is built with native OCIO support.
    Ocio,
    /// LittleCMS, when the application is built with native ICC support.
    IccLittleCms,
}

/// An external profile/configuration reference owned by a native backend.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorProfileRef {
    /// A color space in an OCIO configuration.
    Ocio {
        /// OCIO configuration handle, commonly a path or registry key.
        config: String,
        /// Color-space name in the configuration.
        color_space: String,
    },
    /// An ICC profile handle, commonly a path or registry key.
    Icc {
        /// ICC profile handle.
        profile: String,
    },
}

impl ColorProfileRef {
    /// Refer to a color space in an OCIO configuration.
    pub fn ocio(config: impl Into<String>, color_space: impl Into<String>) -> Self {
        Self::Ocio {
            config: config.into(),
            color_space: color_space.into(),
        }
    }

    /// Refer to an ICC profile.
    pub fn icc(profile: impl Into<String>) -> Self {
        Self::Icc {
            profile: profile.into(),
        }
    }
}

/// OCIO transform boundary. Native OCIO support can be added without changing callers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OcioBackend {
    config: String,
}

impl OcioBackend {
    /// Create an OCIO backend handle without loading native libraries.
    pub fn new(config: impl Into<String>) -> Self {
        Self {
            config: config.into(),
        }
    }

    /// Backend represented by this handle.
    pub const fn backend(&self) -> ColorTransformBackend {
        ColorTransformBackend::Ocio
    }

    /// OCIO configuration handle.
    pub fn config_handle(&self) -> &str {
        &self.config
    }

    /// Transform through OCIO when native integration is enabled.
    pub fn transform(
        &self,
        _scene: &SceneLinearRGB,
        _source: &ColorProfileRef,
        _destination: &ColorProfileRef,
    ) -> Result<SceneLinearRGB, ColorError> {
        Err(ColorError::BackendUnavailable {
            backend: self.backend(),
            operation: format!("OCIO config {} is not linked", self.config),
        })
    }
}

/// ICC/LittleCMS transform boundary. Native support is intentionally optional.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IccLittleCmsBackend {
    profile: String,
}

impl IccLittleCmsBackend {
    /// Create an ICC backend handle without loading native libraries.
    pub fn new(profile: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
        }
    }

    /// Backend represented by this handle.
    pub const fn backend(&self) -> ColorTransformBackend {
        ColorTransformBackend::IccLittleCms
    }

    /// ICC profile handle.
    pub fn profile_handle(&self) -> &str {
        &self.profile
    }

    /// Transform through LittleCMS when native integration is enabled.
    pub fn transform(
        &self,
        _scene: &SceneLinearRGB,
        _source: &ColorProfileRef,
        _destination: &ColorProfileRef,
    ) -> Result<SceneLinearRGB, ColorError> {
        Err(ColorError::BackendUnavailable {
            backend: self.backend(),
            operation: format!("ICC profile {} is not linked", self.profile),
        })
    }
}

/// Matrix-based conversion between the built-in RGB working spaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatrixWorkingSpaceTransform {
    destination: WorkingSpace,
}

impl MatrixWorkingSpaceTransform {
    /// Create a transform to a built-in destination working space.
    pub fn new(destination: WorkingSpace) -> Self {
        Self { destination }
    }

    /// Destination working space.
    pub fn destination(&self) -> WorkingSpace {
        self.destination.clone()
    }

    /// Convert D65-referenced XYZ pixels into the destination working space.
    pub fn transform_xyz(
        &self,
        dimensions: Dimensions,
        xyz_pixels: Vec<[f32; 3]>,
    ) -> Result<SceneLinearRGB, ColorError> {
        let xyz_to_destination =
            invert_matrix(working_space_to_xyz(&self.destination).ok_or_else(|| {
                ColorError::UnsupportedWorkingSpace {
                    from: WorkingSpace::CameraNative,
                    to: self.destination.clone(),
                }
            })?)
            .ok_or_else(|| ColorError::UnsupportedWorkingSpace {
                from: WorkingSpace::CameraNative,
                to: self.destination.clone(),
            })?;
        let pixels = xyz_pixels
            .into_iter()
            .map(|pixel| multiply_vector(xyz_to_destination, pixel))
            .collect();
        SceneLinearRGB::new(dimensions, pixels, self.destination.clone())
    }
}

impl SceneTransform for MatrixWorkingSpaceTransform {
    fn transform(&self, scene: &SceneLinearRGB) -> Result<SceneLinearRGB, ColorError> {
        let source = scene.working_space();
        if source == self.destination {
            return Ok(scene.clone());
        }
        let source_to_xyz =
            working_space_to_xyz(&source).ok_or_else(|| ColorError::UnsupportedWorkingSpace {
                from: source.clone(),
                to: self.destination.clone(),
            })?;
        let xyz_pixels = scene
            .pixels()
            .iter()
            .map(|pixel| multiply_vector(source_to_xyz, *pixel))
            .collect();
        self.transform_xyz(scene.dimensions(), xyz_pixels)
            .and_then(|converted| converted.with_sampling(scene.sampling()))
    }

    fn name(&self) -> &'static str {
        "matrix working-space transform"
    }
}

// The matrices use a D65 XYZ reference. ProPhoto's native D50 primaries are
// Bradford-adapted here so all built-in spaces share one conversion boundary.
fn working_space_to_xyz(space: &WorkingSpace) -> Option<[[f32; 3]; 3]> {
    match space {
        WorkingSpace::Srgb => Some([
            [0.4124564, 0.3575761, 0.1804375],
            [0.2126729, 0.7151522, 0.0721750],
            [0.0193339, 0.119_192, 0.9503041],
        ]),
        WorkingSpace::DisplayP3 => Some([
            [0.48657095, 0.26566769, 0.19821729],
            [0.22897456, 0.69173852, 0.07928691],
            [0.0, 0.04511338, 1.043_944_4],
        ]),
        WorkingSpace::ProPhoto => Some([
            [0.7555907, 0.1127198, 0.0821454],
            [0.2683219, 0.7151153, 0.0165619],
            [0.0039160, -0.0129335, 1.0980752],
        ]),
        WorkingSpace::Rec2020 => Some([
            [0.63695805, 0.144_616_9, 0.16888098],
            [0.262_700_2, 0.67799807, 0.05930172],
            [0.0, 0.02807269, 1.060_985_1],
        ]),
        WorkingSpace::CameraNative | WorkingSpace::Custom(_) => None,
    }
}

fn multiply_vector(matrix: [[f32; 3]; 3], vector: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|row| {
        matrix[row][0] * vector[0] + matrix[row][1] * vector[1] + matrix[row][2] * vector[2]
    })
}

fn invert_matrix(matrix: [[f32; 3]; 3]) -> Option<[[f32; 3]; 3]> {
    let [a, b, c] = matrix[0];
    let [d, e, f] = matrix[1];
    let [g, h, i] = matrix[2];
    let determinant = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if determinant.abs() <= f32::EPSILON {
        return None;
    }
    let inverse_determinant = 1.0 / determinant;
    Some([
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
    ])
}

fn fit_srgb_gamut(mut pixel: [f32; 3]) -> [f32; 3] {
    let minimum = pixel.iter().copied().fold(0.0, f32::min);
    if minimum < 0.0 {
        pixel.iter_mut().for_each(|value| *value -= minimum);
        let maximum = pixel.iter().copied().fold(1.0, f32::max);
        if maximum > 1.0 {
            pixel.iter_mut().for_each(|value| *value /= maximum);
        }
        return pixel.map(|value| value.clamp(0.0, 1.0));
    }
    pixel
}

fn encode_display_pixels(pixels: &[[f32; 3]], workers: usize) -> Vec<[f32; 3]> {
    let encode = |input: &[[f32; 3]], output: &mut [[f32; 3]]| {
        for (pixel, encoded) in input.iter().zip(output) {
            *encoded = fit_srgb_gamut(*pixel).map(encode_srgb);
        }
    };
    // ponytail: at most eight per-call workers; use a shared pool if concurrent
    // preview jobs make thread creation/oversubscription a measured bottleneck.
    let workers = workers.clamp(1, 8).min((pixels.len() / 262_144).max(1));
    let mut output = vec![[0.0; 3]; pixels.len()];
    if workers == 1 {
        encode(pixels, &mut output);
        return output;
    }
    let chunk_size = pixels.len().div_ceil(workers);
    let failed = std::thread::scope(|scope| {
        let mut failed = false;
        for (input, output) in pixels.chunks(chunk_size).zip(output.chunks_mut(chunk_size)) {
            if std::thread::Builder::new()
                .spawn_scoped(scope, move || encode(input, output))
                .is_err()
            {
                failed = true;
            }
        }
        failed
    });
    // A host unable to create workers still gets a real serial result.
    if failed {
        encode(pixels, &mut output);
    }
    output
}

fn encode_srgb(value: f32) -> f32 {
    let value = value.max(0.0);
    let encoded = if value <= 0.0031308 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    encoded.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::{DisplayTransform, SceneLinearRGB, SrgbDisplayTransform};

    #[test]
    fn bounded_display_workers_match_serial_transfer_bit_for_bit() {
        let samples = [-0.0, -0.5, 0.0031308, 0.0031309, 0.75, 1.0, 4.0, f32::MAX];
        for count in [0, 1, 17, 524_289] {
            let pixels: Vec<_> = (0..count)
                .map(|index| {
                    std::array::from_fn(|channel| samples[(index + channel) % samples.len()])
                })
                .collect();
            let expected: Vec<_> = pixels
                .iter()
                .map(|pixel| {
                    super::fit_srgb_gamut(*pixel)
                        .map(super::encode_srgb)
                        .map(f32::to_bits)
                })
                .collect();
            for workers in [0, 1, 2, 8, usize::MAX] {
                let actual: Vec<_> = super::encode_display_pixels(&pixels, workers)
                    .into_iter()
                    .map(|pixel| pixel.map(f32::to_bits))
                    .collect();
                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn srgb_transfer_clips_highlights_only_at_display_boundary() {
        let scene = SceneLinearRGB::from_pixels(1, 1, vec![[4.0, 0.25, 0.0]]).unwrap();
        let display = SrgbDisplayTransform.transform(&scene).unwrap();
        assert_eq!(scene.pixel(0, 0), Some([4.0, 0.25, 0.0]));
        assert_eq!(display.pixel(0, 0).unwrap()[0], 1.0);
    }
}
