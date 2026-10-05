use rawweave_image::Image;
use rawweave_node_api::{EvaluationContext, ParameterValue, Value};
use rawweave_project::EditorCore;
use rawweave_raw::{DeterministicCorpus, DeterministicDecoder, ExifMetadata, RawFrame};
use std::{fs, path::Path};

fn load_example(name: &str, iso: Option<u32>) -> EditorCore {
    let base = DeterministicCorpus::bayer_12_bit();
    let mut camera = base.camera().clone();
    camera.iso = iso; // Synthetic test metadata, never embedded in the examples.
    let frame = RawFrame::new(
        base.mosaic().clone(),
        *base.black_levels(),
        *base.white_levels(),
        camera,
        base.profile().clone(),
        base.lens_profile().cloned(),
        None,
        ExifMetadata::default(),
    )
    .unwrap();
    let mut editor = EditorCore::new_with_raw_decoder(DeterministicDecoder::new(frame));
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/workflows");
    let json = fs::read_to_string(directory.join(format!("{name}.json")))
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    editor
        .load_workflow(&json)
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    editor
}

#[test]
fn example_workflows_load_round_trip_and_render() {
    let names = [
        "basic-tone",
        "monochrome",
        "highlight-mask",
        "web-resize",
        "film-look",
        "raw-development",
        "expression-exposure",
        "conditional-highlights",
        "named-look-router",
        "iso-adaptive-raw",
    ];
    for name in names {
        let mut editor = load_example(name, Some(100));
        let saved = editor.save_workflow().unwrap();
        editor.load_workflow(&saved).unwrap();
        if matches!(name, "raw-development" | "iso-adaptive-raw") {
            assert!(matches!(
                editor
                    .evaluate_raw_workflow_with_bytes("99-display", "display", vec![1, 2, 3])
                    .unwrap(),
                Value::DisplayRGB(_)
            ));
        } else {
            let image = Image::from_pixels(4, 4, vec![[0.25, 0.5, 0.75, 1.0]; 16]).unwrap();
            let result = editor
                .evaluate(
                    "99-output",
                    "image",
                    EvaluationContext::with_source_image(image),
                )
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let Value::Image(image) = result else {
                panic!("{name}: expected image")
            };
            if name == "web-resize" {
                assert_eq!((image.width(), image.height()), (1200, 800));
            } else {
                assert_eq!((image.width(), image.height()), (4, 4));
            }
            assert!(
                image
                    .pixels()
                    .iter()
                    .flatten()
                    .all(|value| value.is_finite()),
                "{name}: nonfinite output"
            );
            if name == "monochrome" {
                for pixel in image.pixels() {
                    assert_eq!(pixel[0], pixel[1]);
                    assert_eq!(pixel[1], pixel[2]);
                }
            }
        }
    }
}

fn render_example(editor: &EditorCore) -> Image {
    let source = Image::from_pixels(4, 4, vec![[0.25, 0.5, 0.75, 1.0]; 16]).unwrap();
    let Value::Image(image) = editor
        .evaluate(
            "99-output",
            "image",
            EvaluationContext::with_source_image(source),
        )
        .unwrap()
    else {
        panic!("expected image output")
    };
    image
}

#[test]
fn expression_example_bounds_the_connected_exposure_and_preserves_the_literal() {
    let mut editor = load_example("expression-exposure", Some(100));
    for (ev, expected) in [(1.5, 0.75), (10.0, 2.0), (-10.0, -2.0)] {
        editor
            .set_node_parameter("10-ev", "value", ParameterValue::Float(ev))
            .unwrap();
        assert_eq!(
            editor
                .evaluate("30-clamp", "value", EvaluationContext::default())
                .unwrap(),
            Value::Float(expected)
        );
        let image = render_example(&editor);
        assert!((image.pixels()[0][0] - 0.25 * 2.0_f32.powf(expected)).abs() < 1e-6);
    }
    let exposure = editor
        .graph()
        .node(&rawweave_core::NodeId::from("40-exposure"))
        .unwrap();
    assert!(exposure.exposed_parameters.contains("exposure"));
    assert_eq!(
        exposure.parameters.get("exposure"),
        Some(&ParameterValue::Float(0.0))
    );
}

#[test]
fn conditional_example_requires_enabled_and_negative_exposure() {
    let mut editor = load_example("conditional-highlights", Some(100));
    let darkened = render_example(&editor);
    editor
        .set_node_parameter("11-enabled", "value", ParameterValue::Boolean(false))
        .unwrap();
    let bypass = render_example(&editor);
    assert_eq!(bypass.pixels()[0], [0.25, 0.5, 0.75, 1.0]);
    assert!(darkened.pixels()[0][0] < bypass.pixels()[0][0]);
    editor
        .set_node_parameter("11-enabled", "value", ParameterValue::Boolean(true))
        .unwrap();
    editor
        .set_node_parameter("12-requested-ev", "value", ParameterValue::Float(0.7))
        .unwrap();
    assert_eq!(render_example(&editor), bypass);
}

#[test]
fn named_router_selects_images_lazily_and_rejects_unknown_names() {
    let mut editor = load_example("named-look-router", Some(100));
    let film = render_example(&editor);
    editor
        .set_node_parameter(
            "20-look",
            "value",
            ParameterValue::String("monochrome".into()),
        )
        .unwrap();
    let mono = render_example(&editor);
    assert_eq!(mono.pixels()[0][0], mono.pixels()[0][1]);
    assert_eq!(mono.pixels()[0][1], mono.pixels()[0][2]);
    assert_ne!(film, mono);
    editor
        .set_node_parameter("20-look", "value", ParameterValue::String("neutral".into()))
        .unwrap();
    editor
        .set_node_parameter("30-film", "points", ParameterValue::String("broken".into()))
        .unwrap();
    assert_eq!(
        render_example(&editor).pixels()[0],
        [0.25, 0.5, 0.75, 1.0],
        "unselected film branch must not execute"
    );
    editor
        .set_node_parameter("20-look", "value", ParameterValue::String("unknown".into()))
        .unwrap();
    assert!(
        editor
            .evaluate(
                "99-output",
                "image",
                EvaluationContext::with_source_image(
                    Image::from_pixels(1, 1, vec![[0.5; 4]]).unwrap()
                )
            )
            .is_err()
    );
}

#[test]
fn raw_example_maps_iso_metadata_into_a_preserved_parameter_override() {
    for (iso, expected) in [(100, 1.0), (6400, 0.75), (51200, 0.5)] {
        let editor = load_example("iso-adaptive-raw", Some(iso));
        assert_eq!(
            editor
                .evaluate(
                    "15-recovery",
                    "value",
                    EvaluationContext::default().with_source_bytes(vec![1, 2, 3])
                )
                .unwrap(),
            Value::Float(expected)
        );
        let highlight = editor
            .graph()
            .node(&rawweave_core::NodeId::from("30-highlight"))
            .unwrap();
        assert!(highlight.exposed_parameters.contains("strength"));
        assert_eq!(
            highlight.parameters.get("strength"),
            Some(&ParameterValue::Float(1.0))
        );
    }
}

#[test]
fn raw_example_reports_missing_iso_and_can_restore_its_literal() {
    let mut editor = load_example("iso-adaptive-raw", None);
    let context = EvaluationContext::default().with_source_bytes(vec![1, 2, 3]);
    let error = editor
        .evaluate("99-display", "display", context.clone())
        .unwrap_err();
    assert!(error.to_string().contains("iso"), "{error}");
    editor
        .disconnect("15-recovery", "value", "30-highlight", "strength")
        .unwrap();
    assert!(matches!(
        editor.evaluate("99-display", "display", context).unwrap(),
        Value::DisplayRGB(_)
    ));
}
