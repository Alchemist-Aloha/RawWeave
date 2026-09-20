use std::collections::BTreeMap;

use rawweave_image::{
    ConfidenceMap, DepthMap, Dimensions, LabelMap, Mask, MaskSet, Region, RegionSet,
};

#[test]
fn spatial_maps_preserve_global_coordinates_and_labels() {
    let labels = BTreeMap::from([
        (String::from("background"), 0_u16),
        (String::from("sky"), 7_u16),
    ]);
    let label_map =
        LabelMap::from_values_with_labels(Dimensions::new(2, 1), (10, 20), vec![7, 0], labels)
            .unwrap();

    assert_eq!(label_map.global_region(), Region::new(10, 20, 2, 1));
    assert_eq!(label_map.pixel_global(10, 20), Some(7));
    assert_eq!(label_map.label_value("sky"), Some(7));
    assert_eq!(label_map.label_name(0), Some("background"));
    assert_eq!(label_map.labels().get("sky"), Some(&7));
}

#[test]
fn confidence_depth_and_mask_sets_are_inspectable_spatial_values() {
    let mask =
        Mask::from_values_with_origin(Dimensions::new(2, 1), (4, 5), vec![0.25, 0.75]).unwrap();
    let confidence = ConfidenceMap::from_mask(mask.clone());
    let depth = DepthMap::from_values(Dimensions::new(2, 1), (4, 5), vec![0.1, 0.9]).unwrap();
    let masks = MaskSet::new(vec![mask]);
    let regions = RegionSet::new(vec![Region::new(4, 5, 1, 1)]);

    assert_eq!(confidence.dimensions(), Dimensions::new(2, 1));
    assert_eq!(confidence.pixel_global(5, 5), Some(0.75));
    assert_eq!(depth.pixel_global(4, 5), Some(0.1));
    assert_eq!(masks.len(), 1);
    assert_eq!(masks.get(0).and_then(|mask| mask.pixel(1, 0)), Some(0.75));
    assert_eq!(regions.len(), 1);
}

#[test]
fn spatial_maps_reject_invalid_origins_and_non_finite_depth() {
    assert!(LabelMap::from_values(Dimensions::new(2, 1), (u32::MAX, 0), vec![0, 1]).is_err());
    assert!(DepthMap::from_values(Dimensions::new(1, 1), (0, 0), vec![f32::NAN]).is_err());
}
