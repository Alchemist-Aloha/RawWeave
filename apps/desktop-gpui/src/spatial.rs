//! Presentation of spatial graph values; never alters their processing data.
use crate::PreviewFrame;
use rawweave_image::{Dimensions, Region};
use rawweave_node_api::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MaskDisplay {
    #[default]
    Grayscale,
    Overlay,
}

// Same categorical data palette as the Tauri spatial viewer, not UI-state colors.
const COLORS: [[f32; 3]; 8] = [
    [0.12, 0.42, 0.78],
    [0.92, 0.32, 0.18],
    [0.22, 0.68, 0.38],
    [0.72, 0.32, 0.78],
    [0.88, 0.66, 0.16],
    [0.14, 0.68, 0.72],
    [0.78, 0.24, 0.46],
    [0.46, 0.52, 0.2],
];

fn union(regions: impl IntoIterator<Item = Region>) -> Result<Option<Region>, String> {
    let mut bounds: Option<Region> = None;
    for region in regions {
        let right = region.end_x().ok_or("Spatial x extent overflow")?;
        let bottom = region.end_y().ok_or("Spatial y extent overflow")?;
        if region.width == 0 || region.height == 0 {
            continue;
        }
        bounds = Some(match bounds {
            None => region,
            Some(previous) => {
                let x = previous.x.min(region.x);
                let y = previous.y.min(region.y);
                Region::new(
                    x,
                    y,
                    previous
                        .end_x()
                        .ok_or("Spatial x extent overflow")?
                        .max(right)
                        - x,
                    previous
                        .end_y()
                        .ok_or("Spatial y extent overflow")?
                        .max(bottom)
                        - y,
                )
            }
        });
    }
    Ok(bounds)
}

pub fn spatial_frame(value: &Value, mip: u8, display: MaskDisplay) -> Result<PreviewFrame, String> {
    if mip > 6 {
        return Err("Invalid spatial preview mip".into());
    }
    let bounds = match value {
        Value::Mask(mask) => mask.global_region(),
        Value::ConfidenceMap(map) => map.global_region(),
        Value::LabelMap(map) => map.global_region(),
        Value::DepthMap(map) => map.global_region(),
        Value::MaskSet(set) => union(set.iter().map(|mask| mask.global_region()))?
            .ok_or("Cannot preview an empty MaskSet")?,
        Value::RegionSet(set) => match union(set.regions().iter().copied())? {
            Some(bounds) => bounds,
            None => {
                return PreviewFrame::from_pixels(
                    Dimensions::new(1, 1),
                    Dimensions::new(1, 1),
                    std::iter::once([0.0; 4]),
                );
            }
        },
        _ => return Err("Not a spatial output".into()),
    };
    bounds.end_x().ok_or("Spatial x extent overflow")?;
    bounds.end_y().ok_or("Spatial y extent overflow")?;
    let scale = 1_u32 << mip;
    let dimensions = Dimensions::new(bounds.width.div_ceil(scale), bounds.height.div_ceil(scale));
    let depth_range = if let Value::DepthMap(map) = value {
        map.values()
            .iter()
            .copied()
            .fold(None, |range: Option<(f64, f64)>, v| {
                let v = f64::from(v);
                Some(range.map_or((v, v), |(lo, hi)| (lo.min(v), hi.max(v))))
            })
    } else {
        None
    };
    let gray = |v| [v, v, v, 1.0];
    let mask_color = |v, color: [f32; 3]| match display {
        MaskDisplay::Grayscale => gray(v),
        MaskDisplay::Overlay => [color[0], color[1], color[2], v],
    };
    // Sample directly into the bounded upload, never allocate a full sparse-union raster.
    let pixels = (0..dimensions.height).flat_map(|y| {
        (0..dimensions.width).map(move |x| {
            let x = bounds.x + x * scale;
            let y = bounds.y + y * scale;
            match value {
                Value::Mask(mask) => {
                    mask_color(mask.pixel_global(x, y).unwrap_or(0.0), [1.0, 0.25, 0.1])
                }
                Value::ConfidenceMap(map) => gray(map.mask().pixel_global(x, y).unwrap_or(0.0)),
                Value::LabelMap(map) => match map.pixel_global(x, y).unwrap_or(0) {
                    0 => [0.0, 0.0, 0.0, 1.0],
                    label => {
                        let [r, g, b] = COLORS[(usize::from(label) - 1) % COLORS.len()];
                        [r, g, b, 1.0]
                    }
                },
                Value::DepthMap(map) => {
                    let (lo, hi) = depth_range.unwrap_or((0.0, 0.0));
                    let v = f64::from(map.pixel_global(x, y).unwrap_or(0.0));
                    gray(if hi == lo {
                        0.5
                    } else {
                        ((v - lo) / (hi - lo)) as f32
                    })
                }
                Value::MaskSet(set) => {
                    // ponytail: linear scan per displayed pixel; spatial indexing if large sets become costly.
                    let (index, v) =
                        set.iter()
                            .enumerate()
                            .fold((0, 0.0), |best, (index, mask)| {
                                let v = mask.pixel_global(x, y).unwrap_or(0.0);
                                if v > best.1 { (index, v) } else { best }
                            });
                    mask_color(v, COLORS[index % COLORS.len()])
                }
                Value::RegionSet(set) => set
                    .regions()
                    .iter()
                    .enumerate()
                    .rfind(|(_, region)| region.contains(x, y))
                    .map_or([0.0; 4], |(index, region)| {
                        let [r, g, b] = COLORS[index % COLORS.len()];
                        let border = x == region.x
                            || y == region.y
                            || Some(x + 1) == region.end_x()
                            || Some(y + 1) == region.end_y();
                        [r, g, b, if border { 1.0 } else { 0.35 }]
                    }),
                _ => [0.0; 4],
            }
        })
    });
    PreviewFrame::from_pixels(dimensions, bounds.dimensions(), pixels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawweave_image::{ConfidenceMap, DepthMap, LabelMap, Mask, MaskSet, RegionSet};

    #[test]
    fn masks_use_local_extent_and_global_samples_with_straight_alpha() {
        let mask =
            Mask::from_values_with_origin(Dimensions::new(3, 1), (20, 30), vec![0.0, 0.5, 1.0])
                .unwrap();
        let value = Value::Mask(mask.clone());
        let gray = spatial_frame(&value, 0, MaskDisplay::Grayscale).unwrap();
        assert_eq!(
            gray.bgra,
            [0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255]
        );
        let overlay = spatial_frame(&value, 0, MaskDisplay::Overlay).unwrap();
        assert_eq!(
            overlay.bgra,
            [26, 64, 255, 0, 26, 64, 255, 128, 26, 64, 255, 255]
        );
        let sampled = spatial_frame(&value, 1, MaskDisplay::Grayscale).unwrap();
        assert_eq!(sampled.dimensions, Dimensions::new(2, 1));
        assert_eq!(sampled.full_dimensions, Dimensions::new(3, 1));
        assert_eq!(sampled.bgra, [0, 0, 0, 255, 255, 255, 255, 255]);
        let confidence = spatial_frame(
            &Value::ConfidenceMap(ConfidenceMap::from_mask(mask)),
            0,
            MaskDisplay::Overlay,
        )
        .unwrap();
        assert_eq!(confidence.bgra, gray.bgra);
    }

    #[test]
    fn sets_use_union_and_strongest_mask_and_bound_sparse_extents() {
        let masks = Value::MaskSet(MaskSet::new(vec![
            Mask::constant(Dimensions::new(2, 1), (10, 10), 0.25).unwrap(),
            Mask::constant(Dimensions::new(1, 1), (11, 10), 0.75).unwrap(),
        ]));
        let frame = spatial_frame(&masks, 0, MaskDisplay::Grayscale).unwrap();
        assert_eq!(frame.dimensions, Dimensions::new(2, 1));
        assert_eq!(frame.bgra, [64, 64, 64, 255, 191, 191, 191, 255]);
        assert!(
            spatial_frame(
                &Value::MaskSet(MaskSet::new(vec![])),
                0,
                MaskDisplay::Grayscale
            )
            .is_err()
        );
        let sparse = Value::RegionSet(RegionSet::new(vec![
            Region::new(0, 0, 1, 1),
            Region::new(1_000_000, 0, 1, 1),
        ]));
        assert!(spatial_frame(&sparse, 0, MaskDisplay::Overlay).is_err());
        assert!(spatial_frame(&masks, 7, MaskDisplay::Grayscale).is_err());
        let overflowing = Value::RegionSet(RegionSet::new(vec![Region::new(u32::MAX, 0, 2, 1)]));
        assert!(spatial_frame(&overflowing, 0, MaskDisplay::Overlay).is_err());
        let over_budget = Value::RegionSet(RegionSet::new(vec![Region::new(0, 0, 8192, 8192)]));
        assert!(spatial_frame(&over_budget, 0, MaskDisplay::Overlay).is_err());
    }

    #[test]
    fn labels_depth_and_regions_have_distinct_display_semantics() {
        let labels = Value::LabelMap(
            LabelMap::from_values(Dimensions::new(3, 1), (8, 9), vec![0, 1, 9]).unwrap(),
        );
        let frame = spatial_frame(&labels, 0, MaskDisplay::Grayscale).unwrap();
        assert_eq!(&frame.bgra[..4], &[0, 0, 0, 255]);
        assert_eq!(&frame.bgra[4..8], &frame.bgra[8..12]);
        let depth = Value::DepthMap(
            DepthMap::from_values(Dimensions::new(3, 1), (8, 9), vec![-2.0, 0.0, 2.0]).unwrap(),
        );
        assert_eq!(
            spatial_frame(&depth, 0, MaskDisplay::Grayscale)
                .unwrap()
                .bgra,
            [0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255]
        );
        let flat = Value::DepthMap(
            DepthMap::from_values(Dimensions::new(1, 1), (0, 0), vec![4.0]).unwrap(),
        );
        assert_eq!(
            spatial_frame(&flat, 0, MaskDisplay::Grayscale)
                .unwrap()
                .bgra,
            [128, 128, 128, 255]
        );
        let regions = Value::RegionSet(RegionSet::new(vec![Region::new(8, 9, 3, 3)]));
        let frame = spatial_frame(&regions, 0, MaskDisplay::Grayscale).unwrap();
        assert_eq!(frame.bgra[3], 255);
        assert_eq!(frame.bgra[19], 89);
        let empty = Value::RegionSet(RegionSet::new(vec![]));
        assert_eq!(
            spatial_frame(&empty, 0, MaskDisplay::Grayscale)
                .unwrap()
                .bgra,
            [0, 0, 0, 0]
        );
    }
}
