use rawweave_image::{
    Dimensions, Image, Mask, PaintMode, PaintPoint, PaintStroke, PaintedMask, Region,
};
use rawweave_node_api::{
    EvaluationContext, ExecutionCapability, Inputs, NodeDescriptor, NodeError, NodeInstance,
    NodePack, NodeRegistry, NodeResult, ParameterDescriptor, ParameterValue, Parameters,
    PortDescriptor, Value,
};

const MAX_IMAGE_PIXELS: u64 = 16_777_216;
const MAX_BLUR_RADIUS: u32 = 64;
const MAX_MASK_RADIUS: u32 = 64;

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

fn set_cpu_full_frame_capabilities(descriptor: &mut NodeDescriptor) {
    descriptor.capabilities = vec![
        ExecutionCapability::Cpu,
        ExecutionCapability::FullFrame,
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
    set_cpu_full_frame_capabilities(&mut descriptor);
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

fn mask_output_descriptor(type_id: &str, name: &str) -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(type_id, name);
    descriptor
        .outputs
        .push(PortDescriptor::output("mask", "Mask", "core.Mask"));
    set_cpu_tile_capabilities(&mut descriptor);
    descriptor
}

fn mask_image_source_descriptor(type_id: &str, name: &str) -> NodeDescriptor {
    let mut descriptor = mask_output_descriptor(type_id, name);
    descriptor
        .inputs
        .push(PortDescriptor::input("image", "Image", "core.Image", true));
    descriptor
}

fn select_label_descriptor(type_id: &str, name: &str) -> NodeDescriptor {
    let mut descriptor = mask_output_descriptor(type_id, name);
    descriptor.inputs.push(PortDescriptor::input(
        "label_map",
        "Label Map",
        "core.LabelMap",
        true,
    ));
    descriptor
        .parameters
        .push(ParameterDescriptor::string("label", "Label", ""));
    descriptor
        .parameters
        .push(ParameterDescriptor::integer("label_id", "Label ID", 0));
    descriptor
}

fn mask_unary_descriptor(type_id: &str, name: &str) -> NodeDescriptor {
    let mut descriptor = mask_output_descriptor(type_id, name);
    descriptor
        .inputs
        .push(PortDescriptor::input("mask", "Mask", "core.Mask", true));
    descriptor
}

fn mask_binary_descriptor(type_id: &str, name: &str) -> NodeDescriptor {
    let mut descriptor = mask_output_descriptor(type_id, name);
    descriptor
        .inputs
        .push(PortDescriptor::input("a", "A", "core.Mask", true));
    descriptor
        .inputs
        .push(PortDescriptor::input("b", "B", "core.Mask", true));
    descriptor
}

fn linear_gradient_descriptor() -> NodeDescriptor {
    let mut descriptor = mask_output_descriptor("core.mask-linear-gradient", "Linear Gradient");
    descriptor.inputs.push(PortDescriptor::input(
        "image",
        "Image (bounds)",
        "core.Image",
        false,
    ));
    for (id, name, default) in [
        ("start_x", "Start X", 0.0),
        ("start_y", "Start Y", 0.0),
        ("end_x", "End X", 1.0),
        ("end_y", "End Y", 0.0),
    ] {
        descriptor
            .parameters
            .push(ParameterDescriptor::float(id, name, default, None, None));
    }
    descriptor
}

fn radial_gradient_descriptor() -> NodeDescriptor {
    let mut descriptor = mask_output_descriptor("core.mask-radial-gradient", "Radial Gradient");
    descriptor.inputs.push(PortDescriptor::input(
        "image",
        "Image (bounds)",
        "core.Image",
        false,
    ));
    for (id, name, default, min) in [
        ("center_x", "Center X", 0.0, None),
        ("center_y", "Center Y", 0.0, None),
        ("radius", "Radius", 1.0, Some(0.0)),
        ("inner_radius", "Inner Radius", 0.0, Some(0.0)),
    ] {
        descriptor
            .parameters
            .push(ParameterDescriptor::float(id, name, default, min, None));
    }
    descriptor
}

fn painted_mask_descriptor() -> NodeDescriptor {
    let mut descriptor = mask_output_descriptor("core.mask-painted", "Painted Mask");
    descriptor.inputs.push(PortDescriptor::input(
        "image",
        "Image (bounds)",
        "core.Image",
        false,
    ));
    for (id, name, default, min) in [
        ("width", "Width", 1.0, Some(0.0)),
        ("height", "Height", 1.0, Some(0.0)),
        ("origin_x", "Origin X", 0.0, Some(0.0)),
        ("origin_y", "Origin Y", 0.0, Some(0.0)),
        ("x", "Brush X", 0.0, Some(0.0)),
        ("y", "Brush Y", 0.0, Some(0.0)),
        ("size", "Brush Size", 1.0, Some(0.0)),
        ("hardness", "Hardness", 1.0, Some(0.0)),
        ("opacity", "Opacity", 1.0, Some(0.0)),
    ] {
        descriptor.parameters.push(ParameterDescriptor::float(
            id,
            name,
            default,
            min,
            if matches!(id, "hardness" | "opacity") {
                Some(1.0)
            } else {
                None
            },
        ));
    }
    descriptor
        .parameters
        .push(ParameterDescriptor::string("mode", "Mode", "add"));
    descriptor
        .parameters
        .push(ParameterDescriptor::string("points", "Points", ""));
    descriptor
}

fn color_qualifier_descriptor() -> NodeDescriptor {
    let mut descriptor =
        mask_image_source_descriptor("core.mask-color-qualifier", "Color Qualifier");
    for (id, name, default, min, max) in [
        ("target_r", "Target Red", 1.0, 0.0, 1.0),
        ("target_g", "Target Green", 1.0, 0.0, 1.0),
        ("target_b", "Target Blue", 1.0, 0.0, 1.0),
        ("tolerance", "Tolerance", 0.1, 0.0, 2.0),
        ("softness", "Softness", 0.0, 0.0, 2.0),
    ] {
        descriptor.parameters.push(ParameterDescriptor::float(
            id,
            name,
            default,
            Some(min),
            Some(max),
        ));
    }
    descriptor
}

fn threshold_descriptor() -> NodeDescriptor {
    let mut descriptor = mask_unary_descriptor("core.mask-threshold", "Threshold");
    descriptor.parameters.push(ParameterDescriptor::float(
        "threshold",
        "Threshold",
        0.5,
        Some(0.0),
        Some(1.0),
    ));
    descriptor.parameters.push(ParameterDescriptor::float(
        "softness",
        "Softness",
        0.0,
        Some(0.0),
        Some(1.0),
    ));
    descriptor
}

fn radius_descriptor(type_id: &str, name: &str) -> NodeDescriptor {
    let mut descriptor = mask_unary_descriptor(type_id, name);
    descriptor.parameters.push(ParameterDescriptor::float(
        "radius",
        "Radius",
        1.0,
        Some(0.0),
        Some(MAX_MASK_RADIUS as f32),
    ));
    descriptor
}

fn local_exposure_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.local-exposure", "Local Exposure");
    descriptor
        .inputs
        .push(PortDescriptor::input("image", "Image", "core.Image", true));
    descriptor
        .inputs
        .push(PortDescriptor::input("mask", "Mask", "core.Mask", false));
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

struct PaintedMaskNode;

impl NodeInstance for PaintedMaskNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let bounds = spatial_bounds(inputs, context, parameters)?;
        let points = paint_points(parameters)?;
        let stroke = PaintStroke::new(
            points,
            float_parameter_alias(parameters, &["size", "brush_size"], 1.0)?,
            float_parameter_alias(parameters, &["hardness"], 1.0)?,
            float_parameter_alias(parameters, &["opacity"], 1.0)?,
            paint_mode(parameters),
        )
        .map_err(|error| NodeError::InvalidParameter(error.to_string()))?;
        let mut painted = PaintedMask::new(bounds.dimensions(), (bounds.x, bounds.y))
            .map_err(|error| NodeError::InvalidParameter(error.to_string()))?;
        painted
            .apply(stroke)
            .map_err(|error| NodeError::InvalidParameter(error.to_string()))?;
        let rendered = painted
            .render()
            .map_err(|error| NodeError::Message(error.to_string()))?;
        Ok(NodeResult::single(
            "mask",
            Value::Mask(mask_requested_region(&rendered, context)?),
        ))
    }
}

struct LinearGradient;

impl NodeInstance for LinearGradient {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let bounds = spatial_bounds(inputs, context, parameters)?;
        let region = requested_region(bounds, context);
        let start_x = float_parameter_alias(parameters, &["start_x", "x0"], 0.0)?;
        let start_y = float_parameter_alias(parameters, &["start_y", "y0"], 0.0)?;
        let end_x = float_parameter_alias(parameters, &["end_x", "x1"], 1.0)?;
        let end_y = float_parameter_alias(parameters, &["end_y", "y1"], 0.0)?;
        let dx = end_x - start_x;
        let dy = end_y - start_y;
        let length_squared = dx * dx + dy * dy;
        let values = region_values(region, |x, y| {
            if length_squared == 0.0 {
                1.0
            } else {
                (((x as f32 - start_x) * dx + (y as f32 - start_y) * dy) / length_squared)
                    .clamp(0.0, 1.0)
            }
        });
        Ok(NodeResult::single(
            "mask",
            Value::Mask(mask_from_region(region, values)?),
        ))
    }
}

struct RadialGradient;

impl NodeInstance for RadialGradient {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let bounds = spatial_bounds(inputs, context, parameters)?;
        let region = requested_region(bounds, context);
        let center_x = float_parameter_alias(parameters, &["center_x", "x"], 0.0)?;
        let center_y = float_parameter_alias(parameters, &["center_y", "y"], 0.0)?;
        let radius = float_parameter_alias(parameters, &["radius", "outer_radius"], 1.0)?;
        let inner_radius = float_parameter_alias(parameters, &["inner_radius"], 0.0)?;
        if radius < 0.0 || inner_radius < 0.0 || inner_radius > radius {
            return Err(NodeError::InvalidParameter("radius".to_owned()));
        }
        let values = region_values(region, |x, y| {
            let distance = ((x as f32 - center_x).powi(2) + (y as f32 - center_y).powi(2)).sqrt();
            if radius == inner_radius {
                if distance <= radius {
                    1.0
                } else {
                    0.0
                }
            } else {
                ((radius - distance) / (radius - inner_radius)).clamp(0.0, 1.0)
            }
        });
        Ok(NodeResult::single(
            "mask",
            Value::Mask(mask_from_region(region, values)?),
        ))
    }
}

struct LuminanceMask;

impl NodeInstance for LuminanceMask {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = image_input(inputs, "image")?;
        let region = requested_region(image.global_region(), context);
        let values = region_values(region, |x, y| {
            image
                .pixel_global(x, y)
                .map(|[red, green, blue, _]| {
                    (0.2126 * red + 0.7152 * green + 0.0722 * blue).clamp(0.0, 1.0)
                })
                .unwrap_or(0.0)
        });
        Ok(NodeResult::single(
            "mask",
            Value::Mask(mask_from_region(region, values)?),
        ))
    }
}

struct ColorQualifier;

impl NodeInstance for ColorQualifier {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = image_input(inputs, "image")?;
        let region = requested_region(image.global_region(), context);
        let target = qualifier_color(parameters)?;
        let tolerance = float_parameter_alias(parameters, &["tolerance", "radius"], 0.1)?;
        let softness = float_parameter_alias(parameters, &["softness", "feather"], 0.0)?;
        if tolerance < 0.0 || softness < 0.0 {
            return Err(NodeError::InvalidParameter("tolerance".to_owned()));
        }
        let values = region_values(region, |x, y| {
            let Some([red, green, blue, _]) = image.pixel_global(x, y) else {
                return 0.0;
            };
            let distance = ((red - target[0]).powi(2)
                + (green - target[1]).powi(2)
                + (blue - target[2]).powi(2))
            .sqrt();
            if softness == 0.0 {
                if distance <= tolerance {
                    1.0
                } else {
                    0.0
                }
            } else {
                ((tolerance + softness - distance) / softness).clamp(0.0, 1.0)
            }
        });
        Ok(NodeResult::single(
            "mask",
            Value::Mask(mask_from_region(region, values)?),
        ))
    }
}

struct SelectLabel;

impl NodeInstance for SelectLabel {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let label_map = match inputs.get("label_map") {
            Some(Value::LabelMap(label_map)) => label_map,
            Some(_) => return Err(NodeError::InvalidParameter("label_map".to_owned())),
            None => return Err(NodeError::MissingInput("label_map".to_owned())),
        };
        let label_id = match parameters.get("label") {
            Some(ParameterValue::String(label)) if !label.trim().is_empty() => label_map
                .label_value(label)
                .ok_or_else(|| NodeError::InvalidParameter("label".to_owned()))?,
            Some(ParameterValue::String(_)) | None => parameters
                .get("label_id")
                .and_then(ParameterValue::as_integer)
                .and_then(|value| u16::try_from(value).ok())
                .ok_or_else(|| NodeError::InvalidParameter("label_id".to_owned()))?,
            Some(_) => return Err(NodeError::InvalidParameter("label".to_owned())),
        };
        let region = requested_region(label_map.global_region(), context);
        let values = region_values(region, |x, y| {
            (label_map.pixel_global(x, y) == Some(label_id)) as u8 as f32
        });
        Ok(NodeResult::single(
            "mask",
            Value::Mask(mask_from_region(region, values)?),
        ))
    }
}

struct MaskInvert;

impl NodeInstance for MaskInvert {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let mask = mask_input(inputs, "mask")?;
        let output = map_mask_region(&mask, context, |value| 1.0 - value)?;
        Ok(NodeResult::single("mask", Value::Mask(output)))
    }
}

struct MaskAdd;
struct MaskSubtract;
struct MaskIntersect;
struct MaskMultiply;

macro_rules! impl_binary_mask_node {
    ($type:ty, $operation:expr, $intersection:expr) => {
        impl NodeInstance for $type {
            fn evaluate(
                &self,
                inputs: &Inputs,
                _parameters: &Parameters,
                context: &EvaluationContext,
            ) -> Result<NodeResult, NodeError> {
                let first = mask_input_any(inputs, &["a", "mask_a"])?;
                let second = mask_input_any(inputs, &["b", "mask_b"])?;
                let output = combine_masks(&first, &second, context, $operation, $intersection)?;
                Ok(NodeResult::single("mask", Value::Mask(output)))
            }
        }
    };
}

impl_binary_mask_node!(MaskAdd, |a: f32, b: f32| (a + b).min(1.0), false);
impl_binary_mask_node!(MaskSubtract, |a: f32, b: f32| (a - b).max(0.0), false);
impl_binary_mask_node!(MaskIntersect, |a: f32, b: f32| a.min(b), true);
impl_binary_mask_node!(MaskMultiply, |a: f32, b: f32| a * b, false);

struct MaskThreshold;

impl NodeInstance for MaskThreshold {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let mask = mask_input(inputs, "mask")?;
        let threshold = float_parameter_alias(parameters, &["threshold", "level"], 0.5)?;
        let softness = float_parameter_alias(parameters, &["softness", "feather"], 0.0)?;
        if !(0.0..=1.0).contains(&threshold) || softness < 0.0 {
            return Err(NodeError::InvalidParameter("threshold".to_owned()));
        }
        let output = map_mask_region(&mask, context, |value| {
            if softness == 0.0 {
                if value >= threshold {
                    1.0
                } else {
                    0.0
                }
            } else {
                ((value - (threshold - softness)) / (2.0 * softness)).clamp(0.0, 1.0)
            }
        })?;
        Ok(NodeResult::single("mask", Value::Mask(output)))
    }
}

struct MaskFeather;
struct MaskBlur;
struct MaskExpand;
struct MaskContract;

macro_rules! impl_filter_mask_node {
    ($type:ty, $operation:expr) => {
        impl NodeInstance for $type {
            fn evaluate(
                &self,
                inputs: &Inputs,
                parameters: &Parameters,
                context: &EvaluationContext,
            ) -> Result<NodeResult, NodeError> {
                let mask = mask_input(inputs, "mask")?;
                let radius = integer_parameter_alias(parameters, &["radius", "amount"], 1)?;
                if radius > MAX_MASK_RADIUS {
                    return Err(NodeError::InvalidParameter("radius".to_owned()));
                }
                let output = $operation(&mask, context, radius)?;
                Ok(NodeResult::single("mask", Value::Mask(output)))
            }
        }
    };
}

impl_filter_mask_node!(MaskFeather, blur_mask);
impl_filter_mask_node!(MaskBlur, blur_mask);
impl_filter_mask_node!(MaskExpand, expand_mask);
impl_filter_mask_node!(MaskContract, contract_mask);

struct LocalExposure;

impl NodeInstance for LocalExposure {
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
                Value::Integer(value) => Some(*value as f32),
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
        let mask = match inputs.get("mask") {
            Some(Value::Mask(mask)) => Some(mask),
            Some(_) => return Err(NodeError::InvalidParameter("mask".to_owned())),
            None => None,
        };
        let multiplier = 2.0_f32.powf(exposure);
        let region = requested_region(image.global_region(), context);
        let mut pixels = Vec::with_capacity(pixel_capacity(region));
        for y in 0..region.height {
            for x in 0..region.width {
                let global_x = region.x + x;
                let global_y = region.y + y;
                let [red, green, blue, alpha] =
                    image.pixel_global(global_x, global_y).ok_or_else(|| {
                        NodeError::Message("requested region was outside the image".to_owned())
                    })?;
                let weight = mask.map_or(1.0, |mask| {
                    mask.pixel_global(global_x, global_y).unwrap_or(0.0)
                });
                let multiplier = 1.0 + (multiplier - 1.0) * weight;
                pixels.push([
                    red * multiplier,
                    green * multiplier,
                    blue * multiplier,
                    alpha,
                ]);
            }
        }
        Ok(NodeResult::single(
            "image",
            Value::Image(image_from_region(&image, region, pixels)?),
        ))
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
        let target_pixels = u64::from(width)
            .checked_mul(u64::from(height))
            .ok_or_else(|| NodeError::InvalidParameter("dimensions".to_owned()))?;
        if width == 0 || height == 0 || target_pixels > MAX_IMAGE_PIXELS {
            return Err(NodeError::InvalidParameter("dimensions".to_owned()));
        }
        let target = Dimensions::new(width, height);
        let region = requested_region(Region::new(0, 0, target.width, target.height), context);
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
            Value::Image(image_from_region(&image, region, pixels)?),
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
        let local_crop_region = Region::new(x, y, width, height);
        if !local_crop_region.is_inside(image.dimensions()) {
            return Err(NodeError::Message(format!(
                "crop region {local_crop_region:?} is outside image dimensions {:?}",
                image.dimensions()
            )));
        }
        let (origin_x, origin_y) = image.origin();
        let crop_origin = (
            origin_x
                .checked_add(x)
                .ok_or_else(|| NodeError::Message("crop origin overflowed".to_owned()))?,
            origin_y
                .checked_add(y)
                .ok_or_else(|| NodeError::Message("crop origin overflowed".to_owned()))?,
        );
        let crop_region = Region::new(crop_origin.0, crop_origin.1, width, height);
        let output_region = requested_region(crop_region, context);
        let mut pixels = Vec::with_capacity(pixel_capacity(output_region));
        for output_y in 0..output_region.height {
            for output_x in 0..output_region.width {
                let source_x = output_region.x + output_x;
                let source_y = output_region.y + output_y;
                pixels.push(image.pixel_global(source_x, source_y).ok_or_else(|| {
                    NodeError::Message("crop source pixel was outside the image".to_owned())
                })?);
            }
        }
        Ok(NodeResult::single(
            "image",
            Value::Image(image_from_region(&image, output_region, pixels)?),
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
        if radius > MAX_BLUR_RADIUS {
            return Err(NodeError::InvalidParameter("radius".to_owned()));
        }
        let region = requested_region(image.global_region(), context);
        let mut pixels = Vec::with_capacity(pixel_capacity(region));
        for y in 0..region.height {
            for x in 0..region.width {
                pixels.push(blur_pixel(&image, region.x + x, region.y + y, radius));
            }
        }
        Ok(NodeResult::single(
            "image",
            Value::Image(image_from_region(&image, region, pixels)?),
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
        let cpu_output = || {
            map_image_region(&image, context, |pixel| {
                let mut output = [0.0; 4];
                for row in 0..4 {
                    output[row] = offsets[row]
                        + matrix[row][0] * pixel[0]
                        + matrix[row][1] * pixel[1]
                        + matrix[row][2] * pixel[2]
                        + matrix[row][3] * pixel[3];
                }
                output
            })
        };
        let output = if let Some(gpu) = context.render_context().and_then(|render| render.gpu()) {
            let region = requested_region(image.global_region(), context);
            let region_image = image_region(&image, region)?;
            match gpu.apply_color_matrix(&region_image, matrix, offsets) {
                Ok(output) => output,
                Err(gpu_error) => cpu_output().map_err(|cpu_error| {
                    NodeError::Message(format!(
                        "GPU color matrix failed: {gpu_error}; CPU fallback failed: {cpu_error}"
                    ))
                })?,
            }
        } else {
            cpu_output()?
        };
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

fn spatial_bounds(
    inputs: &Inputs,
    context: &EvaluationContext,
    parameters: &Parameters,
) -> Result<Region, NodeError> {
    if let Some(value) = inputs.get("image") {
        return match value {
            Value::Image(image) => Ok(image.global_region()),
            _ => Err(NodeError::InvalidParameter("image".to_owned())),
        };
    }
    if let Some(image) = context.source_image.as_ref() {
        return Ok(image.global_region());
    }
    let width = integer_parameter_alias(parameters, &["width"], 1)?;
    let height = integer_parameter_alias(parameters, &["height"], 1)?;
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| NodeError::InvalidParameter("dimensions".to_owned()))?;
    if pixels > MAX_IMAGE_PIXELS {
        return Err(NodeError::InvalidParameter("dimensions".to_owned()));
    }
    let origin_x = integer_parameter_alias(parameters, &["origin_x"], 0)?;
    let origin_y = integer_parameter_alias(parameters, &["origin_y"], 0)?;
    Ok(Region::new(origin_x, origin_y, width, height))
}

fn region_values(region: Region, mut value_at: impl FnMut(u32, u32) -> f32) -> Vec<f32> {
    let mut values = Vec::with_capacity(pixel_capacity(region));
    for y in 0..region.height {
        for x in 0..region.width {
            values.push(value_at(region.x + x, region.y + y));
        }
    }
    values
}

fn mask_from_region(region: Region, values: Vec<f32>) -> Result<Mask, NodeError> {
    Mask::from_values_with_origin(region.dimensions(), (region.x, region.y), values)
        .map_err(|error| NodeError::Message(error.to_string()))
}

fn mask_requested_region(mask: &Mask, context: &EvaluationContext) -> Result<Mask, NodeError> {
    let region = requested_region(mask.global_region(), context);
    let values = region_values(region, |x, y| mask.pixel_global(x, y).unwrap_or(0.0));
    mask_from_region(region, values)
}

fn map_mask_region(
    mask: &Mask,
    context: &EvaluationContext,
    mut map: impl FnMut(f32) -> f32,
) -> Result<Mask, NodeError> {
    let region = requested_region(mask.global_region(), context);
    let values = region_values(region, |x, y| map(mask.pixel_global(x, y).unwrap_or(0.0)));
    mask_from_region(region, values)
}

fn mask_input(inputs: &Inputs, port: &str) -> Result<Mask, NodeError> {
    match inputs.get(port) {
        Some(Value::Mask(mask)) => Ok(mask.clone()),
        Some(_) => Err(NodeError::InvalidParameter(port.to_owned())),
        None => Err(NodeError::MissingInput(port.to_owned())),
    }
}

fn mask_input_any(inputs: &Inputs, ports: &[&str]) -> Result<Mask, NodeError> {
    ports.iter().find_map(|port| inputs.get(*port)).map_or_else(
        || Err(NodeError::MissingInput(ports[0].to_owned())),
        |value| match value {
            Value::Mask(mask) => Ok(mask.clone()),
            _ => Err(NodeError::InvalidParameter(ports[0].to_owned())),
        },
    )
}

fn combine_masks(
    first: &Mask,
    second: &Mask,
    context: &EvaluationContext,
    operation: impl Fn(f32, f32) -> f32,
    intersection: bool,
) -> Result<Mask, NodeError> {
    let bounds = if intersection {
        first
            .global_region()
            .intersection(second.global_region())
            .unwrap_or_else(|| Region::new(first.origin().0, first.origin().1, 0, 0))
    } else {
        union_region(first.global_region(), second.global_region())?
    };
    let region = requested_region(bounds, context);
    let values = region_values(region, |x, y| {
        operation(
            first.pixel_global(x, y).unwrap_or(0.0),
            second.pixel_global(x, y).unwrap_or(0.0),
        )
    });
    mask_from_region(region, values)
}

fn union_region(first: Region, second: Region) -> Result<Region, NodeError> {
    let end_x = first
        .end_x()
        .ok_or_else(|| NodeError::Message("mask region overflowed".to_owned()))?
        .max(
            second
                .end_x()
                .ok_or_else(|| NodeError::Message("mask region overflowed".to_owned()))?,
        );
    let end_y = first
        .end_y()
        .ok_or_else(|| NodeError::Message("mask region overflowed".to_owned()))?
        .max(
            second
                .end_y()
                .ok_or_else(|| NodeError::Message("mask region overflowed".to_owned()))?,
        );
    let x = first.x.min(second.x);
    let y = first.y.min(second.y);
    Ok(Region::new(
        x,
        y,
        end_x
            .checked_sub(x)
            .ok_or_else(|| NodeError::Message("mask region overflowed".to_owned()))?,
        end_y
            .checked_sub(y)
            .ok_or_else(|| NodeError::Message("mask region overflowed".to_owned()))?,
    ))
}

fn blur_mask(mask: &Mask, context: &EvaluationContext, radius: u32) -> Result<Mask, NodeError> {
    let region = requested_region(mask.global_region(), context);
    if radius == 0 {
        return mask_requested_region(mask, context);
    }
    let radius = i64::from(radius);
    let values = region_values(region, |x, y| {
        let mut total = 0.0;
        let mut count = 0.0;
        for offset_y in -radius..=radius {
            for offset_x in -radius..=radius {
                total += mask_pixel_clamped(mask, i64::from(x) + offset_x, i64::from(y) + offset_y);
                count += 1.0;
            }
        }
        total / count
    });
    mask_from_region(region, values)
}

fn expand_mask(mask: &Mask, context: &EvaluationContext, radius: u32) -> Result<Mask, NodeError> {
    morphology_mask(mask, context, radius, f32::max, 0.0)
}

fn contract_mask(mask: &Mask, context: &EvaluationContext, radius: u32) -> Result<Mask, NodeError> {
    morphology_mask(mask, context, radius, f32::min, 0.0)
}

fn morphology_mask(
    mask: &Mask,
    context: &EvaluationContext,
    radius: u32,
    operation: impl Fn(f32, f32) -> f32,
    outside: f32,
) -> Result<Mask, NodeError> {
    let region = requested_region(mask.global_region(), context);
    let radius = i64::from(radius);
    let values = region_values(region, |x, y| {
        let mut value = outside;
        for offset_y in -radius..=radius {
            for offset_x in -radius..=radius {
                let sample = mask_pixel(mask, i64::from(x) + offset_x, i64::from(y) + offset_y)
                    .unwrap_or(outside);
                value = operation(value, sample);
            }
        }
        value
    });
    mask_from_region(region, values)
}

fn mask_pixel(mask: &Mask, x: i64, y: i64) -> Option<f32> {
    (x >= 0 && y >= 0)
        .then_some((x as u32, y as u32))
        .and_then(|(x, y)| mask.pixel_global(x, y))
}

fn mask_pixel_clamped(mask: &Mask, x: i64, y: i64) -> f32 {
    let region = mask.global_region();
    if region.width == 0 || region.height == 0 {
        return 0.0;
    }
    let max_x = i64::from(region.end_x().unwrap_or(u32::MAX)) - 1;
    let max_y = i64::from(region.end_y().unwrap_or(u32::MAX)) - 1;
    let x = x.clamp(i64::from(region.x), max_x) as u32;
    let y = y.clamp(i64::from(region.y), max_y) as u32;
    mask.pixel_global(x, y).unwrap_or(0.0)
}

fn paint_points(parameters: &Parameters) -> Result<Vec<PaintPoint>, NodeError> {
    if let Some(ParameterValue::String(serialized)) = parameters.get("points") {
        let mut points = Vec::new();
        for pair in serialized.split(';').filter(|pair| !pair.trim().is_empty()) {
            let mut values = pair.split(',').map(str::trim);
            let x = values
                .next()
                .and_then(|value| value.parse::<f32>().ok())
                .ok_or_else(|| NodeError::InvalidParameter("points".to_owned()))?;
            let y = values
                .next()
                .and_then(|value| value.parse::<f32>().ok())
                .ok_or_else(|| NodeError::InvalidParameter("points".to_owned()))?;
            points.push(PaintPoint::new(x, y));
        }
        if !points.is_empty() {
            return Ok(points);
        }
    }
    Ok(vec![PaintPoint::new(
        float_parameter_alias(parameters, &["x", "center_x"], 0.0)?,
        float_parameter_alias(parameters, &["y", "center_y"], 0.0)?,
    )])
}

fn paint_mode(parameters: &Parameters) -> PaintMode {
    match parameters.get("mode") {
        Some(ParameterValue::String(value)) if value.eq_ignore_ascii_case("subtract") => {
            PaintMode::Subtract
        }
        _ => PaintMode::Add,
    }
}

fn qualifier_color(parameters: &Parameters) -> Result<[f32; 3], NodeError> {
    if let Some(ParameterValue::String(value)) = parameters.get("color") {
        let value = value.trim().trim_start_matches('#');
        let channels = if value.len() == 6 {
            [
                u8::from_str_radix(&value[0..2], 16).ok().map(f32::from),
                u8::from_str_radix(&value[2..4], 16).ok().map(f32::from),
                u8::from_str_radix(&value[4..6], 16).ok().map(f32::from),
            ]
            .map(|channel| channel.map(|channel| channel / 255.0))
        } else {
            let mut parsed = [None; 3];
            for (index, channel) in value.split(',').enumerate().take(3) {
                parsed[index] = channel.trim().parse::<f32>().ok();
            }
            parsed
        };
        if let [Some(red), Some(green), Some(blue)] = channels {
            return Ok([red, green, blue]);
        }
        return Err(NodeError::InvalidParameter("color".to_owned()));
    }
    Ok([
        float_parameter_alias(parameters, &["target_r", "red", "r", "color_r"], 1.0)?,
        float_parameter_alias(parameters, &["target_g", "green", "g", "color_g"], 1.0)?,
        float_parameter_alias(parameters, &["target_b", "blue", "b", "color_b"], 1.0)?,
    ])
}

fn float_parameter_alias(
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

fn integer_parameter_alias(
    parameters: &Parameters,
    aliases: &[&str],
    default: u32,
) -> Result<u32, NodeError> {
    for alias in aliases {
        if let Some(value) = parameters.get(*alias) {
            let value = match value {
                ParameterValue::Integer(value) if *value >= 0 => *value as f64,
                ParameterValue::Float(value) if value.is_finite() => f64::from(*value),
                _ => return Err(NodeError::InvalidParameter((*alias).to_owned())),
            };
            if value.fract() != 0.0 || value > f64::from(u32::MAX) {
                return Err(NodeError::InvalidParameter((*alias).to_owned()));
            }
            return Ok(value as u32);
        }
    }
    Ok(default)
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
        _ => Err(NodeError::InvalidParameter(port.to_owned())),
    }
}

fn image_from_region(
    source: &Image,
    region: Region,
    pixels: Vec<[f32; 4]>,
) -> Result<Image, NodeError> {
    Image::from_pixels_with_origin(
        region.dimensions(),
        (region.x, region.y),
        pixels,
        source.pixel_format(),
        source.color_metadata(),
    )
    .map_err(|error| NodeError::Message(error.to_string()))
}

fn image_region(image: &Image, region: Region) -> Result<Image, NodeError> {
    if region.width == 0 || region.height == 0 {
        return Image::from_pixels_with_origin(
            region.dimensions(),
            (region.x, region.y),
            Vec::new(),
            image.pixel_format(),
            image.color_metadata(),
        )
        .map_err(|error| NodeError::Message(error.to_string()));
    }
    let (origin_x, origin_y) = image.origin();
    let local_region = Region::new(
        region.x.checked_sub(origin_x).ok_or_else(|| {
            NodeError::Message("requested region was outside the image".to_owned())
        })?,
        region.y.checked_sub(origin_y).ok_or_else(|| {
            NodeError::Message("requested region was outside the image".to_owned())
        })?,
        region.width,
        region.height,
    );
    image
        .view(local_region)
        .and_then(|view| view.to_image())
        .map_err(|error| NodeError::Message(error.to_string()))
}

fn map_image_region(
    image: &Image,
    context: &EvaluationContext,
    mut map: impl FnMut([f32; 4]) -> [f32; 4],
) -> Result<Image, NodeError> {
    let region = requested_region(image.global_region(), context);
    let mut pixels = Vec::with_capacity(pixel_capacity(region));
    for y in 0..region.height {
        for x in 0..region.width {
            let pixel = image
                .pixel_global(region.x + x, region.y + y)
                .ok_or_else(|| {
                    NodeError::Message("requested region was outside the image".to_owned())
                })?;
            pixels.push(map(pixel));
        }
    }
    image_from_region(image, region, pixels)
}

fn requested_region(full: Region, context: &EvaluationContext) -> Region {
    match context.requested_region() {
        Some(region) => region
            .intersection(full)
            .unwrap_or_else(|| Region::new(region.x, region.y, 0, 0)),
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
    let bounds = image.global_region();
    let min_x = bounds.x as i64;
    let min_y = bounds.y as i64;
    let max_x = bounds.end_x().unwrap_or(u32::MAX) as i64 - 1;
    let max_y = bounds.end_y().unwrap_or(u32::MAX) as i64 - 1;
    let mut total = [0.0; 4];
    let mut count = 0.0;
    let radius = radius as i64;
    for offset_y in -radius..=radius {
        for offset_x in -radius..=radius {
            let source_x = (x as i64 + offset_x).clamp(min_x, max_x) as u32;
            let source_y = (y as i64 + offset_y).clamp(min_y, max_y) as u32;
            let pixel = image.pixel_global(source_x, source_y).unwrap_or([0.0; 4]);
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

fn painted_mask_factory() -> Box<dyn NodeInstance> {
    Box::new(PaintedMaskNode)
}

fn linear_gradient_factory() -> Box<dyn NodeInstance> {
    Box::new(LinearGradient)
}

fn radial_gradient_factory() -> Box<dyn NodeInstance> {
    Box::new(RadialGradient)
}

fn luminance_mask_factory() -> Box<dyn NodeInstance> {
    Box::new(LuminanceMask)
}

fn color_qualifier_factory() -> Box<dyn NodeInstance> {
    Box::new(ColorQualifier)
}

fn select_label_factory() -> Box<dyn NodeInstance> {
    Box::new(SelectLabel)
}

fn mask_invert_factory() -> Box<dyn NodeInstance> {
    Box::new(MaskInvert)
}

fn mask_add_factory() -> Box<dyn NodeInstance> {
    Box::new(MaskAdd)
}

fn mask_subtract_factory() -> Box<dyn NodeInstance> {
    Box::new(MaskSubtract)
}

fn mask_intersect_factory() -> Box<dyn NodeInstance> {
    Box::new(MaskIntersect)
}

fn mask_multiply_factory() -> Box<dyn NodeInstance> {
    Box::new(MaskMultiply)
}

fn mask_threshold_factory() -> Box<dyn NodeInstance> {
    Box::new(MaskThreshold)
}

fn mask_feather_factory() -> Box<dyn NodeInstance> {
    Box::new(MaskFeather)
}

fn mask_blur_factory() -> Box<dyn NodeInstance> {
    Box::new(MaskBlur)
}

fn mask_expand_factory() -> Box<dyn NodeInstance> {
    Box::new(MaskExpand)
}

fn mask_contract_factory() -> Box<dyn NodeInstance> {
    Box::new(MaskContract)
}

fn local_exposure_factory() -> Box<dyn NodeInstance> {
    Box::new(LocalExposure)
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
    registry.register(painted_mask_descriptor(), painted_mask_factory)?;
    registry.register(linear_gradient_descriptor(), linear_gradient_factory)?;
    registry.register(radial_gradient_descriptor(), radial_gradient_factory)?;
    registry.register(
        mask_image_source_descriptor("core.mask-luminance", "Luminance Mask"),
        luminance_mask_factory,
    )?;
    registry.register(color_qualifier_descriptor(), color_qualifier_factory)?;
    registry.register(
        select_label_descriptor("core.select-label", "Select Label"),
        select_label_factory,
    )?;
    registry.register(
        select_label_descriptor("core.label-map-select", "Label Map Select"),
        select_label_factory,
    )?;
    registry.register(
        mask_unary_descriptor("core.mask-invert", "Mask Invert"),
        mask_invert_factory,
    )?;
    registry.register(
        mask_binary_descriptor("core.mask-add", "Mask Add"),
        mask_add_factory,
    )?;
    registry.register(
        mask_binary_descriptor("core.mask-subtract", "Mask Subtract"),
        mask_subtract_factory,
    )?;
    registry.register(
        mask_binary_descriptor("core.mask-intersect", "Mask Intersect"),
        mask_intersect_factory,
    )?;
    registry.register(
        mask_binary_descriptor("core.mask-multiply", "Mask Multiply"),
        mask_multiply_factory,
    )?;
    registry.register(threshold_descriptor(), mask_threshold_factory)?;
    registry.register(
        radius_descriptor("core.mask-feather", "Mask Feather"),
        mask_feather_factory,
    )?;
    registry.register(
        radius_descriptor("core.mask-blur", "Mask Blur"),
        mask_blur_factory,
    )?;
    registry.register(
        radius_descriptor("core.mask-expand", "Mask Expand"),
        mask_expand_factory,
    )?;
    registry.register(
        radius_descriptor("core.mask-contract", "Mask Contract"),
        mask_contract_factory,
    )?;
    registry.register(local_exposure_descriptor(), local_exposure_factory)?;
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
