use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use rawweave_color::{DisplayRGB, SceneLinearRGB};
use rawweave_image::{ConfidenceMap, DepthMap, Image, LabelMap, Mask, MaskSet, Region, RegionSet};
use rawweave_raw::{CameraMetadata, EmbeddedPreview, ExifMetadata, Mosaic, RawFrame};
use rawweave_rendering::{PreviewQuality, RenderContext, TileCoord, TileRequest};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

pub type TypeId = String;
pub type PortId = String;
pub type Parameters = BTreeMap<String, ParameterValue>;
pub type Inputs = BTreeMap<String, Value>;
pub type Outputs = BTreeMap<String, Value>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortDescriptor {
    pub id: PortId,
    pub name: String,
    pub data_type: TypeId,
    pub required: bool,
}

impl PortDescriptor {
    pub fn input(
        id: impl Into<String>,
        name: impl Into<String>,
        data_type: impl Into<String>,
        required: bool,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            data_type: data_type.into(),
            required,
        }
    }

    pub fn output(
        id: impl Into<String>,
        name: impl Into<String>,
        data_type: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            data_type: data_type.into(),
            required: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParameterType {
    Float,
    Integer,
    Boolean,
    String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParameterValue {
    Float(#[serde(deserialize_with = "deserialize_finite_float")] f32),
    Integer(i64),
    Boolean(bool),
    String(String),
}

/// A validated linear RGBA control value.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColorValue {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl ColorValue {
    /// Build a color when every channel is finite.
    pub fn new(red: f32, green: f32, blue: f32, alpha: f32) -> Option<Self> {
        [red, green, blue, alpha]
            .into_iter()
            .all(f32::is_finite)
            .then_some(Self {
                red,
                green,
                blue,
                alpha,
            })
    }

    /// Build an opaque color from in-range channels.
    pub fn rgb(red: f32, green: f32, blue: f32) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 1.0,
        }
    }

    /// Build a color with an explicit alpha channel.
    pub fn rgba(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }
}

/// Descriptive capture metadata projected into graph space.
///
/// Fields mirror [`rawweave_raw::CameraMetadata`]; absent values stay `None`
/// rather than being invented.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    pub make: String,
    pub model: String,
    pub lens: Option<String>,
    pub iso: Option<u32>,
    pub aperture: Option<f32>,
    pub shutter_seconds: Option<f32>,
    pub focal_length_mm: Option<f32>,
    pub capture_time: Option<String>,
    pub orientation: Option<String>,
    pub rating: Option<u32>,
    pub tags: BTreeMap<String, String>,
}

impl Metadata {
    /// Project RAW camera and EXIF metadata into the portable `core.Metadata`
    /// value. The `Rating` EXIF tag is parsed when present and well formed.
    pub fn from_sources(camera: &CameraMetadata, exif: &ExifMetadata) -> Self {
        Self {
            make: camera.make.clone(),
            model: camera.model.clone(),
            lens: camera.lens.clone(),
            iso: camera.iso,
            aperture: camera.aperture,
            shutter_seconds: camera.shutter_seconds,
            focal_length_mm: camera.focal_length_mm,
            capture_time: camera.capture_time.clone(),
            orientation: Some(camera.orientation.as_str().to_owned()),
            rating: exif
                .tags
                .get("Rating")
                .and_then(|rating| rating.parse::<u32>().ok()),
            tags: exif.tags.clone(),
        }
    }
}

fn deserialize_finite_float<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| serde::de::Error::custom("float parameter must be finite"))
}

impl ParameterValue {
    pub fn parameter_type(&self) -> ParameterType {
        match self {
            Self::Float(_) => ParameterType::Float,
            Self::Integer(_) => ParameterType::Integer,
            Self::Boolean(_) => ParameterType::Boolean,
            Self::String(_) => ParameterType::String,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match self {
            Self::Float(value) => Some(*value),
            Self::Integer(value) => Some(*value as f32),
            _ => None,
        }
    }

    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }
}

impl From<i64> for ParameterValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<f32> for ParameterValue {
    fn from(value: f32) -> Self {
        Self::Float(value)
    }
}

impl From<bool> for ParameterValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<String> for ParameterValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for ParameterValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParameterDescriptor {
    pub id: String,
    pub name: String,
    pub parameter_type: ParameterType,
    pub default: ParameterValue,
    pub min: Option<f32>,
    pub max: Option<f32>,
}

impl ParameterDescriptor {
    pub fn float(
        id: impl Into<String>,
        name: impl Into<String>,
        default: f32,
        min: Option<f32>,
        max: Option<f32>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            parameter_type: ParameterType::Float,
            default: ParameterValue::Float(default),
            min,
            max,
        }
    }

    pub fn string(
        id: impl Into<String>,
        name: impl Into<String>,
        default: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            parameter_type: ParameterType::String,
            default: ParameterValue::String(default.into()),
            min: None,
            max: None,
        }
    }

    pub fn integer(id: impl Into<String>, name: impl Into<String>, default: i64) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            parameter_type: ParameterType::Integer,
            default: ParameterValue::Integer(default),
            min: None,
            max: None,
        }
    }

    pub fn boolean(id: impl Into<String>, name: impl Into<String>, default: bool) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            parameter_type: ParameterType::Boolean,
            default: ParameterValue::Boolean(default),
            min: None,
            max: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeDescriptor {
    pub type_id: TypeId,
    pub name: String,
    pub version: u32,
    pub inputs: Vec<PortDescriptor>,
    pub outputs: Vec<PortDescriptor>,
    pub parameters: Vec<ParameterDescriptor>,
    #[serde(default)]
    pub capabilities: Vec<ExecutionCapability>,
    /// Selector-gated input ports. When present, the graph evaluates the
    /// selector first and skips input ports outside the selected branch so
    /// unselected expensive image branches are not rendered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lazy_inputs: Vec<LazyInputGate>,
}

/// A selector port that decides which input ports are required to evaluate a
/// node. `required` ports are always evaluated (including the selector).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LazyInputGate {
    pub selector: String,
    #[serde(default)]
    pub required: Vec<String>,
    pub branches: Vec<LazyBranch>,
}

/// Input ports that are only needed when the selector matches `condition`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LazyBranch {
    pub condition: LazyCondition,
    pub inputs: Vec<String>,
}

/// A selector outcome that gates a branch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LazyCondition {
    True,
    False,
    Index(u32),
    Key(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExecutionCapability {
    Cpu,
    Gpu,
    TileLocal,
    RegionAware,
    FullFrame,
}

impl NodeDescriptor {
    pub fn new(type_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            type_id: type_id.into(),
            name: name.into(),
            version: 1,
            inputs: Vec::new(),
            outputs: Vec::new(),
            parameters: Vec::new(),
            capabilities: Vec::new(),
            lazy_inputs: Vec::new(),
        }
    }

    pub fn supports(&self, capability: ExecutionCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Select the most specific executable capability for the current request.
    ///
    /// Backend availability is considered before CPU execution, while tile and
    /// region scope are selected only when the request asks for a sub-region.
    pub fn select_capability(&self, context: &EvaluationContext) -> Option<ExecutionCapability> {
        if context
            .render_context()
            .is_some_and(|render_context| render_context.gpu_available())
            && self.supports(ExecutionCapability::Gpu)
        {
            return Some(ExecutionCapability::Gpu);
        }
        if context.requested_region().is_some() && self.supports(ExecutionCapability::TileLocal) {
            return Some(ExecutionCapability::TileLocal);
        }
        if context.requested_region().is_some() && self.supports(ExecutionCapability::RegionAware) {
            return Some(ExecutionCapability::RegionAware);
        }
        if self.supports(ExecutionCapability::FullFrame) {
            return Some(ExecutionCapability::FullFrame);
        }
        self.supports(ExecutionCapability::Cpu)
            .then_some(ExecutionCapability::Cpu)
    }

    pub fn select_execution_capability(
        &self,
        context: &EvaluationContext,
    ) -> Option<ExecutionCapability> {
        self.select_capability(context)
    }

    pub fn parameter_defaults(&self) -> Parameters {
        self.parameters
            .iter()
            .map(|parameter| (parameter.id.clone(), parameter.default.clone()))
            .collect()
    }

    pub fn input(&self, id: &str) -> Option<&PortDescriptor> {
        self.inputs.iter().find(|port| port.id == id)
    }

    pub fn output(&self, id: &str) -> Option<&PortDescriptor> {
        self.outputs.iter().find(|port| port.id == id)
    }

    pub fn parameter(&self, id: &str) -> Option<&ParameterDescriptor> {
        self.parameters.iter().find(|parameter| parameter.id == id)
    }

    pub fn lazy_input_gate(&self, selector: &str) -> Option<&LazyInputGate> {
        self.lazy_inputs
            .iter()
            .find(|gate| gate.selector == selector)
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Image(Image),
    Mask(Mask),
    MaskSet(MaskSet),
    LabelMap(LabelMap),
    ConfidenceMap(ConfidenceMap),
    DepthMap(DepthMap),
    RegionSet(RegionSet),
    Float(f32),
    Integer(i64),
    Boolean(bool),
    String(String),
    Enum(String),
    Color(ColorValue),
    Condition(bool),
    Metadata(Metadata),
    Bytes(Vec<u8>),
    RawFrame(RawFrame),
    Mosaic(Mosaic),
    SceneLinearRGB(SceneLinearRGB),
    DisplayRGB(DisplayRGB),
    CameraMetadata(CameraMetadata),
    ExifMetadata(ExifMetadata),
    CameraProfile(rawweave_raw::CameraProfile),
    LensProfile(rawweave_raw::LensProfile),
    EmbeddedPreview(EmbeddedPreview),
}

impl Value {
    pub fn data_type(&self) -> &'static str {
        match self {
            Self::Image(_) => "core.Image",
            Self::Mask(_) => "core.Mask",
            Self::MaskSet(_) => "core.MaskSet",
            Self::LabelMap(_) => "core.LabelMap",
            Self::ConfidenceMap(_) => "core.ConfidenceMap",
            Self::DepthMap(_) => "core.DepthMap",
            Self::RegionSet(_) => "core.RegionSet",
            Self::Float(_) => "value.Float",
            Self::Integer(_) => "value.Integer",
            Self::Boolean(_) => "value.Boolean",
            Self::String(_) => "value.String",
            Self::Enum(_) => "value.Enum",
            Self::Color(_) => "value.Color",
            Self::Condition(_) => "value.Condition",
            Self::Metadata(_) => "core.Metadata",
            Self::Bytes(_) => "core.Bytes",
            Self::RawFrame(_) => "raw.Frame",
            Self::Mosaic(_) => "raw.Mosaic",
            Self::SceneLinearRGB(_) => "color.SceneLinearRGB",
            Self::DisplayRGB(_) => "color.DisplayRGB",
            Self::CameraMetadata(_) => "raw.CameraMetadata",
            Self::ExifMetadata(_) => "raw.ExifMetadata",
            Self::CameraProfile(_) => "raw.CameraProfile",
            Self::LensProfile(_) => "raw.LensProfile",
            Self::EmbeddedPreview(_) => "raw.EmbeddedPreview",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeResult {
    pub outputs: Outputs,
}

impl NodeResult {
    pub fn new(outputs: Outputs) -> Self {
        Self { outputs }
    }

    pub fn single(id: impl Into<String>, value: Value) -> Self {
        Self::new([(id.into(), value)].into_iter().collect())
    }
}

#[derive(Clone, Debug, Default)]
pub struct EvaluationContext {
    pub source_image: Option<Image>,
    pub source_bytes: Option<Vec<u8>>,
    pub source_path: Option<PathBuf>,
    pub external_inputs: BTreeMap<String, Value>,
    pub assets: BTreeMap<String, Vec<u8>>,
    /// Per-instance/per-image parameter overrides keyed by node and parameter
    /// id. A connection still takes precedence over an override.
    pub parameter_overrides: BTreeMap<(String, String), ParameterValue>,
    pub requested_region: Option<Region>,
    pub render_context: Option<RenderContext>,
    pub tile: TileCoord,
    pub mip_level: u8,
    pub quality: PreviewQuality,
}

impl EvaluationContext {
    pub fn with_source_image(source_image: Image) -> Self {
        Self {
            source_image: Some(source_image),
            ..Self::default()
        }
    }

    pub fn with_source_bytes(mut self, source_bytes: Vec<u8>) -> Self {
        self.source_bytes = Some(source_bytes);
        self
    }

    pub fn with_source_path(mut self, source_path: impl AsRef<std::path::Path>) -> Self {
        self.source_path = Some(source_path.as_ref().to_path_buf());
        self
    }

    pub fn with_external_input(mut self, id: impl Into<String>, value: Value) -> Self {
        self.external_inputs.insert(id.into(), value);
        self
    }

    pub fn with_external_inputs<I, K>(mut self, inputs: I) -> Self
    where
        I: IntoIterator<Item = (K, Value)>,
        K: Into<String>,
    {
        self.external_inputs
            .extend(inputs.into_iter().map(|(id, value)| (id.into(), value)));
        self
    }

    pub fn with_parameter_override(
        mut self,
        node_id: impl Into<String>,
        parameter_id: impl Into<String>,
        value: impl Into<ParameterValue>,
    ) -> Self {
        self.parameter_overrides
            .insert((node_id.into(), parameter_id.into()), value.into());
        self
    }

    pub fn parameter_override(&self, node_id: &str, parameter_id: &str) -> Option<&ParameterValue> {
        self.parameter_overrides
            .get(&(node_id.to_owned(), parameter_id.to_owned()))
    }

    pub fn with_asset(mut self, id: impl Into<String>, bytes: Vec<u8>) -> Self {
        self.assets.insert(id.into(), bytes);
        self
    }

    pub fn with_assets<I, K>(mut self, assets: I) -> Self
    where
        I: IntoIterator<Item = (K, Vec<u8>)>,
        K: Into<String>,
    {
        self.assets
            .extend(assets.into_iter().map(|(id, bytes)| (id.into(), bytes)));
        self
    }

    pub fn with_requested_region(mut self, requested_region: Region) -> Self {
        self.requested_region = Some(requested_region);
        self
    }

    pub fn with_render_context(mut self, render_context: RenderContext) -> Self {
        self.render_context = Some(render_context);
        self
    }

    pub fn with_tile_request(mut self, request: TileRequest) -> Self {
        self.requested_region = Some(request.region);
        self.tile = request.tile;
        self.mip_level = request.mip_level;
        self.quality = request.quality;
        self
    }

    pub fn with_tile(mut self, tile: TileCoord) -> Self {
        self.tile = tile;
        self
    }

    pub fn with_mip_level(mut self, mip_level: u8) -> Self {
        self.mip_level = mip_level;
        self
    }

    pub fn with_quality(mut self, quality: PreviewQuality) -> Self {
        self.quality = quality;
        self
    }

    pub fn requested_region(&self) -> Option<Region> {
        self.requested_region
    }

    pub fn render_context(&self) -> Option<&RenderContext> {
        self.render_context.as_ref()
    }

    pub fn tile(&self) -> TileCoord {
        self.tile
    }

    pub fn mip_level(&self) -> u8 {
        self.mip_level
    }

    pub fn quality(&self) -> PreviewQuality {
        self.quality
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum NodeError {
    #[error("missing required input '{0}'")]
    MissingInput(String),
    #[error("missing source image")]
    MissingSourceImage,
    #[error("parameter '{0}' is missing or has the wrong type")]
    InvalidParameter(String),
    #[error("node evaluation failed: {0}")]
    Message(String),
}

pub trait NodeInstance: Send + Sync {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError>;
}

pub type NodeFactory = fn() -> Box<dyn NodeInstance>;

#[derive(Clone)]
struct RegisteredNode {
    descriptor: NodeDescriptor,
    factory: SharedNodeFactory,
}

type SharedNodeFactory = Arc<dyn Fn() -> Box<dyn NodeInstance> + Send + Sync>;

impl fmt::Debug for RegisteredNode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RegisteredNode")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Default)]
pub struct NodeRegistry {
    nodes: BTreeMap<TypeId, RegisteredNode>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("node type '{0}' is already registered")]
    DuplicateType(String),
    #[error("node type identifier cannot be empty")]
    EmptyType,
}

impl NodeRegistry {
    pub fn register(
        &mut self,
        descriptor: NodeDescriptor,
        factory: NodeFactory,
    ) -> Result<(), RegistryError> {
        self.register_factory(descriptor, factory)
    }

    pub fn register_factory<F>(
        &mut self,
        descriptor: NodeDescriptor,
        factory: F,
    ) -> Result<(), RegistryError>
    where
        F: Fn() -> Box<dyn NodeInstance> + Send + Sync + 'static,
    {
        if descriptor.type_id.is_empty() {
            return Err(RegistryError::EmptyType);
        }
        if self.nodes.contains_key(&descriptor.type_id) {
            return Err(RegistryError::DuplicateType(descriptor.type_id));
        }
        self.nodes.insert(
            descriptor.type_id.clone(),
            RegisteredNode {
                descriptor,
                factory: Arc::new(factory),
            },
        );
        Ok(())
    }

    pub fn descriptor(&self, type_id: &str) -> Option<&NodeDescriptor> {
        self.nodes.get(type_id).map(|node| &node.descriptor)
    }

    pub fn instantiate(&self, type_id: &str) -> Option<Box<dyn NodeInstance>> {
        self.nodes.get(type_id).map(|node| (node.factory)())
    }

    pub fn descriptors(&self) -> Vec<NodeDescriptor> {
        self.nodes
            .values()
            .map(|node| node.descriptor.clone())
            .collect()
    }
}

pub trait NodePack {
    fn id(&self) -> &'static str;
    fn register(&self, registry: &mut NodeRegistry) -> Result<(), RegistryError>;
}
