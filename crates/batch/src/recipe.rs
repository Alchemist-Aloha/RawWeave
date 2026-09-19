use crate::model::{
    BatchItem, BitDepth, CollisionPolicy, ColorSpace, Compression, MetadataPolicy, OutputFormat,
    OutputRecipe, OutputSharpening, Resolution,
};
use crate::{BatchError, OutputRecord, sha256_file};
use image::ImageEncoder;
use png::{BitDepth as PngBitDepth, ColorType as PngColorType, Encoder as PngEncoder};
use rawweave_image::{ColorDomain, ColorMetadata, Image};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub fn write_output(
    source: &Image,
    recipe: &OutputRecipe,
    item: &BatchItem,
    index: usize,
    metadata: Option<&BTreeMap<String, String>>,
) -> Result<PathBuf, BatchError> {
    recipe.validate()?;
    let prepared = prepare_image(source, recipe)?;
    fs::create_dir_all(&recipe.destination).map_err(|source| BatchError::Io {
        operation: "create output directory",
        path: recipe.destination.clone(),
        source,
    })?;
    let requested = crate::model::rendered_output_path(recipe, item, index);
    let destination = resolve_collision(requested, recipe.collision_policy)?;
    if recipe.collision_policy == CollisionPolicy::Skip && destination.exists() {
        return Ok(destination);
    }
    let temporary = temporary_path(&destination);
    match encode_file(&prepared, recipe, &temporary) {
        Ok(()) => atomic_install(&temporary, &destination)?,
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
    }
    if recipe.metadata_policy == MetadataPolicy::Sidecar {
        write_sidecar(&destination, metadata)?;
    }
    Ok(destination)
}

fn resolve_collision(path: PathBuf, policy: CollisionPolicy) -> Result<PathBuf, BatchError> {
    if !path.exists() || policy == CollisionPolicy::Overwrite || policy == CollisionPolicy::Skip {
        return Ok(path);
    }
    if policy == CollisionPolicy::Error {
        return Err(BatchError::OutputCollision(path));
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("output");
    let extension = path.extension().and_then(|value| value.to_str());
    for suffix in 1_u32..=1_000_000 {
        let filename = match extension {
            Some(extension) => format!("{stem}-{suffix}.{extension}"),
            None => format!("{stem}-{suffix}"),
        };
        let candidate = parent.join(filename);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(BatchError::OutputCollision(path))
}

fn temporary_path(destination: &Path) -> PathBuf {
    let file_name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("output");
    destination.with_file_name(format!(".{file_name}.rawweave-{}.tmp", std::process::id()))
}

fn atomic_install(temporary: &Path, destination: &Path) -> Result<(), BatchError> {
    if destination.exists() {
        let _ = fs::remove_file(destination);
    }
    fs::rename(temporary, destination).map_err(|source| BatchError::Io {
        operation: "install output atomically",
        path: destination.to_path_buf(),
        source,
    })
}

fn encode_file(image: &Image, recipe: &OutputRecipe, path: &Path) -> Result<(), BatchError> {
    match recipe.format {
        OutputFormat::Jpeg => encode_jpeg(image, recipe, path),
        OutputFormat::Png => encode_png(image, recipe, path),
        OutputFormat::Tiff => encode_tiff(image, recipe, path),
        OutputFormat::OpenExr => encode_exr(image, path),
    }
}

fn encode_jpeg(image: &Image, recipe: &OutputRecipe, path: &Path) -> Result<(), BatchError> {
    if recipe.bit_depth != BitDepth::Eight {
        return Err(BatchError::UnsupportedBitDepth {
            format: OutputFormat::Jpeg,
            bit_depth: recipe.bit_depth,
        });
    }
    let bytes = rgba8(image);
    let rgb = bytes
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect::<Vec<_>>();
    let file = File::create(path).map_err(|source| BatchError::Io {
        operation: "create JPEG temporary",
        path: path.to_path_buf(),
        source,
    })?;
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(file, recipe.quality);
    encoder
        .write_image(
            &rgb,
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|error| BatchError::Processor(format!("JPEG encoding failed: {error}")))
}

fn encode_png(image: &Image, recipe: &OutputRecipe, path: &Path) -> Result<(), BatchError> {
    if recipe.bit_depth == BitDepth::Float32 {
        return Err(BatchError::UnsupportedBitDepth {
            format: OutputFormat::Png,
            bit_depth: recipe.bit_depth,
        });
    }
    let file = File::create(path).map_err(|source| BatchError::Io {
        operation: "create PNG temporary",
        path: path.to_path_buf(),
        source,
    })?;
    let mut encoder = PngEncoder::new(file, image.width(), image.height());
    encoder.set_color(PngColorType::Rgba);
    encoder.set_depth(match recipe.bit_depth {
        BitDepth::Eight => PngBitDepth::Eight,
        BitDepth::Sixteen => PngBitDepth::Sixteen,
        BitDepth::Float32 => unreachable!(),
    });
    encoder.set_compression(match recipe.compression {
        Compression::Fast => png::Compression::Fast,
        Compression::Best => png::Compression::Best,
        Compression::Default | Compression::Lossless => png::Compression::Default,
    });
    let bytes = match recipe.bit_depth {
        BitDepth::Eight => rgba8(image),
        BitDepth::Sixteen => rgba16(image),
        BitDepth::Float32 => unreachable!(),
    };
    let mut writer = encoder
        .write_header()
        .map_err(|error| BatchError::Processor(format!("PNG header failed: {error}")))?;
    writer
        .write_image_data(&bytes)
        .map_err(|error| BatchError::Processor(format!("PNG encoding failed: {error}")))
}

fn encode_tiff(image: &Image, recipe: &OutputRecipe, path: &Path) -> Result<(), BatchError> {
    let mut file = File::create(path).map_err(|source| BatchError::Io {
        operation: "create TIFF temporary",
        path: path.to_path_buf(),
        source,
    })?;
    let encoder = image::codecs::tiff::TiffEncoder::new(&mut file);
    let result = match recipe.bit_depth {
        BitDepth::Eight => encoder.write_image(
            &rgba8(image),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        ),
        BitDepth::Sixteen => encoder.write_image(
            &rgba16(image),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba16,
        ),
        BitDepth::Float32 => encoder.write_image(
            &rgba32(image),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba32F,
        ),
    };
    result.map_err(|error| BatchError::Processor(format!("TIFF encoding failed: {error}")))
}

fn encode_exr(image: &Image, path: &Path) -> Result<(), BatchError> {
    let pixels = image.pixels().to_vec();
    exr::prelude::write_rgba_file(
        path,
        image.width() as usize,
        image.height() as usize,
        move |x, y| {
            let index = y.saturating_mul(image.width() as usize).saturating_add(x);
            let pixel = pixels.get(index).copied().unwrap_or([0.0; 4]);
            (pixel[0], pixel[1], pixel[2], pixel[3])
        },
    )
    .map_err(|error| BatchError::Processor(format!("OpenEXR encoding failed: {error}")))
}

fn prepare_image(source: &Image, recipe: &OutputRecipe) -> Result<Image, BatchError> {
    if recipe.icc_profile.is_some() {
        return Err(BatchError::UnsupportedColorTransform(
            "ICC/LittleCMS output profiles are not linked in this build".to_owned(),
        ));
    }
    if recipe.ocio_transform.is_some() {
        return Err(BatchError::UnsupportedColorTransform(
            "OCIO output transforms are not linked in this build".to_owned(),
        ));
    }
    let converted = match &recipe.color_space {
        ColorSpace::LinearSrgb => convert_domain(source, ColorDomain::LinearSrgb, false),
        ColorSpace::Srgb => convert_domain(source, ColorDomain::Srgb, true),
        ColorSpace::DisplayP3 => {
            return Err(BatchError::UnsupportedColorTransform(
                "Display-P3 transform requires an ICC or OCIO provider".to_owned(),
            ));
        }
        ColorSpace::Named(name) => {
            return Err(BatchError::UnsupportedColorTransform(name.clone()));
        }
    }?;
    let resized = resize(&converted, recipe.resolution)?;
    sharpen(&resized, recipe.sharpening)
}

fn convert_domain(source: &Image, target: ColorDomain, encode: bool) -> Result<Image, BatchError> {
    if source.color_domain() == target {
        return Ok(source.clone());
    }
    let pixels = source
        .pixels()
        .iter()
        .copied()
        .map(|mut pixel| {
            for channel in &mut pixel[..3] {
                *channel = if encode {
                    linear_to_srgb(*channel)
                } else {
                    srgb_to_linear(*channel)
                };
            }
            pixel
        })
        .collect();
    Image::from_pixels_with_origin(
        source.dimensions(),
        source.origin(),
        pixels,
        source.pixel_format(),
        ColorMetadata {
            domain: target,
            alpha_is_premultiplied: source.color_metadata().alpha_is_premultiplied,
        },
    )
    .map_err(|error| BatchError::Processor(format!("color conversion failed: {error}")))
}

fn linear_to_srgb(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    let value = value.max(0.0);
    if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_to_linear(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    let value = value.clamp(0.0, 1.0);
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn resize(source: &Image, resolution: Resolution) -> Result<Image, BatchError> {
    let (width, height) = match resolution {
        Resolution::Original => return Ok(source.clone()),
        Resolution::Exact { width, height } => (width, height),
        Resolution::LongEdge(edge) => {
            if source.width() >= source.height() {
                let height = ((u64::from(source.height()) * u64::from(edge))
                    / u64::from(source.width()))
                .max(1);
                (
                    edge,
                    u32::try_from(height).map_err(|_| {
                        BatchError::InvalidRecipe("resize height overflow".to_owned())
                    })?,
                )
            } else {
                let width = ((u64::from(source.width()) * u64::from(edge))
                    / u64::from(source.height()))
                .max(1);
                (
                    u32::try_from(width).map_err(|_| {
                        BatchError::InvalidRecipe("resize width overflow".to_owned())
                    })?,
                    edge,
                )
            }
        }
    };
    if width == 0 || height == 0 {
        return Err(BatchError::InvalidRecipe(
            "resize dimensions must be non-zero".to_owned(),
        ));
    }
    let mut pixels = Vec::with_capacity(
        (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| BatchError::InvalidRecipe("resize dimensions overflow".to_owned()))?,
    );
    for y in 0..height {
        let source_y = ((u64::from(y) * u64::from(source.height())) / u64::from(height)) as u32;
        for x in 0..width {
            let source_x = ((u64::from(x) * u64::from(source.width())) / u64::from(width)) as u32;
            pixels.push(
                source
                    .pixel(
                        source_x.min(source.width().saturating_sub(1)),
                        source_y.min(source.height().saturating_sub(1)),
                    )
                    .unwrap_or([0.0; 4]),
            );
        }
    }
    Image::from_pixels_with_metadata(
        width,
        height,
        pixels,
        source.pixel_format(),
        source.color_domain(),
    )
    .map_err(|error| BatchError::Processor(format!("resize failed: {error}")))
}

fn sharpen(source: &Image, sharpening: OutputSharpening) -> Result<Image, BatchError> {
    let OutputSharpening::UnsharpMask {
        radius,
        amount,
        threshold,
    } = sharpening
    else {
        return Ok(source.clone());
    };
    if !amount.is_finite() || !threshold.is_finite() || amount < 0.0 || threshold < 0.0 {
        return Err(BatchError::InvalidRecipe(
            "sharpening parameters must be finite and non-negative".to_owned(),
        ));
    }
    let radius = radius.max(1);
    let mut pixels = Vec::with_capacity(source.pixels().len());
    for y in 0..source.height() {
        for x in 0..source.width() {
            let current = source.pixel(x, y).unwrap_or([0.0; 4]);
            let mut average = [0.0; 3];
            let mut count: f32 = 0.0;
            let r = radius as i64;
            for oy in -r..=r {
                for ox in -r..=r {
                    let sx = i64::from(x) + ox;
                    let sy = i64::from(y) + oy;
                    if sx >= 0
                        && sy >= 0
                        && sx < i64::from(source.width())
                        && sy < i64::from(source.height())
                    {
                        let pixel = source.pixel(sx as u32, sy as u32).unwrap_or([0.0; 4]);
                        for channel in 0..3 {
                            average[channel] += pixel[channel];
                        }
                        count += 1.0;
                    }
                }
            }
            let mut output = current;
            for channel in 0..3 {
                let blur = average[channel] / count.max(1.0);
                let detail = current[channel] - blur;
                output[channel] = if detail.abs() >= threshold {
                    current[channel] + amount * detail
                } else {
                    current[channel]
                };
            }
            pixels.push(output);
        }
    }
    Image::from_pixels_with_metadata(
        source.width(),
        source.height(),
        pixels,
        source.pixel_format(),
        source.color_domain(),
    )
    .map_err(|error| BatchError::Processor(format!("sharpening failed: {error}")))
}

fn rgba8(image: &Image) -> Vec<u8> {
    image
        .pixels()
        .iter()
        .flat_map(|pixel| pixel.map(|value| quantize(value, 255.0) as u8))
        .collect()
}

fn rgba16(image: &Image) -> Vec<u8> {
    image
        .pixels()
        .iter()
        .flat_map(|pixel| {
            pixel
                .iter()
                .flat_map(|value| (quantize(*value, 65_535.0) as u16).to_be_bytes())
        })
        .collect()
}

fn rgba32(image: &Image) -> Vec<u8> {
    image
        .pixels()
        .iter()
        .flat_map(|pixel| pixel.iter().flat_map(|value| value.to_ne_bytes()))
        .collect()
}

fn quantize(value: f32, scale: f32) -> u32 {
    if !value.is_finite() {
        return 0;
    }
    (value.clamp(0.0, 1.0) * scale).round() as u32
}

#[derive(Serialize)]
struct Sidecar<'a> {
    source: &'a Path,
    metadata: Option<&'a BTreeMap<String, String>>,
}

fn write_sidecar(
    path: &Path,
    metadata: Option<&BTreeMap<String, String>>,
) -> Result<(), BatchError> {
    let sidecar = path.with_extension("json");
    let bytes = serde_json::to_vec_pretty(&Sidecar {
        source: path,
        metadata,
    })?;
    fs::write(&sidecar, bytes).map_err(|source| BatchError::Io {
        operation: "write output metadata sidecar",
        path: sidecar,
        source,
    })
}

pub(crate) fn record_for_path(path: PathBuf) -> Result<OutputRecord, BatchError> {
    let metadata = fs::metadata(&path).map_err(|source| BatchError::Io {
        operation: "inspect written output",
        path: path.clone(),
        source,
    })?;
    Ok(OutputRecord {
        sha256: sha256_file(&path)?,
        byte_len: metadata.len(),
        path,
    })
}
