use rawweave_node_api::{
    EvaluationContext, ExecutionCapability, Inputs, NodeDescriptor, NodeError, NodeInstance,
    NodePack, NodeRegistry, NodeResult, ParameterDescriptor, Parameters, PortDescriptor,
};

const AI_CAPABILITIES: &[ExecutionCapability] =
    &[ExecutionCapability::Cpu, ExecutionCapability::FullFrame];

fn shared_parameters() -> Vec<ParameterDescriptor> {
    vec![
        ParameterDescriptor::string("provider_id", "Provider", "comfyui"),
        ParameterDescriptor::string("workflow_id", "Workflow", ""),
        ParameterDescriptor::string("workflow_definition", "Workflow definition", "{}"),
        ParameterDescriptor::string("prompt", "Prompt", ""),
        ParameterDescriptor::string("negative_prompt", "Negative prompt", ""),
        ParameterDescriptor::float("strength", "Strength", 0.75, Some(0.0), Some(1.0)),
        ParameterDescriptor::integer("steps", "Steps", 20),
        ParameterDescriptor::integer("seed", "Seed", 0),
    ]
}

fn descriptor(
    type_id: &'static str,
    name: &'static str,
    inputs: Vec<PortDescriptor>,
    mut parameters: Vec<ParameterDescriptor>,
) -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(type_id, name);
    descriptor.inputs = inputs;
    descriptor
        .outputs
        .push(PortDescriptor::output("image", "Image", "core.Image"));
    descriptor.parameters.append(&mut parameters);
    descriptor.capabilities = AI_CAPABILITIES.to_vec();
    descriptor.evaluation_policy = rawweave_node_api::EvaluationPolicy::ManualCheckpoint;
    descriptor
}

fn image_input() -> PortDescriptor {
    PortDescriptor::input("image", "Image", "core.Image", true)
}

fn mask_input() -> PortDescriptor {
    PortDescriptor::input("mask", "Mask", "core.Mask", true)
}

/// Build the four provider-backed image generation checkpoint descriptors.
pub fn descriptors() -> Vec<NodeDescriptor> {
    let shared = shared_parameters();
    vec![
        descriptor(
            "ai.img2img",
            "AI Img2Img",
            vec![image_input()],
            shared.clone(),
        ),
        descriptor(
            "ai.inpaint",
            "AI Inpaint",
            vec![image_input(), mask_input()],
            shared.clone(),
        ),
        descriptor(
            "ai.generative-fill",
            "AI Generative Fill",
            vec![image_input(), mask_input()],
            shared.clone(),
        ),
        descriptor("ai.upscale", "AI Upscale", vec![image_input()], {
            let mut parameters = shared;
            parameters.push(ParameterDescriptor::float(
                "scale",
                "Scale",
                2.0,
                Some(1.0),
                Some(8.0),
            ));
            parameters
        }),
    ]
}

#[derive(Debug, Default)]
struct ProviderCheckpointNode;

impl NodeInstance for ProviderCheckpointNode {
    fn evaluate(
        &self,
        _inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        Err(NodeError::Message(
            "AI checkpoint nodes require an explicit provider generation".to_owned(),
        ))
    }
}

#[derive(Debug, Default)]
pub struct AiNodePack;

impl AiNodePack {
    pub const ID: &'static str = "ai";
}

impl NodePack for AiNodePack {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn register(
        &self,
        registry: &mut NodeRegistry,
    ) -> Result<(), rawweave_node_api::RegistryError> {
        for descriptor in descriptors() {
            registry.register(descriptor, || Box::new(ProviderCheckpointNode))?;
        }
        Ok(())
    }
}

pub fn register_nodes(registry: &mut NodeRegistry) -> Result<(), rawweave_node_api::RegistryError> {
    AiNodePack.register(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawweave_node_api::EvaluationPolicy;

    #[test]
    fn registers_all_step11_operations_as_manual_checkpoint_nodes() {
        let mut registry = NodeRegistry::default();
        register_nodes(&mut registry).unwrap();
        let registered = registry
            .descriptors()
            .into_iter()
            .map(|descriptor| descriptor.type_id)
            .collect::<Vec<_>>();
        assert_eq!(
            registered,
            vec![
                "ai.generative-fill",
                "ai.img2img",
                "ai.inpaint",
                "ai.upscale",
            ]
        );
        for descriptor in descriptors() {
            assert_eq!(
                descriptor.evaluation_policy,
                EvaluationPolicy::ManualCheckpoint
            );
            assert_eq!(descriptor.outputs[0].data_type, "core.Image");
        }
    }

    #[test]
    fn mask_operations_require_a_mask_and_upscale_has_a_bounded_scale() {
        let nodes = descriptors();
        for type_id in ["ai.inpaint", "ai.generative-fill"] {
            let node = nodes.iter().find(|node| node.type_id == type_id).unwrap();
            assert_eq!(node.inputs.len(), 2);
            assert_eq!(node.inputs[1].data_type, "core.Mask");
        }
        let upscale = nodes
            .iter()
            .find(|node| node.type_id == "ai.upscale")
            .unwrap();
        let scale = upscale.parameter("scale").unwrap();
        assert_eq!(scale.min, Some(1.0));
        assert_eq!(scale.max, Some(8.0));
    }
}
