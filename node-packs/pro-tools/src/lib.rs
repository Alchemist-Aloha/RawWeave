//! First-party professional photographic tools built on the public node API.
//!
//! The pack deliberately operates on `core.Image` values and keeps the graph
//! contract free of private shortcuts. All algorithms are deterministic,
//! bounded, and preserve the source image's global origin and metadata.

#[cfg(test)]
use std::cell::Cell;

use rawweave_image::{Image, Mask, Region};
use rawweave_node_api::{
    EvaluationContext, ExecutionCapability, Inputs, NodeDescriptor, NodeError, NodeInstance,
    NodePack, NodeRegistry, NodeResult, ParameterDescriptor, ParameterValue, Parameters,
    PortDescriptor, RegistryError, Value,
};

/// Pixel budget for the region one node evaluates in a single pass.
///
/// The same ceiling the source decoders and the image-set aggregate use: a node
/// that cannot take a high-resolution frame cannot preview the photographs those
/// paths already accept.
const MAX_IMAGE_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_RADIUS: u32 = 64;
const MAX_LUT_POINTS: usize = 4096;
const MAX_LUT_BYTES: usize = 64 * 1024;
const EPSILON: f32 = 1.0e-6;

fn cpu_region_capabilities(descriptor: &mut NodeDescriptor) {
    descriptor.capabilities = vec![ExecutionCapability::Cpu, ExecutionCapability::RegionAware];
}

fn cpu_full_frame_capabilities(descriptor: &mut NodeDescriptor) {
    descriptor.capabilities = vec![
        ExecutionCapability::Cpu,
        ExecutionCapability::FullFrame,
        ExecutionCapability::RegionAware,
    ];
}

fn image_descriptor(
    type_id: &str,
    name: &str,
    parameters: Vec<ParameterDescriptor>,
    full_frame: bool,
) -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(type_id, name);
    descriptor
        .inputs
        .push(PortDescriptor::input("image", "Image", "core.Image", true));
    descriptor
        .outputs
        .push(PortDescriptor::output("image", "Image", "core.Image"));
    descriptor.parameters = parameters;
    if full_frame {
        cpu_full_frame_capabilities(&mut descriptor);
    } else {
        cpu_region_capabilities(&mut descriptor);
    }
    descriptor
}

fn output_descriptor(
    type_id: &str,
    name: &str,
    outputs: &[(&str, &str, &str)],
    parameters: Vec<ParameterDescriptor>,
    full_frame: bool,
) -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(type_id, name);
    descriptor
        .inputs
        .push(PortDescriptor::input("image", "Image", "core.Image", true));
    descriptor.outputs = outputs
        .iter()
        .map(|(id, name, data_type)| PortDescriptor::output(*id, *name, *data_type))
        .collect();
    descriptor.parameters = parameters;
    if full_frame {
        cpu_full_frame_capabilities(&mut descriptor);
    } else {
        cpu_region_capabilities(&mut descriptor);
    }
    descriptor
}

fn float_parameter(
    id: &'static str,
    name: &'static str,
    default: f32,
    min: Option<f32>,
    max: Option<f32>,
) -> ParameterDescriptor {
    ParameterDescriptor::float(id, name, default, min, max)
}

fn integer_parameter(id: &'static str, name: &'static str, default: i64) -> ParameterDescriptor {
    ParameterDescriptor::integer(id, name, default)
}

fn boolean_parameter(id: &'static str, name: &'static str, default: bool) -> ParameterDescriptor {
    ParameterDescriptor::boolean(id, name, default)
}

fn string_parameter(
    id: &'static str,
    name: &'static str,
    default: &'static str,
) -> ParameterDescriptor {
    ParameterDescriptor::string(id, name, default)
}

fn radius_parameter(default: i64, max: i64) -> ParameterDescriptor {
    integer_parameter("radius", "Radius", default).with_bounds(Some(1.0), Some(max as f32))
}

trait ParameterDescriptorBounds {
    fn with_bounds(self, min: Option<f32>, max: Option<f32>) -> Self;
}

impl ParameterDescriptorBounds for ParameterDescriptor {
    fn with_bounds(mut self, min: Option<f32>, max: Option<f32>) -> Self {
        self.min = min;
        self.max = max;
        self
    }
}

fn shared_radius_parameters(default: i64, max: i64) -> Vec<ParameterDescriptor> {
    vec![radius_parameter(default, max)]
}

fn descriptors_for_aliases(
    aliases: &[(&'static str, &'static str)],
    parameters: Vec<ParameterDescriptor>,
    full_frame: bool,
) -> Vec<NodeDescriptor> {
    aliases
        .iter()
        .map(|(type_id, name)| image_descriptor(type_id, name, parameters.clone(), full_frame))
        .collect()
}

fn image_input(inputs: &Inputs) -> Result<Image, NodeError> {
    match inputs.get("image") {
        Some(Value::Image(image)) => Ok(image.clone()),
        Some(_) => Err(NodeError::InvalidParameter("image".to_owned())),
        None => Err(NodeError::MissingInput("image".to_owned())),
    }
}

fn finite_float(parameters: &Parameters, id: &str, default: f32) -> Result<f32, NodeError> {
    let value = match parameters.get(id) {
        None => default,
        Some(ParameterValue::Float(value)) => *value,
        Some(ParameterValue::Integer(value)) => *value as f32,
        Some(_) => return Err(NodeError::InvalidParameter(id.to_owned())),
    };
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| NodeError::InvalidParameter(id.to_owned()))
}

fn bounded_float(
    parameters: &Parameters,
    id: &str,
    default: f32,
    min: f32,
    max: f32,
) -> Result<f32, NodeError> {
    let value = finite_float(parameters, id, default)?;
    if !(min..=max).contains(&value) {
        return Err(NodeError::InvalidParameter(id.to_owned()));
    }
    Ok(value)
}

fn integer_parameter_value(
    parameters: &Parameters,
    id: &str,
    default: u32,
    max: u32,
) -> Result<u32, NodeError> {
    let value = match parameters.get(id) {
        None => i64::from(default),
        Some(ParameterValue::Integer(value)) => *value,
        Some(ParameterValue::Float(value)) if value.is_finite() && value.fract() == 0.0 => {
            if *value < 0.0 || *value > u32::MAX as f32 {
                return Err(NodeError::InvalidParameter(id.to_owned()));
            }
            *value as i64
        }
        Some(_) => return Err(NodeError::InvalidParameter(id.to_owned())),
    };
    if value < 0 || value > i64::from(max) {
        return Err(NodeError::InvalidParameter(id.to_owned()));
    }
    Ok(value as u32)
}

fn boolean_parameter_value(
    parameters: &Parameters,
    id: &str,
    default: bool,
) -> Result<bool, NodeError> {
    match parameters.get(id) {
        None => Ok(default),
        Some(ParameterValue::Boolean(value)) => Ok(*value),
        Some(_) => Err(NodeError::InvalidParameter(id.to_owned())),
    }
}

fn string_parameter_value<'a>(
    parameters: &'a Parameters,
    id: &str,
    default: &'a str,
) -> Result<&'a str, NodeError> {
    match parameters.get(id) {
        None => Ok(default),
        Some(ParameterValue::String(value)) => Ok(value.as_str()),
        Some(_) => Err(NodeError::InvalidParameter(id.to_owned())),
    }
}

fn source_region(image: &Image, context: &EvaluationContext) -> Region {
    match context.requested_region() {
        Some(requested) => requested
            .intersection(image.global_region())
            .unwrap_or_else(|| Region::new(requested.x, requested.y, 0, 0)),
        None => image.global_region(),
    }
}

fn checked_pixel_count(region: Region) -> Result<usize, NodeError> {
    let count = u64::from(region.width)
        .checked_mul(u64::from(region.height))
        .ok_or_else(|| NodeError::InvalidParameter("dimensions".to_owned()))?;
    if count > MAX_IMAGE_PIXELS {
        return Err(NodeError::Message(format!(
            "region {}x{} holds {count} pixels, exceeding the {MAX_IMAGE_PIXELS}-pixel node budget",
            region.width, region.height
        )));
    }
    usize::try_from(count).map_err(|_| NodeError::InvalidParameter("dimensions".to_owned()))
}

fn from_region(source: &Image, region: Region, pixels: Vec<[f32; 4]>) -> Result<Image, NodeError> {
    checked_pixel_count(region)?;
    Image::from_pixels_with_origin(
        region.dimensions(),
        (region.x, region.y),
        pixels,
        source.pixel_format(),
        source.color_metadata(),
    )
    .map_err(|error| NodeError::Message(error.to_string()))
}

fn map_region(
    source: &Image,
    context: &EvaluationContext,
    mut map: impl FnMut(u32, u32, [f32; 4]) -> [f32; 4],
) -> Result<Image, NodeError> {
    let region = source_region(source, context);
    let mut pixels = Vec::with_capacity(checked_pixel_count(region)?);
    for y in 0..region.height {
        for x in 0..region.width {
            let global_x = region.x + x;
            let global_y = region.y + y;
            let pixel = source.pixel_global(global_x, global_y).ok_or_else(|| {
                NodeError::Message("requested region was outside the image".to_owned())
            })?;
            pixels.push(map(global_x, global_y, pixel));
        }
    }
    from_region(source, region, pixels)
}

fn sample_nearest(image: &Image, x: i64, y: i64) -> [f32; 4] {
    let bounds = image.global_region();
    if bounds.width == 0 || bounds.height == 0 {
        return [0.0; 4];
    }
    let max_x = i64::from(bounds.end_x().unwrap_or(u32::MAX).saturating_sub(1));
    let max_y = i64::from(bounds.end_y().unwrap_or(u32::MAX).saturating_sub(1));
    let x = x.clamp(i64::from(bounds.x), max_x) as u32;
    let y = y.clamp(i64::from(bounds.y), max_y) as u32;
    image.pixel_global(x, y).unwrap_or([0.0; 4])
}

fn sample_bilinear(image: &Image, x: f32, y: f32) -> [f32; 4] {
    let bounds = image.global_region();
    if bounds.width == 0 || bounds.height == 0 || !x.is_finite() || !y.is_finite() {
        return [0.0; 4];
    }
    let max_x = bounds.end_x().unwrap_or(u32::MAX).saturating_sub(1) as f32;
    let max_y = bounds.end_y().unwrap_or(u32::MAX).saturating_sub(1) as f32;
    let x = x.clamp(bounds.x as f32, max_x);
    let y = y.clamp(bounds.y as f32, max_y);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = x0.saturating_add(1).min(max_x as u32);
    let y1 = y0.saturating_add(1).min(max_y as u32);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let p00 = image.pixel_global(x0, y0).unwrap_or([0.0; 4]);
    let p10 = image.pixel_global(x1, y0).unwrap_or(p00);
    let p01 = image.pixel_global(x0, y1).unwrap_or(p00);
    let p11 = image.pixel_global(x1, y1).unwrap_or(p01);
    std::array::from_fn(|channel| {
        let top = p00[channel] + (p10[channel] - p00[channel]) * tx;
        let bottom = p01[channel] + (p11[channel] - p01[channel]) * tx;
        top + (bottom - top) * ty
    })
}

#[derive(Clone, Copy, Debug, Default)]
struct AxisSegment {
    start: usize,
    end: usize,
    repetitions: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct AxisSegments {
    values: [AxisSegment; 3],
    len: usize,
}

impl AxisSegments {
    fn for_clamped_range(start: i64, end: i64, origin: u32, length: u32) -> Self {
        let mut segments = Self::default();
        if length == 0 {
            return segments;
        }

        let origin = i64::from(origin);
        let image_end = origin + i64::from(length);
        let left_end = end.min(origin);
        segments.push(0, 1, left_end - start);

        let interior_start = start.max(origin);
        let interior_end = end.min(image_end);
        segments.push(
            interior_start - origin,
            interior_end - origin,
            if interior_end > interior_start { 1 } else { 0 },
        );

        let right_start = start.max(image_end);
        segments.push(i64::from(length) - 1, i64::from(length), end - right_start);
        segments
    }

    fn push(&mut self, start: i64, end: i64, repetitions: i64) {
        if start >= end || repetitions <= 0 {
            return;
        }
        let (Some(start), Some(end), Some(repetitions)) = (
            usize::try_from(start).ok(),
            usize::try_from(end).ok(),
            usize::try_from(repetitions).ok(),
        ) else {
            return;
        };
        if let Some(slot) = self.values.get_mut(self.len) {
            *slot = AxisSegment {
                start,
                end,
                repetitions,
            };
            self.len += 1;
        }
    }

    fn iter(&self) -> impl Iterator<Item = &AxisSegment> {
        self.values.iter().take(self.len)
    }
}

/// A four-channel summed-area table for bounded, edge-clamped neighborhoods.
///
/// The table stores prefix sums for the image's local coordinates. Queries are
/// split into at most three source ranges per axis so samples outside the image
/// repeat the nearest edge pixel exactly as `sample_nearest` does.
struct IntegralImage {
    width: u32,
    height: u32,
    origin: (u32, u32),
    stride: usize,
    sums: Vec<[f32; 4]>,
    #[cfg(test)]
    build_work_units: usize,
    #[cfg(test)]
    query_work_units: Cell<usize>,
}

impl IntegralImage {
    fn new(image: &Image) -> Result<Self, NodeError> {
        checked_pixel_count(image.global_region())?;
        let width = image.width();
        let height = image.height();
        let stride = usize::try_from(width)
            .ok()
            .and_then(|width| width.checked_add(1))
            .ok_or_else(|| NodeError::InvalidParameter("dimensions".to_owned()))?;
        let rows = usize::try_from(height)
            .ok()
            .and_then(|height| height.checked_add(1))
            .ok_or_else(|| NodeError::InvalidParameter("dimensions".to_owned()))?;
        let sum_count = stride
            .checked_mul(rows)
            .ok_or_else(|| NodeError::InvalidParameter("dimensions".to_owned()))?;
        let mut sums = Vec::new();
        sums.try_reserve_exact(sum_count)
            .map_err(|_| NodeError::InvalidParameter("dimensions".to_owned()))?;
        sums.resize(sum_count, [0.0; 4]);

        for y in 0..usize::try_from(height).unwrap_or(0) {
            for x in 0..usize::try_from(width).unwrap_or(0) {
                let source_index = y
                    .checked_mul(usize::try_from(width).unwrap_or(0))
                    .and_then(|row| row.checked_add(x))
                    .ok_or_else(|| NodeError::InvalidParameter("dimensions".to_owned()))?;
                let prefix_index = (y + 1)
                    .checked_mul(stride)
                    .and_then(|row| row.checked_add(x + 1))
                    .ok_or_else(|| NodeError::InvalidParameter("dimensions".to_owned()))?;
                let above_index = y
                    .checked_mul(stride)
                    .and_then(|row| row.checked_add(x + 1))
                    .ok_or_else(|| NodeError::InvalidParameter("dimensions".to_owned()))?;
                let left_index = (y + 1)
                    .checked_mul(stride)
                    .and_then(|row| row.checked_add(x))
                    .ok_or_else(|| NodeError::InvalidParameter("dimensions".to_owned()))?;
                let diagonal_index = y
                    .checked_mul(stride)
                    .and_then(|row| row.checked_add(x))
                    .ok_or_else(|| NodeError::InvalidParameter("dimensions".to_owned()))?;
                let Some(&pixel) = image.pixels().get(source_index) else {
                    return Err(NodeError::Message("image pixels are incomplete".to_owned()));
                };
                let Some(&above) = sums.get(above_index) else {
                    return Err(NodeError::Message(
                        "integral image dimensions overflowed".to_owned(),
                    ));
                };
                let Some(&left) = sums.get(left_index) else {
                    return Err(NodeError::Message(
                        "integral image dimensions overflowed".to_owned(),
                    ));
                };
                let Some(&diagonal) = sums.get(diagonal_index) else {
                    return Err(NodeError::Message(
                        "integral image dimensions overflowed".to_owned(),
                    ));
                };
                let Some(slot) = sums.get_mut(prefix_index) else {
                    return Err(NodeError::Message(
                        "integral image dimensions overflowed".to_owned(),
                    ));
                };
                *slot = std::array::from_fn(|channel| {
                    pixel[channel] + above[channel] + left[channel] - diagonal[channel]
                });
            }
        }

        Ok(Self {
            width,
            height,
            origin: image.origin(),
            stride,
            sums,
            #[cfg(test)]
            build_work_units: usize::try_from(width)
                .ok()
                .and_then(|width| {
                    usize::try_from(height)
                        .ok()
                        .and_then(|height| width.checked_mul(height))
                })
                .unwrap_or(0),
            #[cfg(test)]
            query_work_units: Cell::new(0),
        })
    }

    fn rectangle_sum(
        &self,
        x_start: usize,
        x_end: usize,
        y_start: usize,
        y_end: usize,
    ) -> [f32; 4] {
        #[cfg(test)]
        self.query_work_units
            .set(self.query_work_units.get().saturating_add(1));

        let top_left_index = y_start
            .checked_mul(self.stride)
            .and_then(|row| row.checked_add(x_start));
        let top_right_index = y_start
            .checked_mul(self.stride)
            .and_then(|row| row.checked_add(x_end));
        let bottom_left_index = y_end
            .checked_mul(self.stride)
            .and_then(|row| row.checked_add(x_start));
        let bottom_right_index = y_end
            .checked_mul(self.stride)
            .and_then(|row| row.checked_add(x_end));
        let (
            Some(top_left_index),
            Some(top_right_index),
            Some(bottom_left_index),
            Some(bottom_right_index),
        ) = (
            top_left_index,
            top_right_index,
            bottom_left_index,
            bottom_right_index,
        )
        else {
            return [0.0; 4];
        };
        let (Some(&top_left), Some(&top_right), Some(&bottom_left), Some(&bottom_right)) = (
            self.sums.get(top_left_index),
            self.sums.get(top_right_index),
            self.sums.get(bottom_left_index),
            self.sums.get(bottom_right_index),
        ) else {
            return [0.0; 4];
        };
        std::array::from_fn(|channel| {
            bottom_right[channel] - top_right[channel] - bottom_left[channel] + top_left[channel]
        })
    }

    fn average(&self, x: u32, y: u32, radius: u32) -> [f32; 4] {
        if self.width == 0 || self.height == 0 {
            return [0.0; 4];
        }
        let radius = i64::from(radius);
        let x_segments = AxisSegments::for_clamped_range(
            i64::from(x).saturating_sub(radius),
            i64::from(x).saturating_add(radius).saturating_add(1),
            self.origin.0,
            self.width,
        );
        let y_segments = AxisSegments::for_clamped_range(
            i64::from(y).saturating_sub(radius),
            i64::from(y).saturating_add(radius).saturating_add(1),
            self.origin.1,
            self.height,
        );
        let mut total = [0.0; 4];
        for x_segment in x_segments.iter() {
            for y_segment in y_segments.iter() {
                let rectangle = self.rectangle_sum(
                    x_segment.start,
                    x_segment.end,
                    y_segment.start,
                    y_segment.end,
                );
                let repetitions = x_segment.repetitions.saturating_mul(y_segment.repetitions);
                let repetitions = u32::try_from(repetitions)
                    .map(|value| value as f32)
                    .unwrap_or(f32::MAX);
                for channel in 0..4 {
                    total[channel] += rectangle[channel] * repetitions;
                }
            }
        }
        let side = radius
            .saturating_mul(2)
            .saturating_add(1)
            .try_into()
            .unwrap_or(u32::MAX);
        let count = side as f32 * side as f32;
        total.map(|value| value / count)
    }

    #[cfg(test)]
    fn build_work_units(&self) -> usize {
        self.build_work_units
    }

    #[cfg(test)]
    fn query_work_units(&self) -> usize {
        self.query_work_units.get()
    }
}

fn luminance([red, green, blue, _]: [f32; 4]) -> f32 {
    0.2126 * red + 0.7152 * green + 0.0722 * blue
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

// ---------------------------------------------------------------------------
// Detail and optical nodes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum ImageOperation {
    Denoise,
    DetailSeparation,
    Sharpen,
    Deconvolution,
    LocalContrast,
    Defringe,
    ChromaticAberration,
    Distortion,
    Vignetting,
    ToneMap,
    ColorZones,
    SelectiveColor,
    ChannelMixer,
    PerceptualSaturation,
    GamutCompression,
    Lut,
    FilmCurve,
    Grain,
    Halation,
    Bloom,
    DyeLayer,
    SplitToning,
}

struct ImageNode {
    operation: ImageOperation,
}

impl NodeInstance for ImageNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = image_input(inputs)?;
        match self.operation {
            ImageOperation::Denoise => evaluate_denoise(&image, parameters, context),
            ImageOperation::DetailSeparation => {
                evaluate_detail_separation(&image, parameters, context)
            }
            ImageOperation::Sharpen => evaluate_sharpen(&image, parameters, context, false),
            ImageOperation::Deconvolution => evaluate_sharpen(&image, parameters, context, true),
            ImageOperation::LocalContrast => evaluate_local_contrast(&image, parameters, context),
            ImageOperation::Defringe => evaluate_defringe(&image, parameters, context),
            ImageOperation::ChromaticAberration => {
                evaluate_chromatic_aberration(&image, parameters, context)
            }
            ImageOperation::Distortion => evaluate_distortion(&image, parameters, context),
            ImageOperation::Vignetting => evaluate_vignetting(&image, parameters, context),
            ImageOperation::ToneMap => evaluate_tone_map(&image, parameters, context),
            ImageOperation::ColorZones => evaluate_color_zones(&image, parameters, context),
            ImageOperation::SelectiveColor => evaluate_selective_color(&image, parameters, context),
            ImageOperation::ChannelMixer => evaluate_channel_mixer(&image, parameters, context),
            ImageOperation::PerceptualSaturation => {
                evaluate_perceptual_saturation(&image, parameters, context)
            }
            ImageOperation::GamutCompression => {
                evaluate_gamut_compression(&image, parameters, context)
            }
            ImageOperation::Lut => evaluate_lut(&image, parameters, context),
            ImageOperation::FilmCurve => evaluate_film_curve(&image, parameters, context),
            ImageOperation::Grain => evaluate_grain(&image, parameters, context),
            ImageOperation::Halation => evaluate_halation(&image, parameters, context),
            ImageOperation::Bloom => evaluate_bloom(&image, parameters, context),
            ImageOperation::DyeLayer => evaluate_dye_layer(&image, parameters, context),
            ImageOperation::SplitToning => evaluate_split_toning(&image, parameters, context),
        }
    }
}

fn evaluate_denoise(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let radius = integer_parameter_value(parameters, "radius", 1, MAX_RADIUS)?;
    if radius == 0 {
        return Ok(NodeResult::single(
            "image",
            Value::Image(image_region(image, context)?),
        ));
    }
    let strength = bounded_float(parameters, "strength", 0.5, 0.0, 1.0)?;
    let preserve_detail = bounded_float(parameters, "preserve_detail", 0.5, 0.0, 1.0)?;
    let integral = IntegralImage::new(image)?;
    let output = map_region(image, context, |x, y, pixel| {
        let average = integral.average(x, y, radius);
        let difference = [
            (pixel[0] - average[0]).abs(),
            (pixel[1] - average[1]).abs(),
            (pixel[2] - average[2]).abs(),
        ]
        .into_iter()
        .fold(0.0, f32::max)
        .min(1.0);
        let weight = strength * (1.0 - preserve_detail * difference);
        [
            pixel[0] + (average[0] - pixel[0]) * weight,
            pixel[1] + (average[1] - pixel[1]) * weight,
            pixel[2] + (average[2] - pixel[2]) * weight,
            pixel[3],
        ]
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn evaluate_detail_separation(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let radius = integer_parameter_value(parameters, "radius", 2, MAX_RADIUS)?;
    if radius == 0 {
        return Err(NodeError::InvalidParameter("radius".to_owned()));
    }
    let region = source_region(image, context);
    let integral = IntegralImage::new(image)?;
    let mut base_pixels = Vec::with_capacity(checked_pixel_count(region)?);
    let mut detail_pixels = Vec::with_capacity(checked_pixel_count(region)?);
    for y in 0..region.height {
        for x in 0..region.width {
            let global_x = region.x + x;
            let global_y = region.y + y;
            let source = image.pixel_global(global_x, global_y).ok_or_else(|| {
                NodeError::Message("requested region was outside the image".to_owned())
            })?;
            let base = integral.average(global_x, global_y, radius);
            base_pixels.push([base[0], base[1], base[2], source[3]]);
            detail_pixels.push([
                source[0] - base[0],
                source[1] - base[1],
                source[2] - base[2],
                source[3],
            ]);
        }
    }
    Ok(NodeResult::new(
        [
            (
                "base".to_owned(),
                Value::Image(from_region(image, region, base_pixels)?),
            ),
            (
                "detail".to_owned(),
                Value::Image(from_region(image, region, detail_pixels)?),
            ),
        ]
        .into_iter()
        .collect(),
    ))
}

fn evaluate_sharpen(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
    deconvolution: bool,
) -> Result<NodeResult, NodeError> {
    let radius = integer_parameter_value(parameters, "radius", 1, MAX_RADIUS)?;
    let amount = bounded_float(parameters, "amount", 0.5, 0.0, 8.0)?;
    let threshold = bounded_float(parameters, "threshold", 0.0, 0.0, 1.0)?;
    let iterations = integer_parameter_value(parameters, "iterations", 1, 8)?;
    let integral = IntegralImage::new(image)?;
    let output = map_region(image, context, |x, y, pixel| {
        let mut current = pixel;
        for _ in 0..iterations.max(1) {
            let average = integral.average(x, y, radius);
            let difference = [
                current[0] - average[0],
                current[1] - average[1],
                current[2] - average[2],
            ];
            let strength = if deconvolution {
                amount / iterations.max(1) as f32
            } else {
                amount
            };
            for channel in 0..3 {
                if difference[channel].abs() >= threshold {
                    current[channel] += difference[channel] * strength;
                }
            }
        }
        current
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn evaluate_local_contrast(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let radius = integer_parameter_value(parameters, "radius", 4, MAX_RADIUS)?;
    let amount = bounded_float(parameters, "amount", 0.5, -4.0, 4.0)?;
    let integral = IntegralImage::new(image)?;
    let output = map_region(image, context, |x, y, pixel| {
        let average = integral.average(x, y, radius);
        [
            pixel[0] + (pixel[0] - average[0]) * amount,
            pixel[1] + (pixel[1] - average[1]) * amount,
            pixel[2] + (pixel[2] - average[2]) * amount,
            pixel[3],
        ]
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn evaluate_defringe(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let threshold = bounded_float(parameters, "threshold", 0.05, 0.0, 4.0)?;
    let amount = bounded_float(parameters, "amount", 1.0, 0.0, 1.0)?;
    let output = map_region(image, context, |_x, _y, pixel| {
        let min = pixel[0].min(pixel[1]).min(pixel[2]);
        let max = pixel[0].max(pixel[1]).max(pixel[2]);
        let spread = max - min;
        if spread <= threshold {
            return pixel;
        }
        let mean = (pixel[0] + pixel[1] + pixel[2]) / 3.0;
        let weight = ((spread - threshold) / (1.0 + spread)).clamp(0.0, 1.0) * amount;
        [
            pixel[0] + (mean - pixel[0]) * weight,
            pixel[1] + (mean - pixel[1]) * weight,
            pixel[2] + (mean - pixel[2]) * weight,
            pixel[3],
        ]
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn normalized_coordinate(index: u32, origin: u32, size: u32) -> f32 {
    if size <= 1 {
        0.0
    } else {
        ((index.saturating_sub(origin)) as f32 / (size - 1) as f32) * 2.0 - 1.0
    }
}

fn evaluate_chromatic_aberration(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let amount = bounded_float(parameters, "amount", 0.01, -0.25, 0.25)?;
    let center_x = bounded_float(parameters, "center_x", 0.5, 0.0, 1.0)?;
    let center_y = bounded_float(parameters, "center_y", 0.5, 0.0, 1.0)?;
    let bounds = image.global_region();
    let output = map_region(image, context, |x, y, pixel| {
        let nx = normalized_coordinate(x, bounds.x, bounds.width) * 0.5 + 0.5;
        let ny = normalized_coordinate(y, bounds.y, bounds.height) * 0.5 + 0.5;
        let dx = nx - center_x;
        let dy = ny - center_y;
        let red = sample_bilinear(
            image,
            bounds.x as f32
                + (center_x + dx * (1.0 + amount)).clamp(0.0, 1.0)
                    * bounds.width.saturating_sub(1) as f32,
            bounds.y as f32
                + (center_y + dy * (1.0 + amount)).clamp(0.0, 1.0)
                    * bounds.height.saturating_sub(1) as f32,
        )[0];
        let blue = sample_bilinear(
            image,
            bounds.x as f32
                + (center_x + dx * (1.0 - amount)).clamp(0.0, 1.0)
                    * bounds.width.saturating_sub(1) as f32,
            bounds.y as f32
                + (center_y + dy * (1.0 - amount)).clamp(0.0, 1.0)
                    * bounds.height.saturating_sub(1) as f32,
        )[2];
        [red, pixel[1], blue, pixel[3]]
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn evaluate_distortion(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let k1 = bounded_float(parameters, "k1", 0.0, -2.0, 2.0)?;
    let k2 = bounded_float(parameters, "k2", 0.0, -2.0, 2.0)?;
    let p1 = bounded_float(parameters, "p1", 0.0, -1.0, 1.0)?;
    let p2 = bounded_float(parameters, "p2", 0.0, -1.0, 1.0)?;
    let bounds = image.global_region();
    let output = map_region(image, context, |x, y, _pixel| {
        let nx = normalized_coordinate(x, bounds.x, bounds.width);
        let ny = normalized_coordinate(y, bounds.y, bounds.height);
        let radius_squared = nx * nx + ny * ny;
        let radial = 1.0 + k1 * radius_squared + k2 * radius_squared * radius_squared;
        let source_x = nx * radial + 2.0 * p1 * nx * ny + p2 * (radius_squared + 2.0 * nx * nx);
        let source_y = ny * radial + p1 * (radius_squared + 2.0 * ny * ny) + 2.0 * p2 * nx * ny;
        let source_x = bounds.x as f32
            + ((source_x + 1.0) * 0.5).clamp(0.0, 1.0) * bounds.width.saturating_sub(1) as f32;
        let source_y = bounds.y as f32
            + ((source_y + 1.0) * 0.5).clamp(0.0, 1.0) * bounds.height.saturating_sub(1) as f32;
        sample_bilinear(image, source_x, source_y)
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn evaluate_vignetting(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let amount = bounded_float(parameters, "amount", 0.25, -1.0, 4.0)?;
    let midpoint = bounded_float(parameters, "midpoint", 0.5, 0.0, 1.0)?;
    let feather = bounded_float(parameters, "feather", 0.5, 0.01, 1.0)?;
    let bounds = image.global_region();
    let output = map_region(image, context, |x, y, pixel| {
        let nx = normalized_coordinate(x, bounds.x, bounds.width);
        let ny = normalized_coordinate(y, bounds.y, bounds.height);
        let distance = ((nx * nx + ny * ny) * 0.5).sqrt();
        let falloff = smoothstep((distance - midpoint) / feather);
        let gain = (1.0 + amount * falloff).max(0.0);
        [pixel[0] * gain, pixel[1] * gain, pixel[2] * gain, pixel[3]]
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

// ---------------------------------------------------------------------------
// Color nodes
// ---------------------------------------------------------------------------

fn evaluate_tone_map(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let exposure = bounded_float(parameters, "exposure", 0.0, -32.0, 32.0)?;
    let contrast = bounded_float(parameters, "contrast", 1.0, 0.0, 8.0)?;
    let operator = string_parameter_value(parameters, "operator", "reinhard")?;
    if !matches!(operator, "reinhard" | "filmic" | "aces") {
        return Err(NodeError::InvalidParameter("operator".to_owned()));
    }
    let multiplier = 2.0_f32.powf(exposure);
    let output = map_region(image, context, |_x, _y, mut pixel| {
        for channel in &mut pixel[..3] {
            let value = *channel * multiplier;
            let mapped = match operator {
                "filmic" => filmic_curve(value),
                "aces" => aces_curve(value),
                _ => signed_reinhard(value),
            };
            *channel = (mapped - 0.5) * contrast + 0.5;
        }
        pixel
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn signed_reinhard(value: f32) -> f32 {
    if value >= 0.0 {
        value / (1.0 + value)
    } else {
        -((-value) / (1.0 + -value))
    }
}

fn filmic_curve(value: f32) -> f32 {
    let value = value.max(0.0);
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    ((value * (a * value + b)) / (value * (c * value + d) + e)).clamp(0.0, 1.0)
}

fn aces_curve(value: f32) -> f32 {
    let value = value.max(0.0);
    ((value * (2.51 * value + 0.03)) / (value * (2.43 * value + 0.59) + 0.14)).clamp(0.0, 1.0)
}

fn rgb_to_hsv([red, green, blue]: [f32; 3]) -> [f32; 3] {
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;
    let hue = if delta <= EPSILON {
        0.0
    } else if (max - red).abs() <= EPSILON {
        ((green - blue) / delta).rem_euclid(6.0) / 6.0
    } else if (max - green).abs() <= EPSILON {
        ((blue - red) / delta + 2.0) / 6.0
    } else {
        ((red - green) / delta + 4.0) / 6.0
    };
    [hue, if max <= EPSILON { 0.0 } else { delta / max }, max]
}

fn hsv_to_rgb([hue, saturation, value]: [f32; 3]) -> [f32; 3] {
    if saturation <= EPSILON {
        return [value, value, value];
    }
    let position = hue.rem_euclid(1.0) * 6.0;
    let sector = position.floor() as u32;
    let fraction = position - sector as f32;
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - saturation * fraction);
    let t = value * (1.0 - saturation * (1.0 - fraction));
    match sector % 6 {
        0 => [value, t, p],
        1 => [q, value, p],
        2 => [p, value, t],
        3 => [p, q, value],
        4 => [t, p, value],
        _ => [value, p, q],
    }
}

fn hue_distance(a: f32, b: f32) -> f32 {
    let distance = (a - b).abs().rem_euclid(1.0);
    distance.min(1.0 - distance)
}

fn evaluate_color_zones(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let hue = bounded_float(parameters, "hue", 0.0, 0.0, 1.0)?;
    let width = bounded_float(parameters, "width", 0.2, 0.001, 0.5)?;
    let saturation = bounded_float(parameters, "saturation", 0.0, -4.0, 4.0)?;
    let lightness = bounded_float(parameters, "lightness", 0.0, -4.0, 4.0)?;
    let output = map_region(image, context, |_x, _y, pixel| {
        let mut hsv = rgb_to_hsv([pixel[0], pixel[1], pixel[2]]);
        let weight = (1.0 - hue_distance(hsv[0], hue) / width).clamp(0.0, 1.0);
        hsv[1] = (hsv[1] * (1.0 + saturation * weight)).max(0.0);
        hsv[2] *= 1.0 + lightness * weight;
        let rgb = hsv_to_rgb(hsv);
        [rgb[0], rgb[1], rgb[2], pixel[3]]
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn evaluate_selective_color(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let target = [
        bounded_float(parameters, "target_r", 1.0, 0.0, 1.0)?,
        bounded_float(parameters, "target_g", 1.0, 0.0, 1.0)?,
        bounded_float(parameters, "target_b", 1.0, 0.0, 1.0)?,
    ];
    let tolerance = bounded_float(parameters, "tolerance", 0.25, 0.0, 2.0)?;
    let amount = bounded_float(parameters, "amount", 1.0, -4.0, 4.0)?;
    let output = map_region(image, context, |_x, _y, pixel| {
        let distance = ((pixel[0] - target[0]).powi(2)
            + (pixel[1] - target[1]).powi(2)
            + (pixel[2] - target[2]).powi(2))
        .sqrt();
        let weight = if tolerance <= EPSILON {
            f32::from(distance <= EPSILON)
        } else {
            (1.0 - distance / tolerance).clamp(0.0, 1.0)
        } * amount;
        [
            pixel[0] + (target[0] - pixel[0]) * weight,
            pixel[1] + (target[1] - pixel[1]) * weight,
            pixel[2] + (target[2] - pixel[2]) * weight,
            pixel[3],
        ]
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn matrix_parameter(parameters: &Parameters, row: usize, column: usize) -> Result<f32, NodeError> {
    let id = format!("m{row}{column}");
    let default = f32::from(row == column);
    finite_float(parameters, &id, default)
}

fn evaluate_channel_mixer(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let mut matrix = [[0.0; 3]; 3];
    for (row, values) in matrix.iter_mut().enumerate() {
        for (column, value) in values.iter_mut().enumerate() {
            *value = matrix_parameter(parameters, row, column)?;
        }
    }
    let offsets = [
        finite_float(parameters, "offset_r", 0.0)?,
        finite_float(parameters, "offset_g", 0.0)?,
        finite_float(parameters, "offset_b", 0.0)?,
    ];
    let output = map_region(image, context, |_x, _y, pixel| {
        let input = [pixel[0], pixel[1], pixel[2]];
        let rgb: [f32; 3] = std::array::from_fn(|row| {
            offsets[row]
                + matrix[row][0] * input[0]
                + matrix[row][1] * input[1]
                + matrix[row][2] * input[2]
        });
        [rgb[0], rgb[1], rgb[2], pixel[3]]
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn evaluate_perceptual_saturation(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let amount = bounded_float(parameters, "amount", 1.0, -4.0, 8.0)?;
    let output = map_region(image, context, |_x, _y, pixel| {
        let luma = luminance(pixel);
        [
            luma + (pixel[0] - luma) * amount,
            luma + (pixel[1] - luma) * amount,
            luma + (pixel[2] - luma) * amount,
            pixel[3],
        ]
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn evaluate_gamut_compression(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let limit = bounded_float(parameters, "limit", 1.0, 0.0001, 64.0)?;
    let softness = bounded_float(parameters, "softness", 1.0, 0.0001, 64.0)?;
    let output = map_region(image, context, |_x, _y, mut pixel| {
        for channel in &mut pixel[..3] {
            if *channel > limit {
                let excess = *channel - limit;
                *channel = limit + excess / (1.0 + excess / softness);
            }
        }
        pixel
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn parse_lut_points(value: &str) -> Result<Vec<(f32, f32)>, NodeError> {
    if value.len() > MAX_LUT_BYTES {
        return Err(NodeError::InvalidParameter("points".to_owned()));
    }
    let mut points = Vec::new();
    for pair in value.split(';').filter(|pair| !pair.trim().is_empty()) {
        if points.len() >= MAX_LUT_POINTS {
            return Err(NodeError::InvalidParameter("points".to_owned()));
        }
        let (x, y) = pair
            .split_once(',')
            .ok_or_else(|| NodeError::InvalidParameter("points".to_owned()))?;
        let x = x
            .trim()
            .parse::<f32>()
            .map_err(|_| NodeError::InvalidParameter("points".to_owned()))?;
        let y = y
            .trim()
            .parse::<f32>()
            .map_err(|_| NodeError::InvalidParameter("points".to_owned()))?;
        if !x.is_finite() || !y.is_finite() || !(0.0..=1.0).contains(&x) {
            return Err(NodeError::InvalidParameter("points".to_owned()));
        }
        points.push((x, y));
    }
    if points.len() < 2 {
        return Err(NodeError::InvalidParameter("points".to_owned()));
    }
    points.sort_by(|left, right| left.0.total_cmp(&right.0));
    if points.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(NodeError::InvalidParameter("points".to_owned()));
    }
    Ok(points)
}

fn evaluate_points(points: &[(f32, f32)], value: f32) -> f32 {
    if value <= points[0].0 {
        return points[0].1;
    }
    if value >= points[points.len() - 1].0 {
        return points[points.len() - 1].1;
    }
    for pair in points.windows(2) {
        if value <= pair[1].0 {
            let t = (value - pair[0].0) / (pair[1].0 - pair[0].0);
            return pair[0].1 + (pair[1].1 - pair[0].1) * t;
        }
    }
    points[points.len() - 1].1
}

fn evaluate_lut(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let points = parse_lut_points(string_parameter_value(parameters, "points", "0,0;1,1")?)?;
    let amount = bounded_float(parameters, "amount", 1.0, 0.0, 1.0)?;
    let output = map_region(image, context, |_x, _y, pixel| {
        let mut output = pixel;
        for channel in &mut output[..3] {
            let mapped = evaluate_points(&points, (*channel).clamp(0.0, 1.0));
            *channel += (mapped - *channel) * amount;
        }
        output
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn evaluate_film_curve(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let points = parse_lut_points(string_parameter_value(
        parameters,
        "points",
        "0,0;0.25,0.2;1,1",
    )?)?;
    let exposure = bounded_float(parameters, "exposure", 0.0, -8.0, 8.0)?;
    let amount = bounded_float(parameters, "amount", 1.0, 0.0, 1.0)?;
    let multiplier = 2.0_f32.powf(exposure);
    let output = map_region(image, context, |_x, _y, pixel| {
        let mut output = pixel;
        for channel in &mut output[..3] {
            let mapped = evaluate_points(&points, (*channel * multiplier).clamp(0.0, 1.0));
            *channel += (mapped - *channel) * amount;
        }
        output
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn hash_noise(x: u32, y: u32, seed: u64, channel: u32) -> f32 {
    let mut value = seed
        .wrapping_add(u64::from(x).wrapping_mul(0x9e37_79b9_7f4a_7c15))
        .wrapping_add(u64::from(y).wrapping_mul(0xbf58_476d_1ce4_e5b9))
        .wrapping_add(u64::from(channel).wrapping_mul(0x94d0_49bb_1331_11eb));
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    ((value >> 40) as f32 / (1_u64 << 24) as f32) * 2.0 - 1.0
}

fn evaluate_grain(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let amount = bounded_float(parameters, "amount", 0.05, 0.0, 1.0)?;
    let size = bounded_float(parameters, "size", 1.0, 0.1, 64.0)?;
    let seed = match parameters.get("seed") {
        None => 0,
        Some(ParameterValue::Integer(value)) if *value >= 0 => *value as u64,
        Some(_) => return Err(NodeError::InvalidParameter("seed".to_owned())),
    };
    let monochrome = boolean_parameter_value(parameters, "monochrome", false)?;
    let output = map_region(image, context, |x, y, pixel| {
        let cell_x = (x as f32 / size).floor().max(0.0) as u32;
        let cell_y = (y as f32 / size).floor().max(0.0) as u32;
        let shared = hash_noise(cell_x, cell_y, seed, 0);
        [
            pixel[0]
                + amount
                    * if monochrome {
                        shared
                    } else {
                        hash_noise(cell_x, cell_y, seed, 0)
                    },
            pixel[1]
                + amount
                    * if monochrome {
                        shared
                    } else {
                        hash_noise(cell_x, cell_y, seed, 1)
                    },
            pixel[2]
                + amount
                    * if monochrome {
                        shared
                    } else {
                        hash_noise(cell_x, cell_y, seed, 2)
                    },
            pixel[3],
        ]
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn evaluate_halation(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let amount = bounded_float(parameters, "amount", 0.2, 0.0, 4.0)?;
    let threshold = bounded_float(parameters, "threshold", 0.75, 0.0, 64.0)?;
    let radius = integer_parameter_value(parameters, "radius", 2, MAX_RADIUS)?;
    let integral = IntegralImage::new(image)?;
    let output = map_region(image, context, |x, y, pixel| {
        let average = integral.average(x, y, radius);
        let excess = (luminance(average) - threshold).max(0.0);
        let halo = excess * amount;
        [
            pixel[0] + halo,
            pixel[1] + halo * 0.18,
            pixel[2] + halo * 0.03,
            pixel[3],
        ]
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn evaluate_bloom(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let amount = bounded_float(parameters, "amount", 0.25, 0.0, 4.0)?;
    let threshold = bounded_float(parameters, "threshold", 0.75, 0.0, 64.0)?;
    let radius = integer_parameter_value(parameters, "radius", 3, MAX_RADIUS)?;
    let integral = IntegralImage::new(image)?;
    let output = map_region(image, context, |x, y, pixel| {
        let average = integral.average(x, y, radius);
        let bright = (luminance(average) - threshold).max(0.0) * amount;
        [
            pixel[0] + average[0] * bright,
            pixel[1] + average[1] * bright,
            pixel[2] + average[2] * bright,
            pixel[3],
        ]
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn evaluate_dye_layer(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let cyan = bounded_float(parameters, "cyan", 0.0, -1.0, 1.0)?;
    let magenta = bounded_float(parameters, "magenta", 0.0, -1.0, 1.0)?;
    let yellow = bounded_float(parameters, "yellow", 0.0, -1.0, 1.0)?;
    let strength = bounded_float(parameters, "strength", 1.0, 0.0, 1.0)?;
    let output = map_region(image, context, |_x, _y, pixel| {
        [
            pixel[0] * (1.0 - cyan * strength),
            pixel[1] * (1.0 - magenta * strength),
            pixel[2] * (1.0 - yellow * strength),
            pixel[3],
        ]
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn evaluate_split_toning(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let shadow_hue = bounded_float(parameters, "shadow_hue", 0.6, 0.0, 1.0)?;
    let highlight_hue = bounded_float(parameters, "highlight_hue", 0.1, 0.0, 1.0)?;
    let shadow_saturation = bounded_float(parameters, "shadow_saturation", 0.0, 0.0, 1.0)?;
    let highlight_saturation = bounded_float(parameters, "highlight_saturation", 0.0, 0.0, 1.0)?;
    let balance = bounded_float(parameters, "balance", 0.5, 0.0, 1.0)?;
    let output = map_region(image, context, |_x, _y, pixel| {
        let luma = luminance(pixel).clamp(0.0, 1.0);
        let highlight_weight = smoothstep((luma - (balance - 0.25)) / 0.5);
        let shadow_weight = 1.0 - highlight_weight;
        let shadow = hsv_to_rgb([shadow_hue, shadow_saturation, luma]);
        let highlight = hsv_to_rgb([highlight_hue, highlight_saturation, luma]);
        [
            pixel[0] + (shadow[0] * shadow_weight + highlight[0] * highlight_weight - luma) * 0.25,
            pixel[1] + (shadow[1] * shadow_weight + highlight[1] * highlight_weight - luma) * 0.25,
            pixel[2] + (shadow[2] * shadow_weight + highlight[2] * highlight_weight - luma) * 0.25,
            pixel[3],
        ]
    })?;
    Ok(NodeResult::single("image", Value::Image(output)))
}

fn image_region(image: &Image, context: &EvaluationContext) -> Result<Image, NodeError> {
    let region = source_region(image, context);
    let mut pixels = Vec::with_capacity(checked_pixel_count(region)?);
    for y in 0..region.height {
        for x in 0..region.width {
            pixels.push(
                image
                    .pixel_global(region.x + x, region.y + y)
                    .ok_or_else(|| {
                        NodeError::Message("requested region was outside the image".to_owned())
                    })?,
            );
        }
    }
    from_region(image, region, pixels)
}

// ---------------------------------------------------------------------------
// Analysis nodes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum AnalysisOperation {
    Histogram,
    Clipping,
    Noise,
    Sharpness,
    DynamicRange,
}

struct AnalysisNode {
    operation: AnalysisOperation,
}

impl NodeInstance for AnalysisNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = image_input(inputs)?;
        match self.operation {
            AnalysisOperation::Histogram => evaluate_histogram(&image, context),
            AnalysisOperation::Clipping => evaluate_clipping(&image, context),
            AnalysisOperation::Noise => evaluate_noise(&image, parameters, context),
            AnalysisOperation::Sharpness => evaluate_sharpness(&image, context),
            AnalysisOperation::DynamicRange => evaluate_dynamic_range(&image, context),
        }
    }
}

fn luminances(image: &Image, context: &EvaluationContext) -> Result<(Region, Vec<f32>), NodeError> {
    let region = source_region(image, context);
    let mut values = Vec::with_capacity(checked_pixel_count(region)?);
    for y in 0..region.height {
        for x in 0..region.width {
            let pixel = image
                .pixel_global(region.x + x, region.y + y)
                .ok_or_else(|| {
                    NodeError::Message("requested region was outside the image".to_owned())
                })?;
            values.push(luminance(pixel));
        }
    }
    Ok((region, values))
}

fn evaluate_histogram(image: &Image, context: &EvaluationContext) -> Result<NodeResult, NodeError> {
    let (_region, mut values) = luminances(image, context)?;
    if values.is_empty() {
        return Err(NodeError::Message(
            "cannot analyze an empty image".to_owned(),
        ));
    }
    values.sort_by(|left, right| left.total_cmp(right));
    let sum = values.iter().copied().sum::<f32>();
    let percentile = |fraction: f32| {
        let index = ((values.len() - 1) as f32 * fraction).round() as usize;
        values[index]
    };
    let count = values.len() as f32;
    Ok(NodeResult::new(
        [
            ("mean".to_owned(), Value::Float(sum / count)),
            ("minimum".to_owned(), Value::Float(values[0])),
            ("maximum".to_owned(), Value::Float(values[values.len() - 1])),
            ("percentile_low".to_owned(), Value::Float(percentile(0.01))),
            ("percentile_high".to_owned(), Value::Float(percentile(0.99))),
            (
                "clipped_low".to_owned(),
                Value::Float(values.iter().filter(|value| **value <= 0.0).count() as f32 / count),
            ),
            (
                "clipped_high".to_owned(),
                Value::Float(values.iter().filter(|value| **value >= 1.0).count() as f32 / count),
            ),
        ]
        .into_iter()
        .collect(),
    ))
}

fn evaluate_clipping(image: &Image, context: &EvaluationContext) -> Result<NodeResult, NodeError> {
    let region = source_region(image, context);
    let mut values = Vec::with_capacity(checked_pixel_count(region)?);
    let mut low = 0_u32;
    let mut high = 0_u32;
    for y in 0..region.height {
        for x in 0..region.width {
            let pixel = image
                .pixel_global(region.x + x, region.y + y)
                .ok_or_else(|| {
                    NodeError::Message("requested region was outside the image".to_owned())
                })?;
            let is_low = pixel[..3].iter().any(|value| *value <= 0.0);
            let is_high = pixel[..3].iter().any(|value| *value >= 1.0);
            low += u32::from(is_low);
            high += u32::from(is_high);
            values.push(f32::from(is_low || is_high));
        }
    }
    let total = values.len().max(1) as f32;
    let mask = Mask::from_values_with_origin(region.dimensions(), (region.x, region.y), values)
        .map_err(|error| NodeError::Message(error.to_string()))?;
    Ok(NodeResult::new(
        [
            ("mask".to_owned(), Value::Mask(mask)),
            ("low_clipped".to_owned(), Value::Float(low as f32 / total)),
            ("high_clipped".to_owned(), Value::Float(high as f32 / total)),
        ]
        .into_iter()
        .collect(),
    ))
}

fn evaluate_noise(
    image: &Image,
    parameters: &Parameters,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let radius = integer_parameter_value(parameters, "radius", 1, MAX_RADIUS)?;
    let (region, values) = luminances(image, context)?;
    if values.is_empty() {
        return Err(NodeError::Message(
            "cannot analyze an empty image".to_owned(),
        ));
    }
    let integral = IntegralImage::new(image)?;
    let mut total = 0.0;
    let mut count = 0.0;
    for y in 0..region.height {
        for x in 0..region.width {
            let global_x = region.x + x;
            let global_y = region.y + y;
            let average = integral.average(global_x, global_y, radius);
            let difference = luminance(image.pixel_global(global_x, global_y).unwrap_or([0.0; 4]))
                - luminance(average);
            total += difference * difference;
            count += 1.0;
        }
    }
    Ok(NodeResult::single(
        "value",
        Value::Float((total / count).sqrt()),
    ))
}

fn evaluate_sharpness(image: &Image, context: &EvaluationContext) -> Result<NodeResult, NodeError> {
    let (region, values) = luminances(image, context)?;
    if values.is_empty() {
        return Err(NodeError::Message(
            "cannot analyze an empty image".to_owned(),
        ));
    }
    let mut total = 0.0;
    let mut count = 0.0;
    for y in 0..region.height {
        for x in 0..region.width {
            let global_x = region.x + x;
            let global_y = region.y + y;
            let current = image.pixel_global(global_x, global_y).unwrap_or([0.0; 4]);
            let right = sample_nearest(image, i64::from(global_x) + 1, i64::from(global_y));
            let down = sample_nearest(image, i64::from(global_x), i64::from(global_y) + 1);
            total += (luminance(current) - luminance(right)).abs();
            total += (luminance(current) - luminance(down)).abs();
            count += 2.0;
        }
    }
    Ok(NodeResult::single("value", Value::Float(total / count)))
}

fn evaluate_dynamic_range(
    image: &Image,
    context: &EvaluationContext,
) -> Result<NodeResult, NodeError> {
    let (_region, values) = luminances(image, context)?;
    let min = values
        .iter()
        .copied()
        .filter(|value| *value > 0.0)
        .fold(f32::INFINITY, f32::min);
    let max = values.iter().copied().fold(0.0, f32::max);
    let range = if min.is_finite() && max > 0.0 {
        (max / min).log2()
    } else {
        0.0
    };
    Ok(NodeResult::single("value", Value::Float(range.max(0.0))))
}

// ---------------------------------------------------------------------------
// Descriptors, factories, and pack registration
// ---------------------------------------------------------------------------

fn image_parameters(operation: ImageOperation) -> Vec<ParameterDescriptor> {
    match operation {
        ImageOperation::Denoise => vec![
            radius_parameter(1, MAX_RADIUS as i64),
            float_parameter("strength", "Strength", 0.5, Some(0.0), Some(1.0)),
            float_parameter(
                "preserve_detail",
                "Preserve Detail",
                0.5,
                Some(0.0),
                Some(1.0),
            ),
        ],
        ImageOperation::DetailSeparation => shared_radius_parameters(2, MAX_RADIUS as i64),
        ImageOperation::Sharpen => vec![
            radius_parameter(1, MAX_RADIUS as i64),
            float_parameter("amount", "Amount", 0.5, Some(0.0), Some(8.0)),
            float_parameter("threshold", "Threshold", 0.0, Some(0.0), Some(1.0)),
        ],
        ImageOperation::Deconvolution => vec![
            radius_parameter(1, MAX_RADIUS as i64),
            float_parameter("amount", "Amount", 0.5, Some(0.0), Some(8.0)),
            float_parameter("threshold", "Threshold", 0.0, Some(0.0), Some(1.0)),
            integer_parameter("iterations", "Iterations", 1).with_bounds(Some(1.0), Some(8.0)),
        ],
        ImageOperation::LocalContrast => vec![
            radius_parameter(4, MAX_RADIUS as i64),
            float_parameter("amount", "Amount", 0.5, Some(-4.0), Some(4.0)),
        ],
        ImageOperation::Defringe => vec![
            float_parameter("threshold", "Threshold", 0.05, Some(0.0), Some(4.0)),
            float_parameter("amount", "Amount", 1.0, Some(0.0), Some(1.0)),
        ],
        ImageOperation::ChromaticAberration => vec![
            float_parameter("amount", "Amount", 0.01, Some(-0.25), Some(0.25)),
            float_parameter("center_x", "Center X", 0.5, Some(0.0), Some(1.0)),
            float_parameter("center_y", "Center Y", 0.5, Some(0.0), Some(1.0)),
        ],
        ImageOperation::Distortion => vec![
            float_parameter("k1", "Radial K1", 0.0, Some(-2.0), Some(2.0)),
            float_parameter("k2", "Radial K2", 0.0, Some(-2.0), Some(2.0)),
            float_parameter("p1", "Tangential P1", 0.0, Some(-1.0), Some(1.0)),
            float_parameter("p2", "Tangential P2", 0.0, Some(-1.0), Some(1.0)),
        ],
        ImageOperation::Vignetting => vec![
            float_parameter("amount", "Amount", 0.25, Some(-1.0), Some(4.0)),
            float_parameter("midpoint", "Midpoint", 0.5, Some(0.0), Some(1.0)),
            float_parameter("feather", "Feather", 0.5, Some(0.01), Some(1.0)),
        ],
        ImageOperation::ToneMap => vec![
            float_parameter("exposure", "Exposure", 0.0, Some(-32.0), Some(32.0)),
            float_parameter("contrast", "Contrast", 1.0, Some(0.0), Some(8.0)),
            string_parameter("operator", "Operator", "reinhard"),
        ],
        ImageOperation::ColorZones => vec![
            float_parameter("hue", "Hue", 0.0, Some(0.0), Some(1.0)),
            float_parameter("width", "Width", 0.2, Some(0.001), Some(0.5)),
            float_parameter("saturation", "Saturation", 0.0, Some(-4.0), Some(4.0)),
            float_parameter("lightness", "Lightness", 0.0, Some(-4.0), Some(4.0)),
        ],
        ImageOperation::SelectiveColor => vec![
            float_parameter("target_r", "Target Red", 1.0, Some(0.0), Some(1.0)),
            float_parameter("target_g", "Target Green", 1.0, Some(0.0), Some(1.0)),
            float_parameter("target_b", "Target Blue", 1.0, Some(0.0), Some(1.0)),
            float_parameter("tolerance", "Tolerance", 0.25, Some(0.0), Some(2.0)),
            float_parameter("amount", "Amount", 1.0, Some(-4.0), Some(4.0)),
        ],
        ImageOperation::ChannelMixer => {
            let mut parameters = Vec::new();
            for row in 0..3 {
                for column in 0..3 {
                    parameters.push(float_parameter(
                        match (row, column) {
                            (0, 0) => "m00",
                            (0, 1) => "m01",
                            (0, 2) => "m02",
                            (1, 0) => "m10",
                            (1, 1) => "m11",
                            (1, 2) => "m12",
                            (2, 0) => "m20",
                            (2, 1) => "m21",
                            _ => "m22",
                        },
                        "Matrix",
                        f32::from(row == column),
                        Some(-8.0),
                        Some(8.0),
                    ));
                }
            }
            parameters.extend([
                float_parameter("offset_r", "Red Offset", 0.0, None, None),
                float_parameter("offset_g", "Green Offset", 0.0, None, None),
                float_parameter("offset_b", "Blue Offset", 0.0, None, None),
            ]);
            parameters
        }
        ImageOperation::PerceptualSaturation => {
            vec![float_parameter(
                "amount",
                "Amount",
                1.0,
                Some(-4.0),
                Some(8.0),
            )]
        }
        ImageOperation::GamutCompression => vec![
            float_parameter("limit", "Limit", 1.0, Some(0.0001), Some(64.0)),
            float_parameter("softness", "Softness", 1.0, Some(0.0001), Some(64.0)),
        ],
        ImageOperation::Lut => vec![
            string_parameter("points", "Control Points", "0,0;1,1"),
            float_parameter("amount", "Amount", 1.0, Some(0.0), Some(1.0)),
        ],
        ImageOperation::FilmCurve => vec![
            string_parameter("points", "Control Points", "0,0;0.25,0.2;1,1"),
            float_parameter("exposure", "Exposure", 0.0, Some(-8.0), Some(8.0)),
            float_parameter("amount", "Amount", 1.0, Some(0.0), Some(1.0)),
        ],
        ImageOperation::Grain => vec![
            float_parameter("amount", "Amount", 0.05, Some(0.0), Some(1.0)),
            float_parameter("size", "Size", 1.0, Some(0.1), Some(64.0)),
            integer_parameter("seed", "Seed", 0),
            boolean_parameter("monochrome", "Monochrome", false),
        ],
        ImageOperation::Halation | ImageOperation::Bloom => vec![
            float_parameter("amount", "Amount", 0.2, Some(0.0), Some(4.0)),
            float_parameter("threshold", "Threshold", 0.75, Some(0.0), Some(64.0)),
            radius_parameter(2, MAX_RADIUS as i64),
        ],
        ImageOperation::DyeLayer => vec![
            float_parameter("cyan", "Cyan", 0.0, Some(-1.0), Some(1.0)),
            float_parameter("magenta", "Magenta", 0.0, Some(-1.0), Some(1.0)),
            float_parameter("yellow", "Yellow", 0.0, Some(-1.0), Some(1.0)),
            float_parameter("strength", "Strength", 1.0, Some(0.0), Some(1.0)),
        ],
        ImageOperation::SplitToning => vec![
            float_parameter("shadow_hue", "Shadow Hue", 0.6, Some(0.0), Some(1.0)),
            float_parameter("highlight_hue", "Highlight Hue", 0.1, Some(0.0), Some(1.0)),
            float_parameter(
                "shadow_saturation",
                "Shadow Saturation",
                0.0,
                Some(0.0),
                Some(1.0),
            ),
            float_parameter(
                "highlight_saturation",
                "Highlight Saturation",
                0.0,
                Some(0.0),
                Some(1.0),
            ),
            float_parameter("balance", "Balance", 0.5, Some(0.0), Some(1.0)),
        ],
    }
}

fn analysis_descriptor(
    type_id: &str,
    name: &str,
    outputs: &[(&str, &str, &str)],
    parameters: Vec<ParameterDescriptor>,
    full_frame: bool,
) -> NodeDescriptor {
    output_descriptor(type_id, name, outputs, parameters, full_frame)
}

pub fn descriptors() -> Vec<NodeDescriptor> {
    let mut descriptors = Vec::new();
    let image_specs = [
        (
            ImageOperation::Denoise,
            &[
                ("pro.advanced-denoise", "Advanced Denoise"),
                ("pro.denoise", "Denoise"),
            ][..],
            true,
        ),
        (
            ImageOperation::DetailSeparation,
            &[
                ("pro.detail-separation", "Detail Separation"),
                ("pro.wavelet-detail-separation", "Wavelet Detail Separation"),
            ][..],
            true,
        ),
        (
            ImageOperation::Deconvolution,
            &[("pro.deconvolution", "Deconvolution")][..],
            true,
        ),
        (
            ImageOperation::Sharpen,
            &[
                ("pro.sharpen", "Advanced Sharpen"),
                ("pro.advanced-sharpen", "Advanced Sharpen"),
            ][..],
            true,
        ),
        (
            ImageOperation::LocalContrast,
            &[
                ("pro.local-contrast", "Local Contrast"),
                ("pro.texture", "Texture"),
            ][..],
            true,
        ),
        (
            ImageOperation::Defringe,
            &[("pro.defringe", "Defringe")][..],
            false,
        ),
        (
            ImageOperation::ChromaticAberration,
            &[
                ("pro.chromatic-aberration", "Chromatic Aberration"),
                (
                    "pro.chromatic-aberration-correction",
                    "Chromatic Aberration Correction",
                ),
            ][..],
            false,
        ),
        (
            ImageOperation::Distortion,
            &[
                ("pro.distortion-correction", "Distortion Correction"),
                ("pro.distortion", "Distortion"),
                ("pro.perspective-correction", "Perspective Correction"),
            ][..],
            false,
        ),
        (
            ImageOperation::Vignetting,
            &[("pro.vignetting", "Vignetting")][..],
            false,
        ),
        (
            ImageOperation::ToneMap,
            &[
                ("pro.tone-map", "Advanced Tone Mapping"),
                ("pro.advanced-tone-map", "Advanced Tone Mapping"),
            ][..],
            false,
        ),
        (
            ImageOperation::ColorZones,
            &[("pro.color-zones", "Color Zones")][..],
            false,
        ),
        (
            ImageOperation::SelectiveColor,
            &[("pro.selective-color", "Selective Color")][..],
            false,
        ),
        (
            ImageOperation::ChannelMixer,
            &[("pro.channel-mixer", "Channel Mixer")][..],
            false,
        ),
        (
            ImageOperation::PerceptualSaturation,
            &[("pro.perceptual-saturation", "Perceptual Saturation")][..],
            false,
        ),
        (
            ImageOperation::GamutCompression,
            &[("pro.gamut-compression", "Gamut Compression")][..],
            false,
        ),
        (
            ImageOperation::Lut,
            &[("pro.lut", "LUT"), ("pro.lut-tools", "LUT Tools")][..],
            false,
        ),
        (
            ImageOperation::FilmCurve,
            &[
                ("pro.film-curve", "Film Curve"),
                ("pro.film-simulation", "Film Simulation"),
            ][..],
            false,
        ),
        (ImageOperation::Grain, &[("pro.grain", "Grain")][..], false),
        (
            ImageOperation::Halation,
            &[("pro.halation", "Halation")][..],
            true,
        ),
        (ImageOperation::Bloom, &[("pro.bloom", "Bloom")][..], true),
        (
            ImageOperation::DyeLayer,
            &[("pro.dye-layer", "Dye Layer")][..],
            false,
        ),
        (
            ImageOperation::SplitToning,
            &[("pro.split-toning", "Split Toning")][..],
            false,
        ),
    ];
    for (operation, aliases, full_frame) in image_specs {
        descriptors.extend(
            descriptors_for_aliases(aliases, image_parameters(operation), full_frame)
                .into_iter()
                .zip(aliases.iter())
                .map(|(mut descriptor, _)| {
                    if matches!(operation, ImageOperation::DetailSeparation) {
                        descriptor.outputs = vec![
                            PortDescriptor::output("base", "Base", "core.Image"),
                            PortDescriptor::output("detail", "Detail", "core.Image"),
                        ];
                    }
                    descriptor
                }),
        );
    }

    let analysis_outputs = [
        (
            AnalysisOperation::Histogram,
            &[
                ("pro.histogram-statistics", "Histogram Statistics"),
                ("pro.histogram", "Histogram"),
            ][..],
            vec![
                ("mean", "Mean", "value.Float"),
                ("minimum", "Minimum", "value.Float"),
                ("maximum", "Maximum", "value.Float"),
                ("percentile_low", "Low Percentile", "value.Float"),
                ("percentile_high", "High Percentile", "value.Float"),
                ("clipped_low", "Low Clipped", "value.Float"),
                ("clipped_high", "High Clipped", "value.Float"),
            ],
            Vec::new(),
            true,
        ),
        (
            AnalysisOperation::Clipping,
            &[
                ("pro.clipping-analysis", "Clipping Analysis"),
                ("pro.clipping", "Clipping"),
            ][..],
            vec![
                ("mask", "Clipping Mask", "core.Mask"),
                ("low_clipped", "Low Clipped", "value.Float"),
                ("high_clipped", "High Clipped", "value.Float"),
            ],
            Vec::new(),
            true,
        ),
        (
            AnalysisOperation::Noise,
            &[
                ("pro.noise-estimate", "Noise Estimate"),
                ("pro.noise", "Noise"),
            ][..],
            vec![("value", "Noise", "value.Float")],
            vec![radius_parameter(1, MAX_RADIUS as i64)],
            true,
        ),
        (
            AnalysisOperation::Sharpness,
            &[
                ("pro.sharpness-estimate", "Sharpness Estimate"),
                ("pro.sharpness", "Sharpness"),
            ][..],
            vec![("value", "Sharpness", "value.Float")],
            Vec::new(),
            true,
        ),
        (
            AnalysisOperation::DynamicRange,
            &[
                ("pro.dynamic-range-estimate", "Dynamic Range Estimate"),
                ("pro.dynamic-range", "Dynamic Range"),
            ][..],
            vec![("value", "Dynamic Range", "value.Float")],
            Vec::new(),
            true,
        ),
    ];
    for (operation, aliases, outputs, parameters, full_frame) in analysis_outputs {
        for (type_id, name) in aliases {
            descriptors.push(analysis_descriptor(
                type_id,
                name,
                &outputs,
                parameters.clone(),
                full_frame,
            ));
        }
        let _ = operation;
    }
    descriptors
}

fn factory_for_type(type_id: &str) -> Box<dyn NodeInstance> {
    let operation = match type_id {
        "pro.advanced-denoise" | "pro.denoise" => ImageOperation::Denoise,
        "pro.detail-separation" | "pro.wavelet-detail-separation" => {
            ImageOperation::DetailSeparation
        }
        "pro.deconvolution" => ImageOperation::Deconvolution,
        "pro.sharpen" | "pro.advanced-sharpen" => ImageOperation::Sharpen,
        "pro.local-contrast" | "pro.texture" => ImageOperation::LocalContrast,
        "pro.defringe" => ImageOperation::Defringe,
        "pro.chromatic-aberration" | "pro.chromatic-aberration-correction" => {
            ImageOperation::ChromaticAberration
        }
        "pro.distortion-correction" | "pro.distortion" | "pro.perspective-correction" => {
            ImageOperation::Distortion
        }
        "pro.vignetting" => ImageOperation::Vignetting,
        "pro.tone-map" | "pro.advanced-tone-map" => ImageOperation::ToneMap,
        "pro.color-zones" => ImageOperation::ColorZones,
        "pro.selective-color" => ImageOperation::SelectiveColor,
        "pro.channel-mixer" => ImageOperation::ChannelMixer,
        "pro.perceptual-saturation" => ImageOperation::PerceptualSaturation,
        "pro.gamut-compression" => ImageOperation::GamutCompression,
        "pro.lut" | "pro.lut-tools" => ImageOperation::Lut,
        "pro.film-curve" | "pro.film-simulation" => ImageOperation::FilmCurve,
        "pro.grain" => ImageOperation::Grain,
        "pro.halation" => ImageOperation::Halation,
        "pro.bloom" => ImageOperation::Bloom,
        "pro.dye-layer" => ImageOperation::DyeLayer,
        "pro.split-toning" => ImageOperation::SplitToning,
        _ => {
            return Box::new(AnalysisNode {
                operation: match type_id {
                    "pro.histogram-statistics" | "pro.histogram" => AnalysisOperation::Histogram,
                    "pro.clipping-analysis" | "pro.clipping" => AnalysisOperation::Clipping,
                    "pro.noise-estimate" | "pro.noise" => AnalysisOperation::Noise,
                    "pro.sharpness-estimate" | "pro.sharpness" => AnalysisOperation::Sharpness,
                    _ => AnalysisOperation::DynamicRange,
                },
            });
        }
    };
    Box::new(ImageNode { operation })
}

/// Professional photographic node pack.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProToolsPack;

impl ProToolsPack {
    pub const ID: &'static str = "pro-tools";
}

/// Compatibility alias for callers that use the longer pack name.
pub type ProfessionalNodePack = ProToolsPack;

/// Compatibility alias for callers that use the short pack name.
pub type ProNodePack = ProToolsPack;

impl NodePack for ProToolsPack {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn register(&self, registry: &mut NodeRegistry) -> Result<(), RegistryError> {
        for descriptor in descriptors() {
            let type_id = descriptor.type_id.clone();
            registry.register_factory(descriptor, move || factory_for_type(&type_id))?;
        }
        Ok(())
    }
}

pub fn register_nodes(registry: &mut NodeRegistry) -> Result<(), RegistryError> {
    ProToolsPack.register(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lut_parser_sorts_points_and_rejects_duplicates() {
        let points = parse_lut_points("1,1;0,0;0.5,0.75").unwrap();
        assert_eq!(points, vec![(0.0, 0.0), (0.5, 0.75), (1.0, 1.0)]);
        assert!(parse_lut_points("0,0;0,1").is_err());
    }

    #[test]
    fn noise_is_stable_for_global_coordinates() {
        assert_eq!(hash_noise(4, 7, 42, 1), hash_noise(4, 7, 42, 1));
        assert_ne!(hash_noise(4, 7, 42, 1), hash_noise(5, 7, 42, 1));
    }

    #[test]
    fn descriptors_have_stable_public_ids() {
        let ids = descriptors()
            .into_iter()
            .map(|descriptor| descriptor.type_id)
            .collect::<Vec<_>>();
        assert!(ids.windows(2).all(|pair| pair[0] != pair[1]));
    }

    fn neighborhood_test_image(width: u32, height: u32, origin: (u32, u32)) -> Image {
        let pixels = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    [
                        ((x * 3 + y * 5) % 17) as f32 / 17.0,
                        ((x * 7 + y * 2 + 1) % 19) as f32 / 19.0,
                        ((x * 11 + y * 13 + 2) % 23) as f32 / 23.0,
                        1.0,
                    ]
                })
            })
            .collect();
        Image::from_pixels_with_origin(
            rawweave_image::Dimensions::new(width, height),
            origin,
            pixels,
            rawweave_image::PixelFormat::default(),
            rawweave_image::ColorMetadata::default(),
        )
        .unwrap()
    }

    fn naive_average_pixel(image: &Image, x: u32, y: u32, radius: u32) -> [f32; 4] {
        if radius == 0 {
            return sample_nearest(image, i64::from(x), i64::from(y));
        }
        let radius = i64::from(radius);
        let mut total = [0.0; 4];
        let mut count = 0.0;
        for offset_y in -radius..=radius {
            for offset_x in -radius..=radius {
                let pixel = sample_nearest(image, i64::from(x) + offset_x, i64::from(y) + offset_y);
                for channel in 0..4 {
                    total[channel] += pixel[channel];
                }
                count += 1.0;
            }
        }
        total.map(|value| value / count)
    }

    #[test]
    fn summed_area_averages_match_naive_reference_for_nonzero_origin() {
        let image = neighborhood_test_image(9, 7, (17, 23));
        let summed_area = IntegralImage::new(&image).unwrap();

        for radius in [1, 2, 4] {
            for y in 23..30 {
                for x in 17..26 {
                    let expected = naive_average_pixel(&image, x, y, radius);
                    let actual = summed_area.average(x, y, radius);
                    for channel in 0..4 {
                        assert!(
                            (actual[channel] - expected[channel]).abs() <= 1.0e-5,
                            "radius={radius} coordinate=({x},{y}) channel={channel}: {actual:?} != {expected:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn summed_area_query_work_is_bounded_by_a_constant_per_pixel() {
        let width = 64;
        let height = 64;
        let image = neighborhood_test_image(width, height, (31, 47));
        let summed_area = IntegralImage::new(&image).unwrap();

        for y in 47..(47 + height) {
            for x in 31..(31 + width) {
                let _ = summed_area.average(x, y, MAX_RADIUS);
            }
        }

        let pixel_count = width as usize * height as usize;
        assert_eq!(summed_area.build_work_units(), pixel_count);
        assert!(summed_area.query_work_units() <= pixel_count * 9);
    }
}
