//! Headless GPUI layout/input regressions, not GPU pixel or OS-portal tests.
use super::{Editor, Workspace};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Focusable, Point, TestAppContext, WindowBounds, WindowOptions, px, size,
};

#[gpui_kit::test]
fn compact_workflow_keeps_controls_separate_and_dock_actions_out_of_history(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    cx.update(super::configure_theme);
    let (handle, editor) = cx.update(|cx| {
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
    });
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
        assert!(window.try_find("parameters").is_none());
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
