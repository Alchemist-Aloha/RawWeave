use rawweave_image::Image;
use rawweave_node_api::{
    AlignmentProvenance, AlignmentState, AlignmentTransform, ImageSet, ImageSetMember,
    ImageSetOrder, ImageSetSourceDescriptor, Metadata, Value,
};

fn image(value: f32) -> Image {
    Image::from_pixels(2, 1, vec![[value, value, value, 1.0]; 2]).unwrap()
}

#[test]
fn imageset_preserves_order_identity_and_metadata() {
    let mut first_metadata = Metadata::default();
    first_metadata.tags.insert("role".into(), "near".into());
    let set = ImageSet::new(
        ImageSetOrder::Ordered,
        vec![
            ImageSetMember::new("near", image(1.0), first_metadata.clone()),
            ImageSetMember::new("far", image(2.0), Metadata::default()),
        ],
    )
    .unwrap()
    .with_shared_metadata(Metadata {
        make: "Fixture".into(),
        ..Metadata::default()
    });

    assert_eq!(set.member_ids(), ["near", "far"]);
    assert_eq!(set.member("near").unwrap().metadata, first_metadata);
    assert_eq!(set.shared_metadata().make, "Fixture");
    assert_eq!(set.alignment(), AlignmentState::Unaligned);
    assert_eq!(Value::ImageSet(set).data_type(), "core.ImageSet");
}

#[test]
fn imageset_json_round_trip_keeps_alignment_and_members() {
    let set = ImageSet::new(
        ImageSetOrder::Unordered,
        vec![
            ImageSetMember::new("a", image(1.0), Metadata::default())
                .with_source(ImageSetSourceDescriptor::new("/photos/a.jpg")),
        ],
    )
    .unwrap()
    .with_alignment(AlignmentState::Aligned {
        reference_member: "a".into(),
        transforms: [("a".into(), AlignmentTransform::identity())]
            .into_iter()
            .collect(),
        provenance: AlignmentProvenance::new("translation-ssd", 1, 8),
    });

    let encoded = serde_json::to_vec(&set).unwrap();
    let decoded: ImageSet = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, set);
    assert_eq!(
        decoded.member("a").unwrap().source().unwrap().path(),
        "/photos/a.jpg"
    );
}

#[test]
fn unordered_sets_have_canonical_member_order_and_ids() {
    let image_a = image(1.0);
    let image_z = image(2.0);
    let first = ImageSet::new(
        ImageSetOrder::Unordered,
        vec![
            ImageSetMember::new("z", image_z.clone(), Metadata::default()),
            ImageSetMember::new("a", image_a.clone(), Metadata::default()),
        ],
    )
    .unwrap();
    let second = ImageSet::new(
        ImageSetOrder::Unordered,
        vec![
            ImageSetMember::new("a", image_a, Metadata::default()),
            ImageSetMember::new("z", image_z, Metadata::default()),
        ],
    )
    .unwrap();

    assert_eq!(first.member_ids(), ["a", "z"]);
    assert_eq!(first, second);
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
}

#[test]
fn imageset_persistence_rejects_oversized_identity_and_metadata() {
    let long_id = "x".repeat(rawweave_node_api::MAX_IMAGE_SET_MEMBER_ID_BYTES + 1);
    assert!(matches!(
        ImageSet::new(
            ImageSetOrder::Ordered,
            vec![ImageSetMember::new(
                long_id,
                image(1.0),
                Metadata::default()
            )],
        ),
        Err(rawweave_node_api::ImageSetError::MemberIdTooLong { .. })
    ));

    let oversized = ImageSet::new(
        ImageSetOrder::Ordered,
        vec![ImageSetMember::new("a", image(1.0), Metadata::default())],
    )
    .unwrap()
    .with_shared_metadata(Metadata {
        make: "x".repeat(rawweave_node_api::MAX_IMAGE_SET_METADATA_BYTES + 1),
        ..Metadata::default()
    });
    assert!(serde_json::to_vec(&oversized).is_err());
}
