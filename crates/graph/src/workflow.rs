use std::collections::{BTreeMap, BTreeSet};

use rawweave_core::NodeId;
use rawweave_node_api::{NodeRegistry, ParameterDescriptor, ParameterType, ParameterValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{Graph, GraphEdge, GraphError, GraphNode};

/// Stable identity and revision label for a reusable workflow.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowIdentity {
    pub id: String,
    pub version: String,
}

impl WorkflowIdentity {
    pub fn new(id: impl Into<String>, version: impl Into<String>) -> Result<Self, WorkflowError> {
        let id = id.into();
        let version = version.into();
        if id.trim().is_empty() {
            return Err(WorkflowError::InvalidIdentity(
                "workflow id cannot be empty".to_owned(),
            ));
        }
        if version.trim().is_empty() {
            return Err(WorkflowError::InvalidIdentity(
                "workflow version cannot be empty".to_owned(),
            ));
        }
        Ok(Self { id, version })
    }
}

/// Human-facing metadata that does not affect workflow execution.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowMetadata {
    pub name: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub recommended_input_type: Option<String>,
    #[serde(default)]
    pub minimum_app_version: Option<String>,
}

impl WorkflowMetadata {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
}

/// Metadata used when publishing a workflow as a template.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateMetadata {
    pub name: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub recommended_input_type: Option<String>,
    #[serde(default)]
    pub minimum_app_version: Option<String>,
}

impl TemplateMetadata {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_thumbnail(mut self, thumbnail: impl Into<String>) -> Self {
        self.thumbnail = Some(thumbnail.into());
        self
    }

    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_license(mut self, license: impl Into<String>) -> Self {
        self.license = Some(license.into());
        self
    }

    pub fn with_recommended_input_type(mut self, input_type: impl Into<String>) -> Self {
        self.recommended_input_type = Some(input_type.into());
        self
    }

    pub fn with_minimum_app_version(mut self, version: impl Into<String>) -> Self {
        self.minimum_app_version = Some(version.into());
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowPortDirection {
    Input,
    Output,
}

/// A graph port promoted to the public boundary of a reusable workflow.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowPort {
    pub id: String,
    pub name: String,
    pub direction: WorkflowPortDirection,
    pub node_id: NodeId,
    pub port_id: String,
    pub data_type: String,
    #[serde(default)]
    pub required: bool,
}

impl WorkflowPort {
    fn input(
        id: impl Into<String>,
        node_id: NodeId,
        port_id: impl Into<String>,
        name: impl Into<String>,
        data_type: impl Into<String>,
        required: bool,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            direction: WorkflowPortDirection::Input,
            node_id,
            port_id: port_id.into(),
            data_type: data_type.into(),
            required,
        }
    }

    fn output(
        id: impl Into<String>,
        node_id: NodeId,
        port_id: impl Into<String>,
        name: impl Into<String>,
        data_type: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            direction: WorkflowPortDirection::Output,
            node_id,
            port_id: port_id.into(),
            data_type: data_type.into(),
            required: false,
        }
    }
}

/// A public control mapped to a concrete node parameter in the internal graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkflowParameter {
    pub id: String,
    pub name: String,
    pub node_id: NodeId,
    pub parameter_id: String,
    pub parameter_type: ParameterType,
    pub default: ParameterValue,
}

impl WorkflowParameter {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        node_id: NodeId,
        parameter_id: impl Into<String>,
        parameter_type: ParameterType,
        default: ParameterValue,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            node_id,
            parameter_id: parameter_id.into(),
            parameter_type,
            default,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodePackDependency {
    pub id: String,
    pub version: String,
}

impl NodePackDependency {
    pub fn new(id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubgraphDependency {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub hash: String,
}

impl SubgraphDependency {
    pub fn new(id: impl Into<String>, version: impl Into<String>, hash: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            hash: hash.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeManifest {
    pub type_id: String,
    pub version: u32,
}

impl NodeManifest {
    pub fn new(type_id: impl Into<String>, version: u32) -> Self {
        Self {
            type_id: type_id.into(),
            version,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformRequirement {
    pub operating_system: String,
    pub architecture: String,
}

impl PlatformRequirement {
    pub fn new(operating_system: impl Into<String>, architecture: impl Into<String>) -> Self {
        Self {
            operating_system: operating_system.into(),
            architecture: architecture.into(),
        }
    }
}

/// Serializable package metadata for node packs and their bundled blueprints.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodePackManifest {
    pub package_id: String,
    pub version: String,
    #[serde(default)]
    pub nodes: Vec<NodeManifest>,
    #[serde(default)]
    pub subgraphs: Vec<SubgraphDependency>,
    #[serde(default)]
    pub workflow_templates: Vec<TemplateMetadata>,
    #[serde(default)]
    pub dependencies: Vec<NodePackDependency>,
    #[serde(default)]
    pub platform_requirements: Vec<PlatformRequirement>,
    #[serde(default)]
    pub license: Option<String>,
}

impl NodePackManifest {
    pub fn new(package_id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            package_id: package_id.into(),
            version: version.into(),
            nodes: Vec::new(),
            subgraphs: Vec::new(),
            workflow_templates: Vec::new(),
            dependencies: Vec::new(),
            platform_requirements: Vec::new(),
            license: None,
        }
    }

    pub fn with_node(mut self, node: NodeManifest) -> Self {
        self.nodes.push(node);
        self
    }

    pub fn with_subgraph(mut self, dependency: SubgraphDependency) -> Self {
        self.subgraphs.push(dependency);
        self
    }

    pub fn with_workflow_template(mut self, template: TemplateMetadata) -> Self {
        self.workflow_templates.push(template);
        self
    }

    pub fn with_dependency(mut self, id: impl Into<String>, version: impl Into<String>) -> Self {
        self.dependencies.push(NodePackDependency::new(id, version));
        self
    }

    pub fn with_platform_requirement(mut self, requirement: PlatformRequirement) -> Self {
        self.platform_requirements.push(requirement);
        self
    }

    pub fn with_license(mut self, license: impl Into<String>) -> Self {
        self.license = Some(license.into());
        self
    }
}

/// Resource limits applied before accepting an imported blueprint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkflowImportLimits {
    pub max_bytes: usize,
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_dependencies: usize,
    pub max_metadata_bytes: usize,
}

impl Default for WorkflowImportLimits {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
            max_depth: 32,
            max_nodes: 10_000,
            max_edges: 20_000,
            max_dependencies: 2_000,
            max_metadata_bytes: 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependencyStatus {
    Available,
    Missing,
    VersionMismatch { required: String, available: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyDiagnostic {
    pub id: String,
    pub required_version: String,
    pub available_version: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DependencyReport {
    pub available: Vec<DependencyDiagnostic>,
    pub missing: Vec<DependencyDiagnostic>,
    pub mismatched: Vec<DependencyDiagnostic>,
    pub disabled_nodes: Vec<String>,
    statuses: BTreeMap<String, DependencyStatus>,
}

impl DependencyReport {
    pub fn status(&self, id: &str) -> Option<DependencyStatus> {
        self.statuses.get(id).cloned()
    }

    pub fn is_compatible(&self) -> bool {
        self.missing.is_empty() && self.mismatched.is_empty() && self.disabled_nodes.is_empty()
    }
}

#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error(transparent)]
    Graph(#[from] GraphError),
    #[error("workflow serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("workflow export is not valid UTF-8: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),
    #[error("workflow import exceeds the {resource} limit of {limit}")]
    ImportLimitExceeded {
        resource: &'static str,
        limit: usize,
    },
    #[error("embedded nested workflow '{0}' is missing its required hash")]
    MissingNestedHash(String),
    #[error("embedded nested workflow '{id}' hash does not match its dependency")]
    NestedHashMismatch {
        id: String,
        expected: String,
        actual: String,
    },
    #[error("invalid workflow identity: {0}")]
    InvalidIdentity(String),
    #[error("workflow selection cannot be empty")]
    EmptySelection,
    #[error("workflow parameter '{0}' already exists")]
    DuplicateParameter(String),
    #[error("workflow port '{0}' already exists")]
    DuplicatePort(String),
    #[error("workflow dependency '{0}' already exists")]
    DuplicateDependency(String),
    #[error("workflow target '{node}:{parameter}' does not exist")]
    MissingParameterTarget { node: NodeId, parameter: String },
    #[error("workflow parameter '{parameter}' has an incompatible default value")]
    ParameterTypeMismatch { parameter: String },
    #[error("nested workflow '{0}' would introduce a cycle")]
    NestedCycle(String),
    #[error("workflow node map key '{key}' does not match embedded node id '{node}'")]
    NodeIdMismatch { key: String, node: String },
    #[error("workflow node '{node}' descriptor does not match registered node type '{type_id}'")]
    NodeDescriptorMismatch { node: NodeId, type_id: String },
    #[error("workflow node '{node}' contains invalid parameter '{parameter}'")]
    InvalidNodeParameter { node: NodeId, parameter: String },
    #[error("workflow dependency '{0}' has an empty id or version")]
    InvalidDependency(String),
    #[error("workflow parameter '{0}' is inconsistent with its graph target")]
    InvalidParameter(String),
    #[error("workflow port '{0}' is invalid")]
    InvalidPort(String),
    #[error("nested workflow key '{key}' does not match identity '{id}'")]
    NestedIdentityMismatch { key: String, id: String },
    #[error("nested workflow '{0}' is missing its dependency entry")]
    MissingNestedDependency(String),
}

/// A reusable, serializable graph boundary. Runtime registries and render
/// caches remain outside the document and are supplied when importing it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub identity: WorkflowIdentity,
    pub graph: Graph,
    #[serde(default)]
    pub parameters: BTreeMap<String, WorkflowParameter>,
    #[serde(default)]
    pub inputs: Vec<WorkflowPort>,
    #[serde(default)]
    pub outputs: Vec<WorkflowPort>,
    #[serde(default)]
    pub subgraph_dependencies: Vec<SubgraphDependency>,
    #[serde(default)]
    pub node_pack_dependencies: Vec<NodePackDependency>,
    #[serde(default)]
    pub metadata: WorkflowMetadata,
    #[serde(default)]
    pub nested_subgraphs: BTreeMap<String, Box<WorkflowDefinition>>,
}

impl WorkflowDefinition {
    pub fn new(
        id: impl Into<String>,
        version: impl Into<String>,
        graph: Graph,
        metadata: WorkflowMetadata,
    ) -> Result<Self, WorkflowError> {
        let identity = WorkflowIdentity::new(id, version)?;
        let definition = Self {
            identity,
            graph,
            parameters: BTreeMap::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            subgraph_dependencies: Vec::new(),
            node_pack_dependencies: Vec::new(),
            metadata,
            nested_subgraphs: BTreeMap::new(),
        };
        definition.validate()?;
        Ok(definition)
    }

    pub fn from_selection(
        graph: &Graph,
        selection: &BTreeSet<NodeId>,
        id: impl Into<String>,
        version: impl Into<String>,
        metadata: WorkflowMetadata,
    ) -> Result<Self, WorkflowError> {
        if selection.is_empty() {
            return Err(WorkflowError::EmptySelection);
        }
        let selected_graph = graph.clone_selection(selection)?;
        let mut definition = Self::new(id, version, selected_graph, metadata)?;

        let exposed_parameters = definition
            .graph
            .nodes()
            .values()
            .flat_map(|node| {
                node.exposed_parameters
                    .iter()
                    .map(|parameter_id| (node.id.clone(), parameter_id.clone()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for (node_id, parameter_id) in exposed_parameters {
            definition.expose_parameter(&node_id, &parameter_id)?;
        }

        for edge in graph.edges() {
            if selection.contains(&edge.to_node) && !selection.contains(&edge.from_node) {
                let node = graph
                    .node(&edge.to_node)
                    .ok_or_else(|| GraphError::MissingNode(edge.to_node.clone()))?;
                let port =
                    input_port(node, &edge.to_port).ok_or_else(|| GraphError::MissingPort {
                        node: edge.to_node.clone(),
                        port: edge.to_port.clone(),
                    })?;
                let id = format!("input:{}:{}", edge.to_node, edge.to_port);
                if !definition.inputs.iter().any(|input| input.id == id) {
                    definition.inputs.push(WorkflowPort::input(
                        id,
                        edge.to_node.clone(),
                        edge.to_port.clone(),
                        port.name,
                        port.data_type,
                        port.required,
                    ));
                }
            }
            if selection.contains(&edge.from_node) && !selection.contains(&edge.to_node) {
                let node = graph
                    .node(&edge.from_node)
                    .ok_or_else(|| GraphError::MissingNode(edge.from_node.clone()))?;
                let port = node.descriptor.output(&edge.from_port).ok_or_else(|| {
                    GraphError::MissingPort {
                        node: edge.from_node.clone(),
                        port: edge.from_port.clone(),
                    }
                })?;
                let id = format!("output:{}:{}", edge.from_node, edge.from_port);
                if !definition.outputs.iter().any(|output| output.id == id) {
                    definition.outputs.push(WorkflowPort::output(
                        id,
                        edge.from_node.clone(),
                        edge.from_port.clone(),
                        port.name.clone(),
                        port.data_type.clone(),
                    ));
                }
            }
        }
        definition.validate()?;
        Ok(definition)
    }

    pub fn from_selection_slice(
        graph: &Graph,
        selection: &[NodeId],
        id: impl Into<String>,
        version: impl Into<String>,
        metadata: WorkflowMetadata,
    ) -> Result<Self, WorkflowError> {
        Self::from_selection(
            graph,
            &selection.iter().cloned().collect(),
            id,
            version,
            metadata,
        )
    }

    pub fn identity(&self) -> &WorkflowIdentity {
        &self.identity
    }

    pub fn version(&self) -> &str {
        &self.identity.version
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn graph_mut(&mut self) -> &mut Graph {
        &mut self.graph
    }

    pub fn parameters(&self) -> &BTreeMap<String, WorkflowParameter> {
        &self.parameters
    }

    pub fn inputs(&self) -> &[WorkflowPort] {
        &self.inputs
    }

    pub fn outputs(&self) -> &[WorkflowPort] {
        &self.outputs
    }

    pub fn ports(&self) -> Vec<&WorkflowPort> {
        self.inputs.iter().chain(self.outputs.iter()).collect()
    }

    pub fn expose_parameter(
        &mut self,
        node_id: &NodeId,
        parameter_id: &str,
    ) -> Result<&WorkflowParameter, WorkflowError> {
        let (name, parameter_type, default) = {
            let node = self
                .graph
                .node(node_id)
                .ok_or_else(|| GraphError::MissingNode(node_id.clone()))?;
            let descriptor = node.descriptor.parameter(parameter_id).ok_or_else(|| {
                WorkflowError::MissingParameterTarget {
                    node: node_id.clone(),
                    parameter: parameter_id.to_owned(),
                }
            })?;
            let default = node.parameters.get(parameter_id).cloned().ok_or_else(|| {
                GraphError::MissingParameter {
                    node: node_id.clone(),
                    parameter: parameter_id.to_owned(),
                }
            })?;
            (descriptor.name.clone(), descriptor.parameter_type, default)
        };
        let id = format!("{}:{}", node_id, parameter_id);
        if self.parameters.contains_key(&id) {
            return Err(WorkflowError::DuplicateParameter(id));
        }
        self.graph.expose_parameter(node_id, parameter_id)?;
        let parameter = WorkflowParameter::new(
            id.clone(),
            name,
            node_id.clone(),
            parameter_id,
            parameter_type,
            default,
        );
        self.parameters.insert(id.clone(), parameter);
        self.parameters
            .get(&id)
            .ok_or(WorkflowError::DuplicateParameter(id))
    }

    pub fn add_parameter(&mut self, parameter: WorkflowParameter) -> Result<(), WorkflowError> {
        if self.parameters.contains_key(&parameter.id) {
            return Err(WorkflowError::DuplicateParameter(parameter.id));
        }
        let node = self
            .graph
            .node(&parameter.node_id)
            .ok_or_else(|| GraphError::MissingNode(parameter.node_id.clone()))?;
        let descriptor = node
            .descriptor
            .parameter(&parameter.parameter_id)
            .ok_or_else(|| WorkflowError::MissingParameterTarget {
                node: parameter.node_id.clone(),
                parameter: parameter.parameter_id.clone(),
            })?;
        if descriptor.parameter_type != parameter.parameter_type
            || descriptor.parameter_type != parameter.default.parameter_type()
        {
            return Err(WorkflowError::ParameterTypeMismatch {
                parameter: parameter.id,
            });
        }
        self.graph.set_parameter(
            &parameter.node_id,
            &parameter.parameter_id,
            parameter.default.clone(),
        )?;
        self.graph
            .expose_parameter(&parameter.node_id, &parameter.parameter_id)?;
        self.parameters.insert(parameter.id.clone(), parameter);
        Ok(())
    }

    pub fn hide_parameter(&mut self, id: &str) -> Result<WorkflowParameter, WorkflowError> {
        let parameter = self.parameters.get(id).cloned().ok_or_else(|| {
            WorkflowError::MissingParameterTarget {
                node: NodeId::from(id),
                parameter: id.to_owned(),
            }
        })?;
        self.graph
            .unexpose_parameter(&parameter.node_id, &parameter.parameter_id)?;
        self.parameters
            .remove(id)
            .ok_or_else(|| WorkflowError::MissingParameterTarget {
                node: NodeId::from(id),
                parameter: id.to_owned(),
            })
    }

    pub fn set_parameter(&mut self, id: &str, value: ParameterValue) -> Result<(), WorkflowError> {
        let parameter =
            self.parameters
                .get(id)
                .ok_or_else(|| WorkflowError::MissingParameterTarget {
                    node: NodeId::from(id),
                    parameter: id.to_owned(),
                })?;
        if parameter.parameter_type != value.parameter_type() {
            return Err(WorkflowError::ParameterTypeMismatch {
                parameter: id.to_owned(),
            });
        }
        let node_id = parameter.node_id.clone();
        let parameter_id = parameter.parameter_id.clone();
        self.graph
            .set_parameter(&node_id, &parameter_id, value.clone())?;
        if let Some(parameter) = self.parameters.get_mut(id) {
            parameter.default = value;
        }
        Ok(())
    }

    pub fn expose_input(
        &mut self,
        node_id: &NodeId,
        port_id: &str,
    ) -> Result<&WorkflowPort, WorkflowError> {
        let node = self
            .graph
            .node(node_id)
            .ok_or_else(|| GraphError::MissingNode(node_id.clone()))?;
        let port = input_port(node, port_id).ok_or_else(|| GraphError::MissingPort {
            node: node_id.clone(),
            port: port_id.to_owned(),
        })?;
        let id = format!("input:{}:{}", node_id, port_id);
        if self.inputs.iter().any(|input| input.id == id) {
            return Err(WorkflowError::DuplicatePort(id));
        }
        self.inputs.push(WorkflowPort::input(
            id.clone(),
            node_id.clone(),
            port_id,
            port.name,
            port.data_type,
            port.required,
        ));
        self.inputs.last().ok_or(WorkflowError::DuplicatePort(id))
    }

    pub fn expose_output(
        &mut self,
        node_id: &NodeId,
        port_id: &str,
    ) -> Result<&WorkflowPort, WorkflowError> {
        let node = self
            .graph
            .node(node_id)
            .ok_or_else(|| GraphError::MissingNode(node_id.clone()))?;
        let port = node
            .descriptor
            .output(port_id)
            .ok_or_else(|| GraphError::MissingPort {
                node: node_id.clone(),
                port: port_id.to_owned(),
            })?;
        let id = format!("output:{}:{}", node_id, port_id);
        if self.outputs.iter().any(|output| output.id == id) {
            return Err(WorkflowError::DuplicatePort(id));
        }
        self.outputs.push(WorkflowPort::output(
            id.clone(),
            node_id.clone(),
            port_id,
            port.name.clone(),
            port.data_type.clone(),
        ));
        self.outputs.last().ok_or(WorkflowError::DuplicatePort(id))
    }

    pub fn hide_port(&mut self, id: &str) -> bool {
        let before = self.inputs.len() + self.outputs.len();
        self.inputs.retain(|port| port.id != id);
        self.outputs.retain(|port| port.id != id);
        before != self.inputs.len() + self.outputs.len()
    }

    pub fn add_node_pack_dependency(
        &mut self,
        id: impl Into<String>,
        version: impl Into<String>,
    ) -> Result<(), WorkflowError> {
        let dependency = NodePackDependency::new(id, version);
        if self
            .node_pack_dependencies
            .iter()
            .any(|existing| existing.id == dependency.id)
        {
            return Err(WorkflowError::DuplicateDependency(dependency.id));
        }
        self.node_pack_dependencies.push(dependency);
        Ok(())
    }

    pub fn add_subgraph_dependency(
        &mut self,
        dependency: SubgraphDependency,
    ) -> Result<(), WorkflowError> {
        if self
            .subgraph_dependencies
            .iter()
            .any(|existing| existing.id == dependency.id)
        {
            return Err(WorkflowError::DuplicateDependency(dependency.id));
        }
        self.subgraph_dependencies.push(dependency);
        Ok(())
    }

    pub fn add_nested_subgraph(
        &mut self,
        subgraph: WorkflowDefinition,
    ) -> Result<(), WorkflowError> {
        let id = subgraph.identity.id.clone();
        if id == self.identity.id || subgraph.contains_nested(&self.identity.id) {
            return Err(WorkflowError::NestedCycle(id));
        }
        if self.nested_subgraphs.contains_key(&id) {
            return Err(WorkflowError::DuplicateDependency(id));
        }
        let dependency = SubgraphDependency::new(id.clone(), subgraph.version(), subgraph.hash());
        self.add_subgraph_dependency(dependency)?;
        self.nested_subgraphs.insert(id, Box::new(subgraph));
        Ok(())
    }

    pub fn nested_subgraph(&self, id: &str) -> Option<&WorkflowDefinition> {
        self.nested_subgraphs.get(id).map(Box::as_ref)
    }

    pub fn open_subgraph(&self, id: &str) -> Option<&WorkflowDefinition> {
        self.nested_subgraph(id)
    }

    pub fn open_subgraph_mut(&mut self, id: &str) -> Option<&mut WorkflowDefinition> {
        self.nested_subgraphs.get_mut(id).map(Box::as_mut)
    }

    pub fn instantiate(&self) -> Graph {
        self.graph.clone()
    }

    pub fn expand(&self) -> Graph {
        self.instantiate()
    }

    pub fn copy_internals(&self) -> Graph {
        self.instantiate()
    }

    pub fn to_json(&self) -> Result<String, WorkflowError> {
        self.validate_for_import()?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn export(&self) -> Result<Vec<u8>, WorkflowError> {
        Ok(self.to_json()?.into_bytes())
    }

    pub fn from_json(json: &str, registry: NodeRegistry) -> Result<Self, WorkflowError> {
        Self::import(json.as_bytes(), registry)
    }

    pub fn from_json_with_artifact_store(
        json: &str,
        registry: NodeRegistry,
        artifact_store: crate::ArtifactStore,
    ) -> Result<Self, WorkflowError> {
        Self::import_with_artifact_store(json.as_bytes(), registry, artifact_store)
    }

    pub fn import(bytes: &[u8], registry: NodeRegistry) -> Result<Self, WorkflowError> {
        Self::import_with_limits(bytes, registry, WorkflowImportLimits::default())
    }

    pub fn import_with_artifact_store(
        bytes: &[u8],
        registry: NodeRegistry,
        artifact_store: crate::ArtifactStore,
    ) -> Result<Self, WorkflowError> {
        Self::import_with_limits_and_artifact_store(
            bytes,
            registry,
            WorkflowImportLimits::default(),
            artifact_store,
        )
    }

    pub fn preflight(bytes: &[u8], registry: NodeRegistry) -> Result<Self, WorkflowError> {
        Self::import(bytes, registry)
    }

    pub fn import_with_limits(
        bytes: &[u8],
        registry: NodeRegistry,
        limits: WorkflowImportLimits,
    ) -> Result<Self, WorkflowError> {
        Self::import_with_limits_and_artifact_store(
            bytes,
            registry,
            limits,
            crate::ArtifactStore::memory(),
        )
    }

    pub fn import_with_limits_and_artifact_store(
        bytes: &[u8],
        registry: NodeRegistry,
        limits: WorkflowImportLimits,
        artifact_store: crate::ArtifactStore,
    ) -> Result<Self, WorkflowError> {
        if bytes.len() > limits.max_bytes {
            return Err(WorkflowError::ImportLimitExceeded {
                resource: "bytes",
                limit: limits.max_bytes,
            });
        }
        check_json_nesting_depth(bytes, limits.max_depth)?;
        let json = std::str::from_utf8(bytes)?;
        let mut definition = serde_json::from_str::<Self>(json)?;
        definition.attach_registry(&registry);
        definition.attach_artifact_store(&artifact_store);
        definition.check_import_limits(&limits)?;
        definition.validate_for_import()?;
        definition.validate_checkpoint_artifacts(&artifact_store)?;
        Ok(definition)
    }

    pub fn hash(&self) -> String {
        let mut edges = self.graph.edges().to_vec();
        edges.sort_by_key(|edge| {
            (
                edge.from_node.clone(),
                edge.from_port.clone(),
                edge.to_node.clone(),
                edge.to_port.clone(),
            )
        });
        let mut inputs = self.inputs.clone();
        inputs.sort_by_key(|port| port.id.clone());
        let mut outputs = self.outputs.clone();
        outputs.sort_by_key(|port| port.id.clone());
        let mut subgraph_dependencies = self.subgraph_dependencies.clone();
        subgraph_dependencies.sort_by_key(|dependency| {
            (
                dependency.id.clone(),
                dependency.version.clone(),
                dependency.hash.clone(),
            )
        });
        let mut node_pack_dependencies = self.node_pack_dependencies.clone();
        node_pack_dependencies
            .sort_by_key(|dependency| (dependency.id.clone(), dependency.version.clone()));
        let nested_hashes: BTreeMap<&str, String> = self
            .nested_subgraphs
            .iter()
            .map(|(id, workflow)| (id.as_str(), workflow.hash()))
            .collect();
        let document = HashDocument {
            identity: &self.identity,
            nodes: self.graph.nodes(),
            edges: &edges,
            parameters: &self.parameters,
            inputs: &inputs,
            outputs: &outputs,
            subgraph_dependencies: &subgraph_dependencies,
            node_pack_dependencies: &node_pack_dependencies,
            nested_hashes,
        };
        let bytes = serde_json::to_vec(&document)
            .unwrap_or_else(|_| b"rawweave-invalid-workflow-document".to_vec());
        stable_hash(&bytes)
    }

    pub fn diagnose_dependencies(
        &self,
        available_packs: &[NodePackManifest],
        available_subgraphs: &[SubgraphDependency],
    ) -> DependencyReport {
        let mut report = DependencyReport::default();
        for dependency in &self.node_pack_dependencies {
            let available = available_packs
                .iter()
                .find(|manifest| manifest.package_id == dependency.id);
            let status = match available {
                None => {
                    let diagnostic = DependencyDiagnostic {
                        id: dependency.id.clone(),
                        required_version: dependency.version.clone(),
                        available_version: None,
                    };
                    report.missing.push(diagnostic);
                    DependencyStatus::Missing
                }
                Some(manifest) if manifest.version != dependency.version => {
                    report.available.push(DependencyDiagnostic {
                        id: dependency.id.clone(),
                        required_version: dependency.version.clone(),
                        available_version: Some(manifest.version.clone()),
                    });
                    let diagnostic = DependencyDiagnostic {
                        id: dependency.id.clone(),
                        required_version: dependency.version.clone(),
                        available_version: Some(manifest.version.clone()),
                    };
                    report.mismatched.push(diagnostic);
                    DependencyStatus::VersionMismatch {
                        required: dependency.version.clone(),
                        available: manifest.version.clone(),
                    }
                }
                Some(manifest) => {
                    report.available.push(DependencyDiagnostic {
                        id: dependency.id.clone(),
                        required_version: dependency.version.clone(),
                        available_version: Some(manifest.version.clone()),
                    });
                    DependencyStatus::Available
                }
            };
            report.statuses.insert(dependency.id.clone(), status);
        }

        for dependency in &self.subgraph_dependencies {
            let available = available_subgraphs
                .iter()
                .find(|candidate| candidate.id == dependency.id);
            let status = match available {
                None => {
                    report.missing.push(DependencyDiagnostic {
                        id: dependency.id.clone(),
                        required_version: dependency.version.clone(),
                        available_version: None,
                    });
                    DependencyStatus::Missing
                }
                Some(candidate) if candidate.version != dependency.version => {
                    report.mismatched.push(DependencyDiagnostic {
                        id: dependency.id.clone(),
                        required_version: dependency.version.clone(),
                        available_version: Some(candidate.version.clone()),
                    });
                    DependencyStatus::VersionMismatch {
                        required: dependency.version.clone(),
                        available: candidate.version.clone(),
                    }
                }
                Some(candidate)
                    if !dependency.hash.is_empty() && dependency.hash != candidate.hash =>
                {
                    report.mismatched.push(DependencyDiagnostic {
                        id: dependency.id.clone(),
                        required_version: dependency.version.clone(),
                        available_version: Some(candidate.version.clone()),
                    });
                    DependencyStatus::VersionMismatch {
                        required: dependency.version.clone(),
                        available: candidate.version.clone(),
                    }
                }
                Some(candidate) => {
                    report.available.push(DependencyDiagnostic {
                        id: dependency.id.clone(),
                        required_version: dependency.version.clone(),
                        available_version: Some(candidate.version.clone()),
                    });
                    DependencyStatus::Available
                }
            };
            report.statuses.insert(dependency.id.clone(), status);
        }

        let known_nodes: BTreeSet<&str> = available_packs
            .iter()
            .flat_map(|manifest| manifest.nodes.iter().map(|node| node.type_id.as_str()))
            .collect();
        report.disabled_nodes = self
            .graph
            .nodes()
            .values()
            .filter(|node| !known_nodes.contains(node.type_id.as_str()))
            .map(|node| node.id.to_string())
            .collect();
        report
    }

    pub fn validate(&self) -> Result<(), WorkflowError> {
        self.validate_with_availability(false)
    }

    fn validate_for_import(&self) -> Result<(), WorkflowError> {
        self.validate_with_availability(true)
    }

    fn validate_with_availability(&self, allow_unavailable: bool) -> Result<(), WorkflowError> {
        WorkflowIdentity::new(self.identity.id.clone(), self.identity.version.clone())?;
        if allow_unavailable {
            self.graph.validate_for_import()?;
        } else {
            self.graph.validate()?;
        }
        let registry = self.graph.registry();
        for (key, node) in self.graph.nodes() {
            NodeId::try_new(key.as_str()).map_err(GraphError::InvalidNodeId)?;
            if key != &node.id {
                return Err(WorkflowError::NodeIdMismatch {
                    key: key.to_string(),
                    node: node.id.to_string(),
                });
            }
            match registry.descriptor(&node.type_id) {
                Some(registered) if &node.descriptor != registered => {
                    return Err(WorkflowError::NodeDescriptorMismatch {
                        node: node.id.clone(),
                        type_id: node.type_id.clone(),
                    });
                }
                Some(_) => {}
                None if !allow_unavailable => {
                    return Err(GraphError::UnknownNodeType {
                        type_id: node.type_id.clone(),
                    }
                    .into());
                }
                None if node.descriptor.type_id != node.type_id => {
                    return Err(WorkflowError::NodeDescriptorMismatch {
                        node: node.id.clone(),
                        type_id: node.type_id.clone(),
                    });
                }
                None => {}
            }
            for descriptor in &node.descriptor.parameters {
                validate_parameter_descriptor(&node.id, descriptor)?;
                let value = node.parameters.get(&descriptor.id).ok_or_else(|| {
                    GraphError::MissingParameter {
                        node: node.id.clone(),
                        parameter: descriptor.id.clone(),
                    }
                })?;
                validate_parameter_value(&node.id, descriptor, value)?;
            }
            for parameter_id in node.parameters.keys() {
                if node.descriptor.parameter(parameter_id).is_none() {
                    return Err(GraphError::MissingParameter {
                        node: node.id.clone(),
                        parameter: parameter_id.clone(),
                    }
                    .into());
                }
            }
            for parameter_id in &node.exposed_parameters {
                if node.descriptor.parameter(parameter_id).is_none() {
                    return Err(GraphError::MissingParameter {
                        node: node.id.clone(),
                        parameter: parameter_id.clone(),
                    }
                    .into());
                }
            }
        }

        let mut parameter_targets = BTreeSet::new();
        for (key, parameter) in &self.parameters {
            if key != &parameter.id || !parameter_targets.insert(key.as_str()) {
                return Err(WorkflowError::InvalidParameter(key.clone()));
            }
            let node = self
                .graph
                .node(&parameter.node_id)
                .ok_or_else(|| GraphError::MissingNode(parameter.node_id.clone()))?;
            let descriptor = node
                .descriptor
                .parameter(&parameter.parameter_id)
                .ok_or_else(|| WorkflowError::MissingParameterTarget {
                    node: parameter.node_id.clone(),
                    parameter: parameter.parameter_id.clone(),
                })?;
            if descriptor.parameter_type != parameter.parameter_type {
                return Err(WorkflowError::ParameterTypeMismatch {
                    parameter: parameter.id.clone(),
                });
            }
            validate_parameter_value(&parameter.node_id, descriptor, &parameter.default)?;
            if node.parameters.get(&parameter.parameter_id) != Some(&parameter.default)
                || !node.exposed_parameters.contains(&parameter.parameter_id)
            {
                return Err(WorkflowError::InvalidParameter(parameter.id.clone()));
            }
        }

        self.validate_ports()?;
        validate_dependencies(&self.node_pack_dependencies)?;
        validate_subgraph_dependencies(&self.subgraph_dependencies)?;
        for (key, nested) in &self.nested_subgraphs {
            if key != &nested.identity.id {
                return Err(WorkflowError::NestedIdentityMismatch {
                    key: key.clone(),
                    id: nested.identity.id.clone(),
                });
            }
            if key == &self.identity.id || nested.contains_nested(&self.identity.id) {
                return Err(WorkflowError::NestedCycle(key.clone()));
            }
            let dependency = self
                .subgraph_dependencies
                .iter()
                .find(|dependency| dependency.id == *key)
                .ok_or_else(|| WorkflowError::MissingNestedDependency(key.clone()))?;
            if dependency.version != nested.version() {
                return Err(WorkflowError::InvalidDependency(key.clone()));
            }
            if dependency.hash.is_empty() {
                return Err(WorkflowError::MissingNestedHash(key.clone()));
            }
            let actual_hash = nested.hash();
            if dependency.hash != actual_hash {
                return Err(WorkflowError::NestedHashMismatch {
                    id: key.clone(),
                    expected: dependency.hash.clone(),
                    actual: actual_hash,
                });
            }
            nested.validate_with_availability(allow_unavailable)?;
        }
        Ok(())
    }

    fn check_import_limits(&self, limits: &WorkflowImportLimits) -> Result<(), WorkflowError> {
        let mut counts = ImportCounts::default();
        self.count_import_resources(1, limits, &mut counts)?;
        Ok(())
    }

    fn count_import_resources(
        &self,
        depth: usize,
        limits: &WorkflowImportLimits,
        counts: &mut ImportCounts,
    ) -> Result<(), WorkflowError> {
        if depth > limits.max_depth {
            return Err(WorkflowError::ImportLimitExceeded {
                resource: "depth",
                limit: limits.max_depth,
            });
        }
        counts.nodes = counts.nodes.checked_add(self.graph.nodes().len()).ok_or(
            WorkflowError::ImportLimitExceeded {
                resource: "nodes",
                limit: limits.max_nodes,
            },
        )?;
        if counts.nodes > limits.max_nodes {
            return Err(WorkflowError::ImportLimitExceeded {
                resource: "nodes",
                limit: limits.max_nodes,
            });
        }
        counts.edges = counts.edges.checked_add(self.graph.edges().len()).ok_or(
            WorkflowError::ImportLimitExceeded {
                resource: "edges",
                limit: limits.max_edges,
            },
        )?;
        if counts.edges > limits.max_edges {
            return Err(WorkflowError::ImportLimitExceeded {
                resource: "edges",
                limit: limits.max_edges,
            });
        }
        counts.dependencies = counts
            .dependencies
            .checked_add(self.node_pack_dependencies.len())
            .and_then(|count| count.checked_add(self.subgraph_dependencies.len()))
            .ok_or(WorkflowError::ImportLimitExceeded {
                resource: "dependencies",
                limit: limits.max_dependencies,
            })?;
        if counts.dependencies > limits.max_dependencies {
            return Err(WorkflowError::ImportLimitExceeded {
                resource: "dependencies",
                limit: limits.max_dependencies,
            });
        }
        counts.metadata_bytes = counts
            .metadata_bytes
            .checked_add(metadata_size(&self.metadata))
            .ok_or(WorkflowError::ImportLimitExceeded {
                resource: "metadata",
                limit: limits.max_metadata_bytes,
            })?;
        if counts.metadata_bytes > limits.max_metadata_bytes {
            return Err(WorkflowError::ImportLimitExceeded {
                resource: "metadata",
                limit: limits.max_metadata_bytes,
            });
        }
        for nested in self.nested_subgraphs.values() {
            nested.count_import_resources(depth + 1, limits, counts)?;
        }
        Ok(())
    }

    fn validate_ports(&self) -> Result<(), WorkflowError> {
        let mut ids = BTreeSet::new();
        for port in &self.inputs {
            validate_port_id(port, WorkflowPortDirection::Input, &mut ids)?;
            let node = self
                .graph
                .node(&port.node_id)
                .ok_or_else(|| GraphError::MissingNode(port.node_id.clone()))?;
            let expected =
                input_port(node, &port.port_id).ok_or_else(|| GraphError::MissingPort {
                    node: port.node_id.clone(),
                    port: port.port_id.clone(),
                })?;
            if port.name != expected.name
                || port.data_type != expected.data_type
                || port.required != expected.required
            {
                return Err(WorkflowError::InvalidPort(port.id.clone()));
            }
        }
        for port in &self.outputs {
            validate_port_id(port, WorkflowPortDirection::Output, &mut ids)?;
            let node = self
                .graph
                .node(&port.node_id)
                .ok_or_else(|| GraphError::MissingNode(port.node_id.clone()))?;
            let expected =
                node.descriptor
                    .output(&port.port_id)
                    .ok_or_else(|| GraphError::MissingPort {
                        node: port.node_id.clone(),
                        port: port.port_id.clone(),
                    })?;
            if port.name != expected.name || port.data_type != expected.data_type || port.required {
                return Err(WorkflowError::InvalidPort(port.id.clone()));
            }
        }
        Ok(())
    }

    fn attach_registry(&mut self, registry: &NodeRegistry) {
        self.graph = self.graph.clone().with_registry(registry.clone());
        for nested in self.nested_subgraphs.values_mut() {
            nested.attach_registry(registry);
        }
    }

    fn attach_artifact_store(&mut self, store: &crate::ArtifactStore) {
        self.graph = self.graph.clone().with_artifact_store(store.clone());
        for nested in self.nested_subgraphs.values_mut() {
            nested.attach_artifact_store(store);
        }
    }

    fn validate_checkpoint_artifacts(
        &self,
        store: &crate::ArtifactStore,
    ) -> Result<(), WorkflowError> {
        self.graph.validate_checkpoint_artifacts(store)?;
        for nested in self.nested_subgraphs.values() {
            nested.validate_checkpoint_artifacts(store)?;
        }
        Ok(())
    }

    fn contains_nested(&self, id: &str) -> bool {
        self.nested_subgraphs
            .values()
            .any(|nested| nested.identity.id == id || nested.contains_nested(id))
    }
}

fn check_json_nesting_depth(
    bytes: &[u8],
    workflow_depth_limit: usize,
) -> Result<(), WorkflowError> {
    let parser_limit = workflow_depth_limit.saturating_mul(16).saturating_add(64);
    let mut depth = 0_usize;
    let mut maximum = 0_usize;
    let mut in_string = false;
    let mut escaped = false;
    for byte in bytes {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match *byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth = depth.saturating_add(1);
                maximum = maximum.max(depth);
                if maximum > parser_limit {
                    return Err(WorkflowError::ImportLimitExceeded {
                        resource: "depth",
                        limit: workflow_depth_limit,
                    });
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

#[derive(Default)]
struct ImportCounts {
    nodes: usize,
    edges: usize,
    dependencies: usize,
    metadata_bytes: usize,
}

fn metadata_size(metadata: &WorkflowMetadata) -> usize {
    let optional_size = |value: &Option<String>| value.as_deref().map_or(0, str::len);
    metadata
        .name
        .len()
        .saturating_add(optional_size(&metadata.author))
        .saturating_add(optional_size(&metadata.description))
        .saturating_add(optional_size(&metadata.thumbnail))
        .saturating_add(
            metadata
                .tags
                .iter()
                .map(String::len)
                .fold(0, usize::saturating_add),
        )
        .saturating_add(optional_size(&metadata.license))
        .saturating_add(optional_size(&metadata.recommended_input_type))
        .saturating_add(optional_size(&metadata.minimum_app_version))
}

fn validate_parameter_descriptor(
    node_id: &NodeId,
    descriptor: &ParameterDescriptor,
) -> Result<(), WorkflowError> {
    if descriptor.min.is_some_and(|value| !value.is_finite())
        || descriptor.max.is_some_and(|value| !value.is_finite())
        || descriptor
            .min
            .zip(descriptor.max)
            .is_some_and(|(minimum, maximum)| minimum > maximum)
    {
        return Err(WorkflowError::InvalidNodeParameter {
            node: node_id.clone(),
            parameter: descriptor.id.clone(),
        });
    }
    validate_parameter_value(node_id, descriptor, &descriptor.default)?;
    Ok(())
}

fn validate_parameter_value(
    node_id: &NodeId,
    descriptor: &ParameterDescriptor,
    value: &ParameterValue,
) -> Result<(), GraphError> {
    if descriptor.parameter_type != value.parameter_type() {
        return Err(GraphError::ParameterTypeMismatch {
            node: node_id.clone(),
            parameter: descriptor.id.clone(),
        });
    }
    if let ParameterValue::Float(number) = value {
        if !number.is_finite() {
            return Err(GraphError::ParameterNotFinite {
                node: node_id.clone(),
                parameter: descriptor.id.clone(),
            });
        }
        if descriptor.min.is_some_and(|minimum| *number < minimum)
            || descriptor.max.is_some_and(|maximum| *number > maximum)
        {
            return Err(GraphError::ParameterOutOfRange {
                node: node_id.clone(),
                parameter: descriptor.id.clone(),
            });
        }
    }
    Ok(())
}

fn validate_port_id(
    port: &WorkflowPort,
    direction: WorkflowPortDirection,
    ids: &mut BTreeSet<String>,
) -> Result<(), WorkflowError> {
    if port.direction != direction
        || port.id.trim().is_empty()
        || port.name.trim().is_empty()
        || port.port_id.trim().is_empty()
        || port.data_type.trim().is_empty()
        || !ids.insert(port.id.clone())
    {
        return Err(WorkflowError::InvalidPort(port.id.clone()));
    }
    Ok(())
}

fn validate_dependencies(dependencies: &[NodePackDependency]) -> Result<(), WorkflowError> {
    let mut ids = BTreeSet::new();
    for dependency in dependencies {
        if dependency.id.trim().is_empty() || dependency.version.trim().is_empty() {
            return Err(WorkflowError::InvalidDependency(dependency.id.clone()));
        }
        if !ids.insert(dependency.id.as_str()) {
            return Err(WorkflowError::DuplicateDependency(dependency.id.clone()));
        }
    }
    Ok(())
}

fn validate_subgraph_dependencies(
    dependencies: &[SubgraphDependency],
) -> Result<(), WorkflowError> {
    let mut ids = BTreeSet::new();
    for dependency in dependencies {
        if dependency.id.trim().is_empty() || dependency.version.trim().is_empty() {
            return Err(WorkflowError::InvalidDependency(dependency.id.clone()));
        }
        if !ids.insert(dependency.id.as_str()) {
            return Err(WorkflowError::DuplicateDependency(dependency.id.clone()));
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct HashDocument<'a> {
    identity: &'a WorkflowIdentity,
    nodes: &'a BTreeMap<NodeId, GraphNode>,
    edges: &'a [GraphEdge],
    parameters: &'a BTreeMap<String, WorkflowParameter>,
    inputs: &'a [WorkflowPort],
    outputs: &'a [WorkflowPort],
    subgraph_dependencies: &'a [SubgraphDependency],
    node_pack_dependencies: &'a [NodePackDependency],
    nested_hashes: BTreeMap<&'a str, String>,
}

fn input_port(node: &GraphNode, port_id: &str) -> Option<PortInfo> {
    if let Some(port) = node.descriptor.input(port_id) {
        return Some(PortInfo {
            name: port.name.clone(),
            data_type: port.data_type.clone(),
            required: port.required,
        });
    }
    if node.exposed_parameters.contains(port_id) {
        let parameter = node.descriptor.parameter(port_id)?;
        return Some(PortInfo {
            name: parameter.name.clone(),
            data_type: parameter_data_type(parameter.parameter_type).to_owned(),
            required: false,
        });
    }
    None
}

struct PortInfo {
    name: String,
    data_type: String,
    required: bool,
}

fn parameter_data_type(parameter_type: ParameterType) -> &'static str {
    match parameter_type {
        ParameterType::Float => "value.Float",
        ParameterType::Integer => "value.Integer",
        ParameterType::Boolean => "value.Boolean",
        ParameterType::String => "value.String",
    }
}

fn stable_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
