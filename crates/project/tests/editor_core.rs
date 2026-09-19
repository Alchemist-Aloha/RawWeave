use rawweave_core::NodeId;
use rawweave_graph::Graph;
use rawweave_image::Image;
use rawweave_node_api::{EvaluationContext, Inputs, NodePack, NodeRegistry, Parameters, Value};
use rawweave_project::{EditorCore, built_in_node_pack_manifests};
use rawweave_raw::{DeterministicCorpus, DeterministicDecoder};
use rawweave_raw_nodes::RawNodePack;

#[test]
fn editor_core_registers_the_backbone_node_packs() {
    let editor = EditorCore::new();
    let types = editor
        .node_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.type_id)
        .collect::<Vec<_>>();
    assert_eq!(types.len(), 38);
    for type_id in [
        "core.image-input",
        "core.constant-float",
        "core.constant-integer",
        "core.constant-boolean",
        "core.constant-string",
        "core.metadata",
        "core.equal",
        "core.greater-than",
        "core.less-than",
        "core.compare",
        "core.and",
        "core.or",
        "core.not",
        "core.switch",
        "core.select",
        "core.enum-select",
        "core.map-range",
        "core.clamp",
        "core.curve",
        "core.expression",
        "core.string-match",
        "core.exposure",
        "core.invert",
        "core.resize",
        "core.crop",
        "core.blur",
        "core.levels",
        "core.curves",
        "core.color-matrix",
        "core.output",
        "raw.decode",
        "raw.black-level",
        "raw.white-balance",
        "raw.highlight-reconstruction",
        "raw.demosaic",
        "raw.camera-transform",
        "raw.lens-correction",
        "raw.display-transform",
    ] {
        assert!(types.iter().any(|registered| registered == type_id));
    }
}

#[test]
fn built_in_pack_manifests_describe_the_registered_nodes() {
    let editor_types = EditorCore::new()
        .node_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.type_id)
        .collect::<std::collections::BTreeSet<_>>();
    let manifest_types = built_in_node_pack_manifests()
        .into_iter()
        .flat_map(|manifest| manifest.nodes.into_iter().map(|node| node.type_id))
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(manifest_types, editor_types);
}

#[test]
fn editor_core_saves_reloads_and_evaluates_a_workflow() {
    let mut editor = EditorCore::new();
    editor.add_node("input", "core.image-input").unwrap();
    editor.add_node("exposure", "core.exposure").unwrap();
    editor.add_node("output", "core.output").unwrap();
    editor
        .connect("input", "image", "exposure", "image")
        .unwrap();
    editor
        .connect("exposure", "image", "output", "image")
        .unwrap();
    editor
        .set_node_parameter("exposure", "exposure", 1.0_f32.into())
        .unwrap();

    let saved = editor.save_workflow().unwrap();
    let mut reloaded = EditorCore::new();
    reloaded.load_workflow(&saved).unwrap();
    let result = reloaded
        .evaluate(
            "output",
            "image",
            EvaluationContext::with_source_image(
                Image::from_pixels(1, 1, vec![[0.25, 0.5, 0.75, 1.0]]).unwrap(),
            ),
        )
        .unwrap();
    assert_eq!(
        result,
        Value::Image(Image::from_pixels(1, 1, vec![[0.5, 1.0, 1.5, 1.0]]).unwrap())
    );
}

#[test]
fn editor_core_round_trips_a_raw_pipeline_and_executes_it_with_a_test_decoder() {
    let mut editor = EditorCore::new();
    for (node_id, type_id) in [
        ("decode", "raw.decode"),
        ("black", "raw.black-level"),
        ("white-balance", "raw.white-balance"),
        ("demosaic", "raw.demosaic"),
        ("camera", "raw.camera-transform"),
        ("display", "raw.display-transform"),
    ] {
        editor.add_node(node_id, type_id).unwrap();
    }
    editor.connect("decode", "frame", "black", "frame").unwrap();
    editor
        .connect("black", "mosaic", "white-balance", "mosaic")
        .unwrap();
    editor
        .connect("white-balance", "mosaic", "demosaic", "mosaic")
        .unwrap();
    editor
        .connect("demosaic", "scene", "camera", "scene")
        .unwrap();
    editor
        .connect("camera", "scene", "display", "scene")
        .unwrap();
    editor
        .set_node_parameter("white-balance", "red_gain", 1.25_f32.into())
        .unwrap();

    let saved = editor.save_workflow().unwrap();
    let document: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert!(document.get("registry").is_none());
    assert!(document.get("render_cache").is_none());
    assert!(
        document["nodes"]
            .as_object()
            .unwrap()
            .values()
            .all(|node| node.get("runtime_outputs").is_none())
    );

    let mut reloaded = EditorCore::new();
    reloaded.load_workflow(&saved).unwrap();
    assert_eq!(reloaded.graph().nodes().len(), 6);
    assert_eq!(reloaded.graph().edges().len(), 5);
    assert_eq!(
        reloaded
            .graph()
            .node(&NodeId::from("display"))
            .unwrap()
            .type_id,
        "raw.display-transform"
    );

    let restored = Graph::from_json(&saved, deterministic_registry()).unwrap();
    assert_eq!(restored.nodes().len(), 6);
    assert_eq!(restored.edges().len(), 5);

    let registry = deterministic_registry();
    let decoded = evaluate(
        &registry,
        "raw.decode",
        [("bytes".to_owned(), Value::Bytes(Vec::new()))]
            .into_iter()
            .collect(),
        Parameters::new(),
    );
    let black = evaluate(
        &registry,
        "raw.black-level",
        [("frame".to_owned(), decoded.outputs["frame"].clone())]
            .into_iter()
            .collect(),
        Parameters::new(),
    );
    let balanced = evaluate(
        &registry,
        "raw.white-balance",
        [("mosaic".to_owned(), black.outputs["mosaic"].clone())]
            .into_iter()
            .collect(),
        [("red_gain".to_owned(), 1.25_f32.into())]
            .into_iter()
            .collect(),
    );
    let scene = evaluate(
        &registry,
        "raw.demosaic",
        [("mosaic".to_owned(), balanced.outputs["mosaic"].clone())]
            .into_iter()
            .collect(),
        Parameters::new(),
    );
    let camera = evaluate(
        &registry,
        "raw.camera-transform",
        [("scene".to_owned(), scene.outputs["scene"].clone())]
            .into_iter()
            .collect(),
        Parameters::new(),
    );
    let display = evaluate(
        &registry,
        "raw.display-transform",
        [("scene".to_owned(), camera.outputs["scene"].clone())]
            .into_iter()
            .collect(),
        Parameters::new(),
    );
    assert!(matches!(
        display.outputs.get("display"),
        Some(Value::DisplayRGB(_))
    ));
}

#[test]
fn reloaded_raw_workflow_evaluates_from_context_bytes_end_to_end() {
    let decoder = DeterministicDecoder::new(DeterministicCorpus::bayer_12_bit());
    let mut editor = EditorCore::new_with_raw_decoder(decoder.clone());
    for (node_id, type_id) in [
        ("decode", "raw.decode"),
        ("black", "raw.black-level"),
        ("white-balance", "raw.white-balance"),
        ("highlight", "raw.highlight-reconstruction"),
        ("demosaic", "raw.demosaic"),
        ("camera", "raw.camera-transform"),
        ("lens", "raw.lens-correction"),
        ("display", "raw.display-transform"),
    ] {
        editor.add_node(node_id, type_id).unwrap();
    }
    editor.connect("decode", "frame", "black", "frame").unwrap();
    editor
        .connect("black", "mosaic", "white-balance", "mosaic")
        .unwrap();
    editor
        .connect("white-balance", "mosaic", "highlight", "mosaic")
        .unwrap();
    editor
        .connect("highlight", "mosaic", "demosaic", "mosaic")
        .unwrap();
    editor
        .connect("demosaic", "scene", "camera", "scene")
        .unwrap();
    editor
        .connect("decode", "camera_profile", "camera", "camera_profile")
        .unwrap();
    editor.connect("camera", "scene", "lens", "scene").unwrap();
    editor
        .connect("decode", "lens_profile", "lens", "lens_profile")
        .unwrap();
    editor.connect("lens", "scene", "display", "scene").unwrap();

    let saved = editor.save_workflow().unwrap();
    let mut reloaded = EditorCore::new_with_raw_decoder(decoder);
    reloaded.load_workflow(&saved).unwrap();
    let result = reloaded
        .evaluate_raw_workflow_with_bytes("display", "display", vec![1, 2, 3])
        .unwrap();

    assert!(matches!(result, Value::DisplayRGB(_)));
}

fn deterministic_registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    RawNodePack::with_decoder(DeterministicDecoder::new(
        DeterministicCorpus::bayer_12_bit(),
    ))
    .register(&mut registry)
    .unwrap();
    registry
}

fn evaluate(
    registry: &NodeRegistry,
    type_id: &str,
    inputs: Inputs,
    parameters: Parameters,
) -> rawweave_node_api::NodeResult {
    registry
        .instantiate(type_id)
        .unwrap()
        .evaluate(&inputs, &parameters, &EvaluationContext::default())
        .unwrap()
}
