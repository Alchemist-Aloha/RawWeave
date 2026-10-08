use gpui_flow::{FlowNode, FlowPoint, FlowState, HandleDef, HandlePosition, HandleType};

#[test]
fn embedded_graph_handles_use_canvas_origin_and_distinct_port_positions() {
    let node = FlowNode::new("node", 40.0, 30.0)
        .size(200.0, 120.0)
        .handles(vec![
            HandleDef::source(HandlePosition::Right).id("scene"),
            HandleDef::source(HandlePosition::Right).id("profile"),
        ]);
    let mut state = FlowState::new(vec![node], vec![]);
    state.canvas_origin = FlowPoint::new(200.0, 50.0);
    assert_eq!(
        state.find_handle_center(&"node".into(), &Some("scene".into()), HandlePosition::Right),
        Some((440.0, 120.0))
    );
    assert_eq!(
        state.find_handle_center(
            &"node".into(),
            &Some("profile".into()),
            HandlePosition::Right
        ),
        Some((440.0, 160.0))
    );
}

#[test]
fn handles_sharing_an_id_across_input_and_output_resolve_by_type() {
    // Exposure/output/crop all name their image input and image output "image".
    let node = FlowNode::new("exposure", 10.0, 20.0)
        .size(180.0, 80.0)
        .handles(vec![
            HandleDef::target(HandlePosition::Left).id("image"),
            HandleDef::source(HandlePosition::Right).id("image"),
        ]);
    let state = FlowState::new(vec![node], vec![]);
    assert_eq!(
        state.handle_position(
            &"exposure".into(),
            &Some("image".into()),
            HandleType::Source
        ),
        Some(HandlePosition::Right),
        "a source lookup must not resolve an input declared with the same id"
    );
    assert_eq!(
        state.handle_position(
            &"exposure".into(),
            &Some("image".into()),
            HandleType::Target
        ),
        Some(HandlePosition::Left)
    );
    // The renderer derives anchors from this position, so a source edge starts on
    // the right edge and never on the node's own input side.
    let position = state
        .handle_position(
            &"exposure".into(),
            &Some("image".into()),
            HandleType::Source,
        )
        .unwrap();
    assert_eq!(
        state.find_handle_center(&"exposure".into(), &Some("image".into()), position),
        Some((190.0, 60.0))
    );
}
