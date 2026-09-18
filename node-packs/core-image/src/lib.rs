use rawweave_image::{Dimensions, Image, Region};
use rawweave_node_api::{
    EvaluationContext, ExecutionCapability, Inputs, NodeDescriptor, NodeError, NodeInstance,
    NodePack, NodeRegistry, NodeResult, ParameterDescriptor, ParameterValue, Parameters,
    PortDescriptor, Value,
};

fn set_cpu_region_capabilities(descriptor: &mut NodeDescriptor) {
    descriptor.capabilities = vec![ExecutionCapability::Cpu, ExecutionCapability::RegionAware];
}

fn set_cpu_tile_capabilities(descriptor: &mut NodeDescriptor) {
    descriptor.capabilities = vec![
        ExecutionCapability::Cpu,
        ExecutionCapability::TileLocal,
        ExecutionCapability::RegionAware,
    ];
}

fn image_input_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.image-input", "Image Input");
    descriptor
        .outputs
        .push(PortDescriptor::output("image", "Image", "core.Image"));
    set_cpu_region_capabilities(&mut descriptor);
    descriptor
}

fn exposure_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.exposure", "Exposure");
    descriptor
        .inputs
        .push(PortDescriptor::input("image", "Image", "core.Image", true));
    descriptor.inputs.push(PortDescriptor::input(
        "exposure",
        "Exposure",
        "value.Float",
        false,
    ));
    descriptor
        .outputs
        .push(PortDescriptor::output("image", "Image", "core.Image"));
    descriptor.parameters.push(ParameterDescriptor::float(
        "exposure", "Exposure", 0.0, None, None,
    ));
    set_cpu_tile_capabilities(&mut descriptor);
    descriptor
}

fn invert_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.invert", "Invert");
    descriptor
        .inputs
        .push(PortDescriptor::input("image", "Image", "core.Image", true));
    descriptor
        .outputs
        .push(PortDescriptor::output("image", "Image", "core.Image"));
    set_cpu_tile_capabilities(&mut descriptor);
    descriptor
}

fn output_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.output", "Output");
    descriptor
        .inputs
        .push(PortDescriptor::input("image", "Image", "core.Image", true));
    descriptor
        .outputs
        .push(PortDescriptor::output("image", "Image", "core.Image"));
    set_cpu_region_capabilities(&mut descriptor);
    descriptor
}

fn image_processing_descriptor(type_id: &str, name: &str) -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(type_id, name);
    descriptor
        .inputs
        .push(PortDescriptor::input("image", "Image", "core.Image", true));
    descriptor
        .outputs
        .push(PortDescriptor::output("image", "Image", "core.Image"));
    set_cpu_region_capabilities(&mut descriptor);
    descriptor
}

fn resize_descriptor() -> NodeDescriptor {
    let mut descriptor = image_processing_descriptor("core.resize", "Resize");
    descriptor.parameters.push(ParameterDescriptor::float(
        "width",
        "Width",
        1.0,
        Some(1.0),
        None,
    ));
    descriptor.parameters.push(ParameterDescriptor::float(
        "height",
        "Height",
        1.0,
        Some(1.0),
        None,
    ));
    descriptor
}

fn crop_descriptor() -> NodeDescriptor {
    let mut descriptor = image_processing_descriptor("core.crop", "Crop");
    descriptor
        .parameters
        .push(ParameterDescriptor::float("x", "X", 0.0, Some(0.0), None));
    descriptor
        .parameters
        .push(ParameterDescriptor::float("y", "Y", 0.0, Some(0.0), None));
    descriptor.parameters.push(ParameterDescriptor::float(
        "width",
        "Width",
        1.0,
        Some(1.0),
        None,
    ));
    descriptor.parameters.push(ParameterDescriptor::float(
        "height",
        "Height",
        1.0,
        Some(1.0),
        None,
    ));
    descriptor
}

fn blur_descriptor() -> NodeDescriptor {
    let mut descriptor = image_processing_descriptor("core.blur", "Blur");
    descriptor.parameters.push(ParameterDescriptor::float(
        "radius",
        "Radius",
        1.0,
        Some(0.0),
        Some(64.0),
    ));
    descriptor
}

fn levels_descriptor() -> NodeDescriptor {
    let mut descriptor = image_processing_descriptor("core.levels", "Levels");
    descriptor.parameters.push(ParameterDescriptor::float(
        "black_point",
        "Black Point",
        0.0,
        None,
        None,
    ));
    descriptor.parameters.push(ParameterDescriptor::float(
        "white_point",
        "White Point",
        1.0,
        None,
        None,
    ));
    descriptor.parameters.push(ParameterDescriptor::float(
        "gamma",
        "Gamma",
        1.0,
        Some(0.0001),
        None,
    ));
    descriptor
}

fn curves_descriptor() -> NodeDescriptor {
    let mut descriptor = image_processing_descriptor("core.curves", "Curves");
    descriptor.parameters.push(ParameterDescriptor::float(
        "gamma",
        "Gamma",
        1.0,
        Some(0.0001),
        None,
    ));
    descriptor
}

fn color_matrix_descriptor() -> NodeDescriptor {
    let mut descriptor = image_processing_descriptor("core.color-matrix", "Color Matrix");
    descriptor.capabilities = vec![
        ExecutionCapability::Cpu,
        ExecutionCapability::Gpu,
        ExecutionCapability::TileLocal,
        ExecutionCapability::RegionAware,
    ];
    for row in 0..4 {
        for column in 0..4 {
            let default = if row == column { 1.0 } else { 0.0 };
            descriptor.parameters.push(ParameterDescriptor::float(
                format!("m{row}{column}"),
                format!("Matrix {row}{column}"),
                default,
                None,
                None,
            ));
        }
    }
    for (id, name) in [
        ("offset_r", "Red Offset"),
        ("offset_g", "Green Offset"),
        ("offset_b", "Blue Offset"),
        ("offset_a", "Alpha Offset"),
    ] {
        descriptor
            .parameters
            .push(ParameterDescriptor::float(id, name, 0.0, None, None));
    }
    descriptor
}

struct ImageInput;

impl NodeInstance for ImageInput {
    fn evaluate(
        &self,
        _inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        context
            .source_image
            .clone()
            .map(|image| NodeResult::single("image", Value::Image(image)))
            .ok_or(NodeError::MissingSourceImage)
    }
}

struct Exposure;

impl NodeInstance for Exposure {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = image_input(inputs, "image")?;
        let exposure = inputs
            .get("exposure")
            .and_then(|value| match value {
                Value::Float(value) => Some(*value),
                _ => None,
            })
            .or_else(|| {
                parameters
                    .get("exposure")
                    .and_then(ParameterValue::as_float)
            })
            .ok_or_else(|| NodeError::InvalidParameter("exposure".to_owned()))?;
        if !exposure.is_finite() {
            return Err(NodeError::InvalidParameter("exposure".to_owned()));
        }
        let multiplier = 2.0_f32.powf(exposure);
        let output = map_image_region(&image, context, |[red, green, blue, alpha]| {
            [
                red * multiplier,
                green * multiplier,
                blue * multiplier,
                alpha,
            ]
        })?;
        Ok(NodeResult::single("image", Value::Image(output)))
    }
}

struct Invert;

impl NodeInstance for Invert {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = image_input(inputs, "image")?;
        let output = map_image_region(&image, context, |[red, green, blue, alpha]| {
            [1.0 - red, 1.0 - green, 1.0 - blue, alpha]
        })?;
        Ok(NodeResult::single("image", Value::Image(output)))
    }
}

struct Resize;

impl NodeInstance for Resize {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = image_input(inputs, "image")?;
        let width = integer_parameter(parameters, "width", 1)?;
        let height = integer_parameter(parameters, "height", 1)?;
        let target = Dimensions::new(width, height);
        let region = requested_region(target, context);
        let mut pixels = Vec::with_capacity(pixel_capacity(region));
        for y in 0..region.height {
            for x in 0..region.width {
                let output_x = region.x + x;
                let output_y = region.y + y;
                pixels.push(resample_nearest(&image, target, output_x, output_y));
            }
        }
        Ok(NodeResult::single(
            "image",
            Value::Image(image_from_pixels(&image, region.dimensions(), pixels)?),
        ))
    }
}

struct Crop;

impl NodeInstance for Crop {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = image_input(inputs, "image")?;
        let x = integer_parameter(parameters, "x", 0)?;
        let y = integer_parameter(parameters, "y", 0)?;
        let width = integer_parameter(parameters, "width", 1)?;
        let height = integer_parameter(parameters, "height", 1)?;
        let crop_region = Region::new(x, y, width, height);
        if !crop_region.is_inside(image.dimensions()) {
            return Err(NodeError::Message(format!(
                "crop region {crop_region:?} is outside image dimensions {:?}",
                image.dimensions()
            )));
        }
        let output_region = requested_region(crop_region.dimensions(), context);
        let mut pixels = Vec::with_capacity(pixel_capacity(output_region));
        for output_y in 0..output_region.height {
            for output_x in 0..output_region.width {
                let source_x = crop_region.x + output_region.x + output_x;
                let source_y = crop_region.y + output_region.y + output_y;
                pixels.push(image.pixel(source_x, source_y).ok_or_else(|| {
                    NodeError::Message("crop source pixel was outside the image".to_owned())
                })?);
            }
        }
        Ok(NodeResult::single(
            "image",
            Value::Image(image_from_pixels(
                &image,
                output_region.dimensions(),
                pixels,
            )?),
        ))
    }
}

struct Blur;

impl NodeInstance for Blur {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = image_input(inputs, "image")?;
        let radius = integer_parameter(parameters, "radius", 1)?;
        let region = requested_region(image.dimensions(), context);
        let mut pixels = Vec::with_capacity(pixel_capacity(region));
        for y in 0..region.height {
            for x in 0..region.width {
                pixels.push(blur_pixel(&image, region.x + x, region.y + y, radius));
            }
        }
        Ok(NodeResult::single(
            "image",
            Value::Image(image_from_pixels(&image, region.dimensions(), pixels)?),
        ))
    }
}

struct Levels;

impl NodeInstance for Levels {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = image_input(inputs, "image")?;
        let black_point = float_parameter(parameters, "black_point", 0.0)?;
        let white_point = float_parameter(parameters, "white_point", 1.0)?;
        let gamma = float_parameter(parameters, "gamma", 1.0)?;
        if white_point <= black_point || gamma <= 0.0 {
            return Err(NodeError::InvalidParameter("levels".to_owned()));
        }
        let output = map_image_region(&image, context, |[red, green, blue, alpha]| {
            [
                level_channel(red, black_point, white_point, gamma),
                level_channel(green, black_point, white_point, gamma),
                level_channel(blue, black_point, white_point, gamma),
                alpha,
            ]
        })?;
        Ok(NodeResult::single("image", Value::Image(output)))
    }
}

struct Curves;

impl NodeInstance for Curves {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = image_input(inputs, "image")?;
        let gamma = float_parameter(parameters, "gamma", 1.0)?;
        if gamma <= 0.0 {
            return Err(NodeError::InvalidParameter("gamma".to_owned()));
        }
        let output = map_image_region(&image, context, |[red, green, blue, alpha]| {
            [
                curve_channel(red, gamma),
                curve_channel(green, gamma),
                curve_channel(blue, gamma),
                alpha,
            ]
        })?;
        Ok(NodeResult::single("image", Value::Image(output)))
    }
}

struct ColorMatrix;

impl NodeInstance for ColorMatrix {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = image_input(inputs, "image")?;
        let mut matrix = [[0.0; 4]; 4];
        for (row, values) in matrix.iter_mut().enumerate() {
            for (column, value) in values.iter_mut().enumerate() {
                let default = if row == column { 1.0 } else { 0.0 };
                *value = matrix_parameter(parameters, row, column, default)?;
            }
        }
        let offsets = [
            optional_float_alias(parameters, &["offset_r", "offset_0"], 0.0)?,
            optional_float_alias(parameters, &["offset_g", "offset_1"], 0.0)?,
            optional_float_alias(parameters, &["offset_b", "offset_2"], 0.0)?,
            optional_float_alias(parameters, &["offset_a", "offset_3"], 0.0)?,
        ];
        let output = map_image_region(&image, context, |pixel| {
            let mut output = [0.0; 4];
            for row in 0..4 {
                output[row] = offsets[row]
                    + matrix[row][0] * pixel[0]
                    + matrix[row][1] * pixel[1]
                    + matrix[row][2] * pixel[2]
                    + matrix[row][3] * pixel[3];
            }
            output
        })?;
        Ok(NodeResult::single("image", Value::Image(output)))
    }
}

struct Output;

impl NodeInstance for Output {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        Ok(NodeResult::single("image", image_value(inputs, "image")?))
    }
}

fn image_value(inputs: &Inputs, port: &str) -> Result<Value, NodeError> {
    match inputs.get(port) {
        Some(Value::Image(image)) => Ok(Value::Image(image.clone())),
        Some(_) => Err(NodeError::InvalidParameter(port.to_owned())),
        None => Err(NodeError::MissingInput(port.to_owned())),
    }
}

fn image_input(inputs: &Inputs, port: &str) -> Result<Image, NodeError> {
    match image_value(inputs, port)? {
        Value::Image(image) => Ok(image),
        Value::Float(_) => unreachable!("image_value only returns images"),
    }
}

fn image_from_pixels(
    source: &Image,
    dimensions: Dimensions,
    pixels: Vec<[f32; 4]>,
) -> Result<Image, NodeError> {
    Image::from_pixels_with_color_metadata(
        dimensions.width,
        dimensions.height,
        pixels,
        source.pixel_format(),
        source.color_metadata(),
    )
    .map_err(|error| NodeError::Message(error.to_string()))
}

fn map_image_region(
    image: &Image,
    context: &EvaluationContext,
    mut map: impl FnMut([f32; 4]) -> [f32; 4],
) -> Result<Image, NodeError> {
    let region = requested_region(image.dimensions(), context);
    let mut pixels = Vec::with_capacity(pixel_capacity(region));
    for y in 0..region.height {
        for x in 0..region.width {
            let pixel = image.pixel(region.x + x, region.y + y).ok_or_else(|| {
                NodeError::Message("requested region was outside the image".to_owned())
            })?;
            pixels.push(map(pixel));
        }
    }
    image_from_pixels(image, region.dimensions(), pixels)
}

fn requested_region(dimensions: Dimensions, context: &EvaluationContext) -> Region {
    let full = Region::new(0, 0, dimensions.width, dimensions.height);
    match context.requested_region() {
        Some(region) => region
            .intersection(full)
            .unwrap_or_else(|| Region::new(0, 0, 0, 0)),
        None => full,
    }
}

fn pixel_capacity(region: Region) -> usize {
    (region.width as usize).saturating_mul(region.height as usize)
}

fn resample_nearest(image: &Image, target: Dimensions, x: u32, y: u32) -> [f32; 4] {
    if image.width() == 0 || image.height() == 0 {
        return [0.0; 4];
    }
    let source_x = ((x as u64 * image.width() as u64) / target.width as u64)
        .min(image.width() as u64 - 1) as u32;
    let source_y = ((y as u64 * image.height() as u64) / target.height as u64)
        .min(image.height() as u64 - 1) as u32;
    image.pixel(source_x, source_y).unwrap_or([0.0; 4])
}

fn blur_pixel(image: &Image, x: u32, y: u32, radius: u32) -> [f32; 4] {
    if image.width() == 0 || image.height() == 0 {
        return [0.0; 4];
    }
    let mut total = [0.0; 4];
    let mut count = 0.0;
    let radius = radius as i64;
    for offset_y in -radius..=radius {
        for offset_x in -radius..=radius {
            let source_x = (x as i64 + offset_x).clamp(0, image.width() as i64 - 1) as u32;
            let source_y = (y as i64 + offset_y).clamp(0, image.height() as i64 - 1) as u32;
            let pixel = image.pixel(source_x, source_y).unwrap_or([0.0; 4]);
            for channel in 0..4 {
                total[channel] += pixel[channel];
            }
            count += 1.0;
        }
    }
    total.map(|channel| channel / count)
}

fn level_channel(value: f32, black_point: f32, white_point: f32, gamma: f32) -> f32 {
    ((value - black_point) / (white_point - black_point))
        .clamp(0.0, 1.0)
        .powf(1.0 / gamma)
}

fn curve_channel(value: f32, gamma: f32) -> f32 {
    value.clamp(0.0, 1.0).powf(1.0 / gamma)
}

fn float_parameter(parameters: &Parameters, id: &str, default: f32) -> Result<f32, NodeError> {
    let value = parameters
        .get(id)
        .map_or(Ok(default), |value| match value {
            ParameterValue::Float(value) if value.is_finite() => Ok(*value),
            _ => Err(NodeError::InvalidParameter(id.to_owned())),
        })?;
    Ok(value)
}

fn integer_parameter(parameters: &Parameters, id: &str, default: u32) -> Result<u32, NodeError> {
    let value = float_parameter(parameters, id, default as f32)?;
    if value < 0.0 || value.fract() != 0.0 || value > u32::MAX as f32 {
        return Err(NodeError::InvalidParameter(id.to_owned()));
    }
    Ok(value as u32)
}

fn optional_float_alias(
    parameters: &Parameters,
    aliases: &[&str],
    default: f32,
) -> Result<f32, NodeError> {
    for alias in aliases {
        if parameters.contains_key(*alias) {
            return float_parameter(parameters, alias, default);
        }
    }
    Ok(default)
}

fn matrix_parameter(
    parameters: &Parameters,
    row: usize,
    column: usize,
    default: f32,
) -> Result<f32, NodeError> {
    let compact = format!("m{row}{column}");
    let separated = format!("matrix_{row}_{column}");
    let matrix = format!("matrix_{row}{column}");
    let legacy = format!("matrix{row}{column}");
    for alias in [&compact, &separated, &matrix, &legacy] {
        if parameters.contains_key(alias.as_str()) {
            return float_parameter(parameters, alias, default);
        }
    }
    Ok(default)
}

fn image_input_factory() -> Box<dyn NodeInstance> {
    Box::new(ImageInput)
}

fn exposure_factory() -> Box<dyn NodeInstance> {
    Box::new(Exposure)
}

fn invert_factory() -> Box<dyn NodeInstance> {
    Box::new(Invert)
}

fn resize_factory() -> Box<dyn NodeInstance> {
    Box::new(Resize)
}

fn crop_factory() -> Box<dyn NodeInstance> {
    Box::new(Crop)
}

fn blur_factory() -> Box<dyn NodeInstance> {
    Box::new(Blur)
}

fn levels_factory() -> Box<dyn NodeInstance> {
    Box::new(Levels)
}

fn curves_factory() -> Box<dyn NodeInstance> {
    Box::new(Curves)
}

fn color_matrix_factory() -> Box<dyn NodeInstance> {
    Box::new(ColorMatrix)
}

fn output_factory() -> Box<dyn NodeInstance> {
    Box::new(Output)
}

pub fn register_nodes(registry: &mut NodeRegistry) -> Result<(), rawweave_node_api::RegistryError> {
    registry.register(image_input_descriptor(), image_input_factory)?;
    registry.register(exposure_descriptor(), exposure_factory)?;
    registry.register(invert_descriptor(), invert_factory)?;
    registry.register(resize_descriptor(), resize_factory)?;
    registry.register(crop_descriptor(), crop_factory)?;
    registry.register(blur_descriptor(), blur_factory)?;
    registry.register(levels_descriptor(), levels_factory)?;
    registry.register(curves_descriptor(), curves_factory)?;
    registry.register(color_matrix_descriptor(), color_matrix_factory)?;
    registry.register(output_descriptor(), output_factory)
}

pub struct CoreImagePack;

impl NodePack for CoreImagePack {
    fn id(&self) -> &'static str {
        "core-image"
    }

    fn register(
        &self,
        registry: &mut NodeRegistry,
    ) -> Result<(), rawweave_node_api::RegistryError> {
        register_nodes(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::register_nodes;
    use rawweave_image::Image;
    use rawweave_node_api::{EvaluationContext, Inputs, NodeRegistry, Parameters, Value};

    #[test]
    fn exposure_and_invert_use_the_common_node_api() {
        let mut registry = NodeRegistry::default();
        register_nodes(&mut registry).unwrap();
        let exposure = registry.instantiate("core.exposure").unwrap();
        let image = Image::from_pixels(1, 1, vec![[0.25, 0.5, 0.75, 1.0]]).unwrap();
        let inputs = [("image".to_owned(), Value::Image(image))]
            .into_iter()
            .collect::<Inputs>();
        let parameters = [("exposure".to_owned(), 1.0_f32.into())]
            .into_iter()
            .collect::<Parameters>();
        let result = exposure
            .evaluate(&inputs, &parameters, &EvaluationContext::default())
            .unwrap();
        assert_eq!(
            result.outputs.get("image"),
            Some(&Value::Image(
                Image::from_pixels(1, 1, vec![[0.5, 1.0, 1.5, 1.0]]).unwrap()
            ))
        );
    }
}
