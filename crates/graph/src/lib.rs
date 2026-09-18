use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use rawweave_core::{CoreError, NodeId};
use rawweave_node_api::{
    EvaluationContext, ExecutionCapability, Inputs, NodeDescriptor, NodeError, NodeRegistry,
    NodeResult, ParameterValue, Value,
};
use rawweave_rendering::{CacheKey, GraphRevision, MemoryRenderCache, RenderResult, TileCoord};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: NodeId,
    pub type_id: String,
    pub descriptor: NodeDescriptor,
    pub parameters: rawweave_node_api::Parameters,
}

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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Graph {
    nodes: BTreeMap<NodeId, GraphNode>,
    edges: Vec<GraphEdge>,
    revision: u64,
    #[serde(skip, default)]
    registry: NodeRegistry,
    #[serde(skip, default)]
    render_cache: Arc<Mutex<MemoryRenderCache>>,
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
        }
    }

    pub fn with_render_cache(mut self, cache: MemoryRenderCache) -> Self {
        self.render_cache = Arc::new(Mutex::new(cache));
        self
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
        };
        self.nodes.insert(id, node);
        self.bump_revision();
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
        let source_port =
            source
                .descriptor
                .output(from_port)
                .ok_or_else(|| GraphError::MissingPort {
                    node: from_node.clone(),
                    port: from_port.to_owned(),
                })?;
        let target = self
            .nodes
            .get(&to_node)
            .ok_or_else(|| GraphError::MissingNode(to_node.clone()))?;
        let target_port =
            target
                .descriptor
                .input(to_port)
                .ok_or_else(|| GraphError::MissingPort {
                    node: to_node.clone(),
                    port: to_port.to_owned(),
                })?;
        if source_port.data_type != target_port.data_type {
            return Err(GraphError::TypeMismatch {
                expected: target_port.data_type.clone(),
                actual: source_port.data_type.clone(),
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
            let target_port =
                target
                    .descriptor
                    .input(&edge.to_port)
                    .ok_or_else(|| GraphError::MissingPort {
                        node: edge.to_node.clone(),
                        port: edge.to_port.clone(),
                    })?;
            if source_port.data_type != target_port.data_type {
                return Err(GraphError::TypeMismatch {
                    expected: target_port.data_type.clone(),
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
        let result = self.evaluate_node(node_id, context, &mut memo, &mut visiting)?;
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

    pub fn to_json(&self) -> Result<String, GraphError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(json: &str, registry: NodeRegistry) -> Result<Self, GraphError> {
        let graph = serde_json::from_str::<Graph>(json)?.with_registry(registry);
        graph.validate()?;
        Ok(graph)
    }

    fn evaluate_node(
        &self,
        node_id: &NodeId,
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
        let mut inputs = Inputs::new();
        let mut upstream_hasher = DefaultHasher::new();
        backend_identity(&execution_context).hash(&mut upstream_hasher);
        if let Some(source_image) = execution_context.source_image.as_ref() {
            0_u8.hash(&mut upstream_hasher);
            hash_image(source_image, &mut upstream_hasher);
        }
        for edge in self.edges.iter().filter(|edge| edge.to_node == *node_id) {
            let upstream =
                self.evaluate_node(&edge.from_node, &execution_context, memo, visiting)?;
            let value = upstream
                .result
                .outputs
                .get(&edge.from_port)
                .cloned()
                .ok_or_else(|| GraphError::MissingOutput {
                    node: edge.from_node.clone(),
                    port: edge.from_port.clone(),
                })?;
            edge.from_node.as_str().hash(&mut upstream_hasher);
            edge.from_port.hash(&mut upstream_hasher);
            edge.to_port.hash(&mut upstream_hasher);
            upstream.output_hash.hash(&mut upstream_hasher);
            inputs.insert(edge.to_port.clone(), value);
        }
        for port in &node.descriptor.inputs {
            if port.required && !inputs.contains_key(&port.id) {
                return Err(GraphError::Evaluation {
                    node: node_id.clone(),
                    source: NodeError::MissingInput(port.id.clone()),
                });
            }
        }
        let cache_key = CacheKey::new(
            node.id.as_str(),
            node.descriptor.version,
            hash_parameters(&node.parameters),
            hash_output_schema(node),
            upstream_hasher.finish(),
            execution_context.requested_region().unwrap_or_default(),
            execution_context.tile(),
            execution_context.mip_level(),
            execution_context.quality(),
            backend_identity_hash(&execution_context),
        );
        let current_revision = self.graph_revision();
        let cached = self
            .render_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get_current(&cache_key, current_revision));
        if let Some(result) = cached.and_then(|cached| cached_node_result(node, cached)) {
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
            .evaluate(&inputs, &node.parameters, &execution_context)
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
        }
        visiting.remove(node_id);
        memo.insert(memo_key, evaluated.clone());
        Ok(evaluated)
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

fn cached_node_result(node: &GraphNode, cached: RenderResult) -> Option<NodeResult> {
    let output = node.descriptor.outputs.first()?;
    (node.descriptor.outputs.len() == 1 && output.data_type == "core.Image").then(|| {
        NodeResult::single(
            output.id.clone(),
            Value::Image(cached.image.as_ref().clone()),
        )
    })
}

fn single_image(result: &NodeResult) -> Option<&rawweave_image::Image> {
    (result.outputs.len() == 1)
        .then(|| result.outputs.values().next())
        .flatten()
        .and_then(|value| match value {
            Value::Image(image) => Some(image),
            Value::Float(_) => None,
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
        Value::Float(number) => {
            1_u8.hash(hasher);
            number.to_bits().hash(hasher);
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
