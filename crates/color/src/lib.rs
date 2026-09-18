//! Scene-linear and display color foundations.
//!
//! Color conversion lives here, not in the viewer. The initial implementation provides a
//! serializable scene-linear RGB buffer, a display RGB buffer, and a small transform boundary
//! that can later be backed by OCIO or LittleCMS without changing graph value types.

use rawweave_image::Dimensions;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Named working spaces supported by the initial color boundary.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WorkingSpace {
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
    /// A color sample must be finite.
    #[error("color sample at pixel {pixel}, channel {channel} is not finite")]
    NonFiniteSample { pixel: usize, channel: usize },
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

/// Scene-referred linear RGB pixels. Values are intentionally not clipped to 0..1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneLinearRGB {
    dimensions: Dimensions,
    pixels: Vec<[f32; 3]>,
    working_space: WorkingSpace,
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
            pixels,
            working_space,
        })
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
    }

    /// Copy the buffer into another declared working space without changing samples.
    pub fn with_working_space(&self, working_space: WorkingSpace) -> Self {
        Self {
            dimensions: self.dimensions,
            pixels: self.pixels.clone(),
            working_space,
        }
    }
}

/// Display-referred RGB pixels. Display values are normally clipped to 0..1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisplayRGB {
    dimensions: Dimensions,
    pixels: Vec<[f32; 3]>,
    working_space: WorkingSpace,
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
            pixels,
            working_space,
        })
    }

    /// Buffer dimensions.
    pub const fn dimensions(&self) -> Dimensions {
        self.dimensions
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
        let pixels = scene
            .pixels()
            .iter()
            .map(|pixel| pixel.map(encode_srgb))
            .collect();
        DisplayRGB::new(scene.dimensions(), pixels, WorkingSpace::Srgb)
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
    fn srgb_transfer_clips_highlights_only_at_display_boundary() {
        let scene = SceneLinearRGB::from_pixels(1, 1, vec![[4.0, 0.25, 0.0]]).unwrap();
        let display = SrgbDisplayTransform.transform(&scene).unwrap();
        assert_eq!(scene.pixel(0, 0), Some([4.0, 0.25, 0.0]));
        assert_eq!(display.pixel(0, 0).unwrap()[0], 1.0);
    }
}
