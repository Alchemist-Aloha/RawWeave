use rawweave_graph::WorkflowMetadata;
use rawweave_project::EditorCore;

#[test]
fn editor_core_creates_exports_imports_and_instantiates_a_blueprint() {
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

    let blueprint = editor
        .create_subgraph_from_selection(
            &["exposure"],
            "looks.exposure",
            "1.0.0",
            WorkflowMetadata::new("Exposure"),
        )
        .unwrap();
    assert_eq!(blueprint.graph().nodes().len(), 1);
    assert_eq!(blueprint.inputs().len(), 1);
    assert_eq!(blueprint.outputs().len(), 1);

    let exported = editor.save_blueprint(&blueprint).unwrap();
    let imported = editor.load_blueprint(&exported).unwrap();
    assert_eq!(imported.hash(), blueprint.hash());

    editor.instantiate_blueprint(&imported).unwrap();
    assert_eq!(editor.graph().nodes().len(), 1);
}

#[test]
fn editor_core_can_expose_and_hide_a_blueprint_parameter_without_opening_internals() {
    let mut editor = EditorCore::new();
    editor.add_node("exposure", "core.exposure").unwrap();
    let mut blueprint = editor
        .create_subgraph_from_selection(
            &["exposure"],
            "looks.exposure",
            "1.0.0",
            WorkflowMetadata::new("Exposure"),
        )
        .unwrap();

    editor
        .expose_blueprint_parameter(&mut blueprint, "exposure:exposure")
        .unwrap();
    assert_eq!(blueprint.parameters().len(), 1);
    editor
        .set_blueprint_parameter(&mut blueprint, "exposure:exposure", 2.0_f32.into())
        .unwrap();
    editor
        .hide_blueprint_parameter(&mut blueprint, "exposure:exposure")
        .unwrap();
    assert!(blueprint.parameters().is_empty());
}
