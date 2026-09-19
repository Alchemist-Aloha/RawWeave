use rawweave_core::NodeId;
use rawweave_graph::{
    Graph, GraphError, NodeManifest, NodePackManifest, WorkflowDefinition, WorkflowError,
    WorkflowMetadata,
};
use rawweave_node_api::{
    EvaluationContext, NodeDescriptor, NodePack, NodeRegistry, ParameterValue, Value,
};
use rawweave_raw::RawDecoder;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error(transparent)]
    Graph(#[from] GraphError),
    #[error(transparent)]
    Workflow(#[from] WorkflowError),
    #[error("blueprint parameter id '{0}' must contain exactly one ':'")]
    InvalidBlueprintParameterId(String),
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

fn manifest_for_registry(package_id: &str, registry: &NodeRegistry) -> NodePackManifest {
    registry.descriptors().into_iter().fold(
        NodePackManifest::new(package_id, env!("CARGO_PKG_VERSION")),
        |manifest, descriptor| {
            manifest.with_node(NodeManifest::new(descriptor.type_id, descriptor.version))
        },
    )
}

/// Manifests generated from the descriptors registered by each built-in pack.
/// Keeping this beside registry construction prevents dependency diagnostics
/// from drifting when a built-in node is added or removed.
pub fn built_in_node_pack_manifests() -> Vec<NodePackManifest> {
    let mut image_registry = NodeRegistry::default();
    rawweave_core_image::register_nodes(&mut image_registry)
        .expect("the built-in image node pack must register once");
    let mut values_registry = NodeRegistry::default();
    rawweave_core_values::register_nodes(&mut values_registry)
        .expect("the built-in values node pack must register once");
    let mut raw_registry = NodeRegistry::default();
    rawweave_raw_nodes::register_nodes(&mut raw_registry)
        .expect("the built-in RAW node pack must register once");
    vec![
        manifest_for_registry("core-image", &image_registry),
        manifest_for_registry("core-values", &values_registry),
        manifest_for_registry("raw", &raw_registry),
    ]
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

    pub fn new_with_raw_decoder<D>(decoder: D) -> Self
    where
        D: RawDecoder + 'static,
    {
        let mut registry = NodeRegistry::default();
        rawweave_core_image::register_nodes(&mut registry)
            .expect("the built-in image node pack must register once");
        rawweave_core_values::register_nodes(&mut registry)
            .expect("the built-in values node pack must register once");
        rawweave_raw_nodes::RawNodePack::with_decoder(decoder)
            .register(&mut registry)
            .expect("the built-in RAW node pack must register once");
        Self {
            graph: Graph::new(registry),
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

    /// Expose a node parameter as a typed input port so metadata, analysis, or
    /// logic values can drive it. The stored literal is preserved.
    pub fn expose_parameter(
        &mut self,
        node_id: &str,
        parameter_id: &str,
    ) -> Result<(), ProjectError> {
        self.graph
            .expose_parameter(&NodeId::from(node_id), parameter_id)?;
        Ok(())
    }

    pub fn unexpose_parameter(
        &mut self,
        node_id: &str,
        parameter_id: &str,
    ) -> Result<(), ProjectError> {
        self.graph
            .unexpose_parameter(&NodeId::from(node_id), parameter_id)?;
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

    pub fn evaluate_raw_workflow_with_bytes(
        &self,
        node_id: &str,
        output_port: &str,
        source_bytes: Vec<u8>,
    ) -> Result<Value, ProjectError> {
        self.evaluate(
            node_id,
            output_port,
            EvaluationContext::default().with_source_bytes(source_bytes),
        )
    }

    pub fn save_workflow(&self) -> Result<String, ProjectError> {
        Ok(self.graph.to_json()?)
    }

    pub fn load_workflow(&mut self, json: &str) -> Result<(), ProjectError> {
        let registry = self.graph.registry();
        self.graph = Graph::from_json(json, registry)?;
        Ok(())
    }

    pub fn create_subgraph_from_selection(
        &self,
        selection: &[&str],
        id: impl Into<String>,
        version: impl Into<String>,
        metadata: WorkflowMetadata,
    ) -> Result<WorkflowDefinition, ProjectError> {
        let selection = selection
            .iter()
            .map(|node_id| NodeId::from(*node_id))
            .collect::<Vec<_>>();
        Ok(WorkflowDefinition::from_selection_slice(
            &self.graph,
            &selection,
            id,
            version,
            metadata,
        )?)
    }

    pub fn save_blueprint(&self, blueprint: &WorkflowDefinition) -> Result<Vec<u8>, ProjectError> {
        Ok(blueprint.export()?)
    }

    pub fn load_blueprint(&self, bytes: &[u8]) -> Result<WorkflowDefinition, ProjectError> {
        Ok(WorkflowDefinition::import(bytes, self.graph.registry())?)
    }

    pub fn instantiate_blueprint(
        &mut self,
        blueprint: &WorkflowDefinition,
    ) -> Result<(), ProjectError> {
        blueprint.validate()?;
        self.graph = blueprint.instantiate();
        Ok(())
    }

    pub fn expose_blueprint_parameter(
        &self,
        blueprint: &mut WorkflowDefinition,
        id: &str,
    ) -> Result<(), ProjectError> {
        let (node_id, parameter_id) = split_blueprint_parameter_id(id)?;
        blueprint.expose_parameter(&NodeId::from(node_id), parameter_id)?;
        Ok(())
    }

    pub fn set_blueprint_parameter(
        &self,
        blueprint: &mut WorkflowDefinition,
        id: &str,
        value: ParameterValue,
    ) -> Result<(), ProjectError> {
        blueprint.set_parameter(id, value)?;
        Ok(())
    }

    pub fn hide_blueprint_parameter(
        &self,
        blueprint: &mut WorkflowDefinition,
        id: &str,
    ) -> Result<(), ProjectError> {
        blueprint.hide_parameter(id)?;
        Ok(())
    }
}

fn split_blueprint_parameter_id(id: &str) -> Result<(&str, &str), ProjectError> {
    let mut parts = id.split(':');
    let Some(node_id) = parts.next() else {
        return Err(ProjectError::InvalidBlueprintParameterId(id.to_owned()));
    };
    let Some(parameter_id) = parts.next() else {
        return Err(ProjectError::InvalidBlueprintParameterId(id.to_owned()));
    };
    if node_id.is_empty() || parameter_id.is_empty() || parts.next().is_some() {
        return Err(ProjectError::InvalidBlueprintParameterId(id.to_owned()));
    }
    Ok((node_id, parameter_id))
}
