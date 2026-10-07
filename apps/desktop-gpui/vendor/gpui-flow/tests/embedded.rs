use gpui_flow::{FlowNode, FlowPoint, FlowState, HandleDef, HandlePosition};

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
