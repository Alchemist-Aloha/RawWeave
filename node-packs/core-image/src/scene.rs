//! Typed scene ports for operations whose linear-light meaning is well defined.
use rawweave_color::{MatrixWorkingSpaceTransform, SceneLinearRGB, SceneTransform, WorkingSpace};
use rawweave_image::{ColorMetadata, Dimensions, Image, Region};
use rawweave_node_api::{
    EvaluationContext, ExecutionCapability, Inputs, NodeDescriptor, NodeError, NodeInstance,
    NodeResult, Parameters, PortDescriptor, Value,
};

use super::{
    MAX_BLUR_RADIUS, MAX_IMAGE_PIXELS, float_parameter, integer_parameter, matrix_parameters,
};

pub(super) fn add_ports(descriptor: NodeDescriptor) -> NodeDescriptor {
    rawweave_node_api::with_scene_ports(descriptor)
}

// Reuse the ordinary node unchanged; scene evaluation never converts/copies its input buffer.
struct CompatibleNode {
    type_id: &'static str,
    image_node: Box<dyn NodeInstance>,
}

pub(super) fn compatible(
    type_id: &'static str,
    image_node: Box<dyn NodeInstance>,
) -> Box<dyn NodeInstance> {
    Box::new(CompatibleNode {
        type_id,
        image_node,
    })
}

impl NodeInstance for CompatibleNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let Some(value) = inputs.get("scene") else {
            return self.image_node.evaluate(inputs, parameters, context);
        };
        if inputs.contains_key("image") {
            return Err(NodeError::Message(
                "connect either Image or Scene Linear RGB, not both".into(),
            ));
        }
        let Value::SceneLinearRGB(scene) = value else {
            return Err(NodeError::InvalidParameter("scene".into()));
        };
        let dimensions = scene.dimensions();
        let count = dimensions
            .pixel_count()
            .map_err(|error| NodeError::Message(error.to_string()))?;
        if count as u64 > MAX_IMAGE_PIXELS {
            return Err(NodeError::Message(
                "scene exceeds the node pixel budget".into(),
            ));
        }
        // Scene values are whole-frame rasters, with an optional origin-anchored preview grid.
        // Do not crop them to requested_region: the type cannot represent a regional origin.
        let result = match self.type_id {
            "core.output" => scene.clone(),
            "core.exposure" | "core.local-exposure" => {
                let exposure = match inputs.get("exposure") {
                    Some(Value::Float(value)) => *value,
                    Some(Value::Integer(value)) if self.type_id == "core.local-exposure" => {
                        *value as f32
                    }
                    Some(_) => return Err(NodeError::InvalidParameter("exposure".into())),
                    None => float_parameter(parameters, "exposure", 0.0)?,
                };
                let multiplier = 2.0_f32.powf(exposure);
                if !exposure.is_finite() || !multiplier.is_finite() {
                    return Err(NodeError::InvalidParameter("exposure".into()));
                }
                let mask = if self.type_id == "core.local-exposure" {
                    match inputs.get("mask") {
                        Some(Value::Mask(mask)) => Some(mask),
                        Some(_) => return Err(NodeError::InvalidParameter("mask".into())),
                        None => None,
                    }
                } else {
                    None
                };
                let scale = scene.sampling().map_or(1, |sampling| 1_u32 << sampling.mip);
                let mut index = 0_usize;
                scene
                    .map_pixels(|pixel| {
                        let weight = mask.map_or(1.0, |mask| {
                            let x = index % dimensions.width as usize;
                            let y = index / dimensions.width as usize;
                            mask.pixel_global(x as u32 * scale, y as u32 * scale)
                                .unwrap_or(0.0)
                        });
                        index += 1;
                        let gain = 1.0 + (multiplier - 1.0) * weight;
                        pixel.map(|channel| channel * gain)
                    })
                    .map_err(color_error)?
            }
            "core.levels" => {
                let black = float_parameter(parameters, "black_point", 0.0)?;
                let white = float_parameter(parameters, "white_point", 1.0)?;
                let gamma = float_parameter(parameters, "gamma", 1.0)?;
                let span = white - black;
                if !black.is_finite()
                    || !white.is_finite()
                    || !span.is_finite()
                    || !gamma.is_finite()
                    || span <= 0.0
                    || gamma <= 0.0
                {
                    return Err(NodeError::InvalidParameter("levels".into()));
                }
                scene
                    .map_pixels(|pixel| {
                        pixel.map(|value| signed_power((value - black) / span, gamma.recip()))
                    })
                    .map_err(color_error)?
            }
            "core.curves" => {
                let gamma = float_parameter(parameters, "gamma", 1.0)?;
                if !gamma.is_finite() || gamma <= 0.0 {
                    return Err(NodeError::InvalidParameter("gamma".into()));
                }
                scene
                    .map_pixels(|pixel| pixel.map(|value| signed_power(value, gamma.recip())))
                    .map_err(color_error)?
            }
            "core.invert" => scene
                .map_pixels(|pixel| pixel.map(|value| 1.0 - value))
                .map_err(color_error)?,
            "core.blur" => {
                let radius = integer_parameter(parameters, "radius", 1)?;
                if radius > MAX_BLUR_RADIUS {
                    return Err(NodeError::InvalidParameter("radius".into()));
                }
                let mut index = 0_usize;
                let bounds = Region::new(0, 0, dimensions.width, dimensions.height);
                scene
                    .map_pixels(|_| {
                        let x = (index % dimensions.width as usize) as u32;
                        let y = (index / dimensions.width as usize) as u32;
                        index += 1;
                        let [r, g, b, _] =
                            super::blur_pixel_with_bounds(bounds, x, y, radius, |x, y| {
                                scene
                                    .pixel(x, y)
                                    .map(|[r, g, b]| [r, g, b, 1.0])
                                    .unwrap_or([0.0; 4])
                            });
                        [r, g, b]
                    })
                    .map_err(color_error)?
            }
            "core.resize" => {
                let width = integer_parameter(parameters, "width", 1)?;
                let height = integer_parameter(parameters, "height", 1)?;
                if width == 0
                    || height == 0
                    || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
                {
                    return Err(NodeError::InvalidParameter("dimensions".into()));
                }
                let target = Dimensions::new(width, height);
                let count = target
                    .pixel_count()
                    .map_err(|error| NodeError::Message(error.to_string()))?;
                let mut pixels = Vec::with_capacity(count);
                for y in 0..height {
                    for x in 0..width {
                        let source_x =
                            (u64::from(x) * u64::from(dimensions.width) / u64::from(width)) as u32;
                        let source_y = (u64::from(y) * u64::from(dimensions.height)
                            / u64::from(height)) as u32;
                        pixels.push(scene.pixel(source_x, source_y).ok_or_else(|| {
                            NodeError::Message("cannot resize an empty scene".into())
                        })?);
                    }
                }
                // Resize defines a new full-size grid, so the input preview sampling no longer applies.
                SceneLinearRGB::new(target, pixels, scene.working_space()).map_err(color_error)?
            }
            "core.color-matrix" => {
                let (matrix, offsets) = matrix_parameters(parameters)?;
                if matrix[3] != [0.0, 0.0, 0.0, 1.0] || offsets[3] != 0.0 {
                    return Err(NodeError::Message(
                        "scene RGB has no alpha channel; leave the alpha row unchanged".into(),
                    ));
                }
                // A real CPU path for scene RGB, even when the RGBA node can use a GPU.
                scene
                    .map_pixels(|pixel| {
                        std::array::from_fn(|row| {
                            offsets[row]
                                + matrix[row][0] * pixel[0]
                                + matrix[row][1] * pixel[1]
                                + matrix[row][2] * pixel[2]
                                + matrix[row][3]
                        })
                    })
                    .map_err(color_error)?
            }
            _ => return Err(NodeError::Message("unsupported scene operation".into())),
        };
        Ok(NodeResult::single("scene", Value::SceneLinearRGB(result)))
    }
}

// Extend the existing gamma curve across signed, unbounded scene values.
fn signed_power(value: f32, power: f32) -> f32 {
    value.signum() * value.abs().powf(power)
}

fn color_error(error: rawweave_color::ColorError) -> NodeError {
    NodeError::Message(error.to_string())
}

pub(super) fn conversion_descriptor() -> NodeDescriptor {
    let mut descriptor =
        NodeDescriptor::new("core.scene-linear-to-image", "Scene Linear RGB to Image");
    descriptor.inputs.push(PortDescriptor::input(
        "scene",
        "Scene Linear RGB",
        "color.SceneLinearRGB",
        true,
    ));
    descriptor.outputs.push(PortDescriptor::output(
        "image",
        "Linear sRGB Image",
        "core.Image",
    ));
    // Image cannot carry PreviewSampling. Ask upstream RAW nodes for the full-resolution raster.
    descriptor.capabilities = vec![
        ExecutionCapability::Cpu,
        ExecutionCapability::FullFrame,
        ExecutionCapability::MipInvariant,
    ];
    descriptor
}

struct SceneToImage;

impl NodeInstance for SceneToImage {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let scene = match inputs.get("scene") {
            Some(Value::SceneLinearRGB(scene)) => scene,
            Some(_) => return Err(NodeError::InvalidParameter("scene".into())),
            None => return Err(NodeError::MissingInput("scene".into())),
        };
        if scene.sampling().is_some() {
            return Err(NodeError::Message(
                "conversion requires a full-resolution scene, not a sampled preview".into(),
            ));
        }
        if scene.pixels().len() as u64 > MAX_IMAGE_PIXELS {
            return Err(NodeError::Message(
                "scene exceeds the node pixel budget".into(),
            ));
        }
        // Preserve color meaning: Image can explicitly identify linear sRGB, not every scene space.
        // CameraNative/custom primaries require a supplied camera/working-space transform first.
        let linear = MatrixWorkingSpaceTransform::new(WorkingSpace::Srgb)
            .transform(scene)
            .map_err(color_error)?;
        let pixels = linear
            .pixels()
            .iter()
            .map(|[r, g, b]| [*r, *g, *b, 1.0])
            .collect();
        let image = Image::from_pixels_with_origin(
            linear.dimensions(),
            (0, 0),
            pixels,
            rawweave_image::PixelFormat::Rgba32Float,
            ColorMetadata::default(),
        )
        .map_err(|error| NodeError::Message(error.to_string()))?;
        Ok(NodeResult::single("image", Value::Image(image)))
    }
}

pub(super) fn conversion_factory() -> Box<dyn NodeInstance> {
    Box::new(SceneToImage)
}
