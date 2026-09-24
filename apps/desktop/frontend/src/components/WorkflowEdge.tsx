import {
  BaseEdge,
  EdgeLabelRenderer,
  getSmoothStepPath,
  type Edge,
  type EdgeProps,
} from '@xyflow/react';
import { dataTypeColor } from '../ui/data-type-colors';

export interface WorkflowEdgeData extends Record<string, unknown> {
  dataType?: string | null;
  /** True while the edge is hovered, so the disconnect action is reachable. */
  actionsVisible?: boolean;
  onDisconnect?: (edgeId: string) => void;
}

export type WorkflowFlowEdge = Edge<WorkflowEdgeData, 'rawweave'>;

/**
 * Edge with a built-in disconnect action.
 *
 * Following the React Flow guidance, the interactive control lives in
 * `EdgeLabelRenderer` (an HTML portal) rather than inside the SVG path, so it
 * stays keyboard reachable and styled like the rest of the UI.
 */
export function WorkflowEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  selected,
  data,
  markerEnd,
}: EdgeProps<WorkflowFlowEdge>) {
  const [path, labelX, labelY] = getSmoothStepPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
    borderRadius: 14,
  });
  const color = dataTypeColor(data?.dataType);
  const visible = Boolean(selected) || Boolean(data?.actionsVisible);

  return (
    <>
      <BaseEdge
        id={id}
        markerEnd={markerEnd}
        path={path}
        style={{
          stroke: selected ? '#8cebd3' : color,
          strokeWidth: selected ? 2.4 : 1.6,
        }}
      />
      <EdgeLabelRenderer>
        <button
          aria-label={`Disconnect ${id}`}
          className={`edge-disconnect${visible ? ' is-visible' : ''}`}
          onClick={(event) => {
            event.stopPropagation();
            data?.onDisconnect?.(id);
          }}
          style={{ transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)` }}
          title="Disconnect"
          type="button"
        >
          ✕
        </button>
      </EdgeLabelRenderer>
    </>
  );
}
