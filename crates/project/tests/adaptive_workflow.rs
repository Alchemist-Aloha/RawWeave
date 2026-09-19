//! End-to-end acceptance coverage for Step 04 adaptive workflows:
//! metadata-driven parameter ports, lazy-friendly routing, workflow
//! serialization, and literal preservation.

use rawweave_image::Image;
use rawweave_node_api::{EvaluationContext, ParameterValue, Value};
use rawweave_project::EditorCore;
use rawweave_raw::{
    CameraMetadata, DeterministicCorpus, ExifMetadata, LensProfile, Orientation, RawDecoder,
    RawError, RawFrame,
};

/// Decoder whose returned ISO is derived from the first input byte, so the
/// graph can be re-evaluated against different capture metadata.
struct IsoDecoder;

impl RawDecoder for IsoDecoder {
    fn decode(&self, input: &[u8]) -> Result<RawFrame, RawError> {
        let iso = u32::from(input.first().copied().unwrap_or(0)) * 100;
        let base = DeterministicCorpus::bayer_12_bit();
        RawFrame::new(
            base.mosaic().clone(),
            *base.black_levels(),
            *base.white_levels(),
            CameraMetadata {
                make: "Canon".to_owned(),
                model: "EOS R5".to_owned(),
                lens: Some("RF 50mm F1.2".to_owned()),
                iso: Some(iso),
                aperture: Some(2.8),
                shutter_seconds: Some(1.0 / 125.0),
                focal_length_mm: Some(50.0),
                capture_time: None,
                orientation: Orientation::Normal,
                dimensions: None,
            },
            base.profile().clone(),
            Some(LensProfile::identity("RF 50mm F1.2")),
            None,
            ExifMetadata::default(),
        )
    }
}

fn source_image() -> Image {
    let mut pixels = Vec::new();
    for y in 0..4u32 {
        for x in 0..4u32 {
            let value = x as f32 / 3.0;
            pixels.push([value, y as f32 / 3.0, 0.0, 1.0]);
        }
    }
    Image::from_pixels(4, 4, pixels).unwrap()
}

/// ISO feeds a curve which drives the Blur radius parameter port.
fn adaptive_editor() -> EditorCore {
    let mut editor = EditorCore::new_with_raw_decoder(IsoDecoder);
    editor.add_node("image", "core.image-input").unwrap();
    editor.add_node("raw", "raw.decode").unwrap();
    editor.add_node("metadata", "core.metadata").unwrap();
    editor.add_node("curve", "core.curve").unwrap();
    editor.add_node("blur", "core.blur").unwrap();
    editor.add_node("output", "core.output").unwrap();

    editor
        .connect("raw", "camera", "metadata", "camera")
        .unwrap();
    editor.connect("metadata", "iso", "curve", "value").unwrap();
    editor.expose_parameter("blur", "radius").unwrap();
    editor.connect("curve", "value", "blur", "radius").unwrap();
    editor.connect("image", "image", "blur", "image").unwrap();
    editor.connect("blur", "image", "output", "image").unwrap();
    editor
        .set_node_parameter(
            "curve",
            "points",
            ParameterValue::String("0,0;100,0;6400,8".to_owned()),
        )
        .unwrap();
    editor
}

fn evaluate(editor: &EditorCore, iso_byte: u8) -> Image {
    let context =
        EvaluationContext::with_source_image(source_image()).with_source_bytes(vec![iso_byte]);
    match editor.evaluate("output", "image", context).unwrap() {
        Value::Image(image) => image,
        other => panic!("expected an image, got {other:?}"),
    }
}

#[test]
fn metadata_driven_curve_controls_a_parameter_port() {
    let editor = adaptive_editor();
    let source = source_image();

    let low_iso = evaluate(&editor, 1);
    let high_iso = evaluate(&editor, 64);

    assert_eq!(
        low_iso, source,
        "ISO 100 maps the curve to a zero blur radius, so the image is untouched"
    );
    assert_ne!(
        high_iso, source,
        "ISO 6400 maps the curve to a real blur radius through the exposed parameter port"
    );

    // The stored literal survives the adaptive connection.
    let blur = editor
        .graph()
        .node(&rawweave_core::NodeId::from("blur"))
        .unwrap();
    assert_eq!(
        blur.parameters.get("radius"),
        Some(&ParameterValue::Float(1.0))
    );
}

#[test]
fn changing_capture_metadata_recomputes_downstream_results() {
    let editor = adaptive_editor();
    let first = evaluate(&editor, 64);
    let second = evaluate(&editor, 1);
    assert_ne!(
        first, second,
        "different metadata must not reuse a stale adaptive result"
    );
}

#[test]
fn adaptive_workflow_round_trips_through_serialization() {
    let editor = adaptive_editor();
    let expected = evaluate(&editor, 64);
    let saved = editor.save_workflow().unwrap();

    let mut reloaded = EditorCore::new_with_raw_decoder(IsoDecoder);
    reloaded.load_workflow(&saved).unwrap();

    let reloaded_blur = reloaded
        .graph()
        .node(&rawweave_core::NodeId::from("blur"))
        .unwrap();
    assert!(
        reloaded_blur.exposed_parameters.contains("radius"),
        "the exposed parameter must survive a workflow round trip"
    );
    assert_eq!(evaluate(&reloaded, 64), expected);
}

#[test]
fn disconnecting_the_adaptive_value_restores_the_literal() {
    let mut editor = adaptive_editor();
    let connected = evaluate(&editor, 64);

    editor
        .disconnect("curve", "value", "blur", "radius")
        .unwrap();
    let restored = evaluate(&editor, 64);

    assert_ne!(
        restored, connected,
        "disconnecting must fall back to the stored literal radius of 1.0, not the curve value of 8.0"
    );
    // The literal remains authoritative and is still applied.
    editor
        .set_node_parameter("blur", "radius", ParameterValue::Float(4.0))
        .unwrap();
    assert_ne!(evaluate(&editor, 64), restored);
}
