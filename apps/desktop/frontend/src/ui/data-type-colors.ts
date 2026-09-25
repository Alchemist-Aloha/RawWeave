/**
 * Colours shared by node handles and edges so a data type is recognisable at a
 * glance.
 *
 * A port is marked in wax, like everything else you can point at in this app, so
 * the palette is the four waxes and nothing else: a family gets a wax, members of
 * a family get a tone of it. Ten arbitrary hues told you nothing about what a wire
 * carried; four families plus the port's own label do, and they stay inside the
 * rule that colour here always means one thing.
 *
 * These values cannot come from the stylesheet: React Flow paints handles and
 * edges inline, and a handle has to read on both the lit bench and the dark
 * plane. They are therefore the mid-tone waxes, chosen to hold on either cast.
 */
const WAX = {
  /** The image itself: what the graph is for. */
  image: '#f2efe6',
  /** Spatial data derived from the image: masks, maps, regions. */
  spatial: '#e8a33d',
  /** Colour encodings: what space a signal is in. */
  colour: '#7ba0e8',
  /** Scalars and logic: what drives a parameter. */
  value: '#b8b0a0',
} as const;

const TYPE_COLORS: Record<string, string> = {
  'core.Image': WAX.image,
  'core.Mask': WAX.spatial,
  'core.MaskSet': '#d98a2b',
  'core.LabelMap': '#c98f3a',
  'core.ConfidenceMap': '#f0c072',
  'core.DepthMap': '#8a6a2a',
  'core.RegionSet': '#b3701a',
  'core.Any': WAX.value,
  'color.DisplayRGB': WAX.colour,
  'color.SceneLinearRGB': '#a3bdf0',
  'value.Float': '#cdc5b4',
  'value.Integer': '#cdc5b4',
  'value.Boolean': '#9a927f',
  'value.Condition': '#9a927f',
  'value.String': WAX.value,
};

export const NEUTRAL_TYPE_COLOR = '#8d8676';

export function dataTypeColor(dataType?: string | null): string {
  if (!dataType) return NEUTRAL_TYPE_COLOR;
  return TYPE_COLORS[dataType] ?? NEUTRAL_TYPE_COLOR;
}
