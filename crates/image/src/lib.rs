use std::collections::BTreeMap;
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
    #[error("image origin {origin:?} overflows dimensions {dimensions:?}")]
    OriginOverflow {
        origin: (u32, u32),
        dimensions: Dimensions,
    },
    #[error("region {region:?} is outside image dimensions {dimensions:?}")]
    InvalidRegion {
        region: Region,
        dimensions: Dimensions,
    },
}

/// Errors raised while constructing or editing a spatial mask.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum MaskError {
    #[error("mask dimensions require {expected} values, got {actual}")]
    ValueCountMismatch { expected: usize, actual: usize },
    #[error("mask tile size must be greater than zero")]
    InvalidTileSize,
    #[error("mask origin {origin:?} overflows dimensions {dimensions:?}")]
    OriginOverflow {
        origin: (u32, u32),
        dimensions: Dimensions,
    },
    #[error("mask value must be finite")]
    NonFiniteValue,
    #[error("mask value must be between zero and one")]
    ValueOutOfRange,
    #[error("paint stroke must contain at least one point")]
    EmptyStroke,
    #[error("paint stroke parameter '{0}' is invalid")]
    InvalidStrokeParameter(&'static str),
    #[error("there is no stroke to undo")]
    NothingToUndo,
    #[error("there is no stroke to redo")]
    NothingToRedo,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaskTile {
    pub origin: (u32, u32),
    pub dimensions: Dimensions,
    pub values: Vec<f32>,
}

/// A first-class, immutable, single-channel spatial value.
///
/// Values are stored in row-major tiles while `origin` identifies the tile
/// collection in global image space.  The content hash is stable across
/// processes and is deliberately independent of the runtime revision.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mask {
    dimensions: Dimensions,
    origin: (u32, u32),
    tile_size: u32,
    tiles: Arc<Vec<MaskTile>>,
    revision: u64,
    cache_identity: u64,
}

impl PartialEq for Mask {
    fn eq(&self, other: &Self) -> bool {
        self.dimensions == other.dimensions
            && self.origin == other.origin
            && self.tile_size == other.tile_size
            && self.tiles == other.tiles
    }
}

impl Mask {
    pub const DEFAULT_TILE_SIZE: u32 = 64;

    pub fn new(dimensions: Dimensions, origin: (u32, u32)) -> Result<Self, MaskError> {
        Self::from_values_with_tile_size(
            dimensions,
            origin,
            vec![
                0.0;
                dimensions
                    .pixel_count()
                    .map_err(|_| MaskError::ValueCountMismatch {
                        expected: usize::MAX,
                        actual: 0,
                    })?
            ],
            Self::DEFAULT_TILE_SIZE,
        )
    }

    pub fn constant(
        dimensions: Dimensions,
        origin: (u32, u32),
        value: f32,
    ) -> Result<Self, MaskError> {
        validate_mask_value(value)?;
        let count = dimensions
            .pixel_count()
            .map_err(|_| MaskError::ValueCountMismatch {
                expected: usize::MAX,
                actual: 0,
            })?;
        Self::from_values_with_origin(dimensions, origin, vec![value; count])
    }

    pub fn from_values(dimensions: Dimensions, values: Vec<f32>) -> Result<Self, MaskError> {
        Self::from_values_with_origin(dimensions, (0, 0), values)
    }

    pub fn from_values_with_origin(
        dimensions: Dimensions,
        origin: (u32, u32),
        values: Vec<f32>,
    ) -> Result<Self, MaskError> {
        Self::from_values_with_tile_size(dimensions, origin, values, Self::DEFAULT_TILE_SIZE)
    }

    pub fn from_values_with_tile_size(
        dimensions: Dimensions,
        origin: (u32, u32),
        values: Vec<f32>,
        tile_size: u32,
    ) -> Result<Self, MaskError> {
        if tile_size == 0 {
            return Err(MaskError::InvalidTileSize);
        }
        let expected = dimensions
            .pixel_count()
            .map_err(|_| MaskError::ValueCountMismatch {
                expected: usize::MAX,
                actual: values.len(),
            })?;
        if values.len() != expected {
            return Err(MaskError::ValueCountMismatch {
                expected,
                actual: values.len(),
            });
        }
        if origin.0.checked_add(dimensions.width).is_none()
            || origin.1.checked_add(dimensions.height).is_none()
        {
            return Err(MaskError::OriginOverflow { origin, dimensions });
        }
        for value in &values {
            validate_mask_value(*value)?;
        }
        let tiles = tile_values(dimensions, origin, tile_size, &values);
        let cache_identity = mask_identity(dimensions, origin, tile_size, &values);
        Ok(Self {
            dimensions,
            origin,
            tile_size,
            tiles: Arc::new(tiles),
            revision: next_revision(),
            cache_identity,
        })
    }

    pub fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    pub fn width(&self) -> u32 {
        self.dimensions.width
    }

    pub fn height(&self) -> u32 {
        self.dimensions.height
    }

    pub fn origin(&self) -> (u32, u32) {
        self.origin
    }

    pub fn global_region(&self) -> Region {
        Region::new(self.origin.0, self.origin.1, self.width(), self.height())
    }

    pub fn tile_size(&self) -> u32 {
        self.tile_size
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn tiles(&self) -> &[MaskTile] {
        self.tiles.as_slice()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn cache_identity(&self) -> u64 {
        self.cache_identity
    }

    pub fn values(&self) -> Vec<f32> {
        let mut values = vec![0.0; self.dimensions.pixel_count().unwrap_or(0)];
        for tile in self.tiles.iter() {
            for y in 0..tile.dimensions.height {
                for x in 0..tile.dimensions.width {
                    let tile_index = y as usize * tile.dimensions.width as usize + x as usize;
                    let local_x = tile.origin.0 - self.origin.0 + x;
                    let local_y = tile.origin.1 - self.origin.1 + y;
                    let index = local_y as usize * self.width() as usize + local_x as usize;
                    if let (Some(destination), Some(source)) =
                        (values.get_mut(index), tile.values.get(tile_index))
                    {
                        *destination = *source;
                    }
                }
            }
        }
        values
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<f32> {
        if x >= self.width() || y >= self.height() {
            return None;
        }
        self.pixel_global(self.origin.0 + x, self.origin.1 + y)
    }

    pub fn pixel_global(&self, x: u32, y: u32) -> Option<f32> {
        if !self.global_region().contains(x, y) {
            return None;
        }
        let tile_x = (x - self.origin.0) / self.tile_size;
        let tile_y = (y - self.origin.1) / self.tile_size;
        let tiles_per_row = self.width().div_ceil(self.tile_size);
        let tile_index = tile_y as usize * tiles_per_row as usize + tile_x as usize;
        let tile = self.tiles.get(tile_index)?;
        let local_x = (x - self.origin.0) % self.tile_size;
        let local_y = (y - self.origin.1) % self.tile_size;
        tile.values
            .get(local_y as usize * tile.dimensions.width as usize + local_x as usize)
            .copied()
    }

    pub fn region(&self, region: Region) -> Option<Mask> {
        let region = region.intersection(self.global_region())?;
        let values = (0..region.height)
            .flat_map(|y| {
                (0..region.width)
                    .map(move |x| self.pixel_global(region.x + x, region.y + y).unwrap_or(0.0))
            })
            .collect();
        Mask::from_values_with_tile_size(
            region.dimensions(),
            (region.x, region.y),
            values,
            self.tile_size,
        )
        .ok()
    }

    pub fn map_values(&self, mut map: impl FnMut(f32) -> f32) -> Result<Mask, MaskError> {
        let values = self.values().into_iter().map(&mut map).collect::<Vec<_>>();
        Mask::from_values_with_tile_size(self.dimensions, self.origin, values, self.tile_size)
    }

    pub fn apply_stroke(&self, stroke: &PaintStroke) -> Result<Mask, MaskError> {
        let mut values = self.values();
        for y in 0..self.height() {
            for x in 0..self.width() {
                let global_x = self.origin.0 + x;
                let global_y = self.origin.1 + y;
                let influence = stroke.influence(global_x as f32, global_y as f32);
                if influence == 0.0 {
                    continue;
                }
                let index = y as usize * self.width() as usize + x as usize;
                values[index] = match stroke.mode {
                    PaintMode::Add => (values[index] + influence).min(1.0),
                    PaintMode::Subtract => (values[index] - influence).max(0.0),
                };
            }
        }
        Mask::from_values_with_tile_size(self.dimensions, self.origin, values, self.tile_size)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaintMode {
    Add,
    Subtract,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaintPoint {
    pub x: f32,
    pub y: f32,
}

impl PaintPoint {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaintStroke {
    pub points: Vec<PaintPoint>,
    pub size: f32,
    pub hardness: f32,
    pub opacity: f32,
    pub mode: PaintMode,
}

impl PaintStroke {
    pub fn new(
        points: Vec<PaintPoint>,
        size: f32,
        hardness: f32,
        opacity: f32,
        mode: PaintMode,
    ) -> Result<Self, MaskError> {
        if points.is_empty() {
            return Err(MaskError::EmptyStroke);
        }
        if !size.is_finite() || size <= 0.0 {
            return Err(MaskError::InvalidStrokeParameter("size"));
        }
        if !hardness.is_finite() || !(0.0..=1.0).contains(&hardness) {
            return Err(MaskError::InvalidStrokeParameter("hardness"));
        }
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(MaskError::InvalidStrokeParameter("opacity"));
        }
        if points
            .iter()
            .any(|point| !point.x.is_finite() || !point.y.is_finite())
        {
            return Err(MaskError::InvalidStrokeParameter("point"));
        }
        Ok(Self {
            points,
            size,
            hardness,
            opacity,
            mode,
        })
    }

    fn influence(&self, x: f32, y: f32) -> f32 {
        let distance = self
            .points
            .windows(2)
            .map(|segment| point_segment_distance(x, y, segment[0], segment[1]))
            .fold(f32::INFINITY, f32::min)
            .min(point_segment_distance(x, y, self.points[0], self.points[0]));
        let normalized = distance / (self.size * 0.5);
        if normalized >= 1.0 {
            0.0
        } else {
            let falloff = if normalized <= self.hardness {
                1.0
            } else if self.hardness >= 1.0 {
                0.0
            } else {
                (1.0 - normalized) / (1.0 - self.hardness)
            };
            self.opacity * falloff
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaintedMask {
    dimensions: Dimensions,
    origin: (u32, u32),
    tile_size: u32,
    strokes: Vec<PaintStroke>,
    redo: Vec<PaintStroke>,
}

impl PaintedMask {
    pub fn new(dimensions: Dimensions, origin: (u32, u32)) -> Result<Self, MaskError> {
        Ok(Self {
            dimensions,
            origin,
            tile_size: Mask::DEFAULT_TILE_SIZE,
            strokes: Vec::new(),
            redo: Vec::new(),
        })
    }

    pub fn apply(&mut self, stroke: PaintStroke) -> Result<(), MaskError> {
        self.strokes.push(stroke);
        self.redo.clear();
        Ok(())
    }

    pub fn undo(&mut self) -> Result<(), MaskError> {
        let stroke = self.strokes.pop().ok_or(MaskError::NothingToUndo)?;
        self.redo.push(stroke);
        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), MaskError> {
        let stroke = self.redo.pop().ok_or(MaskError::NothingToRedo)?;
        self.strokes.push(stroke);
        Ok(())
    }

    pub fn strokes(&self) -> &[PaintStroke] {
        &self.strokes
    }

    pub fn render(&self) -> Result<Mask, MaskError> {
        let mut mask = Mask::from_values_with_tile_size(
            self.dimensions,
            self.origin,
            vec![
                0.0;
                self.dimensions
                    .pixel_count()
                    .map_err(|_| MaskError::ValueCountMismatch {
                        expected: usize::MAX,
                        actual: 0,
                    })?
            ],
            self.tile_size,
        )?;
        for stroke in &self.strokes {
            mask = mask.apply_stroke(stroke)?;
        }
        Ok(mask)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MaskSet {
    masks: Vec<Mask>,
}

impl MaskSet {
    pub fn new(masks: Vec<Mask>) -> Self {
        Self { masks }
    }

    pub fn masks(&self) -> &[Mask] {
        &self.masks
    }

    pub fn len(&self) -> usize {
        self.masks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.masks.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Mask> {
        self.masks.get(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Mask> {
        self.masks.iter()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LabelMap {
    dimensions: Dimensions,
    origin: (u32, u32),
    values: Vec<u16>,
    #[serde(default)]
    labels: BTreeMap<String, u16>,
    revision: u64,
    cache_identity: u64,
}

impl LabelMap {
    pub fn from_values(
        dimensions: Dimensions,
        origin: (u32, u32),
        values: Vec<u16>,
    ) -> Result<Self, MaskError> {
        Self::from_values_with_labels(dimensions, origin, values, BTreeMap::new())
    }

    pub fn from_values_with_labels(
        dimensions: Dimensions,
        origin: (u32, u32),
        values: Vec<u16>,
        labels: BTreeMap<String, u16>,
    ) -> Result<Self, MaskError> {
        let expected = dimensions
            .pixel_count()
            .map_err(|_| MaskError::ValueCountMismatch {
                expected: usize::MAX,
                actual: values.len(),
            })?;
        if values.len() != expected {
            return Err(MaskError::ValueCountMismatch {
                expected,
                actual: values.len(),
            });
        }
        validate_spatial_origin(origin, dimensions)?;
        let cache_identity = integer_identity_with_labels(dimensions, origin, &values, &labels);
        Ok(Self {
            dimensions,
            origin,
            values,
            labels,
            revision: next_revision(),
            cache_identity,
        })
    }

    pub fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    pub fn origin(&self) -> (u32, u32) {
        self.origin
    }

    pub fn global_region(&self) -> Region {
        Region::new(
            self.origin.0,
            self.origin.1,
            self.dimensions.width,
            self.dimensions.height,
        )
    }

    pub fn values(&self) -> &[u16] {
        &self.values
    }

    pub fn labels(&self) -> &BTreeMap<String, u16> {
        &self.labels
    }

    pub fn label_value(&self, name: &str) -> Option<u16> {
        self.labels.get(name).copied()
    }

    pub fn label_name(&self, value: u16) -> Option<&str> {
        self.labels
            .iter()
            .find_map(|(name, candidate)| (*candidate == value).then_some(name.as_str()))
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<u16> {
        if x >= self.dimensions.width || y >= self.dimensions.height {
            return None;
        }
        self.values
            .get(y as usize * self.dimensions.width as usize + x as usize)
            .copied()
    }

    pub fn pixel_global(&self, x: u32, y: u32) -> Option<u16> {
        if !self.global_region().contains(x, y) {
            return None;
        }
        self.pixel(x - self.origin.0, y - self.origin.1)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn cache_identity(&self) -> u64 {
        self.cache_identity
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConfidenceMap {
    mask: Mask,
}

impl ConfidenceMap {
    pub fn from_mask(mask: Mask) -> Self {
        Self { mask }
    }

    pub fn mask(&self) -> &Mask {
        &self.mask
    }

    pub fn dimensions(&self) -> Dimensions {
        self.mask.dimensions()
    }

    pub fn origin(&self) -> (u32, u32) {
        self.mask.origin()
    }

    pub fn global_region(&self) -> Region {
        self.mask.global_region()
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<f32> {
        self.mask.pixel(x, y)
    }

    pub fn pixel_global(&self, x: u32, y: u32) -> Option<f32> {
        self.mask.pixel_global(x, y)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DepthMap {
    dimensions: Dimensions,
    origin: (u32, u32),
    values: Vec<f32>,
    revision: u64,
    cache_identity: u64,
}

impl DepthMap {
    pub fn from_values(
        dimensions: Dimensions,
        origin: (u32, u32),
        values: Vec<f32>,
    ) -> Result<Self, MaskError> {
        let expected = dimensions
            .pixel_count()
            .map_err(|_| MaskError::ValueCountMismatch {
                expected: usize::MAX,
                actual: values.len(),
            })?;
        if values.len() != expected {
            return Err(MaskError::ValueCountMismatch {
                expected,
                actual: values.len(),
            });
        }
        validate_spatial_origin(origin, dimensions)?;
        if values.iter().any(|value| !value.is_finite()) {
            return Err(MaskError::NonFiniteValue);
        }
        let cache_identity = float_identity(dimensions, origin, &values);
        Ok(Self {
            dimensions,
            origin,
            values,
            revision: next_revision(),
            cache_identity,
        })
    }

    pub fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    pub fn origin(&self) -> (u32, u32) {
        self.origin
    }

    pub fn global_region(&self) -> Region {
        Region::new(
            self.origin.0,
            self.origin.1,
            self.dimensions.width,
            self.dimensions.height,
        )
    }

    pub fn values(&self) -> &[f32] {
        &self.values
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<f32> {
        if x >= self.dimensions.width || y >= self.dimensions.height {
            return None;
        }
        self.values
            .get(y as usize * self.dimensions.width as usize + x as usize)
            .copied()
    }

    pub fn pixel_global(&self, x: u32, y: u32) -> Option<f32> {
        if !self.global_region().contains(x, y) {
            return None;
        }
        self.pixel(x - self.origin.0, y - self.origin.1)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn cache_identity(&self) -> u64 {
        self.cache_identity
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RegionSet {
    regions: Vec<Region>,
}

impl RegionSet {
    pub fn new(regions: Vec<Region>) -> Self {
        Self { regions }
    }

    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    pub fn len(&self) -> usize {
        self.regions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Region> {
        self.regions.get(index)
    }
}

fn validate_mask_value(value: f32) -> Result<(), MaskError> {
    if !value.is_finite() {
        return Err(MaskError::NonFiniteValue);
    }
    if !(0.0..=1.0).contains(&value) {
        return Err(MaskError::ValueOutOfRange);
    }
    Ok(())
}

fn tile_values(
    dimensions: Dimensions,
    origin: (u32, u32),
    tile_size: u32,
    values: &[f32],
) -> Vec<MaskTile> {
    let tiles_per_row = dimensions.width.div_ceil(tile_size);
    let tiles_per_column = dimensions.height.div_ceil(tile_size);
    let mut tiles =
        Vec::with_capacity((tiles_per_row as usize).saturating_mul(tiles_per_column as usize));
    for tile_y in 0..tiles_per_column {
        for tile_x in 0..tiles_per_row {
            let x = tile_x * tile_size;
            let y = tile_y * tile_size;
            let width = tile_size.min(dimensions.width.saturating_sub(x));
            let height = tile_size.min(dimensions.height.saturating_sub(y));
            let mut tile_values =
                Vec::with_capacity((width as usize).saturating_mul(height as usize));
            for local_y in 0..height {
                let start = (y + local_y) as usize * dimensions.width as usize + x as usize;
                tile_values.extend_from_slice(&values[start..start + width as usize]);
            }
            tiles.push(MaskTile {
                origin: (origin.0 + x, origin.1 + y),
                dimensions: Dimensions::new(width, height),
                values: tile_values,
            });
        }
    }
    tiles
}

fn mask_identity(
    dimensions: Dimensions,
    origin: (u32, u32),
    tile_size: u32,
    values: &[f32],
) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for value in [
        dimensions.width as u64,
        dimensions.height as u64,
        origin.0 as u64,
        origin.1 as u64,
        tile_size as u64,
    ] {
        hash ^= value;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    for value in values {
        hash ^= u64::from(value.to_bits());
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn integer_identity_with_labels(
    dimensions: Dimensions,
    origin: (u32, u32),
    values: &[u16],
    labels: &BTreeMap<String, u16>,
) -> u64 {
    let mut hash = mask_identity(dimensions, origin, 1, &[]);
    for (name, value) in labels {
        hash ^= u64::from(*value);
        hash = hash.wrapping_mul(0x100000001b3);
        for byte in name.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    for value in values {
        hash ^= u64::from(*value);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn validate_spatial_origin(origin: (u32, u32), dimensions: Dimensions) -> Result<(), MaskError> {
    if origin.0.checked_add(dimensions.width).is_none()
        || origin.1.checked_add(dimensions.height).is_none()
    {
        return Err(MaskError::OriginOverflow { origin, dimensions });
    }
    Ok(())
}

fn float_identity(dimensions: Dimensions, origin: (u32, u32), values: &[f32]) -> u64 {
    mask_identity(dimensions, origin, 1, values)
}

fn point_segment_distance(x: f32, y: f32, start: PaintPoint, end: PaintPoint) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx * dx + dy * dy;
    let parameter = if length_squared == 0.0 {
        0.0
    } else {
        ((x - start.x) * dx + (y - start.y) * dy) / length_squared
    };
    let parameter = parameter.clamp(0.0, 1.0);
    let closest_x = start.x + parameter * dx;
    let closest_y = start.y + parameter * dy;
    ((x - closest_x).powi(2) + (y - closest_y).powi(2)).sqrt()
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
        validate_origin(origin, dimensions)?;
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
        validate_origin(origin, dimensions)?;
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

fn validate_origin(origin: (u32, u32), dimensions: Dimensions) -> Result<(), ImageError> {
    if origin.0.checked_add(dimensions.width).is_none()
        || origin.1.checked_add(dimensions.height).is_none()
    {
        return Err(ImageError::OriginOverflow { origin, dimensions });
    }
    Ok(())
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
