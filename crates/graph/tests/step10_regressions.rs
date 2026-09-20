use rawweave_graph::{
    ArtifactId, ArtifactImportLimits, ArtifactStore, Checkpoint, CheckpointArtifact,
    CheckpointError, CheckpointPayload, CheckpointState, GenerationMetadata, Provenance,
};
use rawweave_image::{Dimensions, Image, Mask};
use std::fs;
use std::path::PathBuf;

fn image(value: f32) -> Image {
    Image::from_pixels(1, 1, vec![[value, value, value, 1.0]]).unwrap()
}

fn artifact(payload: CheckpointPayload, dependency: &str) -> CheckpointArtifact {
    CheckpointArtifact::new(
        payload,
        dependency,
        Provenance::new(dependency, 1),
        GenerationMetadata::new(1),
    )
    .unwrap()
}

fn temporary_store_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "rawweave-step10-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    root
}

#[test]
fn artifact_ids_are_strict_and_file_paths_cannot_escape_store_root() {
    let root = temporary_store_root();
    let store = ArtifactStore::new(&root);
    for value in ["../escape", "ABC", "", &"0".repeat(63), &"g".repeat(64)] {
        let id = ArtifactId::Sha256(value.to_owned());
        assert!(matches!(
            store.get(&id),
            Err(CheckpointError::InvalidArtifactId(_))
        ));
    }
    assert!(!root.join("escape.json").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn artifact_id_deserialization_requires_lowercase_sha256_hex() {
    for value in ["ABC", &"A".repeat(64), &"g".repeat(64)] {
        let wire = serde_json::json!({"Sha256": value});
        assert!(serde_json::from_value::<ArtifactId>(wire).is_err());
    }
    let valid = serde_json::json!({
        "Sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    });
    assert!(serde_json::from_value::<ArtifactId>(valid).is_ok());
}

#[test]
fn store_put_enforces_serialized_byte_limit() {
    let artifact = artifact(CheckpointPayload::SpatialData(vec![1, 2, 3, 4]), "input");
    let store = ArtifactStore::memory().with_limits(ArtifactImportLimits {
        max_bytes: 1,
        ..ArtifactImportLimits::default()
    });
    let result = store.put(&artifact);
    assert!(
        matches!(
            result,
            Err(CheckpointError::ImportLimitExceeded {
                resource: "bytes",
                ..
            })
        ),
        "{result:?}"
    );
}

#[test]
fn file_store_rejects_an_artifact_whose_id_differs_from_the_requested_filename() {
    let root = temporary_store_root();
    let store = ArtifactStore::new(&root);
    let first = artifact(CheckpointPayload::SpatialData(vec![1]), "first");
    let second = artifact(CheckpointPayload::SpatialData(vec![2]), "second");
    store.put(&first).unwrap();
    store.put(&second).unwrap();

    fs::write(
        root.join(format!("{}.json", first.id())),
        serde_json::to_vec(&second).unwrap(),
    )
    .unwrap();

    assert!(matches!(
        store.get(first.id()),
        Err(CheckpointError::ArtifactIdMismatch { expected, actual })
            if expected == *first.id() && actual == *second.id()
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn imported_payload_geometry_and_counts_are_validated_deeply() {
    let store = ArtifactStore::memory();
    let valid = artifact(CheckpointPayload::Image(image(0.5)), "input");
    let mut json: serde_json::Value = serde_json::to_value(&valid).unwrap();
    json["payload"]["value"]["pixels"] = serde_json::json!([]);
    let result = store.import(&serde_json::to_vec(&json).unwrap());
    assert!(
        matches!(
            result,
            Err(CheckpointError::Serialization(_) | CheckpointError::InvalidPayload(_))
        ),
        "{result:?}"
    );

    let valid_mask = artifact(
        CheckpointPayload::Mask(Mask::constant(Dimensions::new(2, 1), (0, 0), 0.5).unwrap()),
        "input",
    );
    let mut malformed_mask: serde_json::Value = serde_json::to_value(&valid_mask).unwrap();
    malformed_mask["payload"]["value"]["tiles"][0]["values"] = serde_json::json!([0.5]);
    assert!(matches!(
        store.import(&serde_json::to_vec(&malformed_mask).unwrap()),
        Err(CheckpointError::Serialization(_) | CheckpointError::InvalidPayload(_))
    ));
}

#[test]
fn generation_tokens_reject_cancelled_failed_and_older_commits() {
    let store = ArtifactStore::memory();
    let mut checkpoint = Checkpoint::new("manual", 1);
    checkpoint.set_dependency_hash("input");

    let cancelled = checkpoint.begin_generation_token().unwrap();
    checkpoint.cancel_generation(&cancelled).unwrap();
    assert!(
        checkpoint
            .commit_generation(
                cancelled,
                artifact(CheckpointPayload::Image(image(0.1)), "input"),
                &store,
            )
            .is_err()
    );
    assert_eq!(checkpoint.state(), CheckpointState::Cancelled);

    let failed = checkpoint.begin_generation_token().unwrap();
    checkpoint.fail_generation(&failed, "failed").unwrap();
    assert!(
        checkpoint
            .commit_generation(
                failed,
                artifact(CheckpointPayload::Image(image(0.2)), "input"),
                &store,
            )
            .is_err()
    );
    assert_eq!(checkpoint.state(), CheckpointState::Failed);

    let older = checkpoint.begin_generation_token().unwrap();
    checkpoint.cancel_generation(&older).unwrap();
    let current = checkpoint.begin_generation_token().unwrap();
    assert!(
        checkpoint
            .commit_generation(
                older,
                artifact(CheckpointPayload::Image(image(0.3)), "input"),
                &store,
            )
            .is_err()
    );
    checkpoint
        .commit_generation(
            current,
            artifact(CheckpointPayload::Image(image(0.4)), "input"),
            &store,
        )
        .unwrap();
    assert_eq!(checkpoint.state(), CheckpointState::Current);
}

#[test]
fn restoring_a_generating_checkpoint_makes_it_retryable() {
    let mut checkpoint = Checkpoint::new("manual", 1);
    checkpoint.set_dependency_hash("input");
    checkpoint.begin_generation_token().unwrap();

    let restored: Checkpoint =
        serde_json::from_str(&serde_json::to_string(&checkpoint).unwrap()).unwrap();
    assert_ne!(restored.state(), CheckpointState::Generating);

    let mut restored = restored;
    let token = restored.begin_generation_token().unwrap();
    restored.cancel_generation(&token).unwrap();
}

#[test]
fn generation_store_errors_clear_the_active_token_and_leave_a_retryable_failure() {
    let store = ArtifactStore::memory().with_limits(ArtifactImportLimits {
        max_bytes: 1,
        ..ArtifactImportLimits::default()
    });
    let mut checkpoint = Checkpoint::new("manual", 1);
    checkpoint.set_dependency_hash("input");
    let token = checkpoint.begin_generation_token().unwrap();

    assert!(
        checkpoint
            .commit_generation(
                token,
                artifact(CheckpointPayload::SpatialData(vec![1]), "input"),
                &store,
            )
            .is_err()
    );
    assert_eq!(checkpoint.state(), CheckpointState::Failed);

    let retry = checkpoint.begin_generation_token().unwrap();
    checkpoint.cancel_generation(&retry).unwrap();
}
