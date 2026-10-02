use rawweave_core::NodeId;
use rawweave_graph::Graph;
use rawweave_image::{Dimensions, Image, Region};
use rawweave_node_api::{
    EvaluationContext, ExecutionCapability, Inputs, NodeDescriptor, NodeError, NodeInstance,
    NodeRegistry, NodeResult, Parameters, PortDescriptor, Value,
};
use rawweave_rendering::{GpuContext, PreviewQuality, RenderContext, TileCoord, TileRequest};

const SOURCE_NODE: &str = "test.source";
const FULL_FRAME_NODE: &str = "test.full-frame";
const REGION_NODE: &str = "test.region";
const SHARED_NODE: &str = "test.shared";
const FULL_BRANCH_NODE: &str = "test.full-branch";
const REGION_BRANCH_NODE: &str = "test.region-branch";
const MERGE_NODE: &str = "test.merge";
const BACKEND_NODE: &str = "test.backend";

fn image_descriptor(type_id: &str, name: &str) -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(type_id, name);
    descriptor
        .outputs
        .push(PortDescriptor::output("image", "Image", "core.Image"));
    descriptor.capabilities = vec![ExecutionCapability::Cpu, ExecutionCapability::RegionAware];
    descriptor
}

fn full_frame_descriptor() -> NodeDescriptor {
    let mut descriptor = image_descriptor(FULL_FRAME_NODE, "Full Frame");
    descriptor
        .inputs
        .push(PortDescriptor::input("image", "Image", "core.Image", true));
    descriptor.capabilities = vec![ExecutionCapability::Cpu, ExecutionCapability::FullFrame];
    descriptor
}

fn region_descriptor() -> NodeDescriptor {
    let mut descriptor = image_descriptor(REGION_NODE, "Region Aware");
    descriptor
        .inputs
        .push(PortDescriptor::input("image", "Image", "core.Image", true));
    descriptor
}

fn shared_descriptor() -> NodeDescriptor {
    let mut descriptor = image_descriptor(SHARED_NODE, "Shared");
    descriptor
        .inputs
        .push(PortDescriptor::input("image", "Image", "core.Image", true));
    descriptor.capabilities = vec![
        ExecutionCapability::Cpu,
        ExecutionCapability::FullFrame,
        ExecutionCapability::RegionAware,
    ];
    descriptor
}

fn branch_descriptor(type_id: &str, name: &str, capability: ExecutionCapability) -> NodeDescriptor {
    let mut descriptor = image_descriptor(type_id, name);
    descriptor
        .inputs
        .push(PortDescriptor::input("image", "Image", "core.Image", true));
    descriptor.capabilities = vec![ExecutionCapability::Cpu, capability];
    descriptor
}

fn merge_descriptor() -> NodeDescriptor {
    let mut descriptor = image_descriptor(MERGE_NODE, "Merge");
    descriptor
        .inputs
        .push(PortDescriptor::input("full", "Full", "core.Image", true));
    descriptor.inputs.push(PortDescriptor::input(
        "region",
        "Region",
        "core.Image",
        true,
    ));
    descriptor
}

fn backend_descriptor() -> NodeDescriptor {
    let mut descriptor = image_descriptor(BACKEND_NODE, "Backend Marker");
    descriptor.capabilities = vec![ExecutionCapability::Cpu, ExecutionCapability::Gpu];
    descriptor
}

struct Source;

impl NodeInstance for Source {
    fn evaluate(
        &self,
        _inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let source = context
            .source_image
            .as_ref()
            .ok_or(NodeError::MissingSourceImage)?;
        let output = match context.requested_region() {
            Some(region) => {
                let pixels = (0..region.height)
                    .flat_map(|y| {
                        (0..region.width).map(move |x| {
                            source
                                .pixel_global(region.x + x, region.y + y)
                                .unwrap_or([0.0; 4])
                        })
                    })
                    .collect();
                Image::from_pixels_with_origin(
                    region.dimensions(),
                    (region.x, region.y),
                    pixels,
                    source.pixel_format(),
                    source.color_metadata(),
                )
            }
            None => Ok(source.clone()),
        }
        .map_err(|error: rawweave_image::ImageError| NodeError::Message(error.to_string()))?;
        Ok(NodeResult::single("image", Value::Image(output)))
    }
}

struct FullFrame;

impl NodeInstance for FullFrame {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let Value::Image(image) = inputs
            .get("image")
            .ok_or_else(|| NodeError::MissingInput("image".to_owned()))?
        else {
            return Err(NodeError::InvalidParameter("image".to_owned()));
        };
        let mut pixels = image.pixels().to_vec();
        pixels[0] = if context.requested_region().is_none()
            && context.tile() == TileCoord::default()
            && context.mip_level() == 2
            && context.quality() == PreviewQuality::Draft
        {
            [1.0, 0.0, 0.0, 1.0]
        } else {
            [0.0, 1.0, 0.0, 1.0]
        };
        let output = Image::from_pixels_with_origin(
            image.dimensions(),
            image.origin(),
            pixels,
            image.pixel_format(),
            image.color_metadata(),
        )
        .map_err(|error| NodeError::Message(error.to_string()))?;
        Ok(NodeResult::single("image", Value::Image(output)))
    }
}

struct RegionAware;

impl NodeInstance for RegionAware {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let Value::Image(image) = inputs
            .get("image")
            .ok_or_else(|| NodeError::MissingInput("image".to_owned()))?
        else {
            return Err(NodeError::InvalidParameter("image".to_owned()));
        };
        if context.requested_region() != Some(Region::new(1, 1, 1, 1))
            || context.tile() != TileCoord::new(4, 5)
        {
            return Err(NodeError::Message(
                "region-aware context was not preserved".to_owned(),
            ));
        }
        Ok(NodeResult::single("image", Value::Image(image.clone())))
    }
}

struct Shared;

impl NodeInstance for Shared {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let Value::Image(image) = inputs
            .get("image")
            .ok_or_else(|| NodeError::MissingInput("image".to_owned()))?
        else {
            return Err(NodeError::InvalidParameter("image".to_owned()));
        };
        let mut pixels = image.pixels().to_vec();
        if let Some(first) = pixels.first_mut() {
            *first = if context.requested_region().is_some() {
                [0.0, 1.0, 0.0, 1.0]
            } else {
                [1.0, 0.0, 0.0, 1.0]
            };
        }
        let output = Image::from_pixels_with_origin(
            image.dimensions(),
            image.origin(),
            pixels,
            image.pixel_format(),
            image.color_metadata(),
        )
        .map_err(|error| NodeError::Message(error.to_string()))?;
        Ok(NodeResult::single("image", Value::Image(output)))
    }
}

struct Passthrough;

impl NodeInstance for Passthrough {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        Ok(NodeResult::single("image", image_value(inputs, "image")?))
    }
}

struct Merge;

impl NodeInstance for Merge {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let full = image_value(inputs, "full")?;
        let region = image_value(inputs, "region")?;
        let (Value::Image(full), Value::Image(region)) = (&full, &region) else {
            return Err(NodeError::InvalidParameter("image".to_owned()));
        };
        if full.pixel(0, 0) != Some([1.0, 0.0, 0.0, 1.0])
            || region.pixel(0, 0) != Some([0.0, 1.0, 0.0, 1.0])
        {
            return Err(NodeError::Message(
                "shared ancestor was evaluated with the wrong context".to_owned(),
            ));
        }
        Ok(NodeResult::single("image", Value::Image(full.clone())))
    }
}

struct BackendMarker;

impl NodeInstance for BackendMarker {
    fn evaluate(
        &self,
        _inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let pixel = if context
            .render_context()
            .is_some_and(|render_context| render_context.gpu_available())
        {
            [0.0, 1.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0, 1.0]
        };
        Ok(NodeResult::single(
            "image",
            Value::Image(Image::from_pixels(1, 1, vec![pixel]).unwrap()),
        ))
    }
}

fn image_value(inputs: &Inputs, port: &str) -> Result<Value, NodeError> {
    match inputs.get(port) {
        Some(Value::Image(image)) => Ok(Value::Image(image.clone())),
        Some(_) => Err(NodeError::InvalidParameter(port.to_owned())),
        None => Err(NodeError::MissingInput(port.to_owned())),
    }
}

fn source_factory() -> Box<dyn NodeInstance> {
    Box::new(Source)
}

fn full_frame_factory() -> Box<dyn NodeInstance> {
    Box::new(FullFrame)
}

fn region_factory() -> Box<dyn NodeInstance> {
    Box::new(RegionAware)
}

fn shared_factory() -> Box<dyn NodeInstance> {
    Box::new(Shared)
}

fn full_branch_factory() -> Box<dyn NodeInstance> {
    Box::new(Passthrough)
}

fn region_branch_factory() -> Box<dyn NodeInstance> {
    Box::new(Passthrough)
}

fn merge_factory() -> Box<dyn NodeInstance> {
    Box::new(Merge)
}

fn backend_factory() -> Box<dyn NodeInstance> {
    Box::new(BackendMarker)
}

fn registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    registry
        .register(image_descriptor(SOURCE_NODE, "Source"), source_factory)
        .unwrap();
    registry
        .register(full_frame_descriptor(), full_frame_factory)
        .unwrap();
    registry
        .register(region_descriptor(), region_factory)
        .unwrap();
    registry
        .register(shared_descriptor(), shared_factory)
        .unwrap();
    registry
        .register(
            branch_descriptor(
                FULL_BRANCH_NODE,
                "Full Branch",
                ExecutionCapability::FullFrame,
            ),
            full_branch_factory,
        )
        .unwrap();
    registry
        .register(
            branch_descriptor(
                REGION_BRANCH_NODE,
                "Region Branch",
                ExecutionCapability::RegionAware,
            ),
            region_branch_factory,
        )
        .unwrap();
    registry
        .register(merge_descriptor(), merge_factory)
        .unwrap();
    registry
        .register(backend_descriptor(), backend_factory)
        .unwrap();
    registry
}

fn source_image() -> Image {
    Image::from_pixels(
        2,
        2,
        vec![
            [0.1, 0.0, 0.0, 1.0],
            [0.2, 0.0, 0.0, 1.0],
            [0.3, 0.0, 0.0, 1.0],
            [0.4, 0.0, 0.0, 1.0],
        ],
    )
    .unwrap()
}

#[test]
fn shared_ancestors_are_memoized_separately_for_full_frame_and_region_paths() {
    let mut graph = Graph::new(registry());
    for (id, type_id) in [
        ("source", SOURCE_NODE),
        ("shared", SHARED_NODE),
        ("full", FULL_BRANCH_NODE),
        ("region", REGION_BRANCH_NODE),
        ("merge", MERGE_NODE),
    ] {
        graph.add_node(NodeId::from(id), type_id).unwrap();
    }
    graph
        .connect(
            NodeId::from("source"),
            "image",
            NodeId::from("shared"),
            "image",
        )
        .unwrap();
    graph
        .connect(
            NodeId::from("shared"),
            "image",
            NodeId::from("full"),
            "image",
        )
        .unwrap();
    graph
        .connect(
            NodeId::from("shared"),
            "image",
            NodeId::from("region"),
            "image",
        )
        .unwrap();
    graph
        .connect(NodeId::from("full"), "image", NodeId::from("merge"), "full")
        .unwrap();
    graph
        .connect(
            NodeId::from("region"),
            "image",
            NodeId::from("merge"),
            "region",
        )
        .unwrap();

    let result = graph
        .evaluate(
            &NodeId::from("merge"),
            "image",
            &EvaluationContext::with_source_image(source_image())
                .with_requested_region(Region::new(1, 1, 1, 1)),
        )
        .unwrap();
    let Value::Image(result) = result else {
        panic!("expected image")
    };
    assert_eq!(result.pixel(0, 0), Some([1.0, 0.0, 0.0, 1.0]));
}

#[test]
fn render_cache_distinguishes_cpu_and_gpu_execution() {
    let Some(gpu) = GpuContext::initialize_or_cpu() else {
        return;
    };
    let mut graph = Graph::new(registry());
    graph
        .add_node(NodeId::from("backend"), BACKEND_NODE)
        .unwrap();

    let cpu = graph
        .evaluate(
            &NodeId::from("backend"),
            "image",
            &EvaluationContext::default(),
        )
        .unwrap();
    assert_eq!(
        cpu,
        Value::Image(Image::from_pixels(1, 1, vec![[1.0, 0.0, 0.0, 1.0]]).unwrap())
    );

    let gpu_context =
        EvaluationContext::default().with_render_context(RenderContext::new().with_gpu(gpu));
    let gpu = graph
        .evaluate(&NodeId::from("backend"), "image", &gpu_context)
        .unwrap();
    assert_eq!(
        gpu,
        Value::Image(Image::from_pixels(1, 1, vec![[0.0, 1.0, 0.0, 1.0]]).unwrap())
    );
}

#[test]
fn graph_executes_full_frame_nodes_with_full_frame_inputs_and_context() {
    let mut graph = Graph::new(registry());
    graph.add_node(NodeId::from("source"), SOURCE_NODE).unwrap();
    graph
        .add_node(NodeId::from("full"), FULL_FRAME_NODE)
        .unwrap();
    graph
        .connect(
            NodeId::from("source"),
            "image",
            NodeId::from("full"),
            "image",
        )
        .unwrap();

    let request = TileRequest::new(
        Region::new(1, 1, 1, 1),
        TileCoord::new(4, 5),
        2,
        PreviewQuality::Draft,
    );
    let result = graph
        .evaluate(
            &NodeId::from("full"),
            "image",
            &EvaluationContext::with_source_image(source_image()).with_tile_request(request),
        )
        .unwrap();
    let Value::Image(result) = result else {
        panic!("expected image")
    };

    assert_eq!(result.dimensions(), Dimensions::new(2, 2));
    assert_eq!(result.origin(), (0, 0));
    assert_eq!(result.pixel(0, 0), Some([1.0, 0.0, 0.0, 1.0]));
}

#[test]
fn graph_preserves_region_aware_context_for_region_nodes() {
    let mut graph = Graph::new(registry());
    graph.add_node(NodeId::from("source"), SOURCE_NODE).unwrap();
    graph.add_node(NodeId::from("region"), REGION_NODE).unwrap();
    graph
        .connect(
            NodeId::from("source"),
            "image",
            NodeId::from("region"),
            "image",
        )
        .unwrap();

    let request = TileRequest::new(
        Region::new(1, 1, 1, 1),
        TileCoord::new(4, 5),
        2,
        PreviewQuality::Draft,
    );
    let result = graph
        .evaluate(
            &NodeId::from("region"),
            "image",
            &EvaluationContext::with_source_image(source_image()).with_tile_request(request),
        )
        .unwrap();
    let Value::Image(result) = result else {
        panic!("expected image")
    };

    assert_eq!(result.global_region(), Region::new(1, 1, 1, 1));
}
