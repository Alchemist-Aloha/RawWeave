//! Headless GPUI layout/input regressions, not GPU pixel or OS-portal tests.
use super::{Editor, Workspace};
use gpui_kit::InputEvent as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Focusable, Point, SharedString, TestAppContext, WindowBounds,
    WindowOptions, px, size,
};

fn editor_window(cx: &mut TestAppContext) -> (gpui_kit::AnyWindowHandle, gpui_kit::Entity<Editor>) {
    cx.update(gpui_kit::init);
    cx.update(super::configure_theme);
    cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(1280.0), px(840.0)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut editor = Editor::new(window, cx);
                    editor.preferences_file = None;
                    editor.workspace = Workspace::default();
                    editor
                })
            },
        )
        .unwrap()
    })
}

#[gpui_kit::test]
fn compact_workflow_keeps_controls_separate_and_dock_actions_out_of_history(
    cx: &mut TestAppContext,
) {
    let (handle, editor) = editor_window(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.simulate_window_resize(handle, size(px(943.0), px(505.0)));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            editor.read(cx).session.editor.graph().nodes().is_empty(),
            "start with an empty workflow, as Tauri does"
        );
        assert_eq!(
            window.find("library").bounds().size.width,
            px(editor.read(cx).workspace.library_width),
            "initial dock ignores saved sizing"
        );
        let controls = window.find("viewer-controls-A").bounds();
        let image = window.find("viewer-0-interaction").bounds();
        let scopes = window.find("scopes-panel").bounds();
        assert!(
            image.size.height >= px(80.0),
            "image surface collapsed: {image:?}"
        );
        assert!(
            controls.bottom() <= image.top() + px(1.0),
            "controls overlap image"
        );
        assert!(image.bottom() <= scopes.top(), "scopes overlap image");
        window.press("ctrl-k", cx);
        window.input("core.exposure", cx);
        window.press("enter", cx);
    })
    .unwrap();
    // Input subscriptions are delivered when the outer app update flushes effects.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(editor.read(cx).session.editor.graph().nodes().len(), 1);
        assert!(
            window.find("parameters").bounds().size.width
                <= px(editor.read(cx).canvas_size.0 + 1.0),
            "inspector spills into the viewer"
        );
        let view = editor.read(cx);
        let state = view.flow_state.read(cx);
        let node = &state.nodes[0];
        let y = node.position.y * state.viewport.zoom + state.viewport.y;
        assert!(
            y >= 0.0
                && y + node.measured_height.unwrap_or(px(70.0)).as_f32() * state.viewport.zoom
                    <= view.canvas_size.1 + 1.0,
            "keyboard-created node is outside the visible workflow"
        );
        let before = editor.read(cx).document().unwrap();
        let history = editor.read(cx).undo.len();
        let width = window
            .find("node-content-node-0")
            .bounds()
            .size
            .width
            .as_f32();
        let zoom = editor.read(cx).flow_state.read(cx).viewport.zoom;
        let logical_width = editor.read(cx).flow_state.read(cx).nodes[0]
            .measured_width
            .unwrap()
            .as_f32();
        window.click("workflow-zoom-out", cx);
        window.render_frame(cx);
        let ratio = editor.read(cx).flow_state.read(cx).viewport.zoom / zoom;
        assert!(
            (window
                .find("node-content-node-0")
                .bounds()
                .size
                .width
                .as_f32()
                - width * ratio)
                .abs()
                < 1.0,
            "zoom moves nodes but does not scale their contents"
        );
        assert!(
            (editor.read(cx).flow_state.read(cx).nodes[0]
                .measured_width
                .unwrap()
                .as_f32()
                - logical_width)
                .abs()
                < 1.0 / editor.read(cx).flow_state.read(cx).viewport.zoom,
            "zoom changed world dimensions used by edges and Fit: before {logical_width}, after {}",
            editor.read(cx).flow_state.read(cx).nodes[0]
                .measured_width
                .unwrap()
                .as_f32()
        );
        window.click("fit-workflow", cx);
        assert_eq!(editor.read(cx).document().unwrap(), before);
        assert_eq!(editor.read(cx).undo.len(), history);
        editor.update(cx, |editor, cx| {
            editor.resize_workspace(0, -10000.0, window, cx)
        });
        assert_eq!(editor.read(cx).workspace.library_width, 180.0);
        editor.update(cx, |editor, cx| {
            editor.resize_workspace(1, 10000.0, window, cx)
        });
        assert_eq!(editor.read(cx).workspace.viewer_width, 720.0);
        editor.update(cx, |editor, cx| {
            editor.resize_workspace(1, -340.0, window, cx)
        });
        window.render_frame(cx);
        window.click("hide-library", cx);
        assert!(!editor.read(cx).workspace.library_open);
        window.press("ctrl-k", cx);
        assert!(editor.read(cx).workspace.library_open);
        assert!(
            editor
                .read(cx)
                .search
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
        window.click("hide-inspector", cx);
        assert!(!editor.read(cx).workspace.inspector_open);
        assert!(window.try_find("node-help").is_none());
        assert!(
            window.try_find("parameters").is_some(),
            "hiding help must not hide inline node controls"
        );
        editor.update(cx, |editor, cx| editor.toggle_panel(1, window, cx));
        window.render_frame(cx);
        assert!(window.try_find("viewer-controls-A").is_none());
        editor.update(cx, |editor, cx| editor.toggle_panel(1, window, cx));
        window.render_frame(cx);
        assert!(window.find("viewer-controls-A").visible());
        assert_eq!(editor.read(cx).document().unwrap(), before);
        assert_eq!(editor.read(cx).undo.len(), history);
        let viewport = editor.read(cx).flow_state.read(cx).viewport;
        editor.update(cx, |editor, cx| {
            editor.add_library_node("core.constant-float", Some((13.0, 17.0)), window, cx)
        });
        window.render_frame(cx);
        assert_eq!(
            editor.read(cx).flow_state.read(cx).viewport,
            viewport,
            "explicit drops must not recenter the canvas"
        );
        let focus = editor.read(cx).focus.clone();
        window.focus(&focus, cx);
        window.press("ctrl-a", cx);
        assert_eq!(
            editor
                .read(cx)
                .flow_state
                .read(cx)
                .nodes
                .iter()
                .filter(|node| node.selected)
                .count(),
            2,
            "select-all must work from editor focus, not only the canvas widget"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn wheel_zooms_at_cursor_without_modifiers_or_document_edits(cx: &mut TestAppContext) {
    let (handle, editor) = editor_window(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let before = editor.read(cx).document().unwrap();
        let viewport = editor.read(cx).flow_state.read(cx).viewport;
        let center = window.find("workflow-canvas").bounds().center();
        let state = editor.read(cx).flow_state.read(cx);
        let anchor = (
            (center.x.as_f32() - state.canvas_origin.x - viewport.x) / viewport.zoom,
            (center.y.as_f32() - state.canvas_origin.y - viewport.y) / viewport.zoom,
        );
        window.scroll(
            "workflow-canvas",
            gpui_kit::ScrollDelta::Lines(gpui_kit::point(0.0, 1.0)),
            cx,
        );
        let state = editor.read(cx).flow_state.read(cx);
        assert!(
            state.viewport.zoom > viewport.zoom,
            "plain wheel up must zoom in, not pan"
        );
        assert!(
            ((center.x.as_f32() - state.canvas_origin.x - state.viewport.x) / state.viewport.zoom
                - anchor.0)
                .abs()
                < 0.01
        );
        assert!(
            ((center.y.as_f32() - state.canvas_origin.y - state.viewport.y) / state.viewport.zoom
                - anchor.1)
                .abs()
                < 0.01
        );
        window.scroll(
            "workflow-canvas",
            gpui_kit::ScrollDelta::Lines(gpui_kit::point(0.0, -1.0)),
            cx,
        );
        assert!((editor.read(cx).flow_state.read(cx).viewport.zoom - viewport.zoom).abs() < 0.001);
        assert_eq!(editor.read(cx).document().unwrap(), before);
        assert!(editor.read(cx).undo.is_empty());
    })
    .unwrap();
}

#[gpui_kit::test]
fn connections_can_be_clicked_disconnected_and_undone_from_search_focus(cx: &mut TestAppContext) {
    let (handle, editor) = editor_window(cx);
    cx.update_window(handle, |_, window, cx| {
        editor.update(cx, |editor, cx| {
            editor.session.editor.reset_ordinary_image_graph().unwrap();
            editor.pending_canvas_fit = false;
            editor.rebuild_flow(cx);
        });
        window.render_frame(cx);
        window.render_frame(cx);
        let before = editor.read(cx).document().unwrap();
        editor.update(cx, |editor, cx| {
            editor
                .flow_state
                .update(cx, |state, _| state.viewport.zoom = 0.5);
            editor.flow.update(cx, |_, cx| cx.notify());
        });
        window.render_frame(cx);
        window.press("ctrl-k", cx);
        let state = editor.read(cx).flow_state.read(cx);
        let edge = &state.edges[0];
        let a = state
            .find_handle_center(
                &edge.source,
                &edge.source_handle,
                gpui_flow::HandlePosition::Right,
            )
            .unwrap();
        let b = state
            .find_handle_center(
                &edge.target,
                &edge.target_handle,
                gpui_flow::HandlePosition::Left,
            )
            .unwrap();
        let source = state.get_node(&edge.source).unwrap();
        let source_x = state.viewport.flow_to_screen(source.position).0 + state.canvas_origin.x;
        assert!(
            (a.0 - source_x - source.measured_width.unwrap().as_f32() * state.viewport.zoom).abs()
                < 0.01,
            "connection anchors must follow zoomed node borders"
        );
        let bounds = window.find("workflow-canvas").bounds();
        let offset = gpui_kit::point(
            px((a.0 + b.0) / 2.0) - bounds.origin.x,
            px((a.1 + b.1) / 2.0) - bounds.origin.y,
        );
        window.click_at("workflow-canvas", offset, cx);
        assert!(editor.read(cx).flow_state.read(cx).edges[0].selected);
        window.press("delete", cx);
        assert!(
            editor.read(cx).session.editor.graph().edges().is_empty(),
            "clicking a wire must leave text focus before Delete"
        );
        assert_eq!(editor.read(cx).session.editor.graph().nodes().len(), 2);
        assert_eq!(editor.read(cx).undo.len(), 1);
        editor.update(cx, |editor, cx| editor.history(false, window, cx));
        assert_eq!(editor.read(cx).document().unwrap(), before);
        window.render_frame(cx);
        window.click_at("workflow-canvas", offset, cx);
        window.click("disconnect-selection", cx);
        assert!(editor.read(cx).session.editor.graph().edges().is_empty());
        assert_eq!(editor.read(cx).session.editor.graph().nodes().len(), 2);
    })
    .unwrap();
}

#[gpui_kit::test]
fn port_drags_commit_immediately_in_both_directions_and_replace_input(cx: &mut TestAppContext) {
    let (handle, editor) = editor_window(cx);
    cx.update_window(handle, |_, window, cx| {
        editor.update(cx, |editor, cx| {
            editor.pending_canvas_fit = false;
            editor.add_library_node("core.image-input", Some((10.0, 10.0)), window, cx);
            editor.add_library_node("core.output", Some((290.0, 10.0)), window, cx);
            editor.add_library_node("core.image-input", Some((10.0, 220.0)), window, cx);
            editor.flow_state.update(cx, |state, _| {
                state.viewport = gpui_flow::Viewport::default()
            });
        });
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
    // Separate event turns: notifications from mouse-down cannot hide a missing release notification.
    let port = |id: &str, side, cx: &gpui_kit::App| {
        let center = editor
            .read(cx)
            .flow_state
            .read(cx)
            .find_handle_center(&id.to_owned().into(), &Some("image".into()), side)
            .unwrap();
        gpui_kit::point(px(center.0), px(center.1))
    };
    for (from, from_side, to, to_side) in [
        (
            "node-0",
            gpui_flow::HandlePosition::Right,
            "node-1",
            gpui_flow::HandlePosition::Left,
        ),
        (
            "node-1",
            gpui_flow::HandlePosition::Left,
            "node-2",
            gpui_flow::HandlePosition::Right,
        ),
    ] {
        cx.update_window(handle, |_, window, cx| {
            window.dispatch_event(
                gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: port(from, from_side, cx),
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            // No final mouse-move: release must compute snap at the actual pointer.
            window.dispatch_event(
                gpui_kit::MouseUpEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: port(to, to_side, cx),
                    modifiers: Default::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let edges = editor.read(cx).session.editor.graph().edges();
            assert_eq!(
                edges.len(),
                1,
                "release must sync immediately and replace an occupied input"
            );
            assert_eq!(
                edges[0].from_node.as_str(),
                if from == "node-0" { "node-0" } else { "node-2" }
            );
        })
        .unwrap();
    }
    cx.update_window(handle, |_, window, cx| {
        let history = editor.read(cx).undo.len();
        editor.update(cx, |editor, cx| editor.history(false, window, cx));
        assert_eq!(
            editor.read(cx).session.editor.graph().edges()[0]
                .from_node
                .as_str(),
            "node-0"
        );
        assert_eq!(editor.read(cx).undo.len(), history - 1);
    })
    .unwrap();
}

#[gpui_kit::test]
fn empty_canvas_arrival_block_and_footer_follow_graph_state(cx: &mut TestAppContext) {
    let (handle, editor) = editor_window(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
        let block = window.find("canvas-empty").bounds();
        let canvas = window.find("workflow-canvas").bounds();
        let center = canvas.center();
        assert!(
            (block.center().x - center.x).abs() < px(2.0)
                && (block.center().y - center.y).abs() < px(2.0),
            "arrival block must be centred in the plane: {block:?} in {canvas:?}"
        );
        // The controls share the plane's lower-left corner.
        let controls = window.find("workflow-controls").bounds();
        assert!(controls.left() > canvas.left() && controls.bottom() < canvas.bottom());
        assert!(
            canvas.intersects(&controls) && canvas.intersects(&block),
            "canvas chrome is drawn over the plane it acts on"
        );
        window.click("workflow-zoom-in", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("workflow-zoom-out", cx);
        window.render_frame(cx);
        window.click("fit-workflow", cx);
        window.render_frame(cx);
        // Adding a node retires the arrival block.
        editor.update(cx, |editor, cx| {
            editor.add_library_node("core.constant-float", Some((20.0, 20.0)), window, cx)
        });
        window.render_frame(cx);
        assert!(
            window.try_find("canvas-empty").is_none(),
            "the arrival block must not overlap a populated workflow"
        );
        assert!(editor.read(cx).document().is_ok());
    })
    .unwrap();
}

#[gpui_kit::test]
fn overview_click_centres_on_the_graph_and_ignores_its_own_window_offset(cx: &mut TestAppContext) {
    let (handle, editor) = editor_window(cx);
    cx.update_window(handle, |_, window, cx| {
        editor.update(cx, |editor, cx| {
            editor.pending_canvas_fit = false;
            editor.add_library_node("core.image-input", Some((0.0, 0.0)), window, cx);
            editor.add_library_node("core.output", Some((600.0, 320.0)), window, cx);
            editor.flow_state.update(cx, |state, _| {
                state.viewport = gpui_flow::Viewport::default();
                state.viewport.zoom = 1.0;
            });
        });
        window.render_frame(cx);
        window.render_frame(cx);
        let overview = window.find("workflow-overview").bounds();
        // A click on the minimap centre targets the centre of the graph.
        let center = overview.center();
        window.click_at(
            "workflow-overview",
            gpui_kit::point(center.x - overview.origin.x, center.y - overview.origin.y),
            cx,
        );
        window.render_frame(cx);
        let canvas = window.find("workflow-canvas").bounds();
        let state = editor.read(cx).flow_state.read(cx);
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        for node in &state.nodes {
            let width = node.measured_width.unwrap_or(px(200.0)).as_f32();
            let height = node.measured_height.unwrap_or(px(80.0)).as_f32();
            min_x = min_x.min(node.position.x);
            max_x = max_x.max(node.position.x + width);
            min_y = min_y.min(node.position.y);
            max_y = max_y.max(node.position.y + height);
        }
        let (cx_screen, cy_screen) = state.viewport.flow_to_screen(gpui_flow::FlowPoint::new(
            (min_x + max_x) / 2.0,
            (min_y + max_y) / 2.0,
        ));
        let centred = (
            cx_screen + state.canvas_origin.x,
            cy_screen + state.canvas_origin.y,
        );
        let center = canvas.center();
        assert!(
            (centred.0 - center.x.as_f32()).abs() < 2.0
                && (centred.1 - center.y.as_f32()).abs() < 2.0,
            "overview click must centre the graph under the cursor, got {centred:?} for {center:?}"
        );
        assert!(editor.read(cx).document().is_ok());
    })
    .unwrap();
}

#[gpui_kit::test]
fn library_rows_share_one_left_edge_regardless_of_label_length(cx: &mut TestAppContext) {
    let (handle, editor) = editor_window(cx);
    cx.update_window(handle, |_, window, cx| {
        editor.update(cx, |editor, cx| {
            editor.compatible_only = false;
            editor.collapsed_categories.clear();
            cx.notify();
        });
        window.render_frame(cx);
        window.render_frame(cx);
        let name = |kind: &str| {
            window
                .find(SharedString::from(format!("node-name-{kind}")))
                .bounds()
        };
        let library = window.find("library").bounds();
        // "Crop" and "Image Input" are far apart in width; a centred row would
        // start each of them at a different x.
        let crop = name("core.crop");
        let input = name("core.image-input");
        let output = name("core.output");
        assert!(
            (crop.left() - input.left()).abs() <= px(1.0)
                && (input.left() - output.left()).abs() <= px(1.0),
            "node rows must align to one left edge: crop {crop:?}, input {input:?}, output {output:?}"
        );
        assert!(
            crop.left() > library.left() && crop.left() - library.left() < px(40.0),
            "row text must sit just inside the dock, not float towards its centre: {crop:?} in {library:?}"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn outgoing_edges_anchor_to_the_source_output_not_the_nodes_own_input(cx: &mut TestAppContext) {
    let (handle, editor) = editor_window(cx);
    cx.update_window(handle, |_, window, cx| {
        editor.update(cx, |editor, cx| {
            editor.session.editor.reset_ordinary_image_graph().unwrap();
            editor
                .session
                .editor
                .add_node("exposure", "core.exposure")
                .unwrap();
            editor
                .session
                .editor
                .disconnect("input", "image", "output", "image")
                .unwrap();
            editor
                .session
                .editor
                .connect("input", "image", "exposure", "image")
                .unwrap();
            editor
                .session
                .editor
                .connect("exposure", "image", "output", "image")
                .unwrap();
            editor.pending_canvas_fit = false;
            editor.rebuild_flow(cx);
            editor.flow_state.update(cx, |state, _| {
                state.viewport = gpui_flow::Viewport::default()
            });
        });
        window.render_frame(cx);
        window.render_frame(cx);
        // Exposure names its input and output port "image", so the renderer must
        // resolve the outgoing anchor by direction, not by port id alone.
        let state = editor.read(cx).flow_state.read(cx);
        let edge = state
            .edges
            .iter()
            .find(|edge| edge.source == "exposure")
            .expect("exposure output edge");
        let position = state
            .handle_position(
                &edge.source,
                &edge.source_handle,
                gpui_flow::HandleType::Source,
            )
            .unwrap();
        assert_eq!(
            position,
            gpui_flow::HandlePosition::Right,
            "an outgoing edge must leave the output side"
        );
        let (x, _) = state
            .find_handle_center(&edge.source, &edge.source_handle, position)
            .unwrap();
        let node = state.get_node(&edge.source).unwrap();
        let left = node.position.x * state.viewport.zoom + state.viewport.x + state.canvas_origin.x;
        let width = node.measured_width.unwrap().as_f32() * state.viewport.zoom;
        assert!(
            (x - (left + width)).abs() < 1.0,
            "edge must start at the right border: {x} vs {}",
            left + width
        );
        let labels: Vec<_> = state
            .get_node(&edge.source)
            .unwrap()
            .handles
            .iter()
            .filter_map(|handle| handle.label.as_deref())
            .collect();
        assert!(
            labels.contains(&"Image out · Image"),
            "socket labels must state direction and data type: {labels:?}"
        );
        assert!(
            labels.contains(&"Image in · Image"),
            "an input and an output sharing a port name must still be distinguishable: {labels:?}"
        );
        assert_eq!(
            state.handle_position(
                &edge.target,
                &edge.target_handle,
                gpui_flow::HandleType::Target
            ),
            Some(gpui_flow::HandlePosition::Left)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn inline_curve_drag_edits_one_parameter_without_moving_node_and_undo_restores(
    cx: &mut TestAppContext,
) {
    let (handle, editor) = editor_window(cx);
    cx.update_window(handle, |_, window, cx| {
        editor.update(cx, |editor, cx| {
            editor.add_library_node("core.curve", Some((10.0, 10.0)), window, cx);
            editor
                .session
                .editor
                .set_node_parameter(
                    "node-0",
                    "points",
                    super::ParameterValue::String("0,0;0.5,0.5;1,1".into()),
                )
                .unwrap();
            editor.rebuild_fields(window, cx);
            editor.flow_state.update(cx, |state, _| {
                state.viewport = gpui_flow::Viewport::default();
            });
            editor.flow.update(cx, |_, cx| cx.notify());
        });
        window.render_frame(cx);
        window.render_frame(cx);
        let outer = window.find("node-content-node-0").bounds();
        let fields = window.find("parameters").bounds();
        assert!(
            fields.left() >= outer.left() && fields.right() <= outer.right() + px(1.0),
            "controls must fit the node width"
        );
        window.scroll(
            "inline-node-controls",
            gpui_kit::ScrollDelta::Lines(gpui_kit::point(0.0, -4.0)),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        let plot = window.find(("curve-plot", 0usize)).bounds();
        let center = plot.center();
        let original = editor.read(cx).document().unwrap();
        let position = editor.read(cx).flow_state.read(cx).nodes[0].position;
        let history = editor.read(cx).undo.len();
        window.drag(center, center - gpui_kit::point(px(0.0), px(20.0)), cx);
        assert_eq!(
            editor.read(cx).undo.len(),
            history + 1,
            "curve drag must be one undo step"
        );
        assert_ne!(editor.read(cx).document().unwrap(), original);
        assert_eq!(
            editor.read(cx).flow_state.read(cx).nodes[0].position,
            position,
            "curve gestures must not drag the node"
        );
        editor.update(cx, |editor, cx| editor.history(false, window, cx));
        assert_eq!(editor.read(cx).document().unwrap(), original);
    })
    .unwrap();
}

#[gpui_kit::test]
fn inline_crop_helper_applies_a_rectangle_atomically_and_keeps_ports_fixed(
    cx: &mut TestAppContext,
) {
    let (handle, editor) = editor_window(cx);
    cx.update_window(handle, |_, window, cx| {
        editor.update(cx, |editor, cx| {
            editor
                .session
                .attach(super::Source::Ordinary(
                    rawweave_image::Image::from_pixels(40, 20, vec![[0.5, 0.5, 0.5, 1.0]; 800])
                        .unwrap(),
                ))
                .unwrap();
            editor.session.editor.add_node("crop", "core.crop").unwrap();
            editor
                .session
                .editor
                .connect("input", "image", "crop", "image")
                .unwrap();
            editor.selected = Some("crop".into());
            editor.layout.insert("crop".into(), (0.0, 0.0));
            editor.pending_canvas_fit = false;
            editor.rebuild_fields(window, cx);
            editor.rebuild_flow(cx);
            editor.request_preview(window, cx);
            editor.flow_state.update(cx, |state, _| {
                state.viewport = gpui_flow::Viewport::default()
            });
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    let (before, history, port) = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
            let bounds = window.find("node-image-helper").bounds();
            let thumbnail = window.find("node-input-thumbnail").bounds();
            assert!(
                thumbnail.top() >= bounds.top() && thumbnail.bottom() <= bounds.bottom(),
                "thumbnail must not paint over helper controls"
            );
            assert!(thumbnail.left() >= bounds.left() && thumbnail.right() <= bounds.right());
            let a = bounds.center()
                - gpui_kit::point(bounds.size.width * 0.25, bounds.size.height * 0.1);
            let b = bounds.center()
                + gpui_kit::point(bounds.size.width * 0.25, bounds.size.height * 0.1);
            let state = editor.read(cx).flow_state.read(cx);
            let port = state
                .find_handle_center(
                    &"crop".into(),
                    &Some("image".into()),
                    gpui_flow::HandlePosition::Left,
                )
                .unwrap();
            let before = editor.read(cx).document().unwrap();
            let history = editor.read(cx).undo.len();
            window.drag(a, b, cx);
            window.render_frame(cx);
            (before, history, port)
        })
        .unwrap();
    // Deferred parent application runs after the helper releases its entity borrow.
    cx.update_window(handle, |_,window,cx| {
        window.render_frame(cx);
        assert_eq!(editor.read(cx).undo.len(),history+1);
        let node=editor.read(cx).session.editor.graph().node(&"crop".into()).unwrap();
        assert!(matches!(node.parameters.get("width"),Some(super::ParameterValue::Float(width)) if *width == 20.0));
        assert_ne!(editor.read(cx).document().unwrap(),before);
        let state=editor.read(cx).flow_state.read(cx);
        assert_eq!(state.find_handle_center(&"crop".into(),&Some("image".into()),gpui_flow::HandlePosition::Left).unwrap(),port,"ports must not move when inline controls resize");
        editor.update(cx,|editor,cx|editor.history(false,window,cx));
        assert_eq!(editor.read(cx).document().unwrap(),before);
    }).unwrap();
}

#[gpui_kit::test]
fn library_groups_collapse_and_search_reveals_matches(cx: &mut TestAppContext) {
    let (handle, editor) = editor_window(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("collapse-node-groups", cx);
        assert!(window.try_find("core.image-input").is_none());
        window.press("ctrl-k", cx);
        window.input("core.image-input", cx);
        window.render_frame(cx);
        assert!(window.find("core.image-input").visible());
        assert!(editor.read(cx).session.editor.graph().nodes().is_empty());
        window.click("expand-node-groups", cx);
        assert!(editor.read(cx).collapsed_categories.is_empty());
    })
    .unwrap();
}
