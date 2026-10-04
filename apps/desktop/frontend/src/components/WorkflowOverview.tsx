import { MiniMap, useStore } from '@xyflow/react';

export function WorkflowOverview() {
  const aspect = useStore((state) => state.width > 0 && state.height > 0
    ? state.width / state.height
    : 148 / 100);

  // MiniMap uses these dimensions for its SVG and pointer coordinates too;
  // sizing only its CSS container leaves the default 200×150 SVG overflowing.
  return <MiniMap pannable zoomable style={{
    width: Math.min(148, 100 * aspect),
    height: Math.min(100, 148 / aspect),
  }} />;
}
