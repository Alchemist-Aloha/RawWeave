use rawweave_graph::{
    ArtifactId, ArtifactStore, Checkpoint, CheckpointArtifact, CheckpointAvailability,
    CheckpointPayload, CheckpointState, GenerationMetadata, Provenance,
};
use rawweave_image::Image;
use std::collections::BTreeMap;

fn image(value: f32) -> Image {
    Image::from_pixels(1, 1, vec![[value, value, value, 1.0]]).unwrap()
}

#[test]
fn checkpoint_artifact_identity_is_content_addressed_and_persistent() {
    let store = ArtifactStore::memory();
    let provenance = Provenance::new("input-hash", 3);
    let first = CheckpointArtifact::new(
        CheckpointPayload::Image(image(0.25)),
        "input-hash",
        provenance.clone(),
        GenerationMetadata::new(7),
    )
    .unwrap();
    let second = CheckpointArtifact::new(
        CheckpointPayload::Image(image(0.25)),
        "input-hash",
        provenance,
        GenerationMetadata::new(7),
    )
    .unwrap();

    assert_eq!(first.id(), second.id());
    assert!(matches!(first.id(), ArtifactId::Sha256(_)));
    store.put(&first).unwrap();
    let restored = store.get(first.id()).unwrap().unwrap();
    assert_eq!(restored, first);
    assert!(store.validate(first.id()).unwrap());
}

#[test]
fn checkpoint_state_tracks_missing_fresh_stale_and_incompatible_artifacts() {
    let store = ArtifactStore::memory();
    let mut checkpoint = Checkpoint::new("manual", 3);
    assert_eq!(checkpoint.state(), CheckpointState::Ungenerated);
    assert_eq!(checkpoint.availability(), CheckpointAvailability::Missing);

    checkpoint.set_dependency_hash("input-a");
    let artifact = CheckpointArtifact::new(
        CheckpointPayload::Image(image(0.5)),
        "input-a",
        Provenance::new("input-a", 3),
        GenerationMetadata::new(1),
    )
    .unwrap();
    checkpoint.commit(artifact.clone(), &store).unwrap();
    assert_eq!(checkpoint.state(), CheckpointState::Current);
    assert_eq!(checkpoint.availability(), CheckpointAvailability::Fresh);

    checkpoint.set_dependency_hash("input-b");
    assert_eq!(checkpoint.state(), CheckpointState::Stale);
    assert_eq!(checkpoint.availability(), CheckpointAvailability::Stale);
    assert!(checkpoint.committed_artifact(&store).unwrap().is_some());

    let incompatible = CheckpointArtifact::new(
        CheckpointPayload::Image(image(0.75)),
        "input-b",
        Provenance::new("input-b", 4),
        GenerationMetadata::new(2),
    )
    .unwrap();
    checkpoint.commit(incompatible, &store).unwrap_err();
    assert_eq!(checkpoint.availability(), CheckpointAvailability::Stale);
}

#[test]
fn checkpoint_provenance_hash_includes_all_upstream_inputs() {
    let mut upstream = BTreeMap::new();
    upstream.insert("exposure".to_owned(), "hash-a".to_owned());
    upstream.insert("mask".to_owned(), "hash-b".to_owned());
    let first = Provenance::with_upstream_hashes(upstream.clone(), 1);
    upstream.insert("mask".to_owned(), "hash-c".to_owned());
    let second = Provenance::with_upstream_hashes(upstream, 1);
    assert_ne!(first.dependency_hash(), second.dependency_hash());
}

#[test]
fn generation_that_finishes_after_inputs_change_becomes_stale() {
    let store = ArtifactStore::memory();
    let mut checkpoint = Checkpoint::new("manual", 3);
    checkpoint.set_dependency_hash("input-a");
    let committed = CheckpointArtifact::new(
        CheckpointPayload::Image(image(0.1)),
        "input-a",
        Provenance::new("input-a", 3),
        GenerationMetadata::new(1),
    )
    .unwrap();
    checkpoint.commit(committed, &store).unwrap();

    checkpoint.begin_generation().unwrap();
    checkpoint.set_dependency_hash("input-b");
    let artifact = CheckpointArtifact::new(
        CheckpointPayload::Image(image(0.9)),
        "input-a",
        Provenance::new("input-a", 3),
        GenerationMetadata::new(4),
    )
    .unwrap();
    assert!(checkpoint.commit(artifact, &store).is_err());
    assert_eq!(checkpoint.state(), CheckpointState::Stale);
}
