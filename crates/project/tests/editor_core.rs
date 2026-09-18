use rawweave_image::Image;
use rawweave_node_api::{EvaluationContext, Value};
use rawweave_project::EditorCore;

#[test]
fn editor_core_registers_the_backbone_node_packs() {
    let editor = EditorCore::new();
    let types = editor
        .node_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.type_id)
        .collect::<Vec<_>>();
    assert_eq!(types.len(), 5);
    assert!(types.iter().any(|type_id| type_id == "core.image-input"));
    assert!(types.iter().any(|type_id| type_id == "core.constant-float"));
    assert!(types.iter().any(|type_id| type_id == "core.exposure"));
    assert!(types.iter().any(|type_id| type_id == "core.invert"));
    assert!(types.iter().any(|type_id| type_id == "core.output"));
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
