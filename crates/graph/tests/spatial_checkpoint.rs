use rawweave_graph::{
    ArtifactStore, CheckpointArtifact, CheckpointPayload, GenerationMetadata, Provenance,
};
use rawweave_image::{
    ConfidenceMap, DepthMap, Dimensions, LabelMap, Mask, MaskSet, Region, RegionSet,
};

fn artifact(payload: CheckpointPayload) -> CheckpointArtifact {
    CheckpointArtifact::new(
        payload,
        "spatial-dependency",
        Provenance::new("spatial-dependency", 1),
        GenerationMetadata::new(1),
    )
    .unwrap()
}

#[test]
fn typed_spatial_checkpoint_artifacts_round_trip_as_their_original_graph_values() {
    let mask = Mask::constant(Dimensions::new(2, 1), (3, 4), 0.5).unwrap();
    let payloads = vec![
        (
            CheckpointPayload::MaskSet(MaskSet::new(vec![mask.clone()])),
            "core.MaskSet",
        ),
        (
            CheckpointPayload::LabelMap(
                LabelMap::from_values(Dimensions::new(2, 1), (3, 4), vec![0, 1]).unwrap(),
            ),
            "core.LabelMap",
        ),
        (
            CheckpointPayload::ConfidenceMap(ConfidenceMap::from_mask(mask)),
            "core.ConfidenceMap",
        ),
        (
            CheckpointPayload::DepthMap(
                DepthMap::from_values(Dimensions::new(2, 1), (3, 4), vec![0.1, 0.9]).unwrap(),
            ),
            "core.DepthMap",
        ),
        (
            CheckpointPayload::RegionSet(RegionSet::new(vec![Region::new(3, 4, 1, 1)])),
            "core.RegionSet",
        ),
    ];
    let store = ArtifactStore::memory();

    for (payload, data_type) in payloads {
        let artifact = artifact(payload);
        let id = store.put(&artifact).unwrap();
        let restored = store.require(&id).unwrap();
        assert_eq!(restored.payload.to_value().data_type(), data_type);
    }
}
