use rawweave_core::NodeId;
use rawweave_project::EditorCore;

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
