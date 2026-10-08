pub mod bezier;
pub mod smooth_step;
pub mod straight;

use gpui::{Background, PathBuilder, Point, Window, px};

use crate::store::FlowState;
use crate::types::*;

use self::bezier::get_bezier_path;
use self::smooth_step::get_smooth_step_path;
use self::straight::get_straight_path;

const ARROW_SIZE: f32 = 6.0;

/// Paint all edges for the flow graph.
pub fn paint_edges(state: &FlowState, window: &mut Window) {
    // Viewport culling bounds
    let win_size = window.viewport_size();
    let win_w = win_size.width.as_f32();
    let win_h = win_size.height.as_f32();
    let margin = 100.0;
    let edge_color: Background = gpui::rgb(0xb1b1b7).into();
    let selected_color: Background = gpui::rgb(0x3b82f6).into();

    for edge in &state.edges {
        if edge.hidden {
            continue;
        }

        // Compute handle positions and screen coordinates inline
        let Some(source_handle_pos) =
            state.handle_position(&edge.source, &edge.source_handle, HandleType::Source)
        else {
            continue;
        };
        let Some(target_handle_pos) =
            state.handle_position(&edge.target, &edge.target_handle, HandleType::Target)
        else {
            continue;
        };

        let Some((sx, sy)) =
            state.find_handle_center(&edge.source, &edge.source_handle, source_handle_pos)
        else {
            continue;
        };
        let Some((tx, ty)) =
            state.find_handle_center(&edge.target, &edge.target_handle, target_handle_pos)
        else {
            continue;
        };

        // Cull edges where both endpoints are off-screen
        let both_off_x =
            (sx < -margin && tx < -margin) || (sx > win_w + margin && tx > win_w + margin);
        let both_off_y =
            (sy < -margin && ty < -margin) || (sy > win_h + margin && ty > win_h + margin);
        if both_off_x || both_off_y {
            continue;
        }

        let color: Background = if edge.selected {
            selected_color.clone()
        } else if let Some(c) = edge.color {
            gpui::rgb(c).into()
        } else {
            edge_color.clone()
        };
        let stroke = if edge.selected {
            edge.stroke_width.unwrap_or(2.0).max(3.0)
        } else {
            edge.stroke_width.unwrap_or(2.0)
        };

        match edge.edge_type {
            EdgeType::Bezier { curvature } => {
                let bezier = get_bezier_path(
                    sx,
                    sy,
                    source_handle_pos,
                    tx,
                    ty,
                    target_handle_pos,
                    curvature,
                );

                let mut builder = PathBuilder::stroke(px(stroke));
                builder.move_to(Point::new(px(bezier.source.0), px(bezier.source.1)));
                builder.cubic_bezier_to(
                    Point::new(px(bezier.target.0), px(bezier.target.1)),
                    Point::new(px(bezier.source_control.0), px(bezier.source_control.1)),
                    Point::new(px(bezier.target_control.0), px(bezier.target_control.1)),
                );

                if let Ok(path) = builder.build() {
                    window.paint_path(path, color.clone());
                }

                // Draw arrowhead at target
                paint_arrowhead(
                    window,
                    bezier.target.0,
                    bezier.target.1,
                    bezier.target_control.0,
                    bezier.target_control.1,
                    color,
                );
            }
            EdgeType::Straight => {
                let straight = get_straight_path(sx, sy, tx, ty);

                let mut builder = PathBuilder::stroke(px(stroke));
                builder.move_to(Point::new(px(straight.source.0), px(straight.source.1)));
                builder.line_to(Point::new(px(straight.target.0), px(straight.target.1)));

                if let Ok(path) = builder.build() {
                    window.paint_path(path, color.clone());
                }

                paint_arrowhead(
                    window,
                    straight.target.0,
                    straight.target.1,
                    straight.source.0,
                    straight.source.1,
                    color,
                );
            }
            EdgeType::SmoothStep {
                border_radius: _,
                offset,
            } => {
                let step = get_smooth_step_path(
                    sx,
                    sy,
                    source_handle_pos,
                    tx,
                    ty,
                    target_handle_pos,
                    offset,
                );

                if step.points.len() < 2 {
                    continue;
                }

                let n = step.points.len();
                let last = step.points[n - 1];
                let prev = step.points[n - 2];

                let mut builder = PathBuilder::stroke(px(stroke));
                builder.move_to(Point::new(px(step.points[0].0), px(step.points[0].1)));
                for i in 1..n {
                    builder.line_to(Point::new(px(step.points[i].0), px(step.points[i].1)));
                }

                if let Ok(path) = builder.build() {
                    window.paint_path(path, color.clone());
                }

                paint_arrowhead(window, last.0, last.1, prev.0, prev.1, color);
            }
        }
    }
}

/// Paint a filled triangle arrowhead pointing from (from_x, from_y) toward (tip_x, tip_y).
fn paint_arrowhead(
    window: &mut Window,
    tip_x: f32,
    tip_y: f32,
    from_x: f32,
    from_y: f32,
    color: Background,
) {
    let dx = tip_x - from_x;
    let dy = tip_y - from_y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return;
    }

    // Unit vector along the arrow direction
    let ux = dx / len;
    let uy = dy / len;

    // Perpendicular
    let px = -uy;
    let py = ux;

    let half_width = ARROW_SIZE * 0.5;

    // Base points of the triangle
    let base_x = tip_x - ux * ARROW_SIZE;
    let base_y = tip_y - uy * ARROW_SIZE;

    let p1_x = base_x + px * half_width;
    let p1_y = base_y + py * half_width;
    let p2_x = base_x - px * half_width;
    let p2_y = base_y - py * half_width;

    let mut builder = PathBuilder::fill();
    builder.move_to(Point::new(gpui::px(tip_x), gpui::px(tip_y)));
    builder.line_to(Point::new(gpui::px(p1_x), gpui::px(p1_y)));
    builder.line_to(Point::new(gpui::px(p2_x), gpui::px(p2_y)));
    builder.line_to(Point::new(gpui::px(tip_x), gpui::px(tip_y)));

    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

/// Compute the label position (midpoint) for an edge.
pub fn compute_edge_label_position(state: &FlowState, edge: &FlowEdge) -> Option<(f32, f32)> {
    let source_handle_pos =
        state.handle_position(&edge.source, &edge.source_handle, HandleType::Source)?;
    let target_handle_pos =
        state.handle_position(&edge.target, &edge.target_handle, HandleType::Target)?;
    let (sx, sy) =
        state.find_handle_center(&edge.source, &edge.source_handle, source_handle_pos)?;
    let (tx, ty) =
        state.find_handle_center(&edge.target, &edge.target_handle, target_handle_pos)?;

    match edge.edge_type {
        EdgeType::Bezier { curvature } => {
            let bezier = get_bezier_path(
                sx,
                sy,
                source_handle_pos,
                tx,
                ty,
                target_handle_pos,
                curvature,
            );
            Some((bezier.label_x, bezier.label_y))
        }
        EdgeType::Straight | EdgeType::SmoothStep { .. } => {
            let straight = get_straight_path(sx, sy, tx, ty);
            Some((straight.label_x, straight.label_y))
        }
    }
}

/// Hit test all edges — return the ID of the first edge within `threshold` pixels of (mx, my).
pub fn hit_test_edges(state: &FlowState, mx: f32, my: f32, threshold: f32) -> Option<EdgeId> {
    for edge in &state.edges {
        if edge.hidden {
            continue;
        }

        let source_handle_pos =
            state.handle_position(&edge.source, &edge.source_handle, HandleType::Source);
        let target_handle_pos =
            state.handle_position(&edge.target, &edge.target_handle, HandleType::Target);
        let (Some(source_handle_pos), Some(target_handle_pos)) =
            (source_handle_pos, target_handle_pos)
        else {
            continue;
        };

        let (sx, sy) =
            match state.find_handle_center(&edge.source, &edge.source_handle, source_handle_pos) {
                Some(pos) => pos,
                None => continue,
            };
        let (tx, ty) =
            match state.find_handle_center(&edge.target, &edge.target_handle, target_handle_pos) {
                Some(pos) => pos,
                None => continue,
            };

        let dist = match edge.edge_type {
            EdgeType::Bezier { curvature } => {
                let bezier = get_bezier_path(
                    sx,
                    sy,
                    source_handle_pos,
                    tx,
                    ty,
                    target_handle_pos,
                    curvature,
                );
                point_to_cubic_bezier_distance(
                    mx,
                    my,
                    bezier.source.0,
                    bezier.source.1,
                    bezier.source_control.0,
                    bezier.source_control.1,
                    bezier.target_control.0,
                    bezier.target_control.1,
                    bezier.target.0,
                    bezier.target.1,
                    20,
                )
            }
            EdgeType::Straight | EdgeType::SmoothStep { .. } => {
                point_to_segment_distance(mx, my, sx, sy, tx, ty)
            }
        };

        if dist <= threshold {
            return Some(edge.id.clone());
        }
    }
    None
}

/// Distance from point to line segment.
fn point_to_segment_distance(px: f32, py: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 0.001 {
        return ((px - x1).powi(2) + (py - y1).powi(2)).sqrt();
    }
    let t = ((px - x1) * dx + (py - y1) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let proj_x = x1 + t * dx;
    let proj_y = y1 + t * dy;
    ((px - proj_x).powi(2) + (py - proj_y).powi(2)).sqrt()
}

/// Approximate distance from point to cubic bezier by sampling.
fn point_to_cubic_bezier_distance(
    px: f32,
    py: f32,
    x0: f32,
    y0: f32,
    cx0: f32,
    cy0: f32,
    cx1: f32,
    cy1: f32,
    x1: f32,
    y1: f32,
    samples: usize,
) -> f32 {
    let mut min_dist = f32::MAX;
    for i in 0..=samples {
        let t = i as f32 / samples as f32;
        let u = 1.0 - t;
        let bx = u * u * u * x0 + 3.0 * u * u * t * cx0 + 3.0 * u * t * t * cx1 + t * t * t * x1;
        let by = u * u * u * y0 + 3.0 * u * u * t * cy0 + 3.0 * u * t * t * cy1 + t * t * t * y1;
        let dist = ((px - bx).powi(2) + (py - by).powi(2)).sqrt();
        if dist < min_dist {
            min_dist = dist;
        }
    }
    min_dist
}
