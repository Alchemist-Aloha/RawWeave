use rawweave_core::NodeId;
use rawweave_project::EditorCore;

#[test]
fn raw_scene_processing_conversion_and_display_round_trip_with_mip_requests() {
    use rawweave_node_api::{EvaluationContext, Value};
    use rawweave_raw::{DeterministicCorpus, DeterministicDecoder};
    let decoder = DeterministicDecoder::new(DeterministicCorpus::bayer_12_bit());
    let mut editor = EditorCore::new_with_raw_decoder(decoder.clone());
    for (id, node_type) in [
        ("decode", "raw.decode"),
        ("black", "raw.black-level"),
        ("demosaic", "raw.demosaic"),
        ("camera", "raw.camera-transform"),
        ("exposure", "core.exposure"),
        ("convert", "core.scene-linear-to-image"),
        ("output", "core.output"),
        ("display", "raw.display-transform"),
    ] {
        editor.add_node(id, node_type).unwrap();
    }
    for (from, output, to, input) in [
        ("decode", "frame", "black", "frame"),
        ("black", "mosaic", "demosaic", "mosaic"),
        ("demosaic", "scene", "camera", "scene"),
        ("decode", "camera_profile", "camera", "camera_profile"),
        ("camera", "scene", "exposure", "scene"),
        ("exposure", "scene", "convert", "scene"),
        ("convert", "image", "output", "image"),
        ("exposure", "scene", "display", "scene"),
    ] {
        editor.connect(from, output, to, input).unwrap();
    }
    editor
        .set_node_parameter("exposure", "exposure", 1.0_f32.into())
        .unwrap();
    let saved = editor.save_workflow().unwrap();
    let mut restored = EditorCore::new_with_raw_decoder(decoder);
    restored.load_workflow(&saved).unwrap();
    let context = EvaluationContext::default().with_source_bytes(vec![0]);
    let Value::SceneLinearRGB(base) = restored
        .evaluate("camera", "scene", context.clone())
        .unwrap()
    else {
        panic!()
    };
    let Value::SceneLinearRGB(exposed) = restored
        .evaluate("exposure", "scene", context.clone())
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        exposed.pixels()[0],
        base.pixels()[0].map(|value| value * 2.0)
    );
    assert_eq!(exposed.working_space(), base.working_space());
    let Value::Image(full) = restored
        .evaluate("output", "image", context.clone())
        .unwrap()
    else {
        panic!()
    };
    let Value::Image(repeated) = restored
        .evaluate("output", "image", context.clone())
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(full.pixels().as_ptr(), repeated.pixels().as_ptr());
    let Value::Image(mip) = restored
        .evaluate("output", "image", context.clone().with_mip_level(1))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(full.dimensions(), mip.dimensions());
    assert_eq!(full.pixels(), mip.pixels());
    let Value::SceneLinearRGB(sampled) = restored
        .evaluate("exposure", "scene", context.clone().with_mip_level(1))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(sampled.sampling().unwrap().mip, 1);
    assert!(matches!(
        restored.evaluate("display", "display", context).unwrap(),
        Value::DisplayRGB(_)
    ));
    // RAW values must never be silently treated as ordinary image pixels.
    assert!(
        restored
            .connect("decode", "mosaic", "exposure", "scene")
            .is_err()
    );
}

#[test]
fn scene_pro_detail_mask_and_core_tones_round_trip_with_cached_mip_results() {
    use rawweave_node_api::{EvaluationContext, Value};
    use rawweave_raw::{DeterministicCorpus, DeterministicDecoder};
    let decoder = DeterministicDecoder::new(DeterministicCorpus::bayer_12_bit());
    let mut editor = EditorCore::new_with_raw_decoder(decoder.clone());
    editor.reset_raw_image_graph().unwrap();
    for (id, kind) in [
        ("detail", "pro.detail-separation"),
        ("mask", "core.mask-luminance"),
        ("local", "core.local-exposure"),
        ("levels", "core.levels"),
        ("curves", "core.curves"),
        ("film", "pro.film-curve"),
        ("output", "core.output"),
        ("stats", "pro.histogram"),
    ] {
        editor.add_node(id, kind).unwrap();
    }
    for (from, port, to, input) in [
        ("camera-transform", "scene", "detail", "scene"),
        ("detail", "base_scene", "mask", "scene"),
        ("detail", "base_scene", "local", "scene"),
        ("mask", "mask", "local", "mask"),
        ("local", "scene", "levels", "scene"),
        ("levels", "scene", "curves", "scene"),
        ("curves", "scene", "film", "scene"),
        ("film", "scene", "output", "scene"),
        ("output", "scene", "stats", "scene"),
    ] {
        editor.connect(from, port, to, input).unwrap();
    }
    editor
        .set_node_parameter("local", "exposure", 2.0_f32.into())
        .unwrap();
    let saved = editor.save_workflow().unwrap();
    let mut restored = EditorCore::new_with_raw_decoder(decoder);
    restored.load_workflow(&saved).unwrap();
    let context = EvaluationContext::default()
        .with_source_bytes(vec![0])
        .with_mip_level(1);
    let Value::SceneLinearRGB(scene) = restored
        .evaluate("output", "scene", context.clone())
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        scene.sampling().unwrap().full_dimensions,
        rawweave_image::Dimensions::new(4, 2)
    );
    assert_eq!(scene.sampling().unwrap().mip, 1);
    assert!(scene.pixels().iter().flatten().all(|v| v.is_finite()));
    let Value::SceneLinearRGB(repeat) = restored
        .evaluate("output", "scene", context.clone())
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(scene.pixels().as_ptr(), repeat.pixels().as_ptr());
    let Value::Mask(mask) = restored.evaluate("mask", "mask", context.clone()).unwrap() else {
        panic!()
    };
    assert_eq!(mask.dimensions(), rawweave_image::Dimensions::new(4, 2));
    assert!(
        matches!(restored.evaluate("stats", "mean", context).unwrap(), Value::Float(v) if v.is_finite())
    );
    assert_eq!(restored.graph().edges(), editor.graph().edges());
}

#[test]
fn shared_desktop_workflows_preserve_ids_and_advance_revision() {
    let mut editor = EditorCore::new();
    editor.reset_ordinary_image_graph().unwrap();
    assert_eq!(editor.graph().nodes().len(), 2);
    assert_eq!(editor.graph().edges().len(), 1);
    assert_eq!(
        editor.graph().node(&NodeId::from("input")).unwrap().type_id,
        "core.image-input"
    );
    let revision = editor.graph().revision();
    editor.reset_raw_image_graph().unwrap();
    assert!(editor.graph().revision() > revision);
    assert_eq!(editor.graph().nodes().len(), 8);
    assert_eq!(editor.graph().edges().len(), 9);
    assert_eq!(
        editor
            .graph()
            .node(&NodeId::from("display-transform"))
            .unwrap()
            .type_id,
        "raw.display-transform"
    );
    let workflow = editor.save_workflow().unwrap();
    let mut restored = EditorCore::new();
    restored.load_workflow(&workflow).unwrap();
    assert_eq!(restored.graph().edges(), editor.graph().edges());
    editor.reset_ordinary_image_graph().unwrap();
    assert_eq!(editor.graph().nodes().len(), 2);
}
