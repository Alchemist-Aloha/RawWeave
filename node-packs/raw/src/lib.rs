//! RAW processing nodes built on the common graph value and node APIs.

use std::sync::Arc;

use rawweave_color::{
    DisplayTransform as DisplayTransformTrait, MatrixWorkingSpaceTransform, PreviewSampling,
    SceneLinearRGB, SrgbDisplayTransform, WorkingSpace,
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
    if matches!(
        descriptor.type_id.as_str(),
        DECODE | BLACK_LEVEL | WHITE_BALANCE | HIGHLIGHT_RECONSTRUCTION
    ) {
        descriptor
            .capabilities
            .push(ExecutionCapability::MipInvariant);
    }
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
        "raw.EmbeddedPreview",
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
                    self.decoder.decode_file(path).map_err(raw_error)?
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
        let outputs = [
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
                Value::EmbeddedPreview(frame.embedded_preview().clone()),
            ),
        ]
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
        Ok(NodeResult::new(outputs))
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
        validate_rgb_cfa(frame.mosaic())?;
        let mosaic = frame
            .mosaic()
            .map_samples(|_, sample, color| {
                let channel = cfa_channel(color).expect("validated RGB CFA");
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
        validate_rgb_cfa(&mosaic)?;
        let gains = [
            parameter_alias(parameters, "red_gain", &["red"], 1.0)?,
            parameter_alias(parameters, "green_gain", &["green"], 1.0)?,
            parameter_alias(parameters, "blue_gain", &["blue"], 1.0)?,
        ];
        let balanced = mosaic
            .map_samples(|_, sample, color| {
                sample * gains[cfa_channel(color).expect("validated RGB CFA")]
            })
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
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let mosaic = mosaic_input(inputs, "mosaic")?;
        validate_rgb_cfa(&mosaic)?;
        let full_dimensions = mosaic.dimensions();
        let mip = if context.quality() == rawweave_rendering::PreviewQuality::Final {
            0
        } else {
            context.mip_level()
        };
        let sampling = (mip > 0).then_some(PreviewSampling {
            full_dimensions,
            mip,
        });
        let dimensions = match sampling {
            Some(sampling) => sampling
                .dimensions()
                .map_err(|error| NodeError::Message(error.to_string()))?,
            None => full_dimensions,
        };
        let scale = 1_u32 << mip;
        let mut pixels = Vec::with_capacity(
            dimensions
                .pixel_count()
                .map_err(|_| NodeError::Message("demosaic dimensions overflow".to_owned()))?,
        );
        let bayer = bayer_channels(&mosaic);
        for row in 0..dimensions.height {
            for column in 0..dimensions.width {
                let x = column * scale;
                let y = row * scale;
                let pixel =
                    bayer.and_then(|channels| bayer_demosaic_pixel(&mosaic, channels, x, y));
                pixels.push(match pixel {
                    Some(pixel) => pixel,
                    None => demosaic_pixel(&mosaic, x, y)?,
                });
            }
        }
        let scene = SceneLinearRGB::new(dimensions, pixels, WorkingSpace::CameraNative)
            .and_then(|scene| scene.with_sampling(sampling))
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
            Some(profile) => {
                if profile.xyz_to_camera[3].iter().any(|value| *value != 0.0) {
                    return Err(NodeError::Message(
                        "unsupported camera profile: fourth channel is not supported".to_owned(),
                    ));
                }
                if profile.is_identity() {
                    scene.with_working_space(working_space)
                } else {
                    let xyz_pixels = scene
                        .pixels()
                        .iter()
                        .map(|pixel| {
                            [
                                dot(profile.camera_to_xyz[0], *pixel),
                                dot(profile.camera_to_xyz[1], *pixel),
                                dot(profile.camera_to_xyz[2], *pixel),
                            ]
                        })
                        .collect();
                    MatrixWorkingSpaceTransform::new(working_space)
                        .transform_xyz(scene.dimensions(), xyz_pixels)
                        .and_then(|converted| converted.with_sampling(scene.sampling()))
                        .map_err(|error| NodeError::Message(error.to_string()))?
                }
            }
            None => scene.with_working_space(working_space),
        };
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
        let reference = scene
            .sampling()
            .map_or(dimensions, |sampling| sampling.full_dimensions);
        let stride = scene.sampling().map_or(1, |sampling| 1_u32 << sampling.mip);
        let scale = stride as f32;
        let pixels = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    let nx = normalized_coordinate(x * stride, reference.width);
                    let ny = normalized_coordinate(y * stride, reference.height);
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
                        reference.width,
                    ) / scale;
                    let source_y = denormalize_coordinate(
                        (ny * radial + tangential_y).clamp(-1.0, 1.0),
                        reference.height,
                    ) / scale;
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
            .and_then(|corrected| corrected.with_sampling(scene.sampling()))
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
        _ => Err(NodeError::InvalidParameter(port.to_owned())),
    }
}

fn mosaic_input(inputs: &Inputs, port: &str) -> Result<Mosaic, NodeError> {
    match required_input(inputs, port)? {
        Value::Mosaic(mosaic) => Ok(mosaic.clone()),
        _ => Err(NodeError::InvalidParameter(port.to_owned())),
    }
}

fn scene_input(inputs: &Inputs, port: &str) -> Result<SceneLinearRGB, NodeError> {
    match required_input(inputs, port)? {
        Value::SceneLinearRGB(scene) => Ok(scene.clone()),
        _ => Err(NodeError::InvalidParameter(port.to_owned())),
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

fn validate_rgb_cfa(mosaic: &Mosaic) -> Result<(), NodeError> {
    if mosaic
        .cfa()
        .colors()
        .iter()
        .any(|color| matches!(color, CfaColor::Extra | CfaColor::Unknown))
    {
        return Err(NodeError::Message(
            "unsupported CFA: RGB nodes do not accept extra or unknown channels".to_owned(),
        ));
    }
    for wanted in [CfaColor::Red, CfaColor::Green, CfaColor::Blue] {
        if !mosaic.cfa().colors().contains(&wanted) {
            return Err(NodeError::Message(format!(
                "unsupported CFA: missing {wanted:?} channel"
            )));
        }
    }
    Ok(())
}

fn cfa_channel(color: CfaColor) -> Option<usize> {
    match color {
        CfaColor::Red => Some(0),
        CfaColor::Green => Some(1),
        CfaColor::Blue => Some(2),
        CfaColor::Extra | CfaColor::Unknown => None,
    }
}

// Only recognize the four ordinary Bayer layouts; other CFAs retain the general search.
fn bayer_channels(mosaic: &Mosaic) -> Option<[usize; 4]> {
    if mosaic.cfa().width() != 2 || mosaic.cfa().height() != 2 {
        return None;
    }
    let colors = mosaic.cfa().colors();
    let channels = [
        cfa_channel(*colors.first()?)?,
        cfa_channel(*colors.get(1)?)?,
        cfa_channel(*colors.get(2)?)?,
        cfa_channel(*colors.get(3)?)?,
    ];
    match channels {
        [0, 1, 1, 2] | [2, 1, 1, 0] | [1, 0, 2, 1] | [1, 2, 0, 1] => Some(channels),
        _ => None,
    }
}

// Interior Bayer interpolation has fixed neighbors. Keep the reference's
// row-major summation order (including its initial +0) for bitwise equality.
fn bayer_demosaic_pixel(mosaic: &Mosaic, channels: [usize; 4], x: u32, y: u32) -> Option<[f32; 3]> {
    let dimensions = mosaic.dimensions();
    if x == 0
        || y == 0
        || x >= dimensions.width.saturating_sub(1)
        || y >= dimensions.height.saturating_sub(1)
    {
        return None;
    }
    let width = usize::try_from(dimensions.width).ok()?;
    let index = usize::try_from(y)
        .ok()?
        .checked_mul(width)?
        .checked_add(usize::try_from(x).ok()?)?;
    let top = mosaic.samples().get(index - width - 1..index - width + 2)?;
    let middle = mosaic.samples().get(index - 1..index + 2)?;
    let bottom = mosaic.samples().get(index + width - 1..index + width + 2)?;
    let phase = usize::try_from((y % 2) * 2 + x % 2).ok()?;
    let own = channels[phase];
    let mut pixel = [0.0; 3];
    pixel[own] = middle[1];
    if own == 1 {
        pixel[channels[phase ^ 1]] = (0.0 + middle[0] + middle[2]) / 2.0;
        pixel[channels[phase ^ 2]] = (0.0 + top[1] + bottom[1]) / 2.0;
    } else {
        pixel[1] = (0.0 + top[1] + middle[0] + middle[2] + bottom[1]) / 4.0;
        pixel[channels[phase ^ 3]] = (0.0 + top[0] + top[2] + bottom[0] + bottom[2]) / 4.0;
    }
    Some(pixel)
}

/// Search each ring once for all missing channels, retaining the reference
/// sample order and first-available-ring rule for Bayer and X-Trans borders.
fn demosaic_pixel(mosaic: &Mosaic, x: u32, y: u32) -> Result<[f32; 3], NodeError> {
    let own_channel = mosaic
        .cfa()
        .color_at(x, y)
        .and_then(cfa_channel)
        .ok_or_else(|| NodeError::Message("unsupported CFA channel".to_owned()))?;
    let mut pixels = [0.0; 3];
    let mut ready = [false; 3];
    pixels[own_channel] = mosaic.sample(x, y).ok_or_else(|| {
        NodeError::Message("demosaic sample coordinate is outside the mosaic".to_owned())
    })?;
    ready[own_channel] = true;
    let dimensions = mosaic.dimensions();
    let max_radius = dimensions
        .width
        .max(dimensions.height)
        .min(mosaic.cfa().width().max(mosaic.cfa().height()));
    for radius in 1..=max_radius {
        let min_x = x.saturating_sub(radius);
        let max_x = x.saturating_add(radius).min(dimensions.width - 1);
        let min_y = y.saturating_sub(radius);
        let max_y = y.saturating_add(radius).min(dimensions.height - 1);
        let mut totals = [0.0; 3];
        let mut counts = [0_u32; 3];
        for sample_y in min_y..=max_y {
            for sample_x in min_x..=max_x {
                let on_ring = sample_x == min_x
                    || sample_x == max_x
                    || sample_y == min_y
                    || sample_y == max_y;
                if on_ring
                    && let Some(channel) = mosaic
                        .cfa()
                        .color_at(sample_x, sample_y)
                        .and_then(cfa_channel)
                    && !ready[channel]
                    && let Some(sample) = mosaic.sample(sample_x, sample_y)
                {
                    totals[channel] += sample;
                    counts[channel] += 1;
                }
            }
        }
        for channel in 0..3 {
            if counts[channel] > 0 {
                pixels[channel] = totals[channel] / counts[channel] as f32;
                ready[channel] = true;
            }
        }
        if ready.iter().all(|ready| *ready) {
            return Ok(pixels);
        }
    }
    let missing = ready.iter().position(|ready| !ready).unwrap_or(0);
    let color = [CfaColor::Red, CfaColor::Green, CfaColor::Blue]
        .get(missing)
        .copied()
        .unwrap_or(CfaColor::Unknown);
    Err(NodeError::Message(format!(
        "unsupported CFA: no {color:?} sample is available for demosaic"
    )))
}

#[cfg(test)]
fn demosaic_channel(mosaic: &Mosaic, x: u32, y: u32, wanted: CfaColor) -> Result<f32, NodeError> {
    if mosaic.cfa().color_at(x, y) == Some(wanted) {
        return mosaic.sample(x, y).ok_or_else(|| {
            NodeError::Message("demosaic sample coordinate is outside the mosaic".to_owned())
        });
    }

    let dimensions = mosaic.dimensions();
    let max_radius = dimensions.width.max(dimensions.height);
    for radius in 1..=max_radius {
        let min_x = x.saturating_sub(radius);
        let max_x = x.saturating_add(radius).min(dimensions.width - 1);
        let min_y = y.saturating_sub(radius);
        let max_y = y.saturating_add(radius).min(dimensions.height - 1);
        let mut total = 0.0;
        let mut count = 0_u32;
        for sample_y in min_y..=max_y {
            for sample_x in min_x..=max_x {
                let on_ring = sample_x == min_x
                    || sample_x == max_x
                    || sample_y == min_y
                    || sample_y == max_y;
                if on_ring
                    && mosaic.cfa().color_at(sample_x, sample_y) == Some(wanted)
                    && let Some(sample) = mosaic.sample(sample_x, sample_y)
                {
                    total += sample;
                    count += 1;
                }
            }
        }
        if count > 0 {
            return Ok(total / count as f32);
        }
    }
    Err(NodeError::Message(format!(
        "unsupported CFA: no {wanted:?} sample is available for demosaic"
    )))
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

/// Register RAW nodes with the default rawler adapter (legacy type name retained).
pub fn register_nodes(registry: &mut NodeRegistry) -> Result<(), rawweave_node_api::RegistryError> {
    register_nodes_with_decoder(registry, Arc::new(RawloaderDecoder::default()))
}

/// Node pack containing the initial RAW pipeline stages.
pub struct RawNodePack {
    decoder: Arc<dyn RawDecoder>,
}

impl Default for RawNodePack {
    fn default() -> Self {
        Self {
            decoder: Arc::new(RawloaderDecoder::default()),
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
    fn shared_neighbor_search_matches_channel_reference_at_every_border() {
        for frame in [
            DeterministicCorpus::bayer_12_bit(),
            DeterministicCorpus::xtrans_14_bit(),
        ] {
            let varied = frame
                .mosaic()
                .map_samples(|index, _, _| [-0.0, -0.5, 10_000.0, f32::MAX / 8.0, 0.75][index % 5])
                .unwrap();
            for mosaic in [frame.mosaic(), &varied] {
                for y in 0..mosaic.dimensions().height {
                    for x in 0..mosaic.dimensions().width {
                        let expected = [
                            demosaic_channel(mosaic, x, y, CfaColor::Red).unwrap(),
                            demosaic_channel(mosaic, x, y, CfaColor::Green).unwrap(),
                            demosaic_channel(mosaic, x, y, CfaColor::Blue).unwrap(),
                        ];
                        assert_eq!(
                            demosaic_pixel(mosaic, x, y).unwrap().map(f32::to_bits),
                            expected.map(f32::to_bits)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn bayer_fast_path_matches_reference_for_all_phases_and_borders() {
        use rawweave_raw::CfaPattern;
        let frame = DeterministicCorpus::bayer_12_bit();
        for colors in [
            vec![
                CfaColor::Red,
                CfaColor::Green,
                CfaColor::Green,
                CfaColor::Blue,
            ],
            vec![
                CfaColor::Blue,
                CfaColor::Green,
                CfaColor::Green,
                CfaColor::Red,
            ],
            vec![
                CfaColor::Green,
                CfaColor::Red,
                CfaColor::Blue,
                CfaColor::Green,
            ],
            vec![
                CfaColor::Green,
                CfaColor::Blue,
                CfaColor::Red,
                CfaColor::Green,
            ],
        ] {
            let cfa = CfaPattern::new(2, 2, colors).unwrap();
            for (width, height) in [(2, 2), (2, 7), (7, 2), (3, 3), (9, 8)] {
                let mosaic = Mosaic::new(
                    rawweave_image::Dimensions::new(width, height),
                    (0..width * height)
                        .map(|index| {
                            [-0.0, -0.5, 0.75, 10_000.0, f32::MAX / 8.0][index as usize % 5]
                        })
                        .collect(),
                    12,
                    cfa.clone(),
                    frame.mosaic().orientation(),
                )
                .unwrap();
                let channels = bayer_channels(&mosaic).unwrap();
                for y in 0..height {
                    for x in 0..width {
                        let fast = bayer_demosaic_pixel(&mosaic, channels, x, y);
                        assert_eq!(
                            fast.is_some(),
                            x > 0 && y > 0 && x + 1 < width && y + 1 < height
                        );
                        if let Some(fast) = fast {
                            assert_eq!(
                                fast.map(f32::to_bits),
                                demosaic_pixel(&mosaic, x, y).unwrap().map(f32::to_bits)
                            );
                        }
                    }
                }
            }
        }
        assert!(bayer_channels(DeterministicCorpus::xtrans_14_bit().mosaic()).is_none());
        let unusual = Mosaic::new(
            rawweave_image::Dimensions::new(2, 2),
            vec![0.5; 4],
            12,
            CfaPattern::new(
                2,
                2,
                vec![
                    CfaColor::Red,
                    CfaColor::Red,
                    CfaColor::Green,
                    CfaColor::Blue,
                ],
            )
            .unwrap(),
            frame.mosaic().orientation(),
        )
        .unwrap();
        assert!(bayer_channels(&unusual).is_none());
    }

    #[test]
    fn preview_demosaic_computes_only_requested_rgb_samples_and_final_stays_full_size() {
        use rawweave_rendering::PreviewQuality;
        for frame in [
            DeterministicCorpus::bayer_12_bit(),
            DeterministicCorpus::xtrans_14_bit(),
        ] {
            let inputs = [("mosaic".to_owned(), Value::Mosaic(frame.mosaic().clone()))]
                .into_iter()
                .collect();
            let full = Demosaic
                .evaluate(&inputs, &Parameters::new(), &EvaluationContext::default())
                .unwrap();
            let Value::SceneLinearRGB(full) = &full.outputs["scene"] else {
                panic!()
            };
            for mip in [1, 2, 6] {
                let context = EvaluationContext::default().with_mip_level(mip);
                let reduced = Demosaic
                    .evaluate(&inputs, &Parameters::new(), &context)
                    .unwrap();
                let Value::SceneLinearRGB(reduced) = &reduced.outputs["scene"] else {
                    panic!()
                };
                let expected = rawweave_image::Dimensions::new(
                    full.dimensions().width.div_ceil(1 << mip),
                    full.dimensions().height.div_ceil(1 << mip),
                );
                assert_eq!(reduced.dimensions(), expected);
                assert_eq!(reduced.pixels().len(), expected.pixel_count().unwrap());
                for y in 0..expected.height {
                    for x in 0..expected.width {
                        assert_eq!(
                            reduced.pixel(x, y).unwrap().map(f32::to_bits),
                            full.pixel(x << mip, y << mip).unwrap().map(f32::to_bits)
                        );
                    }
                }
                let final_result = Demosaic
                    .evaluate(
                        &inputs,
                        &Parameters::new(),
                        &context.with_quality(PreviewQuality::Final),
                    )
                    .unwrap();
                assert_eq!(
                    final_result.outputs["scene"],
                    Value::SceneLinearRGB(full.clone())
                );
            }
        }
    }

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
