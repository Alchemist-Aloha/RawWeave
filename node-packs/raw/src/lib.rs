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
use rawweave_raw::{CfaColor, Mosaic, RawDecoder, RawError, RawFrame, RawloaderDecoder};

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
        true,
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
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let bytes = match required_input(inputs, "bytes")? {
            Value::Bytes(bytes) => bytes,
            Value::Image(_)
            | Value::Float(_)
            | Value::RawFrame(_)
            | Value::Mosaic(_)
            | Value::SceneLinearRGB(_)
            | Value::DisplayRGB(_)
            | Value::CameraMetadata(_)
            | Value::ExifMetadata(_) => {
                return Err(NodeError::InvalidParameter("bytes".to_owned()));
            }
        };
        let frame = self.decoder.decode(bytes).map_err(raw_error)?;
        Ok(NodeResult::new(
            [
                ("frame".to_owned(), Value::RawFrame(frame.clone())),
                ("mosaic".to_owned(), Value::Mosaic(frame.mosaic().clone())),
                (
                    "camera".to_owned(),
                    Value::CameraMetadata(frame.camera().clone()),
                ),
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
        let scene = SceneLinearRGB::new(dimensions, pixels, WorkingSpace::Srgb)
            .map_err(|error| NodeError::Message(error.to_string()))?;
        Ok(NodeResult::single("scene", Value::SceneLinearRGB(scene)))
    }
}

struct CameraTransform;

impl NodeInstance for CameraTransform {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let scene = scene_input(inputs, "scene")?;
        Ok(NodeResult::single("scene", Value::SceneLinearRGB(scene)))
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
        Ok(NodeResult::single("scene", Value::SceneLinearRGB(scene)))
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
        | Value::ExifMetadata(_) => Err(NodeError::InvalidParameter(port.to_owned())),
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
        | Value::ExifMetadata(_) => Err(NodeError::InvalidParameter(port.to_owned())),
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
        | Value::ExifMetadata(_) => Err(NodeError::InvalidParameter(port.to_owned())),
    }
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
