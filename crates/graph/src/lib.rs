use std::collections::{BTreeMap, BTreeSet};

use rawweave_core::{CoreError, NodeId};
use rawweave_node_api::{
    EvaluationContext, Inputs, NodeDescriptor, NodeError, NodeRegistry, NodeResult, ParameterValue,
    Value,
};
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
}

impl Graph {
    pub fn new(registry: NodeRegistry) -> Self {
        Self {
            nodes: BTreeMap::new(),
            edges: Vec::new(),
            revision: 0,
            registry,
        }
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
        if self.nodes.remove(id).is_none() {
            return Err(GraphError::MissingNode(id.clone()));
        }
        self.edges
            .retain(|edge| &edge.from_node != id && &edge.to_node != id);
        self.bump_revision();
        Ok(())
    }

    pub fn set_parameter(
        &mut self,
        node_id: &NodeId,
        parameter_id: &str,
        value: ParameterValue,
    ) -> Result<(), GraphError> {
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
        if let ParameterValue::Float(number) = value {
            if descriptor.min.is_some_and(|minimum| number < minimum)
                || descriptor.max.is_some_and(|maximum| number > maximum)
            {
                return Err(GraphError::ParameterOutOfRange {
                    node: node_id.clone(),
                    parameter: parameter_id.to_owned(),
                });
            }
            node.parameters
                .insert(parameter_id.to_owned(), ParameterValue::Float(number));
        } else {
            node.parameters.insert(parameter_id.to_owned(), value);
        }
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
            to_node,
            to_port: to_port.to_owned(),
        });
        if self.has_cycle() {
            self.edges.pop();
            return Err(GraphError::CycleDetected);
        }
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
        self.edges.remove(index);
        self.bump_revision();
        Ok(())
    }

    pub fn validate(&self) -> Result<(), GraphError> {
        for edge in &self.edges {
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
        let mut memo = BTreeMap::new();
        let mut visiting = BTreeSet::new();
        let result = self.evaluate_node(node_id, context, &mut memo, &mut visiting)?;
        result
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
        memo: &mut BTreeMap<NodeId, NodeResult>,
        visiting: &mut BTreeSet<NodeId>,
    ) -> Result<NodeResult, GraphError> {
        if let Some(result) = memo.get(node_id) {
            return Ok(result.clone());
        }
        if !visiting.insert(node_id.clone()) {
            return Err(GraphError::CycleDetected);
        }
        let node = self
            .nodes
            .get(node_id)
            .ok_or_else(|| GraphError::MissingNode(node_id.clone()))?;
        let instance = self.registry.instantiate(&node.type_id).ok_or_else(|| {
            GraphError::UnknownNodeType {
                type_id: node.type_id.clone(),
            }
        })?;
        let mut inputs = Inputs::new();
        for edge in self.edges.iter().filter(|edge| edge.to_node == *node_id) {
            let result = self.evaluate_node(&edge.from_node, context, memo, visiting)?;
            let value = result
                .outputs
                .get(&edge.from_port)
                .cloned()
                .ok_or_else(|| GraphError::MissingOutput {
                    node: edge.from_node.clone(),
                    port: edge.from_port.clone(),
                })?;
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
        let result = instance
            .evaluate(&inputs, &node.parameters, context)
            .map_err(|source| GraphError::Evaluation {
                node: node_id.clone(),
                source,
            })?;
        visiting.remove(node_id);
        memo.insert(node_id.clone(), result.clone());
        Ok(result)
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
