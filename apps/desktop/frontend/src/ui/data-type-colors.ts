/**
 * Colours shared by node handles and edges so a data type is recognisable at a
 * glance. Unknown types fall back to a neutral slate.
 */
const TYPE_COLORS: Record<string, string> = {
  'core.Image': '#6ee7c7',
  'core.Mask': '#ffd479',
  'core.MaskSet': '#ffb066',
  'core.LabelMap': '#7cde9c',
  'core.ConfidenceMap': '#c58cff',
  'core.DepthMap': '#58c6aa',
  'core.RegionSet': '#ef737d',
  'core.Any': '#c9d5e1',
  'color.DisplayRGB': '#7eafff',
  'color.SceneLinearRGB': '#9d8cff',
  'value.Float': '#8fd3ff',
  'value.Integer': '#8fd3ff',
  'value.Boolean': '#f0a3c8',
  'value.Condition': '#f0a3c8',
  'value.String': '#c9d5e1',
};

export const NEUTRAL_TYPE_COLOR = '#5b6b7d';

export function dataTypeColor(dataType?: string | null): string {
  if (!dataType) return NEUTRAL_TYPE_COLOR;
  return TYPE_COLORS[dataType] ?? NEUTRAL_TYPE_COLOR;
}
