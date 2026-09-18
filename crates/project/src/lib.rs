use rawweave_core::NodeId;
use rawweave_graph::{Graph, GraphError};
use rawweave_node_api::{EvaluationContext, NodeDescriptor, NodeRegistry, ParameterValue, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error(transparent)]
    Graph(#[from] GraphError),
}

pub fn default_registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    rawweave_core_image::register_nodes(&mut registry)
        .expect("the built-in image node pack must register once");
    rawweave_core_values::register_nodes(&mut registry)
        .expect("the built-in values node pack must register once");
    rawweave_raw_nodes::register_nodes(&mut registry)
        .expect("the built-in RAW node pack must register once");
    registry
}

/// Application-facing backend API. Tauri commands and other frontends call this
/// interface instead of reaching into the graph implementation directly.
#[derive(Clone, Debug)]
pub struct EditorCore {
    graph: Graph,
}

impl Default for EditorCore {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorCore {
    pub fn new() -> Self {
        Self {
            graph: Graph::new(default_registry()),
        }
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn node_descriptors(&self) -> Vec<NodeDescriptor> {
        self.graph.registry().descriptors()
    }

    pub fn add_node(&mut self, node_id: &str, type_id: &str) -> Result<(), ProjectError> {
        self.graph.add_node(NodeId::from(node_id), type_id)?;
        Ok(())
    }

    pub fn remove_node(&mut self, node_id: &str) -> Result<(), ProjectError> {
        self.graph.remove_node(&NodeId::from(node_id))?;
        Ok(())
    }

    pub fn connect(
        &mut self,
        from_node: &str,
        from_port: &str,
        to_node: &str,
        to_port: &str,
    ) -> Result<(), ProjectError> {
        self.graph.connect(
            NodeId::from(from_node),
            from_port,
            NodeId::from(to_node),
            to_port,
        )?;
        Ok(())
    }

    pub fn disconnect(
        &mut self,
        from_node: &str,
        from_port: &str,
        to_node: &str,
        to_port: &str,
    ) -> Result<(), ProjectError> {
        self.graph.disconnect(
            NodeId::from(from_node),
            from_port,
            NodeId::from(to_node),
            to_port,
        )?;
        Ok(())
    }

    pub fn set_node_parameter(
        &mut self,
        node_id: &str,
        parameter_id: &str,
        value: ParameterValue,
    ) -> Result<(), ProjectError> {
        self.graph
            .set_parameter(&NodeId::from(node_id), parameter_id, value)?;
        Ok(())
    }

    pub fn evaluate(
        &self,
        node_id: &str,
        output_port: &str,
        context: EvaluationContext,
    ) -> Result<Value, ProjectError> {
        Ok(self
            .graph
            .evaluate(&NodeId::from(node_id), output_port, &context)?)
    }

    pub fn save_workflow(&self) -> Result<String, ProjectError> {
        Ok(self.graph.to_json()?)
    }

    pub fn load_workflow(&mut self, json: &str) -> Result<(), ProjectError> {
        self.graph = Graph::from_json(json, default_registry())?;
        Ok(())
    }
}
