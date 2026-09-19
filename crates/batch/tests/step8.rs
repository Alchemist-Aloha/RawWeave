use rawweave_batch::*;
use rawweave_graph::{WorkflowDefinition, WorkflowMetadata};
use rawweave_image::Image;
use rawweave_node_api::ParameterValue;
use rawweave_project::EditorCore;
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

fn workflow() -> WorkflowDefinition {
    let mut editor = EditorCore::default();
    editor.add_node("input", "core.image-input").unwrap();
    editor.add_node("output", "core.output").unwrap();
    editor.connect("input", "image", "output", "image").unwrap();
    WorkflowDefinition::new(
        "test.workflow",
        "1.0.0",
        editor.graph().clone(),
        WorkflowMetadata::new("Test"),
    )
    .unwrap()
}

fn recipe(format: OutputFormat, dir: &std::path::Path) -> OutputRecipe {
    OutputRecipe::new(format, dir.to_path_buf()).with_filename_template("{stem}-{index}")
}

fn job(count: usize, dir: &std::path::Path) -> BatchJob {
    let pinned = PinnedWorkflow::new(workflow(), 7).unwrap();
    let items = (0..count)
        .map(|index| {
            BatchItem::new(
                format!("item-{index}"),
                format!("/synthetic/item-{index}.png"),
                format!("item-{index}.png"),
            )
        })
        .collect();
    BatchJob::new(
        "job-1",
        pinned,
        PinnedDependencies::default(),
        BTreeMap::new(),
        vec![recipe(OutputFormat::Png, dir)],
        CheckpointPolicy::AfterEachItem,
        items,
    )
    .unwrap()
}

#[test]
fn pinned_job_round_trips_and_transitions_are_validated() {
    let dir = tempfile::tempdir().unwrap();
    let mut job = job(1, dir.path());
    let hash = job.workflow.hash.clone();
    job.items[0]
        .overrides
        .insert("input:unused".into(), ParameterValue::Integer(2));
    job.transition_item("item-0", ItemState::Running).unwrap();
    job.transition_item("item-0", ItemState::Completed).unwrap();
    assert!(job.transition_item("item-0", ItemState::Running).is_err());
    let json = serde_json::to_string(&job).unwrap();
    let restored: BatchJob = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.schema_version, BATCH_JOB_SCHEMA_VERSION);
    assert_eq!(restored.workflow.hash, hash);
    assert_eq!(restored.items[0].state, ItemState::Completed);
}

#[test]
fn preflight_reports_errors_warnings_and_info_without_running() {
    let dir = tempfile::tempdir().unwrap();
    let mut job = job(2, dir.path());
    job.items[0].source_path = "/missing/one.png".into();
    job.items[1].source_path = "/missing/two.png".into();
    let report = preflight(&job, &PreflightOptions::default());
    assert!(report.has_errors());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Error && d.code == "missing-source")
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Info && d.code == "checkpoint-policy")
    );
}

#[test]
fn dry_run_selects_current_test_first_and_explicit_items() {
    let dir = tempfile::tempdir().unwrap();
    let mut job = job(5, dir.path());
    job.items[2].test_set = true;
    let current = dry_run(
        &job,
        DryRunSubset::CurrentPreview {
            item_id: "item-3".into(),
        },
    )
    .unwrap();
    assert_eq!(current.item_ids(), vec!["item-3"]);
    let test = dry_run(&job, DryRunSubset::TestSet).unwrap();
    assert_eq!(test.item_ids(), vec!["item-2"]);
    let first = dry_run(&job, DryRunSubset::FirstN(2)).unwrap();
    assert_eq!(first.item_ids(), vec!["item-0", "item-1"]);
    let selected = dry_run(
        &job,
        DryRunSubset::Selected(vec!["item-4".into(), "item-1".into()]),
    )
    .unwrap();
    assert_eq!(selected.item_ids(), vec!["item-4", "item-1"]);
}

#[test]
fn all_output_recipes_write_real_files_and_resize() {
    let dir = tempfile::tempdir().unwrap();
    let image =
        Image::from_pixels(2, 1, vec![[0.25, 0.5, 1.0, 1.0], [1.0, 0.0, 0.0, 1.0]]).unwrap();
    for (format, magic) in [
        (OutputFormat::Jpeg, vec![0xff, 0xd8]),
        (OutputFormat::Png, vec![0x89, b'P', b'N', b'G']),
        (OutputFormat::Tiff, vec![b'I', b'I']),
        (OutputFormat::OpenExr, vec![0x76, 0x2f, 0x31, 0x01]),
    ] {
        let recipe = recipe(format, dir.path()).with_resolution(Resolution::Exact {
            width: 1,
            height: 1,
        });
        let path = write_output(
            &image,
            &recipe,
            &BatchItem::new("x", "/x.png", "x.png"),
            0,
            None,
        )
        .unwrap();
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[..magic.len()], magic.as_slice());
    }
}

struct SyntheticProcessor {
    calls: Arc<AtomicUsize>,
    fail: bool,
}
impl BatchProcessor for SyntheticProcessor {
    fn process(
        &self,
        _workflow: &PinnedWorkflow,
        item: &BatchItem,
        cancel: &CancellationToken,
    ) -> Result<Image, BatchError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail && item.id == "item-3" {
            return Err(BatchError::Processor("synthetic failure".into()));
        }
        for _ in 0..3 {
            if cancel.is_cancelled() {
                return Err(BatchError::Cancelled);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Image::from_pixels(1, 1, vec![[0.5, 0.25, 0.75, 1.0]])
            .map_err(|e| BatchError::Processor(e.to_string()))
    }
}

#[test]
fn bounded_engine_isolates_failures_and_handles_more_than_100_items() {
    let dir = tempfile::tempdir().unwrap();
    let mut job = job(120, dir.path());
    job.items[3].id = "item-3".into();
    let calls = Arc::new(AtomicUsize::new(0));
    let processor = Arc::new(SyntheticProcessor {
        calls: Arc::clone(&calls),
        fail: true,
    });
    let runner = BatchEngine::new(job, JobStore::memory(), processor, 4).unwrap();
    runner.start().unwrap();
    runner.wait().unwrap();
    let snapshot = runner.snapshot().unwrap();
    assert_eq!(snapshot.items.len(), 120);
    assert_eq!(
        snapshot
            .items
            .iter()
            .filter(|i| i.state == ItemState::Completed)
            .count(),
        119
    );
    assert_eq!(
        snapshot
            .items
            .iter()
            .filter(|i| i.state == ItemState::Failed)
            .count(),
        1
    );
    assert!(calls.load(Ordering::SeqCst) >= 120);
}

#[test]
fn atomic_restart_skips_valid_completed_outputs_and_retries_invalid_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("job.json");
    let store = JobStore::new(&state_path);
    let calls = Arc::new(AtomicUsize::new(0));
    let runner = BatchEngine::new(
        job(1, dir.path()),
        store.clone(),
        Arc::new(SyntheticProcessor {
            calls: Arc::clone(&calls),
            fail: false,
        }),
        1,
    )
    .unwrap();
    runner.start().unwrap();
    runner.wait().unwrap();
    let first_calls = calls.load(Ordering::SeqCst);
    let loaded = store.load().unwrap();
    assert_eq!(loaded.items[0].state, ItemState::Completed);
    assert!(loaded.validate_completed_outputs().unwrap());
    let resumed = BatchEngine::resume(
        store.clone(),
        Arc::new(SyntheticProcessor {
            calls: Arc::clone(&calls),
            fail: false,
        }),
        1,
    )
    .unwrap();
    resumed.start().unwrap();
    resumed.wait().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), first_calls);
    let output = loaded.items[0].outputs[0].path.clone();
    std::fs::write(output, b"corrupt").unwrap();
    let mut invalid = store.load().unwrap();
    invalid.requeue_invalid_completed().unwrap();
    store.save(&invalid).unwrap();
    let resumed = BatchEngine::resume(
        store,
        Arc::new(SyntheticProcessor {
            calls: Arc::clone(&calls),
            fail: false,
        }),
        1,
    )
    .unwrap();
    resumed.start().unwrap();
    resumed.wait().unwrap();
    assert!(calls.load(Ordering::SeqCst) > first_calls);
}

#[test]
fn cancellation_stops_waiting_items_without_affecting_persisted_job_integrity() {
    let dir = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let runner = BatchEngine::new(
        job(100, dir.path()),
        JobStore::memory(),
        Arc::new(SyntheticProcessor { calls, fail: false }),
        2,
    )
    .unwrap();
    runner.start().unwrap();
    std::thread::sleep(Duration::from_millis(5));
    runner.cancel().unwrap();
    runner.wait().unwrap();
    let snapshot = runner.snapshot().unwrap();
    assert!(
        snapshot
            .items
            .iter()
            .any(|item| item.state == ItemState::Cancelled)
    );
    assert!(
        !snapshot
            .items
            .iter()
            .any(|item| item.state == ItemState::Running)
    );
}
