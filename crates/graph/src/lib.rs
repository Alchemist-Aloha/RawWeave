use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use rawweave_core::{CoreError, NodeId};
use rawweave_node_api::{
    EvaluationContext, ExecutionCapability, Inputs, LazyCondition, LazyInputGate, NodeDescriptor,
    NodeError, NodeRegistry, NodeResult, ParameterType, ParameterValue, Value,
};
use rawweave_rendering::{
    CacheKey, GraphRevision, MaskRenderResult, MemoryRenderCache, RenderResult, TileCoord,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

mod checkpoint;
mod workflow;

pub use checkpoint::*;
pub use rawweave_node_api::EvaluationPolicy;
pub use workflow::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: NodeId,
    pub type_id: String,
    pub descriptor: NodeDescriptor,
    pub parameters: rawweave_node_api::Parameters,
    /// Parameters optionally exposed as typed input ports. Exposed parameters
    /// may be connected without becoming static descriptor inputs.
    #[serde(default)]
    pub exposed_parameters: BTreeSet<String>,
}

/// The wildcard type id accepted by generic routing nodes such as `Switch`.
pub const ANY_TYPE: &str = "core.Any";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from_node: NodeId,
    pub from_port: String,
    pub to_node: NodeId,
    pub to_port: String,
}

#[derive(Debug, Error)]
pub enum GraphError {
    #[error("node '{0}' already exists")]
    DuplicateNode(NodeId),
    #[error("node '{0}' does not exist")]
    MissingNode(NodeId),
    #[error("node type '{type_id}' is not registered")]
    UnknownNodeType { type_id: String },
    #[error("port '{port}' does not exist on node '{node}'")]
    MissingPort { node: NodeId, port: String },
    #[error("cannot connect '{actual}' to '{expected}'")]
    TypeMismatch { expected: String, actual: String },
    #[error("input '{node}:{port}' already has a connection")]
    InputAlreadyConnected { node: NodeId, port: String },
    #[error("connection does not exist")]
    EdgeNotFound,
    #[error("connection would create a cycle")]
    CycleDetected,
    #[error("parameter '{parameter}' does not exist on node '{node}'")]
    MissingParameter { node: NodeId, parameter: String },
    #[error("parameter '{parameter}' on node '{node}' has the wrong type")]
    ParameterTypeMismatch { node: NodeId, parameter: String },
    #[error("parameter '{parameter}' on node '{node}' is outside its allowed range")]
    ParameterOutOfRange { node: NodeId, parameter: String },
    #[error("parameter '{parameter}' on node '{node}' must be finite")]
    ParameterNotFinite { node: NodeId, parameter: String },
    #[error("output '{node}:{port}' was not produced")]
    MissingOutput { node: NodeId, port: String },
    #[error("node '{node}' failed: {source}")]
    Evaluation { node: NodeId, source: NodeError },
    #[error("invalid node id: {0}")]
    InvalidNodeId(#[from] CoreError),
    #[error("workflow serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("checkpoint evaluation failed: {0}")]
    Checkpoint(#[from] CheckpointError),
}

#[derive(Clone, Debug)]
pub struct Graph {
    nodes: BTreeMap<NodeId, GraphNode>,
    edges: Vec<GraphEdge>,
    revision: u64,
    registry: NodeRegistry,
    render_cache: Arc<Mutex<MemoryRenderCache>>,
    checkpoints: Arc<Mutex<BTreeMap<NodeId, Checkpoint>>>,
    artifact_store: ArtifactStore,
}

#[derive(Serialize, Deserialize)]
struct GraphDocument {
    nodes: BTreeMap<NodeId, GraphNode>,
    edges: Vec<GraphEdge>,
    revision: u64,
    #[serde(default)]
    checkpoints: BTreeMap<NodeId, Checkpoint>,
}

impl Serialize for Graph {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let checkpoints = self
            .checkpoints
            .lock()
            .map_err(|_| serde::ser::Error::custom("checkpoint store is poisoned"))?
            .clone();
        GraphDocument {
            nodes: self.nodes.clone(),
            edges: self.edges.clone(),
            revision: self.revision,
            checkpoints,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Graph {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let document = GraphDocument::deserialize(deserializer)?;
        Ok(Self {
            nodes: document.nodes,
            edges: document.edges,
            revision: document.revision,
            registry: NodeRegistry::default(),
            render_cache: Arc::new(Mutex::new(MemoryRenderCache::default())),
            checkpoints: Arc::new(Mutex::new(document.checkpoints)),
            artifact_store: ArtifactStore::memory(),
        })
    }
}

#[derive(Clone)]
struct EvaluatedNode {
    result: NodeResult,
    output_hash: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum BackendIdentity {
    NoRenderContext,
    Cpu,
    Gpu(u64),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct MemoKey {
    node_id: NodeId,
    requested_output: Option<String>,
    requested_region: Option<rawweave_image::Region>,
    tile: TileCoord,
    mip_level: u8,
    quality: rawweave_rendering::PreviewQuality,
    backend: BackendIdentity,
}

impl Graph {
    pub fn new(registry: NodeRegistry) -> Self {
        Self {
            nodes: BTreeMap::new(),
            edges: Vec::new(),
            revision: 0,
            registry,
            render_cache: Arc::new(Mutex::new(MemoryRenderCache::default())),
            checkpoints: Arc::new(Mutex::new(BTreeMap::new())),
            artifact_store: ArtifactStore::memory(),
        }
    }

    pub fn with_render_cache(mut self, cache: MemoryRenderCache) -> Self {
        self.render_cache = Arc::new(Mutex::new(cache));
        self
    }

    /// Attach the durable store used by checkpoints during normal graph
    /// evaluation. The store is shared by graph clones.
    pub fn with_artifact_store(mut self, store: ArtifactStore) -> Self {
        self.artifact_store = store;
        self
    }

    pub fn artifact_store(&self) -> ArtifactStore {
        self.artifact_store.clone()
    }

    /// Register the committed state for a manual checkpoint node. Normal
    /// demand-driven evaluation uses this state instead of instantiating a
    /// `ManualCheckpoint` node.
    pub fn register_checkpoint(&mut self, checkpoint: Checkpoint) -> Result<(), GraphError> {
        let node_id =
            NodeId::try_new(checkpoint.node_id.as_str()).map_err(GraphError::InvalidNodeId)?;
        let node = self
            .nodes
            .get(&node_id)
            .ok_or_else(|| GraphError::MissingNode(node_id.clone()))?;
        if node.descriptor.evaluation_policy != EvaluationPolicy::ManualCheckpoint {
            return Err(GraphError::Evaluation {
                node: node_id,
                source: NodeError::Message("node is not a manual checkpoint".to_owned()),
            });
        }
        if checkpoint.node_version != node.descriptor.version {
            return Err(GraphError::Checkpoint(
                CheckpointError::NodeVersionMismatch {
                    expected: node.descriptor.version,
                    actual: checkpoint.node_version,
                },
            ));
        }
        let affected = self.downstream_nodes(&node.id);
        self.checkpoints
            .lock()
            .map_err(|_| GraphError::Checkpoint(CheckpointError::StorePoisoned))?
            .insert(node_id, checkpoint);
        self.invalidate_nodes(&affected);
        self.bump_revision();
        Ok(())
    }

    pub fn set_checkpoint(&mut self, checkpoint: Checkpoint) -> Result<(), GraphError> {
        self.register_checkpoint(checkpoint)
    }

    pub fn checkpoint(&self, node_id: &NodeId) -> Result<Option<Checkpoint>, GraphError> {
        Ok(self
            .checkpoints
            .lock()
            .map_err(|_| GraphError::Checkpoint(CheckpointError::StorePoisoned))?
            .get(node_id)
            .cloned())
    }

    pub fn commit_checkpoint(
        &self,
        node_id: &NodeId,
        artifact: CheckpointArtifact,
    ) -> Result<(), GraphError> {
        let mut checkpoints = self
            .checkpoints
            .lock()
            .map_err(|_| GraphError::Checkpoint(CheckpointError::StorePoisoned))?;
        let checkpoint = checkpoints
            .get_mut(node_id)
            .ok_or_else(|| GraphError::MissingNode(node_id.clone()))?;
        checkpoint.commit(artifact, &self.artifact_store)?;
        Ok(())
    }

    pub fn render_cache(&self) -> Arc<Mutex<MemoryRenderCache>> {
        Arc::clone(&self.render_cache)
    }

    pub fn graph_revision(&self) -> GraphRevision {
        GraphRevision::from(self.revision)
    }

    pub fn with_registry(mut self, registry: NodeRegistry) -> Self {
        self.registry = registry;
        self
    }

    pub fn registry(&self) -> NodeRegistry {
        self.registry.clone()
    }

    pub fn nodes(&self) -> &BTreeMap<NodeId, GraphNode> {
        &self.nodes
    }

    pub fn node(&self, id: &NodeId) -> Option<&GraphNode> {
        self.nodes.get(id)
    }

    pub fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn clone_selection(&self, selection: &BTreeSet<NodeId>) -> Result<Self, GraphError> {
        for node_id in selection {
            if !self.nodes.contains_key(node_id) {
                return Err(GraphError::MissingNode(node_id.clone()));
            }
        }
        let nodes = self
            .nodes
            .iter()
            .filter(|(node_id, _)| selection.contains(*node_id))
            .map(|(node_id, node)| (node_id.clone(), node.clone()))
            .collect();
        let edges = self
            .edges
            .iter()
            .filter(|edge| selection.contains(&edge.from_node) && selection.contains(&edge.to_node))
            .cloned()
            .collect();
        Ok(Self {
            nodes,
            edges,
            revision: 0,
            registry: self.registry.clone(),
            render_cache: Arc::new(Mutex::new(MemoryRenderCache::default())),
            checkpoints: Arc::new(Mutex::new(BTreeMap::new())),
            artifact_store: self.artifact_store.clone(),
        })
    }

    pub fn downstream_nodes(&self, source: &NodeId) -> BTreeSet<NodeId> {
        let mut downstream = BTreeSet::new();
        let mut pending = vec![source.clone()];
        while let Some(node) = pending.pop() {
            if !downstream.insert(node.clone()) {
                continue;
            }
            pending.extend(
                self.edges
                    .iter()
                    .filter(|edge| edge.from_node == node)
                    .map(|edge| edge.to_node.clone()),
            );
        }
        downstream
    }

    pub fn add_node(&mut self, id: NodeId, type_id: &str) -> Result<(), GraphError> {
        NodeId::try_new(id.as_str()).map_err(GraphError::InvalidNodeId)?;
        if self.nodes.contains_key(&id) {
            return Err(GraphError::DuplicateNode(id));
        }
        let descriptor = self.registry.descriptor(type_id).cloned().ok_or_else(|| {
            GraphError::UnknownNodeType {
                type_id: type_id.to_owned(),
            }
        })?;
        let node = GraphNode {
            id: id.clone(),
            type_id: type_id.to_owned(),
            parameters: descriptor.parameter_defaults(),
            descriptor,
            exposed_parameters: BTreeSet::new(),
        };
        self.nodes.insert(id, node);
        self.bump_revision();
        Ok(())
    }

    /// Expose a node parameter as a typed input port so it may be driven by a
    /// connection. The stored literal value is preserved.
    pub fn expose_parameter(
        &mut self,
        node_id: &NodeId,
        parameter_id: &str,
    ) -> Result<(), GraphError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| GraphError::MissingNode(node_id.clone()))?;
        if node.descriptor.parameter(parameter_id).is_none() {
            return Err(GraphError::MissingParameter {
                node: node_id.clone(),
                parameter: parameter_id.to_owned(),
            });
        }
        node.exposed_parameters.insert(parameter_id.to_owned());
        self.bump_revision();
        Ok(())
    }

    /// Stop exposing a parameter. Any connection targeting it is removed and
    /// the stored literal value becomes authoritative again.
    pub fn unexpose_parameter(
        &mut self,
        node_id: &NodeId,
        parameter_id: &str,
    ) -> Result<(), GraphError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| GraphError::MissingNode(node_id.clone()))?;
        if node.descriptor.parameter(parameter_id).is_none() {
            return Err(GraphError::MissingParameter {
                node: node_id.clone(),
                parameter: parameter_id.to_owned(),
            });
        }
        let was_exposed = node.exposed_parameters.remove(parameter_id);
        let dropped = self
            .edges
            .iter()
            .any(|edge| edge.to_node == *node_id && edge.to_port == parameter_id);
        self.edges
            .retain(|edge| !(edge.to_node == *node_id && edge.to_port == parameter_id));
        if was_exposed || dropped {
            let affected = self.downstream_nodes(node_id);
            self.invalidate_nodes(&affected);
            self.bump_revision();
        }
        Ok(())
    }

    pub fn remove_node(&mut self, id: &NodeId) -> Result<(), GraphError> {
        let affected = self.downstream_nodes(id);
        if self.nodes.remove(id).is_none() {
            return Err(GraphError::MissingNode(id.clone()));
        }
        self.edges
            .retain(|edge| &edge.from_node != id && &edge.to_node != id);
        self.invalidate_nodes(&affected);
        self.bump_revision();
        Ok(())
    }

    pub fn set_parameter(
        &mut self,
        node_id: &NodeId,
        parameter_id: &str,
        value: ParameterValue,
    ) -> Result<(), GraphError> {
        {
            let node = self
                .nodes
                .get_mut(node_id)
                .ok_or_else(|| GraphError::MissingNode(node_id.clone()))?;
            let descriptor = node.descriptor.parameter(parameter_id).ok_or_else(|| {
                GraphError::MissingParameter {
                    node: node_id.clone(),
                    parameter: parameter_id.to_owned(),
                }
            })?;
            if descriptor.parameter_type != value.parameter_type() {
                return Err(GraphError::ParameterTypeMismatch {
                    node: node_id.clone(),
                    parameter: parameter_id.to_owned(),
                });
            }
            if let ParameterValue::Float(number) = &value {
                if !number.is_finite() {
                    return Err(GraphError::ParameterNotFinite {
                        node: node_id.clone(),
                        parameter: parameter_id.to_owned(),
                    });
                }
                if descriptor.min.is_some_and(|minimum| *number < minimum)
                    || descriptor.max.is_some_and(|maximum| *number > maximum)
                {
                    return Err(GraphError::ParameterOutOfRange {
                        node: node_id.clone(),
                        parameter: parameter_id.to_owned(),
                    });
                }
                node.parameters.insert(parameter_id.to_owned(), value);
            } else {
                node.parameters.insert(parameter_id.to_owned(), value);
            }
        }
        let affected = self.downstream_nodes(node_id);
        self.invalidate_nodes(&affected);
        self.bump_revision();
        Ok(())
    }

    pub fn connect(
        &mut self,
        from_node: NodeId,
        from_port: &str,
        to_node: NodeId,
        to_port: &str,
    ) -> Result<(), GraphError> {
        let source = self
            .nodes
            .get(&from_node)
            .ok_or_else(|| GraphError::MissingNode(from_node.clone()))?;
        let source_type = source
            .descriptor
            .output(from_port)
            .ok_or_else(|| GraphError::MissingPort {
                node: from_node.clone(),
                port: from_port.to_owned(),
            })?
            .data_type
            .clone();
        let target = self
            .nodes
            .get(&to_node)
            .ok_or_else(|| GraphError::MissingNode(to_node.clone()))?;
        let expected_type =
            target_input_type(target, to_port).ok_or_else(|| GraphError::MissingPort {
                node: to_node.clone(),
                port: to_port.to_owned(),
            })?;
        if !types_compatible(&expected_type, &source_type) {
            return Err(GraphError::TypeMismatch {
                expected: expected_type,
                actual: source_type,
            });
        }
        if self
            .edges
            .iter()
            .any(|edge| edge.to_node == to_node && edge.to_port == to_port)
        {
            return Err(GraphError::InputAlreadyConnected {
                node: to_node,
                port: to_port.to_owned(),
            });
        }

        self.edges.push(GraphEdge {
            from_node,
            from_port: from_port.to_owned(),
            to_node: to_node.clone(),
            to_port: to_port.to_owned(),
        });
        if self.has_cycle() {
            self.edges.pop();
            return Err(GraphError::CycleDetected);
        }
        let affected = self.downstream_nodes(&to_node);
        self.invalidate_nodes(&affected);
        self.bump_revision();
        Ok(())
    }

    pub fn disconnect(
        &mut self,
        from_node: NodeId,
        from_port: &str,
        to_node: NodeId,
        to_port: &str,
    ) -> Result<(), GraphError> {
        let index = self
            .edges
            .iter()
            .position(|edge| {
                edge.from_node == from_node
                    && edge.from_port == from_port
                    && edge.to_node == to_node
                    && edge.to_port == to_port
            })
            .ok_or(GraphError::EdgeNotFound)?;
        let affected = self.downstream_nodes(&to_node);
        self.edges.remove(index);
        self.invalidate_nodes(&affected);
        self.bump_revision();
        Ok(())
    }

    pub fn validate(&self) -> Result<(), GraphError> {
        let mut connected_inputs = BTreeSet::new();
        for edge in &self.edges {
            if !connected_inputs.insert((&edge.to_node, edge.to_port.as_str())) {
                return Err(GraphError::InputAlreadyConnected {
                    node: edge.to_node.clone(),
                    port: edge.to_port.clone(),
                });
            }
            let source = self
                .nodes
                .get(&edge.from_node)
                .ok_or_else(|| GraphError::MissingNode(edge.from_node.clone()))?;
            let target = self
                .nodes
                .get(&edge.to_node)
                .ok_or_else(|| GraphError::MissingNode(edge.to_node.clone()))?;
            let source_port = source.descriptor.output(&edge.from_port).ok_or_else(|| {
                GraphError::MissingPort {
                    node: edge.from_node.clone(),
                    port: edge.from_port.clone(),
                }
            })?;
            let expected_type = target_input_type(target, &edge.to_port).ok_or_else(|| {
                GraphError::MissingPort {
                    node: edge.to_node.clone(),
                    port: edge.to_port.clone(),
                }
            })?;
            if !types_compatible(&expected_type, &source_port.data_type) {
                return Err(GraphError::TypeMismatch {
                    expected: expected_type,
                    actual: source_port.data_type.clone(),
                });
            }
        }
        if self.has_cycle() {
            return Err(GraphError::CycleDetected);
        }
        Ok(())
    }

    /// Validate graph topology and the portions of its edges whose node types
    /// are available. This is used while importing workflows that may refer to
    /// an optional node pack; execution still requires full validation.
    pub(crate) fn validate_for_import(&self) -> Result<(), GraphError> {
        let mut connected_inputs = BTreeSet::new();
        for edge in &self.edges {
            if !connected_inputs.insert((&edge.to_node, edge.to_port.as_str())) {
                return Err(GraphError::InputAlreadyConnected {
                    node: edge.to_node.clone(),
                    port: edge.to_port.clone(),
                });
            }
            let source = self
                .nodes
                .get(&edge.from_node)
                .ok_or_else(|| GraphError::MissingNode(edge.from_node.clone()))?;
            let target = self
                .nodes
                .get(&edge.to_node)
                .ok_or_else(|| GraphError::MissingNode(edge.to_node.clone()))?;
            let source_available = self.registry.descriptor(&source.type_id).is_some();
            let target_available = self.registry.descriptor(&target.type_id).is_some();

            let source_port = if source_available {
                Some(source.descriptor.output(&edge.from_port).ok_or_else(|| {
                    GraphError::MissingPort {
                        node: edge.from_node.clone(),
                        port: edge.from_port.clone(),
                    }
                })?)
            } else {
                None
            };
            let expected_type = if target_available {
                Some(target_input_type(target, &edge.to_port).ok_or_else(|| {
                    GraphError::MissingPort {
                        node: edge.to_node.clone(),
                        port: edge.to_port.clone(),
                    }
                })?)
            } else {
                None
            };
            if let (Some(source_port), Some(expected_type)) = (source_port, expected_type)
                && !types_compatible(&expected_type, &source_port.data_type)
            {
                return Err(GraphError::TypeMismatch {
                    expected: expected_type,
                    actual: source_port.data_type.clone(),
                });
            }
        }
        if self.has_cycle() {
            return Err(GraphError::CycleDetected);
        }
        Ok(())
    }

    pub fn evaluate(
        &self,
        node_id: &NodeId,
        output_port: &str,
        context: &EvaluationContext,
    ) -> Result<Value, GraphError> {
        let node = self
            .nodes
            .get(node_id)
            .ok_or_else(|| GraphError::MissingNode(node_id.clone()))?;
        if node.descriptor.output(output_port).is_none() {
            return Err(GraphError::MissingPort {
                node: node_id.clone(),
                port: output_port.to_owned(),
            });
        }
        let mut memo = HashMap::new();
        let mut visiting = BTreeSet::new();
        let result = self.evaluate_node(
            node_id,
            Some(output_port),
            context,
            &mut memo,
            &mut visiting,
        )?;
        result
            .result
            .outputs
            .get(output_port)
            .cloned()
            .ok_or_else(|| GraphError::MissingOutput {
                node: node_id.clone(),
                port: output_port.to_owned(),
            })
    }

    /// Resolve a manual checkpoint without executing its node instance.
    ///
    /// The dependency hash is refreshed from the evaluated upstream outputs;
    /// stale checkpoints continue to serve their last committed artifact so
    /// downstream automatic nodes remain usable until an explicit regenerate.
    pub fn evaluate_checkpoint(
        &self,
        node_id: &NodeId,
        output_port: &str,
        context: &EvaluationContext,
        checkpoint: &mut Checkpoint,
        store: &ArtifactStore,
    ) -> Result<Value, GraphError> {
        let node = self
            .nodes
            .get(node_id)
            .ok_or_else(|| GraphError::MissingNode(node_id.clone()))?;
        if node.descriptor.evaluation_policy != EvaluationPolicy::ManualCheckpoint {
            return Err(GraphError::Evaluation {
                node: node_id.clone(),
                source: NodeError::Message("node is not a manual checkpoint".to_owned()),
            });
        }
        let output =
            node.descriptor
                .output(output_port)
                .ok_or_else(|| GraphError::MissingPort {
                    node: node_id.clone(),
                    port: output_port.to_owned(),
                })?;
        if checkpoint.node_id != node_id.as_str() {
            return Err(GraphError::Checkpoint(CheckpointError::InvalidProvenance(
                "checkpoint belongs to a different node".to_owned(),
            )));
        }
        if checkpoint.node_version != node.descriptor.version {
            return Err(GraphError::Checkpoint(
                CheckpointError::NodeVersionMismatch {
                    expected: node.descriptor.version,
                    actual: checkpoint.node_version,
                },
            ));
        }

        let mut upstream_values = BTreeMap::new();
        for edge in self.edges.iter().filter(|edge| edge.to_node == *node_id) {
            let value = self.evaluate(&edge.from_node, &edge.from_port, context)?;
            upstream_values.insert(
                format!("{}:{}->{}", edge.from_node, edge.from_port, edge.to_port),
                value,
            );
        }
        let mut parameters = node.parameters.clone();
        for parameter in &node.descriptor.parameters {
            if let Some(over) = context.parameter_override(node_id.as_str(), &parameter.id)
                && over.parameter_type() == parameter.parameter_type
            {
                parameters.insert(parameter.id.clone(), over.clone());
            }
        }
        checkpoint.set_dependency_hash(manual_dependency_hash(
            node,
            output_port,
            &parameters,
            &upstream_values,
            context,
        ));
        let artifact = checkpoint
            .committed_artifact(store)?
            .ok_or(CheckpointError::NoCommittedArtifact)?;
        let value = artifact.payload.to_value();
        if !types_compatible(&output.data_type, value.data_type()) {
            return Err(GraphError::TypeMismatch {
                expected: output.data_type.clone(),
                actual: value.data_type().to_owned(),
            });
        }
        Ok(value)
    }

    pub fn to_json(&self) -> Result<String, GraphError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(json: &str, registry: NodeRegistry) -> Result<Self, GraphError> {
        let graph = serde_json::from_str::<Graph>(json)?.with_registry(registry);
        graph.validate()?;
        Ok(graph)
    }

    pub fn from_json_with_artifact_store(
        json: &str,
        registry: NodeRegistry,
        artifact_store: ArtifactStore,
    ) -> Result<Self, GraphError> {
        let graph = serde_json::from_str::<Graph>(json)?
            .with_registry(registry)
            .with_artifact_store(artifact_store.clone());
        graph.validate()?;
        graph.validate_checkpoint_artifacts(&artifact_store)?;
        Ok(graph)
    }

    pub(crate) fn validate_checkpoint_artifacts(
        &self,
        store: &ArtifactStore,
    ) -> Result<(), GraphError> {
        let checkpoints = self
            .checkpoints
            .lock()
            .map_err(|_| GraphError::Checkpoint(CheckpointError::StorePoisoned))?
            .clone();
        for checkpoint in checkpoints.values() {
            if checkpoint.committed_artifact_id().is_some()
                && checkpoint.committed_artifact(store)?.is_none()
            {
                return Err(GraphError::Checkpoint(CheckpointError::NoCommittedArtifact));
            }
        }
        Ok(())
    }

    fn evaluate_port_value(
        &self,
        node_id: &NodeId,
        port: &str,
        context: &EvaluationContext,
        memo: &mut HashMap<MemoKey, EvaluatedNode>,
        visiting: &mut BTreeSet<NodeId>,
    ) -> Result<Option<Value>, GraphError> {
        let Some(edge) = self
            .edges
            .iter()
            .find(|edge| edge.to_node == *node_id && edge.to_port == port)
        else {
            return Ok(None);
        };
        let upstream = self.evaluate_node(
            &edge.from_node,
            Some(&edge.from_port),
            context,
            memo,
            visiting,
        )?;
        upstream
            .result
            .outputs
            .get(&edge.from_port)
            .cloned()
            .map(Some)
            .ok_or_else(|| GraphError::MissingOutput {
                node: edge.from_node.clone(),
                port: edge.from_port.clone(),
            })
    }

    fn evaluate_node(
        &self,
        node_id: &NodeId,
        requested_output: Option<&str>,
        context: &EvaluationContext,
        memo: &mut HashMap<MemoKey, EvaluatedNode>,
        visiting: &mut BTreeSet<NodeId>,
    ) -> Result<EvaluatedNode, GraphError> {
        let node = self
            .nodes
            .get(node_id)
            .ok_or_else(|| GraphError::MissingNode(node_id.clone()))?;
        let capability = node.descriptor.select_execution_capability(context);
        let execution_context = match capability {
            Some(ExecutionCapability::FullFrame) => EvaluationContext {
                requested_region: None,
                tile: TileCoord::default(),
                ..context.clone()
            },
            _ => context.clone(),
        };
        let memo_key = MemoKey {
            node_id: node_id.clone(),
            requested_output: requested_output.map(str::to_owned),
            requested_region: execution_context.requested_region(),
            tile: execution_context.tile(),
            mip_level: execution_context.mip_level(),
            quality: execution_context.quality(),
            backend: backend_identity(&execution_context),
        };
        if let Some(result) = memo.get(&memo_key) {
            return Ok(result.clone());
        }
        if !visiting.insert(node_id.clone()) {
            return Err(GraphError::CycleDetected);
        }
        let mut effective_parameters = node.parameters.clone();
        for parameter in &node.descriptor.parameters {
            if let Some(over) = context.parameter_override(node_id.as_str(), &parameter.id)
                && over.parameter_type() == parameter.parameter_type
            {
                effective_parameters.insert(parameter.id.clone(), over.clone());
            }
        }
        let mut included: Option<BTreeSet<String>> = None;
        if !node.descriptor.lazy_inputs.is_empty() {
            let mut selected = BTreeSet::new();
            for gate in &node.descriptor.lazy_inputs {
                selected.extend(gate.required.iter().cloned());
                let selector = self.evaluate_port_value(
                    node_id,
                    &gate.selector,
                    &execution_context,
                    memo,
                    visiting,
                )?;
                selected.extend(gate_inputs(gate, selector.as_ref()));
            }
            included = Some(selected);
        }
        let mut inputs = Inputs::new();
        let mut upstream_values = BTreeMap::new();
        let mut upstream_hasher = DefaultHasher::new();
        backend_identity(&execution_context).hash(&mut upstream_hasher);
        hash_evaluation_context(&execution_context, &mut upstream_hasher);
        for edge in self.edges.iter().filter(|edge| edge.to_node == *node_id) {
            if included
                .as_ref()
                .is_some_and(|selected| !selected.contains(&edge.to_port))
            {
                continue;
            }
            let upstream = self.evaluate_node(
                &edge.from_node,
                Some(&edge.from_port),
                &execution_context,
                memo,
                visiting,
            )?;
            let value = upstream
                .result
                .outputs
                .get(&edge.from_port)
                .cloned()
                .ok_or_else(|| GraphError::MissingOutput {
                    node: edge.from_node.clone(),
                    port: edge.from_port.clone(),
                })?;
            upstream_values.insert(
                format!("{}:{}->{}", edge.from_node, edge.from_port, edge.to_port),
                value.clone(),
            );
            edge.from_node.as_str().hash(&mut upstream_hasher);
            edge.from_port.hash(&mut upstream_hasher);
            edge.to_port.hash(&mut upstream_hasher);
            upstream.output_hash.hash(&mut upstream_hasher);
            if node.descriptor.input(&edge.to_port).is_none()
                && node.exposed_parameters.contains(&edge.to_port)
            {
                let parameter = node.descriptor.parameter(&edge.to_port).ok_or_else(|| {
                    GraphError::Evaluation {
                        node: node_id.clone(),
                        source: NodeError::InvalidParameter(edge.to_port.clone()),
                    }
                })?;
                let converted =
                    value_to_parameter(&value, parameter.parameter_type).ok_or_else(|| {
                        GraphError::Evaluation {
                            node: node_id.clone(),
                            source: NodeError::InvalidParameter(edge.to_port.clone()),
                        }
                    })?;
                effective_parameters.insert(edge.to_port.clone(), converted);
                continue;
            }
            let coerced = match target_input_type(node, &edge.to_port) {
                Some(expected) => coerce_value(value.clone(), &expected).ok_or_else(|| {
                    GraphError::TypeMismatch {
                        expected,
                        actual: value.data_type().to_owned(),
                    }
                })?,
                None => value,
            };
            inputs.insert(edge.to_port.clone(), coerced);
        }
        for port in &node.descriptor.inputs {
            if port.required && !inputs.contains_key(&port.id) {
                if included
                    .as_ref()
                    .is_some_and(|selected| !selected.contains(&port.id))
                {
                    continue;
                }
                return Err(GraphError::Evaluation {
                    node: node_id.clone(),
                    source: NodeError::MissingInput(port.id.clone()),
                });
            }
        }
        if node.descriptor.evaluation_policy == EvaluationPolicy::ManualCheckpoint {
            let requested_output = requested_output.ok_or_else(|| GraphError::MissingPort {
                node: node_id.clone(),
                port: "<requested output>".to_owned(),
            })?;
            let dependency_hash = manual_dependency_hash(
                node,
                requested_output,
                &effective_parameters,
                &upstream_values,
                &execution_context,
            );
            let value = self.resolve_registered_checkpoint(node_id, dependency_hash)?;
            let output = node.descriptor.output(requested_output).ok_or_else(|| {
                GraphError::MissingPort {
                    node: node_id.clone(),
                    port: requested_output.to_owned(),
                }
            })?;
            if !types_compatible(&output.data_type, value.data_type()) {
                return Err(GraphError::TypeMismatch {
                    expected: output.data_type.clone(),
                    actual: value.data_type().to_owned(),
                });
            }
            let evaluated = EvaluatedNode {
                output_hash: hash_node_result(&NodeResult::single(requested_output, value.clone())),
                result: NodeResult::single(requested_output, value),
            };
            visiting.remove(node_id);
            memo.insert(memo_key, evaluated.clone());
            return Ok(evaluated);
        }
        let cache_key = CacheKey::new(
            node.id.as_str(),
            node.descriptor.version,
            hash_parameters(&effective_parameters),
            hash_output_schema(node),
            upstream_hasher.finish(),
            execution_context.requested_region().unwrap_or_default(),
            execution_context.tile(),
            execution_context.mip_level(),
            execution_context.quality(),
            backend_identity_hash(&execution_context),
        );
        let current_revision = self.graph_revision();
        let cached = self.render_cache.lock().ok().map(|cache| {
            (
                cache.get_current(&cache_key, current_revision),
                cache.get_mask_current(&cache_key, current_revision),
            )
        });
        if let Some((cached_image, cached_mask)) = cached
            && let Some(result) = cached_node_result(node, cached_image, cached_mask)
        {
            let evaluated = EvaluatedNode {
                output_hash: hash_node_result(&result),
                result,
            };
            visiting.remove(node_id);
            memo.insert(memo_key.clone(), evaluated.clone());
            return Ok(evaluated);
        }
        let instance = self.registry.instantiate(&node.type_id).ok_or_else(|| {
            GraphError::UnknownNodeType {
                type_id: node.type_id.clone(),
            }
        })?;
        let result = instance
            .evaluate(&inputs, &effective_parameters, &execution_context)
            .map_err(|source| GraphError::Evaluation {
                node: node_id.clone(),
                source,
            })?;
        let evaluated = EvaluatedNode {
            output_hash: hash_node_result(&result),
            result,
        };
        if let Some(image) = single_image(&evaluated.result) {
            let render = RenderResult::new(image.clone(), current_revision);
            if let Ok(mut cache) = self.render_cache.lock() {
                cache.insert_if_current(cache_key, render, current_revision);
            }
        } else if let Some(mask) = single_mask(&evaluated.result) {
            let render = MaskRenderResult::new(mask.clone(), current_revision);
            if let Ok(mut cache) = self.render_cache.lock() {
                cache.insert_mask_if_current(cache_key, render, current_revision);
            }
        }
        visiting.remove(node_id);
        memo.insert(memo_key, evaluated.clone());
        Ok(evaluated)
    }

    fn resolve_registered_checkpoint(
        &self,
        node_id: &NodeId,
        dependency_hash: String,
    ) -> Result<Value, GraphError> {
        let mut checkpoints = self
            .checkpoints
            .lock()
            .map_err(|_| GraphError::Checkpoint(CheckpointError::StorePoisoned))?;
        let checkpoint = checkpoints
            .get_mut(node_id)
            .ok_or(GraphError::Checkpoint(CheckpointError::NoCommittedArtifact))?;
        checkpoint.set_dependency_hash(dependency_hash);
        let artifact = checkpoint
            .committed_artifact(&self.artifact_store)?
            .ok_or(CheckpointError::NoCommittedArtifact)?;
        Ok(artifact.payload.to_value())
    }

    fn invalidate_nodes(&self, node_ids: &BTreeSet<NodeId>) {
        if let Ok(mut cache) = self.render_cache.lock() {
            cache.invalidate_nodes(node_ids.iter().map(NodeId::as_str));
        }
    }
    fn has_cycle(&self) -> bool {
        let mut adjacency: BTreeMap<&NodeId, Vec<&NodeId>> = BTreeMap::new();
        for edge in &self.edges {
            adjacency
                .entry(&edge.from_node)
                .or_default()
                .push(&edge.to_node);
        }
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        self.nodes
            .keys()
            .any(|node| has_cycle_from(node, &adjacency, &mut visiting, &mut visited))
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.saturating_add(1);
        let revision = self.graph_revision();
        if let Ok(mut cache) = self.render_cache.lock() {
            cache.restamp_revision(revision);
        }
    }
}

fn manual_dependency_hash(
    node: &GraphNode,
    requested_output: &str,
    parameters: &rawweave_node_api::Parameters,
    upstream_values: &BTreeMap<String, Value>,
    context: &EvaluationContext,
) -> String {
    let mut hasher = StableHasher::default();
    hash_field(&mut hasher, b"rawweave-manual-checkpoint-v1");
    hash_field(&mut hasher, node.id.as_str().as_bytes());
    hash_field(&mut hasher, node.type_id.as_bytes());
    node.descriptor.version.hash(&mut hasher);
    hash_field(&mut hasher, requested_output.as_bytes());
    hash_stable_parameters(parameters, &mut hasher);
    for (edge, value) in upstream_values {
        hash_field(&mut hasher, edge.as_bytes());
        hash_stable_value(value, &mut hasher);
    }
    hash_stable_context(context, &mut hasher);
    hasher.finish_hex()
}

#[derive(Default)]
struct StableHasher(Sha256);

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        let digest = self.0.clone().finalize();
        u64::from_be_bytes(digest[..8].try_into().expect("sha256 has eight bytes"))
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
}

impl StableHasher {
    fn finish_hex(self) -> String {
        self.0
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

fn hash_field(hasher: &mut impl Hasher, bytes: &[u8]) {
    (bytes.len() as u64).to_be_bytes().hash(hasher);
    hasher.write(bytes);
}

fn hash_stable_parameters(parameters: &rawweave_node_api::Parameters, hasher: &mut impl Hasher) {
    for (id, value) in parameters {
        hash_field(hasher, id.as_bytes());
        match value {
            ParameterValue::Float(number) => {
                0_u8.hash(hasher);
                number.to_bits().hash(hasher);
            }
            ParameterValue::Integer(number) => {
                1_u8.hash(hasher);
                number.hash(hasher);
            }
            ParameterValue::Boolean(value) => {
                2_u8.hash(hasher);
                value.hash(hasher);
            }
            ParameterValue::String(value) => {
                3_u8.hash(hasher);
                hash_field(hasher, value.as_bytes());
            }
        }
    }
}

fn hash_stable_context(context: &EvaluationContext, hasher: &mut impl Hasher) {
    match context.source_image.as_ref() {
        Some(image) => {
            0_u8.hash(hasher);
            hash_stable_value(&Value::Image(image.clone()), hasher);
        }
        None => 1_u8.hash(hasher),
    }
    match context.source_bytes.as_ref() {
        Some(bytes) => {
            2_u8.hash(hasher);
            hash_field(hasher, bytes);
        }
        None => 3_u8.hash(hasher),
    }
    match context.source_path.as_ref() {
        Some(path) => {
            4_u8.hash(hasher);
            hash_field(hasher, path.as_os_str().to_string_lossy().as_bytes());
        }
        None => 5_u8.hash(hasher),
    }
    for (id, value) in &context.external_inputs {
        hash_field(hasher, id.as_bytes());
        hash_stable_value(value, hasher);
    }
    for (id, bytes) in &context.assets {
        hash_field(hasher, id.as_bytes());
        hash_field(hasher, bytes);
    }
    for ((node_id, parameter_id), value) in &context.parameter_overrides {
        hash_field(hasher, node_id.as_bytes());
        hash_field(hasher, parameter_id.as_bytes());
        hash_stable_parameter(value, hasher);
    }
    context.requested_region().hash(hasher);
    context.tile().hash(hasher);
    context.mip_level().hash(hasher);
    context.quality().hash(hasher);
    context
        .render_context()
        .map(|render_context| render_context.gpu_available())
        .hash(hasher);
}

fn hash_stable_parameter(value: &ParameterValue, hasher: &mut impl Hasher) {
    hash_stable_parameters(
        &[("value".to_owned(), value.clone())].into_iter().collect(),
        hasher,
    );
}

fn hash_stable_value(value: &Value, hasher: &mut impl Hasher) {
    match value {
        Value::Image(image) => {
            0_u8.hash(hasher);
            image.dimensions().hash(hasher);
            image.origin().hash(hasher);
            image.pixel_format().hash(hasher);
            image.color_metadata().hash(hasher);
            for pixel in image.pixels() {
                for channel in pixel {
                    channel.to_bits().hash(hasher);
                }
            }
        }
        _ => hash_value(value, hasher),
    }
}

fn hash_evaluation_context(context: &EvaluationContext, hasher: &mut impl Hasher) {
    match context.source_image.as_ref() {
        Some(source_image) => {
            0_u8.hash(hasher);
            hash_image(source_image, hasher);
        }
        None => 1_u8.hash(hasher),
    }
    match context.source_bytes.as_ref() {
        Some(source_bytes) => {
            2_u8.hash(hasher);
            source_bytes.hash(hasher);
        }
        None => 3_u8.hash(hasher),
    }
    match context.source_path.as_ref() {
        Some(source_path) => {
            4_u8.hash(hasher);
            source_path.as_os_str().hash(hasher);
        }
        None => 5_u8.hash(hasher),
    }
    6_u8.hash(hasher);
    for (id, value) in &context.external_inputs {
        id.hash(hasher);
        hash_value(value, hasher);
    }
    7_u8.hash(hasher);
    for (id, bytes) in &context.assets {
        id.hash(hasher);
        bytes.hash(hasher);
    }
}

fn backend_identity(context: &EvaluationContext) -> BackendIdentity {
    match context.render_context() {
        None => BackendIdentity::NoRenderContext,
        Some(render_context) => render_context.gpu().map_or(BackendIdentity::Cpu, |gpu| {
            BackendIdentity::Gpu(gpu.context_id())
        }),
    }
}

fn backend_identity_hash(context: &EvaluationContext) -> u64 {
    let mut hasher = DefaultHasher::new();
    backend_identity(context).hash(&mut hasher);
    hasher.finish()
}

/// Resolve the expected graph data type for a connection target, accepting
/// either a static input port or an exposed parameter of the same id.
fn target_input_type(node: &GraphNode, port: &str) -> Option<String> {
    if let Some(descriptor) = node.descriptor.input(port) {
        return Some(descriptor.data_type.clone());
    }
    node.exposed_parameters
        .contains(port)
        .then(|| node.descriptor.parameter(port))
        .flatten()
        .map(|parameter| parameter_data_type(parameter.parameter_type).to_owned())
}

fn parameter_data_type(parameter_type: ParameterType) -> &'static str {
    match parameter_type {
        ParameterType::Float => "value.Float",
        ParameterType::Integer => "value.Integer",
        ParameterType::Boolean => "value.Boolean",
        ParameterType::String => "value.String",
    }
}

/// Two ports may connect when their types match, when either side is the
/// `core.Any` wildcard used by routing nodes, or for the documented safe
/// conversions: integer/float numerics and boolean/condition predicates.
fn types_compatible(expected: &str, actual: &str) -> bool {
    expected == actual
        || expected == ANY_TYPE
        || actual == ANY_TYPE
        || matches!(
            (expected, actual),
            ("value.Float", "value.Integer")
                | ("value.Integer", "value.Float")
                | ("value.Condition", "value.Boolean")
                | ("value.Boolean", "value.Condition")
        )
}

/// Convert an upstream value to the type a downstream input port expects.
fn coerce_value(value: Value, expected: &str) -> Option<Value> {
    if expected == ANY_TYPE || value.data_type() == expected {
        return Some(value);
    }
    match (expected, value) {
        ("value.Float", Value::Integer(number)) => Some(Value::Float(number as f32)),
        ("value.Integer", Value::Float(number)) if number.fract() == 0.0 => {
            Some(Value::Integer(number as i64))
        }
        ("value.Condition", Value::Boolean(value)) => Some(Value::Condition(value)),
        ("value.Boolean", Value::Condition(value)) => Some(Value::Boolean(value)),
        _ => None,
    }
}

/// Convert a connected control value into a stored parameter value.
fn value_to_parameter(value: &Value, parameter_type: ParameterType) -> Option<ParameterValue> {
    match (parameter_type, value) {
        (ParameterType::Float, Value::Float(number)) => Some(ParameterValue::Float(*number)),
        (ParameterType::Float, Value::Integer(number)) => {
            Some(ParameterValue::Float(*number as f32))
        }
        (ParameterType::Integer, Value::Integer(number)) => Some(ParameterValue::Integer(*number)),
        (ParameterType::Integer, Value::Float(number)) if number.fract() == 0.0 => {
            Some(ParameterValue::Integer(*number as i64))
        }
        (ParameterType::Boolean, Value::Boolean(value)) => Some(ParameterValue::Boolean(*value)),
        (ParameterType::Boolean, Value::Condition(value)) => Some(ParameterValue::Boolean(*value)),
        (ParameterType::String, Value::String(value)) => {
            Some(ParameterValue::String(value.clone()))
        }
        (ParameterType::String, Value::Enum(value)) => Some(ParameterValue::String(value.clone())),
        _ => None,
    }
}

/// Decide which input ports a lazy gate requires for the observed selector.
fn gate_inputs(gate: &LazyInputGate, selector: Option<&Value>) -> Vec<String> {
    let Some(selector) = selector else {
        return Vec::new();
    };
    gate.branches
        .iter()
        .find(|branch| lazy_condition_matches(&branch.condition, selector))
        .map(|branch| branch.inputs.clone())
        .unwrap_or_default()
}

fn lazy_condition_matches(condition: &LazyCondition, selector: &Value) -> bool {
    match (condition, selector) {
        (LazyCondition::True, Value::Condition(value) | Value::Boolean(value)) => *value,
        (LazyCondition::False, Value::Condition(value) | Value::Boolean(value)) => !*value,
        (LazyCondition::Index(index), Value::Integer(value)) => i64::from(*index) == *value,
        (LazyCondition::Index(index), Value::Float(value)) => {
            value.fract() == 0.0 && *value as i64 == i64::from(*index)
        }
        (LazyCondition::Key(key), Value::String(value) | Value::Enum(value)) => key == value,
        _ => false,
    }
}

fn cached_node_result(
    node: &GraphNode,
    cached_image: Option<RenderResult>,
    cached_mask: Option<MaskRenderResult>,
) -> Option<NodeResult> {
    let output = node.descriptor.outputs.first()?;
    if node.descriptor.outputs.len() != 1 {
        return None;
    }
    match output.data_type.as_str() {
        "core.Image" => cached_image.map(|cached| {
            NodeResult::single(
                output.id.clone(),
                Value::Image(cached.image.as_ref().clone()),
            )
        }),
        "core.Mask" => cached_mask.map(|cached| {
            NodeResult::single(output.id.clone(), Value::Mask(cached.mask.as_ref().clone()))
        }),
        _ => None,
    }
}

fn single_image(result: &NodeResult) -> Option<&rawweave_image::Image> {
    (result.outputs.len() == 1)
        .then(|| result.outputs.values().next())
        .flatten()
        .and_then(|value| match value {
            Value::Image(image) => Some(image),
            _ => None,
        })
}

fn single_mask(result: &NodeResult) -> Option<&rawweave_image::Mask> {
    (result.outputs.len() == 1)
        .then(|| result.outputs.values().next())
        .flatten()
        .and_then(|value| match value {
            Value::Mask(mask) => Some(mask),
            _ => None,
        })
}

fn hash_output_schema(node: &GraphNode) -> u64 {
    let mut hasher = DefaultHasher::new();
    for output in &node.descriptor.outputs {
        output.id.hash(&mut hasher);
        output.name.hash(&mut hasher);
        output.data_type.hash(&mut hasher);
    }
    hasher.finish()
}

fn hash_parameters(parameters: &rawweave_node_api::Parameters) -> u64 {
    let mut hasher = DefaultHasher::new();
    for (id, value) in parameters {
        id.hash(&mut hasher);
        match value {
            ParameterValue::Float(number) => {
                0_u8.hash(&mut hasher);
                number.to_bits().hash(&mut hasher);
            }
            ParameterValue::Integer(number) => {
                3_u8.hash(&mut hasher);
                number.hash(&mut hasher);
            }
            ParameterValue::Boolean(value) => {
                1_u8.hash(&mut hasher);
                value.hash(&mut hasher);
            }
            ParameterValue::String(value) => {
                2_u8.hash(&mut hasher);
                value.hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

fn hash_node_result(result: &NodeResult) -> u64 {
    let mut hasher = DefaultHasher::new();
    for (port, value) in &result.outputs {
        port.hash(&mut hasher);
        hash_value(value, &mut hasher);
    }
    hasher.finish()
}

fn hash_value(value: &Value, hasher: &mut impl Hasher) {
    match value {
        Value::Image(image) => {
            0_u8.hash(hasher);
            hash_image(image, hasher);
        }
        Value::Mask(mask) => {
            19_u8.hash(hasher);
            mask.cache_identity().hash(hasher);
        }
        Value::MaskSet(set) => {
            20_u8.hash(hasher);
            for mask in set.masks() {
                mask.cache_identity().hash(hasher);
            }
        }
        Value::LabelMap(map) => {
            21_u8.hash(hasher);
            map.cache_identity().hash(hasher);
        }
        Value::ConfidenceMap(map) => {
            22_u8.hash(hasher);
            map.mask().cache_identity().hash(hasher);
        }
        Value::DepthMap(map) => {
            23_u8.hash(hasher);
            map.cache_identity().hash(hasher);
        }
        Value::RegionSet(set) => {
            24_u8.hash(hasher);
            set.regions().hash(hasher);
        }
        Value::Float(number) => {
            1_u8.hash(hasher);
            number.to_bits().hash(hasher);
        }
        Value::Integer(number) => {
            12_u8.hash(hasher);
            number.hash(hasher);
        }
        Value::Boolean(value) => {
            13_u8.hash(hasher);
            value.hash(hasher);
        }
        Value::String(value) => {
            14_u8.hash(hasher);
            value.hash(hasher);
        }
        Value::Enum(value) => {
            15_u8.hash(hasher);
            value.hash(hasher);
        }
        Value::Color(color) => {
            16_u8.hash(hasher);
            for channel in [color.red, color.green, color.blue, color.alpha] {
                channel.to_bits().hash(hasher);
            }
        }
        Value::Condition(value) => {
            17_u8.hash(hasher);
            value.hash(hasher);
        }
        Value::Metadata(metadata) => {
            18_u8.hash(hasher);
            hash_metadata(metadata, hasher);
        }
        Value::Bytes(bytes) => {
            2_u8.hash(hasher);
            bytes.hash(hasher);
        }
        Value::RawFrame(frame) => {
            3_u8.hash(hasher);
            hash_raw_frame(frame, hasher);
        }
        Value::Mosaic(mosaic) => {
            4_u8.hash(hasher);
            hash_mosaic(mosaic, hasher);
        }
        Value::SceneLinearRGB(scene) => {
            5_u8.hash(hasher);
            hash_scene_linear(scene, hasher);
        }
        Value::DisplayRGB(display) => {
            6_u8.hash(hasher);
            hash_display(display, hasher);
        }
        Value::CameraMetadata(metadata) => {
            7_u8.hash(hasher);
            hash_camera_metadata(metadata, hasher);
        }
        Value::ExifMetadata(metadata) => {
            8_u8.hash(hasher);
            for (key, value) in &metadata.tags {
                key.hash(hasher);
                value.hash(hasher);
            }
        }
        Value::CameraProfile(profile) => {
            9_u8.hash(hasher);
            hash_camera_profile(profile, hasher);
        }
        Value::LensProfile(profile) => {
            10_u8.hash(hasher);
            hash_lens_profile(profile, hasher);
        }
        Value::EmbeddedPreview(preview) => {
            11_u8.hash(hasher);
            hash_embedded_preview(preview, hasher);
        }
    }
}

fn hash_camera_profile(profile: &rawweave_raw::CameraProfile, hasher: &mut impl Hasher) {
    profile.make.hash(hasher);
    profile.model.hash(hasher);
    for row in &profile.xyz_to_camera {
        for value in row {
            value.to_bits().hash(hasher);
        }
    }
    for row in &profile.camera_to_xyz {
        for value in row {
            value.to_bits().hash(hasher);
        }
    }
}

fn hash_lens_profile(profile: &rawweave_raw::LensProfile, hasher: &mut impl Hasher) {
    profile.name.hash(hasher);
    for value in profile
        .radial_distortion
        .iter()
        .chain(profile.tangential_distortion.iter())
        .chain(profile.vignette.iter())
    {
        value.to_bits().hash(hasher);
    }
    profile.provenance.hash(hasher);
}

fn hash_embedded_preview(preview: &rawweave_raw::EmbeddedPreview, hasher: &mut impl Hasher) {
    preview.bytes().hash(hasher);
    preview.mime_type().hash(hasher);
}

fn hash_raw_frame(frame: &rawweave_raw::RawFrame, hasher: &mut impl Hasher) {
    hash_mosaic(frame.mosaic(), hasher);
    for level in frame.black_levels().iter().chain(frame.white_levels()) {
        level.to_bits().hash(hasher);
    }
    hash_camera_metadata(frame.camera(), hasher);
    hash_camera_profile(frame.profile(), hasher);
    match frame.lens_profile() {
        Some(profile) => {
            true.hash(hasher);
            hash_lens_profile(profile, hasher);
        }
        None => false.hash(hasher),
    }
    hash_embedded_preview(frame.embedded_preview(), hasher);
    for (key, value) in &frame.exif().tags {
        key.hash(hasher);
        value.hash(hasher);
    }
}

fn hash_mosaic(mosaic: &rawweave_raw::Mosaic, hasher: &mut impl Hasher) {
    mosaic.dimensions().hash(hasher);
    mosaic.bit_depth().hash(hasher);
    mosaic.orientation().hash(hasher);
    mosaic.cfa().width().hash(hasher);
    mosaic.cfa().height().hash(hasher);
    mosaic.cfa().colors().hash(hasher);
    for sample in mosaic.samples() {
        sample.to_bits().hash(hasher);
    }
}

fn hash_metadata(metadata: &rawweave_node_api::Metadata, hasher: &mut impl Hasher) {
    metadata.make.hash(hasher);
    metadata.model.hash(hasher);
    metadata.lens.hash(hasher);
    metadata.iso.hash(hasher);
    hash_optional_float(metadata.aperture, hasher);
    hash_optional_float(metadata.shutter_seconds, hasher);
    hash_optional_float(metadata.focal_length_mm, hasher);
    metadata.capture_time.hash(hasher);
    metadata.orientation.hash(hasher);
    metadata.rating.hash(hasher);
    for (key, value) in &metadata.tags {
        key.hash(hasher);
        value.hash(hasher);
    }
}

fn hash_camera_metadata(metadata: &rawweave_raw::CameraMetadata, hasher: &mut impl Hasher) {
    metadata.make.hash(hasher);
    metadata.model.hash(hasher);
    metadata.lens.hash(hasher);
    metadata.iso.hash(hasher);
    hash_optional_float(metadata.aperture, hasher);
    hash_optional_float(metadata.shutter_seconds, hasher);
    hash_optional_float(metadata.focal_length_mm, hasher);
    metadata.capture_time.hash(hasher);
    metadata.orientation.hash(hasher);
}

fn hash_optional_float(value: Option<f32>, hasher: &mut impl Hasher) {
    value.map(f32::to_bits).hash(hasher);
}

fn hash_scene_linear(scene: &rawweave_color::SceneLinearRGB, hasher: &mut impl Hasher) {
    scene.dimensions().hash(hasher);
    hash_working_space(&scene.working_space(), hasher);
    for pixel in scene.pixels() {
        for channel in pixel {
            channel.to_bits().hash(hasher);
        }
    }
}

fn hash_display(display: &rawweave_color::DisplayRGB, hasher: &mut impl Hasher) {
    display.dimensions().hash(hasher);
    hash_working_space(&display.working_space(), hasher);
    for pixel in display.pixels() {
        for channel in pixel {
            channel.to_bits().hash(hasher);
        }
    }
}

fn hash_working_space(space: &rawweave_color::WorkingSpace, hasher: &mut impl Hasher) {
    match space {
        rawweave_color::WorkingSpace::CameraNative => 0_u8.hash(hasher),
        rawweave_color::WorkingSpace::Srgb => 1_u8.hash(hasher),
        rawweave_color::WorkingSpace::DisplayP3 => 2_u8.hash(hasher),
        rawweave_color::WorkingSpace::ProPhoto => 3_u8.hash(hasher),
        rawweave_color::WorkingSpace::Rec2020 => 4_u8.hash(hasher),
        rawweave_color::WorkingSpace::Custom(name) => {
            5_u8.hash(hasher);
            name.hash(hasher);
        }
    }
}

fn hash_image(image: &rawweave_image::Image, hasher: &mut impl Hasher) {
    image.revision().hash(hasher);
    image.dimensions().hash(hasher);
    image.origin().hash(hasher);
    image.pixel_format().hash(hasher);
    image.color_metadata().hash(hasher);
    for pixel in image.pixels() {
        for channel in pixel {
            channel.to_bits().hash(hasher);
        }
    }
}

fn has_cycle_from<'a>(
    node: &'a NodeId,
    adjacency: &BTreeMap<&'a NodeId, Vec<&'a NodeId>>,
    visiting: &mut BTreeSet<&'a NodeId>,
    visited: &mut BTreeSet<&'a NodeId>,
) -> bool {
    if visiting.contains(node) {
        return true;
    }
    if visited.contains(node) {
        return false;
    }
    visiting.insert(node);
    if adjacency.get(node).is_some_and(|children| {
        children
            .iter()
            .any(|child| has_cycle_from(child, adjacency, visiting, visited))
    }) {
        return true;
    }
    visiting.remove(node);
    visited.insert(node);
    false
}
