//! RAW processing nodes built on the common graph value and node APIs.

use std::sync::Arc;

use rawweave_color::{
    DisplayTransform as DisplayTransformTrait, SceneLinearRGB, SrgbDisplayTransform, WorkingSpace,
};
use rawweave_node_api::{
    EvaluationContext, ExecutionCapability, Inputs, NodeDescriptor, NodeError, NodeInstance,
    NodePack, NodeRegistry, NodeResult, ParameterDescriptor, ParameterValue, Parameters,
    PortDescriptor, Value,
};
use rawweave_raw::{
    CfaColor, LensProfile, Mosaic, RawDecoder, RawError, RawFrame, RawloaderDecoder,
};

const DECODE: &str = "raw.decode";
const BLACK_LEVEL: &str = "raw.black-level";
const WHITE_BALANCE: &str = "raw.white-balance";
const HIGHLIGHT_RECONSTRUCTION: &str = "raw.highlight-reconstruction";
const DEMOSAIC: &str = "raw.demosaic";
const CAMERA_TRANSFORM: &str = "raw.camera-transform";
const LENS_CORRECTION: &str = "raw.lens-correction";
const DISPLAY_TRANSFORM: &str = "raw.display-transform";

fn full_frame_capabilities(descriptor: &mut NodeDescriptor) {
    descriptor.capabilities = vec![ExecutionCapability::Cpu, ExecutionCapability::FullFrame];
}

/// Descriptor for the RAW Decode node.
pub fn raw_decode_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(DECODE, "RAW Decode");
    descriptor.inputs.push(PortDescriptor::input(
        "bytes",
        "RAW Bytes",
        "core.Bytes",
        false,
    ));
    descriptor
        .outputs
        .push(PortDescriptor::output("frame", "RAW Frame", "raw.Frame"));
    descriptor
        .outputs
        .push(PortDescriptor::output("mosaic", "Mosaic", "raw.Mosaic"));
    descriptor.outputs.push(PortDescriptor::output(
        "camera",
        "Camera Metadata",
        "raw.CameraMetadata",
    ));
    descriptor.outputs.push(PortDescriptor::output(
        "camera_profile",
        "Camera Profile",
        "raw.CameraProfile",
    ));
    descriptor.outputs.push(PortDescriptor::output(
        "lens_profile",
        "Lens Profile",
        "raw.LensProfile",
    ));
    descriptor.outputs.push(PortDescriptor::output(
        "exif",
        "EXIF Metadata",
        "raw.ExifMetadata",
    ));
    descriptor.outputs.push(PortDescriptor::output(
        "preview",
        "Embedded Preview",
        "core.Bytes",
    ));
    full_frame_capabilities(&mut descriptor);
    descriptor
}

/// Descriptor for black-level normalization.
pub fn black_level_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(BLACK_LEVEL, "Black Level");
    descriptor.inputs.push(PortDescriptor::input(
        "frame",
        "RAW Frame",
        "raw.Frame",
        true,
    ));
    descriptor
        .outputs
        .push(PortDescriptor::output("mosaic", "Mosaic", "raw.Mosaic"));
    full_frame_capabilities(&mut descriptor);
    descriptor
}

/// Descriptor for per-channel white balance gains.
pub fn white_balance_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(WHITE_BALANCE, "White Balance");
    descriptor.inputs.push(PortDescriptor::input(
        "mosaic",
        "Mosaic",
        "raw.Mosaic",
        true,
    ));
    descriptor.parameters.push(ParameterDescriptor::float(
        "red_gain",
        "Red Gain",
        1.0,
        Some(0.0),
        None,
    ));
    descriptor.parameters.push(ParameterDescriptor::float(
        "green_gain",
        "Green Gain",
        1.0,
        Some(0.0),
        None,
    ));
    descriptor.parameters.push(ParameterDescriptor::float(
        "blue_gain",
        "Blue Gain",
        1.0,
        Some(0.0),
        None,
    ));
    descriptor
        .outputs
        .push(PortDescriptor::output("mosaic", "Mosaic", "raw.Mosaic"));
    full_frame_capabilities(&mut descriptor);
    descriptor
}

/// Descriptor for highlight recovery without scene-linear clipping.
pub fn highlight_reconstruction_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(HIGHLIGHT_RECONSTRUCTION, "Highlight Reconstruction");
    descriptor.inputs.push(PortDescriptor::input(
        "mosaic",
        "Mosaic",
        "raw.Mosaic",
        true,
    ));
    descriptor.parameters.push(ParameterDescriptor::float(
        "threshold",
        "Threshold",
        1.0,
        Some(0.0),
        None,
    ));
    descriptor.parameters.push(ParameterDescriptor::float(
        "strength",
        "Recovery Strength",
        1.0,
        Some(0.0),
        Some(1.0),
    ));
    descriptor
        .outputs
        .push(PortDescriptor::output("mosaic", "Mosaic", "raw.Mosaic"));
    full_frame_capabilities(&mut descriptor);
    descriptor
}

/// Descriptor for the initial bilinear CFA demosaic.
pub fn demosaic_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(DEMOSAIC, "Demosaic");
    descriptor.inputs.push(PortDescriptor::input(
        "mosaic",
        "Mosaic",
        "raw.Mosaic",
        true,
    ));
    descriptor.outputs.push(PortDescriptor::output(
        "scene",
        "Scene Linear RGB",
        "color.SceneLinearRGB",
    ));
    full_frame_capabilities(&mut descriptor);
    descriptor
}

/// Descriptor for the camera-profile stage.
pub fn camera_transform_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(CAMERA_TRANSFORM, "Camera Transform");
    descriptor.inputs.push(PortDescriptor::input(
        "scene",
        "Scene Linear RGB",
        "color.SceneLinearRGB",
        true,
    ));
    descriptor.inputs.push(PortDescriptor::input(
        "camera_profile",
        "Camera Profile",
        "raw.CameraProfile",
        false,
    ));
    descriptor.parameters.push(ParameterDescriptor::string(
        "working_space",
        "Working Space",
        "sRGB",
    ));
    descriptor.outputs.push(PortDescriptor::output(
        "scene",
        "Scene Linear RGB",
        "color.SceneLinearRGB",
    ));
    full_frame_capabilities(&mut descriptor);
    descriptor
}

/// Descriptor for lens correction.
pub fn lens_correction_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(LENS_CORRECTION, "Lens Correction");
    descriptor.inputs.push(PortDescriptor::input(
        "scene",
        "Scene Linear RGB",
        "color.SceneLinearRGB",
        true,
    ));
    descriptor.inputs.push(PortDescriptor::input(
        "lens_profile",
        "Lens Profile",
        "raw.LensProfile",
        false,
    ));
    descriptor.outputs.push(PortDescriptor::output(
        "scene",
        "Scene Linear RGB",
        "color.SceneLinearRGB",
    ));
    full_frame_capabilities(&mut descriptor);
    descriptor
}

/// Descriptor for the display transfer stage.
pub fn display_transform_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(DISPLAY_TRANSFORM, "Display Transform");
    descriptor.inputs.push(PortDescriptor::input(
        "scene",
        "Scene Linear RGB",
        "color.SceneLinearRGB",
        true,
    ));
    descriptor.outputs.push(PortDescriptor::output(
        "display",
        "Display RGB",
        "color.DisplayRGB",
    ));
    full_frame_capabilities(&mut descriptor);
    descriptor
}

/// RAW Decode node with an injected decoder boundary.
pub struct RawDecode {
    decoder: Arc<dyn RawDecoder>,
}

impl RawDecode {
    pub fn new(decoder: Arc<dyn RawDecoder>) -> Self {
        Self { decoder }
    }
}

impl NodeInstance for RawDecode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let frame = match inputs.get("bytes") {
            Some(Value::Bytes(bytes)) => self.decoder.decode(bytes).map_err(raw_error)?,
            Some(_) => return Err(NodeError::InvalidParameter("bytes".to_owned())),
            None => {
                if let Some(bytes) = context.source_bytes.as_deref() {
                    self.decoder.decode(bytes).map_err(raw_error)?
                } else if let Some(path) = context.source_path.as_deref() {
                    let bytes = std::fs::read(path).map_err(|error| {
                        NodeError::Message(format!("failed to read RAW source: {error}"))
                    })?;
                    self.decoder.decode(&bytes).map_err(raw_error)?
                } else {
                    return Err(NodeError::MissingInput("bytes".to_owned()));
                }
            }
        };
        let lens_profile = frame.lens_profile().cloned().unwrap_or_else(|| {
            LensProfile::identity(
                frame
                    .camera()
                    .lens
                    .clone()
                    .unwrap_or_else(|| "Unknown lens".to_owned()),
            )
        });
        Ok(NodeResult::new(
            [
                ("frame".to_owned(), Value::RawFrame(frame.clone())),
                ("mosaic".to_owned(), Value::Mosaic(frame.mosaic().clone())),
                (
                    "camera".to_owned(),
                    Value::CameraMetadata(frame.camera().clone()),
                ),
                (
                    "camera_profile".to_owned(),
                    Value::CameraProfile(frame.profile().clone()),
                ),
                ("lens_profile".to_owned(), Value::LensProfile(lens_profile)),
                ("exif".to_owned(), Value::ExifMetadata(frame.exif().clone())),
                (
                    "preview".to_owned(),
                    Value::Bytes(frame.embedded_preview().unwrap_or_default().to_vec()),
                ),
            ]
            .into_iter()
            .collect(),
        ))
    }
}

struct BlackLevel;

impl NodeInstance for BlackLevel {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let frame = raw_frame_input(inputs, "frame")?;
        let mosaic = frame
            .mosaic()
            .map_samples(|_, sample, color| {
                let channel = cfa_channel(color);
                let black = frame.black_levels()[channel];
                let white = frame.white_levels()[channel];
                let range = white - black;
                if range > 0.0 {
                    (sample - black) / range
                } else {
                    0.0
                }
            })
            .map_err(raw_error)?;
        Ok(NodeResult::single("mosaic", Value::Mosaic(mosaic)))
    }
}

struct WhiteBalance;

impl NodeInstance for WhiteBalance {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let mosaic = mosaic_input(inputs, "mosaic")?;
        let gains = [
            parameter_alias(parameters, "red_gain", &["red"], 1.0)?,
            parameter_alias(parameters, "green_gain", &["green"], 1.0)?,
            parameter_alias(parameters, "blue_gain", &["blue"], 1.0)?,
        ];
        let balanced = mosaic
            .map_samples(|_, sample, color| sample * gains[cfa_channel(color).min(2)])
            .map_err(raw_error)?;
        Ok(NodeResult::single("mosaic", Value::Mosaic(balanced)))
    }
}

struct HighlightReconstruction;

impl NodeInstance for HighlightReconstruction {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let mosaic = mosaic_input(inputs, "mosaic")?;
        let threshold = float_parameter(parameters, "threshold", 1.0)?;
        let strength = float_parameter(parameters, "strength", 1.0)?;
        if threshold < 0.0 || !(0.0..=1.0).contains(&strength) {
            return Err(NodeError::InvalidParameter(
                "highlight reconstruction".to_owned(),
            ));
        }
        let recovered = mosaic
            .map_samples(|_, sample, _| {
                if sample > threshold {
                    threshold + (sample - threshold) * strength
                } else {
                    sample
                }
            })
            .map_err(raw_error)?;
        Ok(NodeResult::single("mosaic", Value::Mosaic(recovered)))
    }
}

struct Demosaic;

impl NodeInstance for Demosaic {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let mosaic = mosaic_input(inputs, "mosaic")?;
        let dimensions = mosaic.dimensions();
        let mut pixels = Vec::with_capacity(
            dimensions
                .pixel_count()
                .map_err(|_| NodeError::Message("demosaic dimensions overflow".to_owned()))?,
        );
        for y in 0..dimensions.height {
            for x in 0..dimensions.width {
                pixels.push([
                    demosaic_channel(&mosaic, x, y, CfaColor::Red),
                    demosaic_channel(&mosaic, x, y, CfaColor::Green),
                    demosaic_channel(&mosaic, x, y, CfaColor::Blue),
                ]);
            }
        }
        let scene = SceneLinearRGB::new(dimensions, pixels, WorkingSpace::CameraNative)
            .map_err(|error| NodeError::Message(error.to_string()))?;
        Ok(NodeResult::single("scene", Value::SceneLinearRGB(scene)))
    }
}

struct CameraTransform;

impl NodeInstance for CameraTransform {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let scene = scene_input(inputs, "scene")?;
        let working_space = working_space_parameter(parameters)?;
        let profile = match inputs.get("camera_profile") {
            None => None,
            Some(Value::CameraProfile(profile)) => Some(profile),
            Some(_) => return Err(NodeError::InvalidParameter("camera_profile".to_owned())),
        };
        let transformed = match profile {
            Some(profile) => scene
                .map_pixels(|pixel| {
                    [
                        dot(profile.xyz_to_camera[0], pixel),
                        dot(profile.xyz_to_camera[1], pixel),
                        dot(profile.xyz_to_camera[2], pixel),
                    ]
                })
                .map_err(|error| NodeError::Message(error.to_string()))?,
            None => scene,
        }
        .with_working_space(working_space);
        Ok(NodeResult::single(
            "scene",
            Value::SceneLinearRGB(transformed),
        ))
    }
}

struct LensCorrection;

impl NodeInstance for LensCorrection {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let scene = scene_input(inputs, "scene")?;
        let profile = match inputs.get("lens_profile") {
            None => return Ok(NodeResult::single("scene", Value::SceneLinearRGB(scene))),
            Some(Value::LensProfile(profile)) => profile,
            Some(_) => return Err(NodeError::InvalidParameter("lens_profile".to_owned())),
        };
        if profile.is_identity() {
            return Ok(NodeResult::single("scene", Value::SceneLinearRGB(scene)));
        }

        let dimensions = scene.dimensions();
        let width = dimensions.width;
        let height = dimensions.height;
        let source = &scene;
        let pixels = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    let nx = normalized_coordinate(x, width);
                    let ny = normalized_coordinate(y, height);
                    let radius_squared = nx * nx + ny * ny;
                    let radial = 1.0
                        + profile.radial_distortion[0] * radius_squared
                        + profile.radial_distortion[1] * radius_squared.powi(2)
                        + profile.radial_distortion[2] * radius_squared.powi(3);
                    let tangential_x = 2.0 * profile.tangential_distortion[0] * nx * ny
                        + profile.tangential_distortion[1] * (radius_squared + 2.0 * nx * nx);
                    let tangential_y = profile.tangential_distortion[0]
                        * (radius_squared + 2.0 * ny * ny)
                        + 2.0 * profile.tangential_distortion[1] * nx * ny;
                    let source_x = denormalize_coordinate(
                        (nx * radial + tangential_x).clamp(-1.0, 1.0),
                        width,
                    );
                    let source_y = denormalize_coordinate(
                        (ny * radial + tangential_y).clamp(-1.0, 1.0),
                        height,
                    );
                    let mut pixel = bilinear_sample(source, source_x, source_y);
                    let vignette = 1.0
                        + profile.vignette[0] * radius_squared
                        + profile.vignette[1] * radius_squared.powi(2)
                        + profile.vignette[2] * radius_squared.powi(3);
                    pixel.iter_mut().for_each(|channel| *channel *= vignette);
                    pixel
                })
            })
            .collect();
        let corrected = SceneLinearRGB::new(dimensions, pixels, scene.working_space())
            .map_err(|error| NodeError::Message(error.to_string()))?;
        Ok(NodeResult::single(
            "scene",
            Value::SceneLinearRGB(corrected),
        ))
    }
}

struct DisplayTransform;

impl NodeInstance for DisplayTransform {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let scene = scene_input(inputs, "scene")?;
        let display = SrgbDisplayTransform
            .transform(&scene)
            .map_err(|error| NodeError::Message(error.to_string()))?;
        Ok(NodeResult::single("display", Value::DisplayRGB(display)))
    }
}

fn required_input<'a>(inputs: &'a Inputs, port: &str) -> Result<&'a Value, NodeError> {
    inputs
        .get(port)
        .ok_or_else(|| NodeError::MissingInput(port.to_owned()))
}

fn raw_frame_input(inputs: &Inputs, port: &str) -> Result<RawFrame, NodeError> {
    match required_input(inputs, port)? {
        Value::RawFrame(frame) => Ok(frame.clone()),
        Value::Image(_)
        | Value::Float(_)
        | Value::Bytes(_)
        | Value::Mosaic(_)
        | Value::SceneLinearRGB(_)
        | Value::DisplayRGB(_)
        | Value::CameraMetadata(_)
        | Value::ExifMetadata(_)
        | Value::CameraProfile(_)
        | Value::LensProfile(_) => Err(NodeError::InvalidParameter(port.to_owned())),
    }
}

fn mosaic_input(inputs: &Inputs, port: &str) -> Result<Mosaic, NodeError> {
    match required_input(inputs, port)? {
        Value::Mosaic(mosaic) => Ok(mosaic.clone()),
        Value::Image(_)
        | Value::Float(_)
        | Value::Bytes(_)
        | Value::RawFrame(_)
        | Value::SceneLinearRGB(_)
        | Value::DisplayRGB(_)
        | Value::CameraMetadata(_)
        | Value::ExifMetadata(_)
        | Value::CameraProfile(_)
        | Value::LensProfile(_) => Err(NodeError::InvalidParameter(port.to_owned())),
    }
}

fn scene_input(inputs: &Inputs, port: &str) -> Result<SceneLinearRGB, NodeError> {
    match required_input(inputs, port)? {
        Value::SceneLinearRGB(scene) => Ok(scene.clone()),
        Value::Image(_)
        | Value::Float(_)
        | Value::Bytes(_)
        | Value::RawFrame(_)
        | Value::Mosaic(_)
        | Value::DisplayRGB(_)
        | Value::CameraMetadata(_)
        | Value::ExifMetadata(_)
        | Value::CameraProfile(_)
        | Value::LensProfile(_) => Err(NodeError::InvalidParameter(port.to_owned())),
    }
}

fn dot(matrix_row: [f32; 3], pixel: [f32; 3]) -> f32 {
    matrix_row[0] * pixel[0] + matrix_row[1] * pixel[1] + matrix_row[2] * pixel[2]
}

fn working_space_parameter(parameters: &Parameters) -> Result<WorkingSpace, NodeError> {
    let Some(value) = parameters.get("working_space") else {
        return Ok(WorkingSpace::Srgb);
    };
    let ParameterValue::String(value) = value else {
        return Err(NodeError::InvalidParameter("working_space".to_owned()));
    };
    match value.as_str() {
        "sRGB" | "sRGB-linear" | "Srgb" => Ok(WorkingSpace::Srgb),
        "CameraNative" | "camera-native" => Ok(WorkingSpace::CameraNative),
        "DisplayP3" | "display-p3" => Ok(WorkingSpace::DisplayP3),
        "ProPhoto" | "prophoto" => Ok(WorkingSpace::ProPhoto),
        "Rec2020" | "rec2020" => Ok(WorkingSpace::Rec2020),
        value if value.starts_with("custom:") => Ok(WorkingSpace::Custom(value[7..].to_owned())),
        _ => Err(NodeError::InvalidParameter("working_space".to_owned())),
    }
}

fn normalized_coordinate(index: u32, size: u32) -> f32 {
    if size <= 1 {
        0.0
    } else {
        (index as f32 / (size - 1) as f32) * 2.0 - 1.0
    }
}

fn denormalize_coordinate(value: f32, size: u32) -> f32 {
    if size <= 1 {
        0.0
    } else {
        (value + 1.0) * (size - 1) as f32 * 0.5
    }
}

fn bilinear_sample(scene: &SceneLinearRGB, x: f32, y: f32) -> [f32; 3] {
    let max_x = scene.dimensions().width.saturating_sub(1) as f32;
    let max_y = scene.dimensions().height.saturating_sub(1) as f32;
    let x = x.clamp(0.0, max_x);
    let y = y.clamp(0.0, max_y);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = x0
        .saturating_add(1)
        .min(scene.dimensions().width.saturating_sub(1));
    let y1 = y0
        .saturating_add(1)
        .min(scene.dimensions().height.saturating_sub(1));
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let p00 = scene.pixel(x0, y0).unwrap_or([0.0; 3]);
    let p10 = scene.pixel(x1, y0).unwrap_or(p00);
    let p01 = scene.pixel(x0, y1).unwrap_or(p00);
    let p11 = scene.pixel(x1, y1).unwrap_or(p01);
    std::array::from_fn(|channel| {
        let top = p00[channel] + (p10[channel] - p00[channel]) * tx;
        let bottom = p01[channel] + (p11[channel] - p01[channel]) * tx;
        top + (bottom - top) * ty
    })
}

fn cfa_channel(color: CfaColor) -> usize {
    match color {
        CfaColor::Red => 0,
        CfaColor::Green => 1,
        CfaColor::Blue => 2,
        CfaColor::Extra | CfaColor::Unknown => 1,
    }
}

fn demosaic_channel(mosaic: &Mosaic, x: u32, y: u32, wanted: CfaColor) -> f32 {
    if mosaic.cfa().color_at(x, y) == Some(wanted) {
        return mosaic.sample(x, y).unwrap_or(0.0);
    }
    let mut total = 0.0;
    let mut count = 0_u32;
    for offset_y in -1_i64..=1 {
        for offset_x in -1_i64..=1 {
            let sample_x = x as i64 + offset_x;
            let sample_y = y as i64 + offset_y;
            if sample_x < 0
                || sample_y < 0
                || sample_x >= i64::from(mosaic.dimensions().width)
                || sample_y >= i64::from(mosaic.dimensions().height)
            {
                continue;
            }
            let sample_x = sample_x as u32;
            let sample_y = sample_y as u32;
            if mosaic.cfa().color_at(sample_x, sample_y) == Some(wanted) {
                total += mosaic.sample(sample_x, sample_y).unwrap_or(0.0);
                count += 1;
            }
        }
    }
    if count > 0 {
        total / count as f32
    } else {
        mosaic.sample(x, y).unwrap_or(0.0)
    }
}

fn float_parameter(parameters: &Parameters, id: &str, default: f32) -> Result<f32, NodeError> {
    match parameters.get(id) {
        None => Ok(default),
        Some(ParameterValue::Float(value)) if value.is_finite() => Ok(*value),
        Some(_) => Err(NodeError::InvalidParameter(id.to_owned())),
    }
}

fn parameter_alias(
    parameters: &Parameters,
    id: &str,
    aliases: &[&str],
    default: f32,
) -> Result<f32, NodeError> {
    if parameters.contains_key(id) {
        return float_parameter(parameters, id, default);
    }
    for alias in aliases {
        if parameters.contains_key(*alias) {
            return float_parameter(parameters, alias, default);
        }
    }
    Ok(default)
}

fn raw_error(error: RawError) -> NodeError {
    NodeError::Message(error.to_string())
}

fn black_level_factory() -> Box<dyn NodeInstance> {
    Box::new(BlackLevel)
}

fn white_balance_factory() -> Box<dyn NodeInstance> {
    Box::new(WhiteBalance)
}

fn highlight_reconstruction_factory() -> Box<dyn NodeInstance> {
    Box::new(HighlightReconstruction)
}

fn demosaic_factory() -> Box<dyn NodeInstance> {
    Box::new(Demosaic)
}

fn camera_transform_factory() -> Box<dyn NodeInstance> {
    Box::new(CameraTransform)
}

fn lens_correction_factory() -> Box<dyn NodeInstance> {
    Box::new(LensCorrection)
}

fn display_transform_factory() -> Box<dyn NodeInstance> {
    Box::new(DisplayTransform)
}

/// Register RAW nodes using an explicitly selected decoder.
pub fn register_nodes_with_decoder(
    registry: &mut NodeRegistry,
    decoder: Arc<dyn RawDecoder>,
) -> Result<(), rawweave_node_api::RegistryError> {
    let decode_decoder = Arc::clone(&decoder);
    registry.register_factory(raw_decode_descriptor(), move || {
        Box::new(RawDecode::new(Arc::clone(&decode_decoder)))
    })?;
    registry.register(black_level_descriptor(), black_level_factory)?;
    registry.register(white_balance_descriptor(), white_balance_factory)?;
    registry.register(
        highlight_reconstruction_descriptor(),
        highlight_reconstruction_factory,
    )?;
    registry.register(demosaic_descriptor(), demosaic_factory)?;
    registry.register(camera_transform_descriptor(), camera_transform_factory)?;
    registry.register(lens_correction_descriptor(), lens_correction_factory)?;
    registry.register(display_transform_descriptor(), display_transform_factory)
}

/// Register RAW nodes with any decoder implementation.
pub fn register_nodes_with_decoder_instance<D: RawDecoder + 'static>(
    registry: &mut NodeRegistry,
    decoder: D,
) -> Result<(), rawweave_node_api::RegistryError> {
    register_nodes_with_decoder(registry, Arc::new(decoder))
}

/// Register RAW nodes with the default rawloader adapter.
pub fn register_nodes(registry: &mut NodeRegistry) -> Result<(), rawweave_node_api::RegistryError> {
    register_nodes_with_decoder(registry, Arc::new(RawloaderDecoder))
}

/// Node pack containing the initial RAW pipeline stages.
pub struct RawNodePack {
    decoder: Arc<dyn RawDecoder>,
}

impl Default for RawNodePack {
    fn default() -> Self {
        Self {
            decoder: Arc::new(RawloaderDecoder),
        }
    }
}

impl RawNodePack {
    /// Construct a pack using a decoder implementation, typically a deterministic test decoder.
    pub fn with_decoder<D: RawDecoder + 'static>(decoder: D) -> Self {
        Self {
            decoder: Arc::new(decoder),
        }
    }

    /// Construct a pack from a shared decoder object.
    pub fn with_decoder_arc(decoder: Arc<dyn RawDecoder>) -> Self {
        Self { decoder }
    }
}

impl NodePack for RawNodePack {
    fn id(&self) -> &'static str {
        "raw"
    }

    fn register(
        &self,
        registry: &mut NodeRegistry,
    ) -> Result<(), rawweave_node_api::RegistryError> {
        register_nodes_with_decoder(registry, Arc::clone(&self.decoder))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawweave_raw::{DeterministicCorpus, DeterministicDecoder};

    #[test]
    fn default_descriptors_are_registered() {
        let mut registry = NodeRegistry::default();
        register_nodes_with_decoder_instance(
            &mut registry,
            DeterministicDecoder::new(DeterministicCorpus::bayer_12_bit()),
        )
        .unwrap();
        assert_eq!(registry.descriptors().len(), 8);
        assert_eq!(
            registry.descriptor(DISPLAY_TRANSFORM).unwrap().outputs[0].data_type,
            "color.DisplayRGB"
        );
    }
}
