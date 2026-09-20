use rawweave_image::{ConfidenceMap, DepthMap, Image, Mask, Region, RegionSet};
use rawweave_node_api::{
    EvaluationContext, EvaluationPolicy, ExecutionCapability, Inputs, NodeDescriptor, NodeError,
    NodeInstance, NodePack, NodeRegistry, NodeResult, ParameterDescriptor, Parameters,
    PortDescriptor, Value,
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

fn spatial_descriptor(
    type_id: &'static str,
    name: &'static str,
    inputs: Vec<PortDescriptor>,
    outputs: Vec<PortDescriptor>,
    parameters: Vec<ParameterDescriptor>,
    evaluation_policy: EvaluationPolicy,
) -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(type_id, name);
    descriptor.inputs = inputs;
    descriptor.outputs = outputs;
    descriptor.parameters = parameters;
    descriptor.capabilities = vec![
        ExecutionCapability::Cpu,
        ExecutionCapability::FullFrame,
        ExecutionCapability::RegionAware,
    ];
    descriptor.evaluation_policy = evaluation_policy;
    descriptor
}

fn spatial_output(id: &'static str, name: &'static str, data_type: &'static str) -> PortDescriptor {
    PortDescriptor::output(id, name, data_type)
}

fn spatial_image_input() -> PortDescriptor {
    PortDescriptor::input("image", "Image", "core.Image", true)
}

fn prompt_input() -> PortDescriptor {
    PortDescriptor::input("prompt", "Prompt", "value.String", true)
}

fn manual_spatial_descriptors() -> Vec<NodeDescriptor> {
    let provider_parameters = shared_parameters();
    vec![
        spatial_descriptor(
            "ai.subject-segmentation",
            "AI Subject Segmentation",
            vec![spatial_image_input()],
            vec![
                spatial_output("mask", "Mask", "core.Mask"),
                spatial_output("mask_set", "Masks", "core.MaskSet"),
                spatial_output("confidence", "Confidence", "core.ConfidenceMap"),
            ],
            provider_parameters.clone(),
            EvaluationPolicy::ManualCheckpoint,
        ),
        spatial_descriptor(
            "ai.semantic-segmentation",
            "AI Semantic Segmentation",
            vec![spatial_image_input()],
            vec![
                spatial_output("label_map", "Label Map", "core.LabelMap"),
                spatial_output("confidence", "Confidence", "core.ConfidenceMap"),
            ],
            provider_parameters.clone(),
            EvaluationPolicy::ManualCheckpoint,
        ),
        spatial_descriptor(
            "ai.prompt-segmentation",
            "AI Prompt Segmentation",
            vec![spatial_image_input(), prompt_input()],
            vec![
                spatial_output("mask", "Mask", "core.Mask"),
                spatial_output("confidence", "Confidence", "core.ConfidenceMap"),
            ],
            provider_parameters,
            EvaluationPolicy::ManualCheckpoint,
        ),
        spatial_descriptor(
            "ai.scene-analysis",
            "AI Scene Analysis",
            vec![spatial_image_input()],
            vec![
                spatial_output("label_map", "Label Map", "core.LabelMap"),
                spatial_output("regions", "Regions", "core.RegionSet"),
                spatial_output("confidence", "Confidence", "core.ConfidenceMap"),
            ],
            shared_parameters(),
            EvaluationPolicy::ManualCheckpoint,
        ),
    ]
}

fn automatic_spatial_descriptors() -> Vec<NodeDescriptor> {
    let image = spatial_image_input();
    vec![
        spatial_descriptor(
            "ai.face-detection",
            "AI Face Detection",
            vec![image.clone()],
            vec![
                spatial_output("regions", "Regions", "core.RegionSet"),
                spatial_output("confidence", "Confidence", "core.ConfidenceMap"),
            ],
            Vec::new(),
            EvaluationPolicy::Automatic,
        ),
        spatial_descriptor(
            "ai.skin-mask",
            "AI Skin Mask",
            vec![image.clone()],
            vec![spatial_output("mask", "Mask", "core.Mask")],
            Vec::new(),
            EvaluationPolicy::Automatic,
        ),
        spatial_descriptor(
            "ai.sky-mask",
            "AI Sky Mask",
            vec![image.clone()],
            vec![spatial_output("mask", "Mask", "core.Mask")],
            Vec::new(),
            EvaluationPolicy::Automatic,
        ),
        spatial_descriptor(
            "ai.foreground-mask",
            "AI Foreground Mask",
            vec![image],
            vec![spatial_output("mask", "Mask", "core.Mask")],
            Vec::new(),
            EvaluationPolicy::Automatic,
        ),
        spatial_descriptor(
            "ai.depth-estimation",
            "AI Depth Estimation",
            vec![spatial_image_input()],
            vec![spatial_output("depth", "Depth", "core.DepthMap")],
            Vec::new(),
            EvaluationPolicy::Automatic,
        ),
    ]
}

/// Build provider-backed image generation nodes and Step 12 spatial nodes.
pub fn descriptors() -> Vec<NodeDescriptor> {
    let shared = shared_parameters();
    let mut descriptors = vec![
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
    ];
    descriptors.extend(manual_spatial_descriptors());
    descriptors.extend(automatic_spatial_descriptors());
    descriptors
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

struct FaceDetectionNode;
struct SkinMaskNode;
struct SkyMaskNode;
struct ForegroundMaskNode;
struct DepthEstimationNode;

impl NodeInstance for FaceDetectionNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = source_image(inputs)?;
        let bounds = image.global_region();
        let face = detected_face_region(bounds);
        let regions = context
            .requested_region()
            .map_or_else(Vec::new, |requested| {
                face.and_then(|candidate| candidate.intersection(requested))
                    .into_iter()
                    .collect()
            });
        let regions = if context.requested_region().is_none() {
            face.into_iter().collect()
        } else {
            regions
        };
        let region = requested_region(bounds, context);
        let values = spatial_values(region, |x, y| {
            face.filter(|face| face.contains(x, y)).map_or(0.0, |_| 0.9)
        })?;
        Ok(NodeResult::new(
            [
                (
                    "regions".to_owned(),
                    Value::RegionSet(RegionSet::new(regions)),
                ),
                (
                    "confidence".to_owned(),
                    Value::ConfidenceMap(confidence_from_region(region, values)?),
                ),
            ]
            .into_iter()
            .collect(),
        ))
    }
}

impl NodeInstance for SkinMaskNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = source_image(inputs)?;
        let region = requested_region(image.global_region(), context);
        let values = spatial_values(region, |x, y| {
            image.pixel_global(x, y).map(skin_likelihood).unwrap_or(0.0)
        })?;
        Ok(NodeResult::single(
            "mask",
            Value::Mask(mask_from_region(region, values)?),
        ))
    }
}

impl NodeInstance for SkyMaskNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = source_image(inputs)?;
        let bounds = image.global_region();
        let region = requested_region(bounds, context);
        let values = spatial_values(region, |x, y| {
            image
                .pixel_global(x, y)
                .map(|pixel| sky_likelihood(pixel, bounds, y))
                .unwrap_or(0.0)
        })?;
        Ok(NodeResult::single(
            "mask",
            Value::Mask(mask_from_region(region, values)?),
        ))
    }
}

impl NodeInstance for ForegroundMaskNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = source_image(inputs)?;
        let bounds = image.global_region();
        let region = requested_region(bounds, context);
        let values = spatial_values(region, |x, y| {
            image
                .pixel_global(x, y)
                .map(|pixel| 1.0 - sky_likelihood(pixel, bounds, y))
                .unwrap_or(0.0)
        })?;
        Ok(NodeResult::single(
            "mask",
            Value::Mask(mask_from_region(region, values)?),
        ))
    }
}

impl NodeInstance for DepthEstimationNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = source_image(inputs)?;
        let region = requested_region(image.global_region(), context);
        let values = spatial_values(region, |x, y| {
            image
                .pixel_global(x, y)
                .map(|pixel| 1.0 - luminance(pixel))
                .unwrap_or(0.0)
        })?;
        let depth = DepthMap::from_values(region.dimensions(), (region.x, region.y), values)
            .map_err(|error| NodeError::Message(error.to_string()))?;
        Ok(NodeResult::single("depth", Value::DepthMap(depth)))
    }
}

fn source_image(inputs: &Inputs) -> Result<Image, NodeError> {
    match inputs.get("image") {
        Some(Value::Image(image)) => Ok(image.clone()),
        Some(_) => Err(NodeError::InvalidParameter("image".to_owned())),
        None => Err(NodeError::MissingInput("image".to_owned())),
    }
}

fn requested_region(full: Region, context: &EvaluationContext) -> Region {
    context
        .requested_region()
        .and_then(|requested| requested.intersection(full))
        .unwrap_or(full)
}

fn spatial_values(
    region: Region,
    mut value_at: impl FnMut(u32, u32) -> f32,
) -> Result<Vec<f32>, NodeError> {
    let pixel_count = u64::from(region.width)
        .checked_mul(u64::from(region.height))
        .ok_or_else(|| NodeError::InvalidParameter("dimensions".to_owned()))?;
    if pixel_count > 16_777_216 {
        return Err(NodeError::InvalidParameter("dimensions".to_owned()));
    }
    let mut values = Vec::with_capacity(pixel_count as usize);
    for y in 0..region.height {
        for x in 0..region.width {
            values.push(value_at(region.x + x, region.y + y).clamp(0.0, 1.0));
        }
    }
    Ok(values)
}

fn mask_from_region(region: Region, values: Vec<f32>) -> Result<Mask, NodeError> {
    Mask::from_values_with_origin(region.dimensions(), (region.x, region.y), values)
        .map_err(|error| NodeError::Message(error.to_string()))
}

fn confidence_from_region(region: Region, values: Vec<f32>) -> Result<ConfidenceMap, NodeError> {
    Ok(ConfidenceMap::from_mask(mask_from_region(region, values)?))
}

fn luminance([red, green, blue, _alpha]: [f32; 4]) -> f32 {
    (0.2126 * red + 0.7152 * green + 0.0722 * blue).clamp(0.0, 1.0)
}

fn skin_likelihood([red, green, blue, _alpha]: [f32; 4]) -> f32 {
    let red = red.clamp(0.0, 1.0);
    let green = green.clamp(0.0, 1.0);
    let blue = blue.clamp(0.0, 1.0);
    let warmth = (red - blue).max(0.0);
    let redness = (red - green).max(0.0);
    let brightness = luminance([red, green, blue, 1.0]);
    (warmth * 1.4 + redness * 0.8).clamp(0.0, 1.0) * (0.35 + 0.65 * brightness)
}

fn sky_likelihood([red, green, blue, _alpha]: [f32; 4], bounds: Region, y: u32) -> f32 {
    let blue_dominance = (blue - red.max(green)).max(0.0).clamp(0.0, 1.0);
    let height = bounds.height.saturating_sub(1).max(1) as f32;
    let vertical_prior = (1.0 - (y.saturating_sub(bounds.y) as f32 / height)).clamp(0.0, 1.0);
    (blue_dominance * 1.5 + vertical_prior * 0.1).clamp(0.0, 1.0)
}

fn detected_face_region(bounds: Region) -> Option<Region> {
    if bounds.width < 2 || bounds.height < 2 {
        return None;
    }
    let width = (bounds.width / 3).max(1);
    let height = (bounds.height / 3).max(1);
    Some(Region::new(
        bounds.x + (bounds.width - width) / 2,
        bounds.y + (bounds.height - height) / 3,
        width,
        height,
    ))
}

#[derive(Debug, Default)]
pub struct AiNodePack;

impl AiNodePack {
    pub const ID: &'static str = "ai";
}

fn provider_checkpoint_factory() -> Box<dyn NodeInstance> {
    Box::new(ProviderCheckpointNode)
}

fn face_detection_factory() -> Box<dyn NodeInstance> {
    Box::new(FaceDetectionNode)
}

fn skin_mask_factory() -> Box<dyn NodeInstance> {
    Box::new(SkinMaskNode)
}

fn sky_mask_factory() -> Box<dyn NodeInstance> {
    Box::new(SkyMaskNode)
}

fn foreground_mask_factory() -> Box<dyn NodeInstance> {
    Box::new(ForegroundMaskNode)
}

fn depth_estimation_factory() -> Box<dyn NodeInstance> {
    Box::new(DepthEstimationNode)
}

fn factory_for_type(type_id: &str) -> fn() -> Box<dyn NodeInstance> {
    match type_id {
        "ai.face-detection" => face_detection_factory,
        "ai.skin-mask" => skin_mask_factory,
        "ai.sky-mask" => sky_mask_factory,
        "ai.foreground-mask" => foreground_mask_factory,
        "ai.depth-estimation" => depth_estimation_factory,
        _ => provider_checkpoint_factory,
    }
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
            registry.register(descriptor.clone(), factory_for_type(&descriptor.type_id))?;
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
    fn preserves_step11_provider_nodes_as_manual_checkpoint_nodes() {
        let mut registry = NodeRegistry::default();
        register_nodes(&mut registry).unwrap();
        let step11_ids = [
            "ai.generative-fill",
            "ai.img2img",
            "ai.inpaint",
            "ai.upscale",
        ];
        let registered = registry
            .descriptors()
            .into_iter()
            .filter(|descriptor| step11_ids.contains(&descriptor.type_id.as_str()))
            .map(|descriptor| descriptor.type_id)
            .collect::<Vec<_>>();
        assert_eq!(
            registered,
            step11_ids
                .iter()
                .map(|type_id| (*type_id).to_owned())
                .collect::<Vec<_>>()
        );
        for descriptor in descriptors()
            .into_iter()
            .filter(|descriptor| step11_ids.contains(&descriptor.type_id.as_str()))
        {
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
