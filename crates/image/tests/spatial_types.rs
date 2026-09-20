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

#[test]
fn spatial_map_serialization_preserves_revision_and_cache_identity() {
    let labels = BTreeMap::from([(String::from("sky"), 7_u16)]);
    let label_map =
        LabelMap::from_values_with_labels(Dimensions::new(2, 1), (10, 20), vec![7, 0], labels)
            .unwrap();
    let depth_map = DepthMap::from_values(Dimensions::new(2, 1), (10, 20), vec![0.1, 0.9]).unwrap();

    let label_json = serde_json::to_string(&label_map).unwrap();
    let depth_json = serde_json::to_string(&depth_map).unwrap();
    let restored_label: LabelMap = serde_json::from_str(&label_json).unwrap();
    let restored_depth: DepthMap = serde_json::from_str(&depth_json).unwrap();

    assert_eq!(restored_label, label_map);
    assert_eq!(restored_label.revision(), label_map.revision());
    assert_eq!(restored_label.cache_identity(), label_map.cache_identity());
    assert_eq!(restored_depth, depth_map);
    assert_eq!(restored_depth.revision(), depth_map.revision());
    assert_eq!(restored_depth.cache_identity(), depth_map.cache_identity());
}

#[test]
fn label_map_deserialization_rejects_mismatched_value_buffers() {
    let json = r#"{
        "dimensions": {"width": 2, "height": 1},
        "origin": [0, 0],
        "values": [0],
        "labels": {},
        "revision": 1,
        "cache_identity": 1
    }"#;

    assert!(serde_json::from_str::<LabelMap>(json).is_err());
}

#[test]
fn depth_map_deserialization_rejects_mismatched_and_non_finite_values() {
    let mismatched = r#"{
        "dimensions": {"width": 2, "height": 1},
        "origin": [0, 0],
        "values": [1.0],
        "revision": 1,
        "cache_identity": 1
    }"#;
    let non_finite = r#"{
        "dimensions": {"width": 1, "height": 1},
        "origin": [0, 0],
        "values": [1e39],
        "revision": 1,
        "cache_identity": 1
    }"#;

    assert!(serde_json::from_str::<DepthMap>(mismatched).is_err());
    assert!(serde_json::from_str::<DepthMap>(non_finite).is_err());
}
