use std::collections::HashMap;

use gpui::{Bounds, Pixels, SharedString};

use crate::types::*;

/// Resolved handle position in screen coordinates, stored after layout.
#[derive(Debug, Clone)]
pub struct ResolvedHandle {
    pub node_id: NodeId,
    pub handle_id: Option<SharedString>,
    pub handle_type: HandleType,
    pub position: HandlePosition,
    pub bounds: Bounds<Pixels>,
}

/// Key for looking up a handle: (node_id, handle_id).
pub type HandleKey = (NodeId, Option<SharedString>);

/// A snapshot of the graph state for undo/redo.
#[derive(Clone)]
struct HistoryEntry {
    nodes: Vec<FlowNode>,
    edges: Vec<FlowEdge>,
}

/// The central state store for a flow graph. Wrapped in `Entity<FlowState>`.
pub struct FlowState {
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
    node_lookup: HashMap<NodeId, usize>,
    pub viewport: Viewport,
    /// Global origin of the embedded canvas; node positions remain local.
    pub canvas_origin: FlowPoint,
    pub handle_bounds: HashMap<HandleKey, ResolvedHandle>,
    // Interaction state
    pub drag_state: Option<DragState>,
    pub pan_drag: Option<PanDragState>,
    pub connecting: Option<ConnectionDraft>,
    pub selection_box: Option<SelectionBox>,
    // Undo/redo
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    // Config
    pub min_zoom: f32,
    pub max_zoom: f32,
    pub snap_to_grid: bool,
    pub snap_grid: (f32, f32),
    pub connection_radius: f32,
}

impl FlowState {
    pub fn new(nodes: Vec<FlowNode>, edges: Vec<FlowEdge>) -> Self {
        let mut node_lookup = HashMap::new();
        for (i, node) in nodes.iter().enumerate() {
            node_lookup.insert(node.id.clone(), i);
        }

        Self {
            nodes,
            edges,
            node_lookup,
            viewport: Viewport::default(),
            canvas_origin: FlowPoint::new(0.0, 0.0),
            handle_bounds: HashMap::new(),
            drag_state: None,
            pan_drag: None,
            connecting: None,
            selection_box: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            min_zoom: 0.8,
            max_zoom: 2.5,
            snap_to_grid: false,
            snap_grid: (20.0, 20.0),
            connection_radius: 20.0,
        }
    }

    /// Rebuild the node lookup index.
    pub fn rebuild_lookup(&mut self) {
        self.node_lookup.clear();
        for (i, node) in self.nodes.iter().enumerate() {
            self.node_lookup.insert(node.id.clone(), i);
        }
    }

    /// Push the current state onto the undo stack. Call before destructive operations.
    pub fn push_undo(&mut self) {
        self.undo_stack.push(HistoryEntry {
            nodes: self.nodes.clone(),
            edges: self.edges.clone(),
        });
        // Clear redo stack on new action
        self.redo_stack.clear();
        // Limit history size
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
    }

    /// Undo the last operation. Returns true if undo was performed.
    pub fn undo(&mut self) -> bool {
        if let Some(entry) = self.undo_stack.pop() {
            self.redo_stack.push(HistoryEntry {
                nodes: self.nodes.clone(),
                edges: self.edges.clone(),
            });
            self.nodes = entry.nodes;
            self.edges = entry.edges;
            self.rebuild_lookup();
            true
        } else {
            false
        }
    }

    /// Redo the last undone operation. Returns true if redo was performed.
    pub fn redo(&mut self) -> bool {
        if let Some(entry) = self.redo_stack.pop() {
            self.undo_stack.push(HistoryEntry {
                nodes: self.nodes.clone(),
                edges: self.edges.clone(),
            });
            self.nodes = entry.nodes;
            self.edges = entry.edges;
            self.rebuild_lookup();
            true
        } else {
            false
        }
    }

    /// Get a node by ID.
    pub fn get_node(&self, id: &NodeId) -> Option<&FlowNode> {
        self.node_lookup.get(id).map(|&i| &self.nodes[i])
    }

    /// Get a mutable node by ID.
    pub fn get_node_mut(&mut self, id: &NodeId) -> Option<&mut FlowNode> {
        self.node_lookup.get(id).map(|&i| &mut self.nodes[i])
    }

    /// Set all nodes, rebuilding the index.
    pub fn set_nodes(&mut self, nodes: Vec<FlowNode>) {
        self.nodes = nodes;
        self.rebuild_lookup();
    }

    /// Set all edges.
    pub fn set_edges(&mut self, edges: Vec<FlowEdge>) {
        self.edges = edges;
    }

    /// Apply a batch of node changes.
    pub fn apply_node_changes(&mut self, changes: &[NodeChange]) {
        let mut needs_rebuild = false;

        for change in changes {
            match change {
                NodeChange::Position {
                    id,
                    position,
                    dragging,
                } => {
                    if let Some(node) = self.get_node_mut(id) {
                        node.position = *position;
                        node.dragging = *dragging;
                    }
                }
                NodeChange::Dimensions { id, width, height } => {
                    if let Some(node) = self.get_node_mut(id) {
                        node.measured_width = Some(*width);
                        node.measured_height = Some(*height);
                    }
                }
                NodeChange::Select { id, selected } => {
                    if let Some(node) = self.get_node_mut(id) {
                        node.selected = *selected;
                    }
                }
                NodeChange::Remove { id } => {
                    if let Some(&idx) = self.node_lookup.get(id) {
                        self.nodes.remove(idx);
                        needs_rebuild = true;
                    }
                }
            }
        }

        if needs_rebuild {
            self.rebuild_lookup();
        }
    }

    /// Apply a batch of edge changes.
    pub fn apply_edge_changes(&mut self, changes: &[EdgeChange]) {
        for change in changes {
            match change {
                EdgeChange::Select { id, selected } => {
                    if let Some(edge) = self.edges.iter_mut().find(|e| e.id == *id) {
                        edge.selected = *selected;
                    }
                }
                EdgeChange::Remove { id } => {
                    self.edges.retain(|e| e.id != *id);
                }
            }
        }
    }

    /// Set the viewport, clamping zoom to min/max.
    pub fn set_viewport(&mut self, viewport: Viewport) {
        self.viewport = Viewport {
            x: viewport.x,
            y: viewport.y,
            zoom: viewport.zoom.clamp(self.min_zoom, self.max_zoom),
        };
    }

    /// Register a resolved handle position (called during layout).
    pub fn set_handle_bounds(&mut self, key: HandleKey, resolved: ResolvedHandle) {
        self.handle_bounds.insert(key, resolved);
    }

    /// Find the screen-space center of a handle.
    ///
    /// Computes the position directly from the node's flow coordinates + viewport,
    /// using measured wrapper dimensions when available.
    pub fn find_handle_center(
        &self,
        node_id: &NodeId,
        handle_id: &Option<SharedString>,
        handle_position: HandlePosition,
    ) -> Option<(f32, f32)> {
        let node = self.get_node(node_id)?;
        let (sx, sy) = self.viewport.flow_to_screen(node.position);
        let (sx, sy) = (sx + self.canvas_origin.x, sy + self.canvas_origin.y);
        let side: Vec<_> = node
            .handles
            .iter()
            .filter(|h| h.position == handle_position)
            .collect();
        let index = side.iter().position(|h| &h.id == handle_id).unwrap_or(0);
        let fraction = (index + 1) as f32 / (side.len() + 1).max(2) as f32;

        // Use measured wrapper dimensions if available, otherwise estimate.
        // measured_width/height store the full wrapper size (set by the measurement canvas).
        let w = node.measured_width.map(|p| p.as_f32()).unwrap_or(114.0);
        let h = node.measured_height.map(|p| p.as_f32()).unwrap_or(54.0);

        let (cx, cy) = match handle_position {
            HandlePosition::Top => (sx + w * fraction, sy),
            HandlePosition::Bottom => (sx + w * fraction, sy + h),
            HandlePosition::Left => (sx, sy + h * fraction),
            HandlePosition::Right => (sx + w, sy + h * fraction),
        };
        Some((cx, cy))
    }

    /// Fit the viewport to show all nodes with padding.
    pub fn fit_view(&mut self, padding: f32, container_width: f32, container_height: f32) {
        if self.nodes.is_empty() {
            return;
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for node in &self.nodes {
            if node.hidden {
                continue;
            }
            let w = node.measured_width.map(|p| p.as_f32()).unwrap_or(150.0);
            let h = node.measured_height.map(|p| p.as_f32()).unwrap_or(40.0);
            min_x = min_x.min(node.position.x);
            min_y = min_y.min(node.position.y);
            max_x = max_x.max(node.position.x + w);
            max_y = max_y.max(node.position.y + h);
        }

        let content_width = max_x - min_x + padding * 2.0;
        let content_height = max_y - min_y + padding * 2.0;

        if content_width <= 0.0 || content_height <= 0.0 {
            return;
        }

        let zoom_x = container_width / content_width;
        let zoom_y = container_height / content_height;
        let zoom = zoom_x.min(zoom_y).clamp(self.min_zoom, self.max_zoom);

        self.viewport.zoom = zoom;
        self.viewport.x = (container_width - content_width * zoom) / 2.0 - (min_x - padding) * zoom;
        self.viewport.y =
            (container_height - content_height * zoom) / 2.0 - (min_y - padding) * zoom;
    }

    /// Zoom in by a step, centered on the container center.
    pub fn zoom_in(&mut self, container_width: f32, container_height: f32) {
        let factor = 1.2;
        let cx = container_width / 2.0;
        let cy = container_height / 2.0;
        let old_zoom = self.viewport.zoom;
        let new_zoom = (old_zoom * factor).clamp(self.min_zoom, self.max_zoom);
        self.viewport.x = cx - (cx - self.viewport.x) * (new_zoom / old_zoom);
        self.viewport.y = cy - (cy - self.viewport.y) * (new_zoom / old_zoom);
        self.viewport.zoom = new_zoom;
    }

    /// Zoom out by a step, centered on the container center.
    pub fn zoom_out(&mut self, container_width: f32, container_height: f32) {
        let factor = 1.0 / 1.2;
        let cx = container_width / 2.0;
        let cy = container_height / 2.0;
        let old_zoom = self.viewport.zoom;
        let new_zoom = (old_zoom * factor).clamp(self.min_zoom, self.max_zoom);
        self.viewport.x = cx - (cx - self.viewport.x) * (new_zoom / old_zoom);
        self.viewport.y = cy - (cy - self.viewport.y) * (new_zoom / old_zoom);
        self.viewport.zoom = new_zoom;
    }

    /// Center the viewport on a specific flow coordinate.
    pub fn set_center(
        &mut self,
        flow_x: f32,
        flow_y: f32,
        container_width: f32,
        container_height: f32,
    ) {
        self.viewport.x = container_width / 2.0 - flow_x * self.viewport.zoom;
        self.viewport.y = container_height / 2.0 - flow_y * self.viewport.zoom;
    }

    /// Select all nodes and edges.
    pub fn select_all(&mut self) {
        for node in &mut self.nodes {
            if node.selectable {
                node.selected = true;
            }
        }
        for edge in &mut self.edges {
            if edge.selectable {
                edge.selected = true;
            }
        }
    }

    /// Get nodes that have edges pointing TO the given node.
    pub fn get_incomers(&self, node_id: &NodeId) -> Vec<&FlowNode> {
        self.edges
            .iter()
            .filter(|e| e.target == *node_id)
            .filter_map(|e| self.get_node(&e.source))
            .collect()
    }

    /// Get nodes that the given node has edges pointing TO.
    pub fn get_outgoers(&self, node_id: &NodeId) -> Vec<&FlowNode> {
        self.edges
            .iter()
            .filter(|e| e.source == *node_id)
            .filter_map(|e| self.get_node(&e.target))
            .collect()
    }

    /// Get all edges connected to a node (as source or target).
    pub fn get_connected_edges(&self, node_id: &NodeId) -> Vec<&FlowEdge> {
        self.edges
            .iter()
            .filter(|e| e.source == *node_id || e.target == *node_id)
            .collect()
    }

    /// Find the nearest valid handle to snap to during connection dragging.
    pub fn find_snap_target(
        &self,
        draft: &ConnectionDraft,
        mouse_x: f32,
        mouse_y: f32,
    ) -> Option<SnapTarget> {
        let radius = self.connection_radius;
        let mut best: Option<(f32, SnapTarget)> = None;

        for node in &self.nodes {
            if node.hidden {
                continue;
            }
            // Can't connect to the same node
            if node.id == draft.from_node {
                continue;
            }

            for handle in &node.handles {
                // Source→Target or Target→Source only
                if handle.handle_type == draft.from_type {
                    continue;
                }
                // Check per-handle connectable flag
                if !handle.is_connectable {
                    continue;
                }

                if let Some((hx, hy)) =
                    self.find_handle_center(&node.id, &handle.id, handle.position)
                {
                    let dx = mouse_x - hx;
                    let dy = mouse_y - hy;
                    let dist = (dx * dx + dy * dy).sqrt();

                    if dist <= radius {
                        if best.is_none() || dist < best.as_ref().unwrap().0 {
                            best = Some((
                                dist,
                                SnapTarget {
                                    node_id: node.id.clone(),
                                    handle_id: handle.id.clone(),
                                    handle_type: handle.handle_type,
                                    position: handle.position,
                                    point: (hx, hy),
                                },
                            ));
                        }
                    }
                }
            }
        }

        best.map(|(_, target)| target)
    }

    /// Check if a connection is valid (default rules).
    pub fn is_valid_connection(&self, connection: &Connection) -> bool {
        // No self-connections
        if connection.source == connection.target {
            return false;
        }

        // No duplicate edges
        let duplicate = self.edges.iter().any(|e| {
            e.source == connection.source
                && e.target == connection.target
                && e.source_handle == connection.source_handle
                && e.target_handle == connection.target_handle
        });
        if duplicate {
            return false;
        }

        true
    }

    /// Add an edge from a completed connection.
    pub fn add_edge_from_connection(
        &mut self,
        connection: &Connection,
        edge_id: impl Into<SharedString>,
    ) {
        let mut edge = FlowEdge::new(
            edge_id,
            connection.source.clone(),
            connection.target.clone(),
        );
        if let Some(ref sh) = connection.source_handle {
            edge.source_handle = Some(sh.clone());
        }
        if let Some(ref th) = connection.target_handle {
            edge.target_handle = Some(th.clone());
        }
        self.edges.push(edge);
    }
}
