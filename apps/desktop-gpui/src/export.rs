//! Single-image recipe controls backed by the shared batch encoders.
use rawweave_batch::{
    BitDepth, ColorSpace, Compression, MetadataPolicy, OutputFormat, OutputRecipe,
    OutputSharpening, Resolution,
};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct ExportSettings {
    pub quality: u8,
    pub long_edge: Option<u32>,
    pub png_sixteen: bool,
    pub compression: Compression,
    pub sharpening: OutputSharpening,
}
impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            quality: 92,
            long_edge: None,
            png_sixteen: false,
            compression: Compression::Default,
            sharpening: OutputSharpening::None,
        }
    }
}
impl ExportSettings {
    pub fn from_fields(quality: &str, long_edge: &str) -> Result<Self, String> {
        let quality = quality
            .trim()
            .parse::<u8>()
            .map_err(|_| "JPEG quality must be an integer from 1 to 100")?;
        if !(1..=100).contains(&quality) {
            return Err("JPEG quality must be from 1 to 100".into());
        }
        let long_edge = if long_edge.trim().is_empty() {
            None
        } else {
            let edge = long_edge.trim().parse::<u32>().map_err(
                |_| "Long edge must be an integer from 1 to 8192, or blank for original",
            )?;
            if !(1..=8192).contains(&edge) {
                return Err("Long edge must be from 1 to 8192, or blank for original".into());
            }
            Some(edge)
        };
        Ok(Self {
            quality,
            long_edge,
            ..Self::default()
        })
    }
    pub fn recipe(&self, path: &Path) -> Result<OutputRecipe, String> {
        // Bound custom resizing even for programmatic callers; 8192² fits the shared pixel budget.
        if self
            .long_edge
            .is_some_and(|edge| !(1..=8192).contains(&edge))
        {
            return Err("Long edge must be from 1 to 8192, or blank for original".into());
        }
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let format = match extension.as_str() {
            "jpg" | "jpeg" => OutputFormat::Jpeg,
            "png" => OutputFormat::Png,
            "tif" | "tiff" => OutputFormat::Tiff,
            "exr" => OutputFormat::OpenExr,
            _ => return Err("Use a .png, .jpg, .tif or .exr filename".into()),
        };
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut recipe = OutputRecipe::new(format, parent).with_filename_template("export");
        recipe.resolution = self
            .long_edge
            .map_or(Resolution::Original, Resolution::LongEdge);
        recipe.quality = self.quality;
        recipe.compression = self.compression;
        recipe.sharpening = self.sharpening;
        recipe.metadata_policy = MetadataPolicy::Strip;
        recipe.bit_depth = match format {
            OutputFormat::OpenExr => BitDepth::Float32,
            OutputFormat::Tiff => BitDepth::Sixteen,
            OutputFormat::Png if self.png_sixteen => BitDepth::Sixteen,
            _ => BitDepth::Eight,
        };
        if format == OutputFormat::OpenExr {
            recipe.color_space = ColorSpace::LinearSrgb;
        }
        recipe.validate().map_err(|e| e.to_string())?;
        Ok(recipe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recipes_follow_filename_format_and_keep_linear_exr() {
        let mut settings = ExportSettings::from_fields("85", "1600").unwrap();
        settings.png_sixteen = true;
        settings.compression = Compression::Best;
        settings.sharpening = OutputSharpening::UnsharpMask {
            radius: 1,
            amount: 0.3,
            threshold: 0.01,
        };
        for (extension, format, depth, space) in [
            (
                "PNG",
                OutputFormat::Png,
                BitDepth::Sixteen,
                ColorSpace::Srgb,
            ),
            (
                "jpeg",
                OutputFormat::Jpeg,
                BitDepth::Eight,
                ColorSpace::Srgb,
            ),
            (
                "tiff",
                OutputFormat::Tiff,
                BitDepth::Sixteen,
                ColorSpace::Srgb,
            ),
            (
                "exr",
                OutputFormat::OpenExr,
                BitDepth::Float32,
                ColorSpace::LinearSrgb,
            ),
        ] {
            let recipe = settings
                .recipe(Path::new(&format!("image.{extension}")))
                .unwrap();
            assert_eq!(
                (recipe.format, recipe.bit_depth, recipe.color_space),
                (format, depth, space)
            );
            assert_eq!(recipe.quality, 85);
            assert_eq!(recipe.resolution, Resolution::LongEdge(1600));
            assert_eq!(recipe.metadata_policy, MetadataPolicy::Strip);
            assert_eq!(recipe.compression, Compression::Best);
            assert_eq!(recipe.sharpening, settings.sharpening);
        }
        assert_eq!(
            ExportSettings::default()
                .recipe(Path::new("image.png"))
                .unwrap()
                .resolution,
            Resolution::Original
        );
        assert!(settings.recipe(Path::new("image.bmp")).is_err());
    }
    #[test]
    fn recipe_fields_reject_nonfinite_zero_and_unbounded_allocations() {
        for (quality, edge) in [
            ("0", ""),
            ("101", ""),
            ("NaN", ""),
            ("92", "0"),
            ("92", "8193"),
            ("92", "1e9"),
            ("92", "-1"),
        ] {
            assert!(ExportSettings::from_fields(quality, edge).is_err());
        }
        assert!(ExportSettings::from_fields(" 100 ", " 8192 ").is_ok());
        let settings = ExportSettings {
            quality: 0,
            ..ExportSettings::default()
        };
        assert!(settings.recipe(Path::new("image.jpg")).is_err());
        let settings = ExportSettings {
            long_edge: Some(u32::MAX),
            ..ExportSettings::default()
        };
        assert!(settings.recipe(Path::new("image.png")).is_err());
    }
}
