use rawweave_core::NodeId;
use rawweave_graph::{
    ArtifactStore, Checkpoint, CheckpointArtifact, CheckpointError, Graph, GraphError,
    NodeManifest, NodePackManifest, WorkflowDefinition, WorkflowError, WorkflowMetadata,
};
use rawweave_node_api::{
    EvaluationContext, NodeDescriptor, NodePack, NodeRegistry, ParameterValue, RegistryError, Value,
};
use rawweave_raw::RawDecoder;
use thiserror::Error;

mod external;

pub use external::{ExternalError, ExternalHost, ExternalHostDiagnostics, ExternalNodePack};
pub use rawweave_graph::types_compatible;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error(transparent)]
    Graph(#[from] GraphError),
    #[error(transparent)]
    Workflow(#[from] WorkflowError),
    #[error(transparent)]
    External(#[from] ExternalError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Checkpoint(#[from] CheckpointError),
    #[error("blueprint parameter id '{0}' must contain exactly one ':'")]
    InvalidBlueprintParameterId(String),
}

pub fn default_registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    rawweave_core_image::register_nodes(&mut registry)
        .expect("the built-in image node pack must register once");
    rawweave_core_values::register_nodes(&mut registry)
        .expect("the built-in values node pack must register once");
    rawweave_ai_nodes::register_nodes(&mut registry)
        .expect("the built-in AI node pack must register once");
    rawweave_raw_nodes::register_nodes(&mut registry)
        .expect("the built-in RAW node pack must register once");
    rawweave_pro_tools::register_nodes(&mut registry)
        .expect("the built-in professional node pack must register once");
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
    let mut ai_registry = NodeRegistry::default();
    rawweave_ai_nodes::register_nodes(&mut ai_registry)
        .expect("the built-in AI node pack must register once");
    let mut raw_registry = NodeRegistry::default();
    rawweave_raw_nodes::register_nodes(&mut raw_registry)
        .expect("the built-in RAW node pack must register once");
    let mut pro_tools_registry = NodeRegistry::default();
    rawweave_pro_tools::register_nodes(&mut pro_tools_registry)
        .expect("the built-in professional node pack must register once");
    vec![
        manifest_for_registry("core-image", &image_registry),
        manifest_for_registry("core-values", &values_registry),
        manifest_for_registry("ai", &ai_registry),
        manifest_for_registry("raw", &raw_registry),
        manifest_for_registry("pro-tools", &pro_tools_registry),
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
        rawweave_ai_nodes::register_nodes(&mut registry)
            .expect("the built-in AI node pack must register once");
        rawweave_raw_nodes::RawNodePack::with_decoder(decoder)
            .register(&mut registry)
            .expect("the built-in RAW node pack must register once");
        rawweave_pro_tools::register_nodes(&mut registry)
            .expect("the built-in professional node pack must register once");
        Self {
            graph: Graph::new(registry),
        }
    }

    /// Construct the ordinary-image default graph, shared by desktop frontends.
    pub fn reset_ordinary_image_graph(&mut self) -> Result<(), ProjectError> {
        self.reset_image_graph(
            &[("input", "core.image-input"), ("output", "core.output")],
            &[("input", "image", "output", "image")],
        )
    }

    /// Construct the RAW default graph without changing persisted node IDs.
    pub fn reset_raw_image_graph(&mut self) -> Result<(), ProjectError> {
        self.reset_image_graph(
            &[
                ("raw-decode", "raw.decode"),
                ("black-level", "raw.black-level"),
                ("white-balance", "raw.white-balance"),
                ("highlight-reconstruction", "raw.highlight-reconstruction"),
                ("demosaic", "raw.demosaic"),
                ("camera-transform", "raw.camera-transform"),
                ("lens-correction", "raw.lens-correction"),
                ("display-transform", "raw.display-transform"),
            ],
            &[
                ("raw-decode", "frame", "black-level", "frame"),
                ("black-level", "mosaic", "white-balance", "mosaic"),
                (
                    "white-balance",
                    "mosaic",
                    "highlight-reconstruction",
                    "mosaic",
                ),
                ("highlight-reconstruction", "mosaic", "demosaic", "mosaic"),
                ("demosaic", "scene", "camera-transform", "scene"),
                (
                    "raw-decode",
                    "camera_profile",
                    "camera-transform",
                    "camera_profile",
                ),
                ("camera-transform", "scene", "lens-correction", "scene"),
                (
                    "raw-decode",
                    "lens_profile",
                    "lens-correction",
                    "lens_profile",
                ),
                ("lens-correction", "scene", "display-transform", "scene"),
            ],
        )
    }

    fn reset_image_graph(
        &mut self,
        nodes: &[(&str, &str)],
        edges: &[(&str, &str, &str, &str)],
    ) -> Result<(), ProjectError> {
        let existing: Vec<_> = self.graph.nodes().keys().cloned().collect();
        for id in existing {
            self.remove_node(id.as_str())?;
        }
        for (id, kind) in nodes {
            self.add_node(id, kind)?;
        }
        for (from, output, to, input) in edges {
            self.connect(from, output, to, input)?;
        }
        Ok(())
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn node_descriptors(&self) -> Vec<NodeDescriptor> {
        self.graph.registry().descriptors()
    }

    pub fn register_external_node_pack(
        &mut self,
        pack: ExternalNodePack,
    ) -> Result<(), ProjectError> {
        let mut registry = self.graph.registry();
        pack.register_into(&mut registry)?;
        let graph = std::mem::replace(&mut self.graph, Graph::new(registry.clone()));
        self.graph = graph.with_registry(registry);
        Ok(())
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

    /// Resolve a manual checkpoint from its committed artifact. This explicit
    /// path never invokes the manual node instance itself.
    pub fn evaluate_checkpoint(
        &self,
        node_id: &str,
        output_port: &str,
        context: EvaluationContext,
        checkpoint: &mut Checkpoint,
        store: &ArtifactStore,
    ) -> Result<Value, ProjectError> {
        Ok(self.graph.evaluate_checkpoint(
            &NodeId::from(node_id),
            output_port,
            &context,
            checkpoint,
            store,
        )?)
    }

    pub fn commit_checkpoint(
        &self,
        checkpoint: &mut Checkpoint,
        artifact: CheckpointArtifact,
        store: &ArtifactStore,
    ) -> Result<(), ProjectError> {
        checkpoint.commit(artifact, store)?;
        Ok(())
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

    /// Load a workflow while retaining the durable artifact store used by its
    /// manual checkpoints.
    pub fn load_workflow_with_artifact_store(
        &mut self,
        json: &str,
        artifact_store: ArtifactStore,
    ) -> Result<(), ProjectError> {
        let registry = self.graph.registry();
        self.graph = Graph::from_json_with_artifact_store(json, registry, artifact_store)?;
        Ok(())
    }

    /// Register live manual-checkpoint state with the graph scheduler.
    pub fn register_checkpoint(&mut self, checkpoint: Checkpoint) -> Result<(), ProjectError> {
        self.graph.register_checkpoint(checkpoint)?;
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
