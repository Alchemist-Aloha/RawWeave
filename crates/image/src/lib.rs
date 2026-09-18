use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Pixel = [f32; 4];

static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

impl Dimensions {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn pixel_count(self) -> Result<usize, ImageError> {
        pixel_count(self.width, self.height)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PixelFormat {
    #[default]
    Rgba32Float,
    Rgba16Float,
    Rgba8Unorm,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColorDomain {
    #[default]
    LinearSrgb,
    Srgb,
    DisplayP3,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ColorMetadata {
    pub domain: ColorDomain,
    pub alpha_is_premultiplied: bool,
}

impl Default for ColorMetadata {
    fn default() -> Self {
        Self {
            domain: ColorDomain::LinearSrgb,
            alpha_is_premultiplied: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Region {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn dimensions(self) -> Dimensions {
        Dimensions::new(self.width, self.height)
    }

    pub fn end_x(self) -> Option<u32> {
        self.x.checked_add(self.width)
    }

    pub fn end_y(self) -> Option<u32> {
        self.y.checked_add(self.height)
    }

    pub fn contains(self, x: u32, y: u32) -> bool {
        self.end_x().is_some_and(|end| x >= self.x && x < end)
            && self.end_y().is_some_and(|end| y >= self.y && y < end)
    }

    pub fn is_inside(self, dimensions: Dimensions) -> bool {
        self.end_x().is_some_and(|end| end <= dimensions.width)
            && self.end_y().is_some_and(|end| end <= dimensions.height)
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let end_x = self.end_x()?.min(other.end_x()?);
        let end_y = self.end_y()?.min(other.end_y()?);
        (x < end_x && y < end_y).then(|| Self::new(x, y, end_x - x, end_y - y))
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ImageError {
    #[error("image dimensions {width}x{height} require {expected} pixels, got {actual}")]
    PixelCountMismatch {
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
    #[error("image dimensions are too large")]
    DimensionsOverflow,
    #[error("region {region:?} is outside image dimensions {dimensions:?}")]
    InvalidRegion {
        region: Region,
        dimensions: Dimensions,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Image {
    width: u32,
    height: u32,
    #[serde(default)]
    origin_x: u32,
    #[serde(default)]
    origin_y: u32,
    #[serde(default)]
    pixel_format: PixelFormat,
    #[serde(default)]
    color_metadata: ColorMetadata,
    pixels: Arc<Vec<Pixel>>,
    #[serde(default)]
    revision: u64,
}

impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.origin_x == other.origin_x
            && self.origin_y == other.origin_y
            && self.pixel_format == other.pixel_format
            && self.color_metadata == other.color_metadata
            && self.pixels == other.pixels
    }
}

impl Eq for Image {}

impl Image {
    pub fn new(width: u32, height: u32) -> Result<Self, ImageError> {
        let count = pixel_count(width, height)?;
        Ok(Self {
            width,
            height,
            origin_x: 0,
            origin_y: 0,
            pixel_format: PixelFormat::default(),
            color_metadata: ColorMetadata::default(),
            pixels: Arc::new(vec![[0.0; 4]; count]),
            revision: next_revision(),
        })
    }

    pub fn from_pixels(width: u32, height: u32, pixels: Vec<Pixel>) -> Result<Self, ImageError> {
        Self::from_pixels_with_metadata(
            width,
            height,
            pixels,
            PixelFormat::default(),
            ColorDomain::default(),
        )
    }

    pub fn from_pixels_with_metadata(
        width: u32,
        height: u32,
        pixels: Vec<Pixel>,
        pixel_format: PixelFormat,
        color_domain: ColorDomain,
    ) -> Result<Self, ImageError> {
        Self::from_pixels_with_color_metadata(
            width,
            height,
            pixels,
            pixel_format,
            ColorMetadata {
                domain: color_domain,
                ..ColorMetadata::default()
            },
        )
    }

    pub fn from_pixels_with_color_metadata(
        width: u32,
        height: u32,
        pixels: Vec<Pixel>,
        pixel_format: PixelFormat,
        color_metadata: ColorMetadata,
    ) -> Result<Self, ImageError> {
        let expected = pixel_count(width, height)?;
        if pixels.len() != expected {
            return Err(ImageError::PixelCountMismatch {
                width,
                height,
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            width,
            height,
            origin_x: 0,
            origin_y: 0,
            pixel_format,
            color_metadata,
            pixels: Arc::new(pixels),
            revision: next_revision(),
        })
    }

    pub fn from_pixels_with_origin(
        dimensions: Dimensions,
        origin: (u32, u32),
        pixels: Vec<Pixel>,
        pixel_format: PixelFormat,
        color_metadata: ColorMetadata,
    ) -> Result<Self, ImageError> {
        let expected = dimensions.pixel_count()?;
        if pixels.len() != expected {
            return Err(ImageError::PixelCountMismatch {
                width: dimensions.width,
                height: dimensions.height,
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            width: dimensions.width,
            height: dimensions.height,
            origin_x: origin.0,
            origin_y: origin.1,
            pixel_format,
            color_metadata,
            pixels: Arc::new(pixels),
            revision: next_revision(),
        })
    }

    pub fn from_pixels_with_revision(
        dimensions: Dimensions,
        pixels: Vec<Pixel>,
        pixel_format: PixelFormat,
        color_metadata: ColorMetadata,
        revision: u64,
    ) -> Result<Self, ImageError> {
        Self::from_pixels_with_origin_and_revision(
            dimensions,
            (0, 0),
            pixels,
            pixel_format,
            color_metadata,
            revision,
        )
    }

    fn from_pixels_with_origin_and_revision(
        dimensions: Dimensions,
        origin: (u32, u32),
        pixels: Vec<Pixel>,
        pixel_format: PixelFormat,
        color_metadata: ColorMetadata,
        revision: u64,
    ) -> Result<Self, ImageError> {
        let expected = dimensions.pixel_count()?;
        if pixels.len() != expected {
            return Err(ImageError::PixelCountMismatch {
                width: dimensions.width,
                height: dimensions.height,
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            width: dimensions.width,
            height: dimensions.height,
            origin_x: origin.0,
            origin_y: origin.1,
            pixel_format,
            color_metadata,
            pixels: Arc::new(pixels),
            revision,
        })
    }

    pub fn dimensions(&self) -> Dimensions {
        Dimensions::new(self.width, self.height)
    }

    pub fn origin(&self) -> (u32, u32) {
        (self.origin_x, self.origin_y)
    }

    pub fn global_region(&self) -> Region {
        Region::new(self.origin_x, self.origin_y, self.width, self.height)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixel_format(&self) -> PixelFormat {
        self.pixel_format
    }

    pub fn color_domain(&self) -> ColorDomain {
        self.color_metadata.domain
    }

    pub fn color_metadata(&self) -> ColorMetadata {
        self.color_metadata
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn pixels(&self) -> &[Pixel] {
        self.pixels.as_slice()
    }

    pub fn backing_ptr(&self) -> *const Pixel {
        self.pixels.as_ptr()
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<Pixel> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.pixels
            .get(y as usize * self.width as usize + x as usize)
            .copied()
    }

    pub fn pixel_global(&self, x: u32, y: u32) -> Option<Pixel> {
        if !self.global_region().contains(x, y) {
            return None;
        }
        self.pixel(x - self.origin_x, y - self.origin_y)
    }

    pub fn map_pixels(&self, mut map: impl FnMut(Pixel) -> Pixel) -> Self {
        Self::from_pixels_with_origin_and_revision(
            self.dimensions(),
            self.origin(),
            self.pixels.iter().copied().map(&mut map).collect(),
            self.pixel_format,
            self.color_metadata,
            next_revision(),
        )
        .expect("mapping an existing image preserves its dimensions")
    }

    pub fn view(&self, region: Region) -> Result<ImageView, ImageError> {
        if !region.is_inside(self.dimensions()) {
            return Err(ImageError::InvalidRegion {
                region,
                dimensions: self.dimensions(),
            });
        }
        Ok(ImageView {
            image: self.clone(),
            region,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImageView {
    image: Image,
    region: Region,
}

impl ImageView {
    pub fn region(&self) -> Region {
        self.region
    }

    pub fn dimensions(&self) -> Dimensions {
        self.region.dimensions()
    }

    pub fn origin(&self) -> (u32, u32) {
        (
            self.image.origin_x.saturating_add(self.region.x),
            self.image.origin_y.saturating_add(self.region.y),
        )
    }

    pub fn stride(&self) -> u32 {
        self.image.width
    }

    pub fn backing_ptr(&self) -> *const Pixel {
        self.image.backing_ptr()
    }

    pub fn revision(&self) -> u64 {
        self.image.revision()
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<Pixel> {
        if x >= self.region.width || y >= self.region.height {
            return None;
        }
        self.image.pixel(self.region.x + x, self.region.y + y)
    }

    pub fn pixels(&self) -> Vec<Pixel> {
        (0..self.region.height)
            .flat_map(|y| (0..self.region.width).filter_map(move |x| self.pixel(x, y)))
            .collect()
    }

    pub fn to_image(&self) -> Result<Image, ImageError> {
        Image::from_pixels_with_origin_and_revision(
            self.dimensions(),
            self.origin(),
            self.pixels(),
            self.image.pixel_format(),
            self.image.color_metadata(),
            self.image.revision(),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Cpu,
    Gpu,
}

pub trait GpuImageResource: Send + Sync + fmt::Debug {
    fn resource_id(&self) -> u64;
    fn context_id(&self) -> u64;
    fn as_any(&self) -> &dyn std::any::Any;
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GpuImage {
    pub resource_id: u64,
    pub context_id: u64,
    #[serde(default)]
    pub origin: (u32, u32),
    pub dimensions: Dimensions,
    pub pixel_format: PixelFormat,
    pub color_metadata: ColorMetadata,
    pub revision: u64,
    #[serde(skip)]
    runtime_resource: Option<Arc<dyn GpuImageResource>>,
}

impl fmt::Debug for GpuImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GpuImage")
            .field("resource_id", &self.resource_id)
            .field("context_id", &self.context_id)
            .field("dimensions", &self.dimensions)
            .field("pixel_format", &self.pixel_format)
            .field("color_metadata", &self.color_metadata)
            .field("revision", &self.revision)
            .field("has_runtime_resource", &self.runtime_resource.is_some())
            .finish()
    }
}

impl PartialEq for GpuImage {
    fn eq(&self, other: &Self) -> bool {
        self.resource_id == other.resource_id
            && self.context_id == other.context_id
            && self.origin == other.origin
            && self.dimensions == other.dimensions
            && self.pixel_format == other.pixel_format
            && self.color_metadata == other.color_metadata
            && self.revision == other.revision
    }
}

impl Eq for GpuImage {}

impl GpuImage {
    pub fn new(
        resource_id: u64,
        dimensions: Dimensions,
        pixel_format: PixelFormat,
        color_metadata: ColorMetadata,
        revision: u64,
    ) -> Self {
        Self {
            resource_id,
            context_id: 0,
            origin: (0, 0),
            dimensions,
            pixel_format,
            color_metadata,
            revision,
            runtime_resource: None,
        }
    }

    pub fn from_resource(
        dimensions: Dimensions,
        pixel_format: PixelFormat,
        color_metadata: ColorMetadata,
        revision: u64,
        resource: Arc<dyn GpuImageResource>,
    ) -> Self {
        Self::from_resource_with_origin(
            dimensions,
            (0, 0),
            pixel_format,
            color_metadata,
            revision,
            resource,
        )
    }

    pub fn from_resource_with_origin(
        dimensions: Dimensions,
        origin: (u32, u32),
        pixel_format: PixelFormat,
        color_metadata: ColorMetadata,
        revision: u64,
        resource: Arc<dyn GpuImageResource>,
    ) -> Self {
        Self {
            resource_id: resource.resource_id(),
            context_id: resource.context_id(),
            origin,
            dimensions,
            pixel_format,
            color_metadata,
            revision,
            runtime_resource: Some(resource),
        }
    }

    pub fn runtime_resource(&self) -> Option<&Arc<dyn GpuImageResource>> {
        self.runtime_resource.as_ref()
    }

    pub fn has_runtime_resource(&self) -> bool {
        self.runtime_resource.is_some()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ImageResource {
    Cpu(Image),
    Gpu(GpuImage),
}

impl ImageResource {
    pub fn kind(&self) -> ResourceKind {
        match self {
            Self::Cpu(_) => ResourceKind::Cpu,
            Self::Gpu(_) => ResourceKind::Gpu,
        }
    }

    pub fn dimensions(&self) -> Dimensions {
        match self {
            Self::Cpu(image) => image.dimensions(),
            Self::Gpu(image) => image.dimensions,
        }
    }

    pub fn revision(&self) -> u64 {
        match self {
            Self::Cpu(image) => image.revision(),
            Self::Gpu(image) => image.revision,
        }
    }
}

pub type ImageBuffer = ImageResource;
pub type ImageStorage = ImageResource;

fn next_revision() -> u64 {
    NEXT_REVISION.fetch_add(1, Ordering::Relaxed)
}

fn pixel_count(width: u32, height: u32) -> Result<usize, ImageError> {
    (width as usize)
        .checked_mul(height as usize)
        .ok_or(ImageError::DimensionsOverflow)
}

#[cfg(test)]
mod tests {
    use super::{Image, ImageError, Region};

    #[test]
    fn image_rejects_wrong_pixel_count() {
        let error = Image::from_pixels(2, 1, vec![[0.0; 4]]).unwrap_err();
        assert!(matches!(error, ImageError::PixelCountMismatch { .. }));
    }

    #[test]
    fn image_round_trips_pixels() {
        let image = Image::from_pixels(1, 1, vec![[1.0, 2.0, 3.0, 4.0]]).unwrap();
        assert_eq!(image.pixel(0, 0), Some([1.0, 2.0, 3.0, 4.0]));
        assert_eq!(image.pixel(1, 0), None);
    }

    #[test]
    fn image_views_preserve_the_source_revision() {
        let image = Image::new(2, 2).unwrap();
        let view = image.view(Region::new(0, 0, 1, 1)).unwrap();
        assert_eq!(view.revision(), image.revision());
    }

    #[test]
    fn image_views_share_the_source_backing_without_copying_pixels() {
        let image = Image::from_pixels(
            3,
            2,
            vec![
                [0.0, 0.0, 0.0, 1.0],
                [0.1, 0.2, 0.3, 1.0],
                [0.4, 0.5, 0.6, 1.0],
                [0.7, 0.8, 0.9, 1.0],
                [1.0, 0.9, 0.8, 1.0],
                [0.7, 0.6, 0.5, 1.0],
            ],
        )
        .unwrap();
        let view = image.view(Region::new(1, 0, 2, 2)).unwrap();

        assert_eq!(image.backing_ptr(), view.backing_ptr());
        assert_eq!(view.origin(), (1, 0));
        assert_eq!(view.stride(), 3);
    }
}
