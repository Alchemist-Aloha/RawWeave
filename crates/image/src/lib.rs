use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Pixel = [f32; 4];

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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Image {
    width: u32,
    height: u32,
    pixels: Vec<Pixel>,
}

impl Image {
    pub fn new(width: u32, height: u32) -> Result<Self, ImageError> {
        let count = pixel_count(width, height)?;
        Ok(Self {
            width,
            height,
            pixels: vec![[0.0; 4]; count],
        })
    }

    pub fn from_pixels(width: u32, height: u32, pixels: Vec<Pixel>) -> Result<Self, ImageError> {
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
            pixels,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[Pixel] {
        &self.pixels
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<Pixel> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.pixels
            .get(y as usize * self.width as usize + x as usize)
            .copied()
    }

    pub fn map_pixels(&self, mut map: impl FnMut(Pixel) -> Pixel) -> Self {
        Self {
            width: self.width,
            height: self.height,
            pixels: self.pixels.iter().copied().map(&mut map).collect(),
        }
    }
}

fn pixel_count(width: u32, height: u32) -> Result<usize, ImageError> {
    (width as usize)
        .checked_mul(height as usize)
        .ok_or(ImageError::DimensionsOverflow)
}

#[cfg(test)]
mod tests {
    use super::{Image, ImageError};

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
}
