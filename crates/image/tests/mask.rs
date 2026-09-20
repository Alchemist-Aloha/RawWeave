use rawweave_image::{
    Dimensions, Mask, MaskError, PaintMode, PaintPoint, PaintStroke, PaintedMask, Region,
};

#[test]
fn mask_preserves_global_origin_and_tile_boundaries() {
    let mask = Mask::from_values_with_tile_size(
        Dimensions::new(3, 2),
        (10, 20),
        vec![0.0, 0.25, 0.5, 0.75, 1.0, 0.125],
        2,
    )
    .unwrap();

    assert_eq!(mask.origin(), (10, 20));
    assert_eq!(mask.global_region(), Region::new(10, 20, 3, 2));
    assert_eq!(mask.tile_size(), 2);
    assert_eq!(mask.tile_count(), 2);
    assert_eq!(mask.pixel_global(11, 21), Some(1.0));
    assert_eq!(mask.pixel_global(9, 21), None);
}

#[test]
fn mask_serialization_round_trip_preserves_content_hash_and_runtime_revision() {
    let mask = Mask::from_values_with_tile_size(
        Dimensions::new(3, 2),
        (4, 5),
        vec![0.25, 0.75, 0.5, 0.125, 1.0, 0.0],
        2,
    )
    .unwrap();
    let json = serde_json::to_string(&mask).unwrap();
    let restored: Mask = serde_json::from_str(&json).unwrap();

    assert_eq!(restored, mask);
    assert_eq!(restored.values(), mask.values());
    assert_eq!(restored.cache_identity(), mask.cache_identity());
    assert_eq!(restored.revision(), mask.revision());
}

#[test]
fn mask_revisions_and_cache_identity_are_independent_from_image_revisions() {
    let first = Mask::constant(Dimensions::new(1, 1), (0, 0), 0.5).unwrap();
    let second = Mask::constant(Dimensions::new(1, 1), (0, 0), 0.5).unwrap();
    let changed = Mask::constant(Dimensions::new(1, 1), (0, 0), 0.75).unwrap();

    assert_ne!(first.revision(), second.revision());
    assert_eq!(first.cache_identity(), second.cache_identity());
    assert_ne!(first.cache_identity(), changed.cache_identity());
}

#[test]
fn paint_strokes_replay_deterministically_and_support_undo_redo() {
    let mut painted = PaintedMask::new(Dimensions::new(5, 1), (0, 0)).unwrap();
    let stroke = PaintStroke::new(
        vec![PaintPoint::new(1.0, 0.0), PaintPoint::new(3.0, 0.0)],
        1.0,
        1.0,
        0.75,
        PaintMode::Add,
    )
    .unwrap();
    painted.apply(stroke.clone()).unwrap();
    let first = painted.render().unwrap();
    assert!(first.pixel(1, 0).unwrap() > 0.0);

    painted.undo().unwrap();
    assert_eq!(painted.render().unwrap().pixel(1, 0), Some(0.0));
    painted.redo().unwrap();
    assert_eq!(painted.render().unwrap(), first);
    assert_eq!(painted.strokes()[0], stroke);

    let json = serde_json::to_string(&painted).unwrap();
    let restored: PaintedMask = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.render().unwrap(), first);
}

#[test]
fn mask_rejects_non_finite_and_out_of_range_values() {
    assert!(matches!(
        Mask::from_values(Dimensions::new(1, 1), vec![f32::NAN]),
        Err(MaskError::NonFiniteValue)
    ));
    assert!(matches!(
        Mask::from_values(Dimensions::new(1, 1), vec![1.1]),
        Err(MaskError::ValueOutOfRange)
    ));
}

#[test]
fn mask_deserialization_rejects_zero_tile_size() {
    let json = r#"{
        "dimensions": {"width": 1, "height": 1},
        "origin": [0, 0],
        "tile_size": 0,
        "tiles": [],
        "revision": 1,
        "cache_identity": 1
    }"#;

    assert!(serde_json::from_str::<Mask>(json).is_err());
}

#[test]
fn mask_deserialization_rejects_malformed_tile_lengths() {
    let json = r#"{
        "dimensions": {"width": 2, "height": 1},
        "origin": [0, 0],
        "tile_size": 2,
        "tiles": [{
            "origin": [0, 0],
            "dimensions": {"width": 2, "height": 1},
            "values": [0.5]
        }],
        "revision": 1,
        "cache_identity": 1
    }"#;

    assert!(serde_json::from_str::<Mask>(json).is_err());
}

#[test]
fn paint_stroke_deserialization_rejects_empty_and_invalid_strokes() {
    let empty = r#"{
        "points": [],
        "size": 1.0,
        "hardness": 1.0,
        "opacity": 1.0,
        "mode": "Add"
    }"#;
    let invalid = r#"{
        "points": [{"x": 0.0, "y": 0.0}],
        "size": 0.0,
        "hardness": 1.0,
        "opacity": 1.0,
        "mode": "Add"
    }"#;

    assert!(serde_json::from_str::<PaintStroke>(empty).is_err());
    assert!(serde_json::from_str::<PaintStroke>(invalid).is_err());
}

#[test]
fn painted_mask_deserialization_rejects_invalid_strokes_and_tile_size() {
    let json = r#"{
        "dimensions": {"width": 1, "height": 1},
        "origin": [0, 0],
        "tile_size": 0,
        "strokes": [{
            "points": [],
            "size": 1.0,
            "hardness": 1.0,
            "opacity": 1.0,
            "mode": "Add"
        }],
        "redo": []
    }"#;

    assert!(serde_json::from_str::<PaintedMask>(json).is_err());
}
