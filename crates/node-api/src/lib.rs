use std::collections::BTreeMap;
use std::fmt;

use rawweave_image::{Image, Region};
use rawweave_rendering::RenderContext;
use serde::{Deserialize, Serialize};
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
    Boolean,
    String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParameterValue {
    Float(f32),
    Boolean(bool),
    String(String),
}

impl ParameterValue {
    pub fn parameter_type(&self) -> ParameterType {
        match self {
            Self::Float(_) => ParameterType::Float,
            Self::Boolean(_) => ParameterType::Boolean,
            Self::String(_) => ParameterType::String,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match self {
            Self::Float(value) => Some(*value),
            _ => None,
        }
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
        }
    }

    pub fn supports(&self, capability: ExecutionCapability) -> bool {
        self.capabilities.contains(&capability)
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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Image(Image),
    Float(f32),
}

impl Value {
    pub fn data_type(&self) -> &'static str {
        match self {
            Self::Image(_) => "core.Image",
            Self::Float(_) => "value.Float",
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
    pub requested_region: Option<Region>,
    pub render_context: Option<RenderContext>,
}

impl EvaluationContext {
    pub fn with_source_image(source_image: Image) -> Self {
        Self {
            source_image: Some(source_image),
            requested_region: None,
            render_context: None,
        }
    }

    pub fn with_requested_region(mut self, requested_region: Region) -> Self {
        self.requested_region = Some(requested_region);
        self
    }

    pub fn with_render_context(mut self, render_context: RenderContext) -> Self {
        self.render_context = Some(render_context);
        self
    }

    pub fn requested_region(&self) -> Option<Region> {
        self.requested_region
    }

    pub fn render_context(&self) -> Option<&RenderContext> {
        self.render_context.as_ref()
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
    factory: NodeFactory,
}

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
                factory,
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
