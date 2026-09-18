use std::sync::Arc;

use rawweave_color::{DisplayRGB, SceneLinearRGB, WorkingSpace};
use rawweave_node_api::{EvaluationContext, Inputs, NodePack, NodeRegistry, Parameters, Value};
use rawweave_raw::{DeterministicCorpus, DeterministicDecoder, RawDecoder};
use rawweave_raw_nodes::{RawNodePack, register_nodes_with_decoder};

fn deterministic_registry() -> NodeRegistry {
    let decoder = DeterministicDecoder::new(DeterministicCorpus::bayer_12_bit());
    let mut registry = NodeRegistry::default();
    RawNodePack::with_decoder(decoder)
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

#[test]
fn raw_pack_descriptors_expose_metadata_and_typed_stages() {
    let mut registry = NodeRegistry::default();
    register_nodes_with_decoder(
        &mut registry,
        Arc::new(DeterministicDecoder::new(
            DeterministicCorpus::bayer_12_bit(),
        )),
    )
    .unwrap();

    for type_id in [
        "raw.decode",
        "raw.black-level",
        "raw.white-balance",
        "raw.highlight-reconstruction",
        "raw.demosaic",
        "raw.camera-transform",
        "raw.lens-correction",
        "raw.display-transform",
    ] {
        assert!(registry.descriptor(type_id).is_some(), "missing {type_id}");
    }
    let decode = registry.descriptor("raw.decode").unwrap();
    assert_eq!(decode.outputs[0].data_type, "raw.Frame");
    assert!(
        decode
            .outputs
            .iter()
            .any(|port| port.data_type == "raw.CameraMetadata")
    );
    assert!(
        decode
            .outputs
            .iter()
            .any(|port| port.data_type == "raw.ExifMetadata")
    );
    assert_eq!(
        registry.descriptor("raw.demosaic").unwrap().outputs[0].data_type,
        "color.SceneLinearRGB"
    );
    assert_eq!(
        registry
            .descriptor("raw.display-transform")
            .unwrap()
            .outputs[0]
            .data_type,
        "color.DisplayRGB"
    );
}

#[test]
fn decode_is_deterministic_and_returns_frame_mosaic_and_metadata() {
    let registry = deterministic_registry();
    let first = evaluate(
        &registry,
        "raw.decode",
        [("bytes".to_owned(), Value::Bytes(b"first".to_vec()))]
            .into_iter()
            .collect(),
        Parameters::new(),
    );
    let second = evaluate(
        &registry,
        "raw.decode",
        [("bytes".to_owned(), Value::Bytes(b"second".to_vec()))]
            .into_iter()
            .collect(),
        Parameters::new(),
    );
    assert_eq!(first, second);
    assert!(matches!(
        first.outputs.get("frame"),
        Some(Value::RawFrame(_))
    ));
    assert!(matches!(
        first.outputs.get("mosaic"),
        Some(Value::Mosaic(_))
    ));
    assert!(matches!(
        first.outputs.get("camera"),
        Some(Value::CameraMetadata(_))
    ));
    assert!(matches!(
        first.outputs.get("exif"),
        Some(Value::ExifMetadata(_))
    ));
}

#[test]
fn black_level_and_white_balance_preserve_highlight_headroom() {
    let registry = deterministic_registry();
    let decoded = evaluate(
        &registry,
        "raw.decode",
        [("bytes".to_owned(), Value::Bytes(vec![1, 2, 3]))]
            .into_iter()
            .collect(),
        Parameters::new(),
    );
    let frame = decoded.outputs.get("frame").unwrap().clone();
    let black = evaluate(
        &registry,
        "raw.black-level",
        [("frame".to_owned(), frame)].into_iter().collect(),
        Parameters::new(),
    );
    let mosaic = black.outputs.get("mosaic").unwrap().clone();
    let wb = evaluate(
        &registry,
        "raw.white-balance",
        [("mosaic".to_owned(), mosaic)].into_iter().collect(),
        [
            ("red_gain".to_owned(), 64.0_f32.into()),
            ("green_gain".to_owned(), 1.0_f32.into()),
            ("blue_gain".to_owned(), 0.5_f32.into()),
        ]
        .into_iter()
        .collect(),
    );
    let Value::Mosaic(mosaic) = wb.outputs.get("mosaic").unwrap() else {
        panic!("expected mosaic")
    };
    assert_eq!(
        mosaic.dimensions(),
        DeterministicCorpus::bayer_12_bit().sensor_dimensions()
    );
    assert!(mosaic.samples().iter().any(|sample| *sample > 1.0));
}

#[test]
fn demosaic_has_sensor_dimensions_and_keeps_scene_highlights_unclipped() {
    let registry = deterministic_registry();
    let decoded = evaluate(
        &registry,
        "raw.decode",
        [("bytes".to_owned(), Value::Bytes(vec![]))]
            .into_iter()
            .collect(),
        Parameters::new(),
    );
    let black = evaluate(
        &registry,
        "raw.black-level",
        [(
            "frame".to_owned(),
            decoded.outputs.get("frame").unwrap().clone(),
        )]
        .into_iter()
        .collect(),
        Parameters::new(),
    );
    let balanced = evaluate(
        &registry,
        "raw.white-balance",
        [(
            "mosaic".to_owned(),
            black.outputs.get("mosaic").unwrap().clone(),
        )]
        .into_iter()
        .collect(),
        [("red_gain".to_owned(), 64.0_f32.into())]
            .into_iter()
            .collect(),
    );
    let scene = evaluate(
        &registry,
        "raw.demosaic",
        [(
            "mosaic".to_owned(),
            balanced.outputs.get("mosaic").unwrap().clone(),
        )]
        .into_iter()
        .collect(),
        Parameters::new(),
    );
    let Value::SceneLinearRGB(scene) = scene.outputs.get("scene").unwrap() else {
        panic!("expected scene-linear output")
    };
    assert_eq!(
        scene.dimensions(),
        DeterministicCorpus::bayer_12_bit().sensor_dimensions()
    );
    assert!(scene.pixels().iter().flatten().any(|sample| *sample > 1.0));
}

#[test]
fn lens_correction_defaults_to_a_scene_preserving_pass_through() {
    let registry = deterministic_registry();
    let scene = SceneLinearRGB::new(
        rawweave_image::Dimensions::new(2, 1),
        vec![[2.0, 0.25, 0.0], [0.5, 1.5, 0.75]],
        WorkingSpace::Srgb,
    )
    .unwrap();
    let result = evaluate(
        &registry,
        "raw.lens-correction",
        [("scene".to_owned(), Value::SceneLinearRGB(scene.clone()))]
            .into_iter()
            .collect(),
        Parameters::new(),
    );
    assert_eq!(
        result.outputs.get("scene"),
        Some(&Value::SceneLinearRGB(scene))
    );
}

#[test]
fn display_transform_is_srgb_and_clips_only_at_display_boundary() {
    let registry = deterministic_registry();
    let scene = SceneLinearRGB::from_pixels(1, 1, vec![[4.0, 0.25, 0.0]]).unwrap();
    let result = evaluate(
        &registry,
        "raw.display-transform",
        [("scene".to_owned(), Value::SceneLinearRGB(scene.clone()))]
            .into_iter()
            .collect(),
        Parameters::new(),
    );
    assert_eq!(scene.pixel(0, 0), Some([4.0, 0.25, 0.0]));
    let Value::DisplayRGB(display) = result.outputs.get("display").unwrap() else {
        panic!("expected display output")
    };
    assert_eq!(display.pixel(0, 0).unwrap()[0], 1.0);
    assert!(display.pixel(0, 0).unwrap()[1] > 0.5);
}

#[test]
fn default_pack_uses_rawloader_decoder_without_needing_test_injection() {
    let pack = RawNodePack::default();
    let mut registry = NodeRegistry::default();
    pack.register(&mut registry).unwrap();
    let result = registry.instantiate("raw.decode").unwrap().evaluate(
        &[("bytes".to_owned(), Value::Bytes(b"not raw".to_vec()))]
            .into_iter()
            .collect(),
        &Parameters::new(),
        &EvaluationContext::default(),
    );
    assert!(result.is_err());
}

fn _assert_value_types(_: DisplayRGB, _: Arc<dyn RawDecoder>) {}
