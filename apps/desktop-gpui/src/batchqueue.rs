//! Batch queue construction and reporting.
//!
//! Execution lives in `rawweave_batch::BatchEngine`; this module only turns the
//! current workflow plus a list of sources into a validated job and reports its
//! progress, so the native shell holds no scheduling logic.

use rawweave_batch::{
    BatchItem, BatchJob, BatchProcessor, CheckpointPolicy, ImageFileProcessor, ItemState,
    OutputFormat, OutputRecipe, PinnedDependencies, PinnedWorkflow,
};
use rawweave_graph::{WorkflowDefinition, WorkflowMetadata};
use rawweave_project::EditorCore;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Bounded worker concurrency for one queue.
pub const BATCH_WORKERS: usize = 4;

/// Output choices the batch surface currently offers.
#[derive(Clone, Debug, PartialEq)]
pub struct BatchSettings {
    pub output_dir: PathBuf,
    pub format: OutputFormat,
    pub quality: u8,
}

impl Default for BatchSettings {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::new(),
            format: OutputFormat::Jpeg,
            quality: 92,
        }
    }
}

/// How a queue state is marked, resolved to those tokens by the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Nothing has happened yet.
    Idle,
    /// Finished and accepted.
    Fresh,
    /// Stopped short: caution, still recoverable.
    Held,
    /// Fault.
    Failed,
    /// In flight.
    Working,
}

/// The word printed on the state stamp.
pub fn state_label(state: ItemState) -> &'static str {
    match state {
        ItemState::Waiting => "Waiting",
        ItemState::Running => "Running",
        ItemState::Completed => "Completed",
        ItemState::Skipped => "Skipped",
        ItemState::Failed => "Failed",
        ItemState::Cancelled => "Cancelled",
    }
}

/// The meaning behind that word.
pub fn tone(state: ItemState) -> Tone {
    match state {
        ItemState::Waiting => Tone::Idle,
        ItemState::Running => Tone::Working,
        ItemState::Completed => Tone::Fresh,
        ItemState::Skipped => Tone::Idle,
        ItemState::Failed => Tone::Failed,
        ItemState::Cancelled => Tone::Held,
    }
}

/// Human-facing output format name.
pub fn format_label(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Jpeg => "JPEG",
        OutputFormat::Png => "PNG",
        OutputFormat::Tiff => "TIFF",
        OutputFormat::OpenExr => "OpenEXR",
    }
}

/// One queued source. The id is stable per queue position and session counter.
pub fn queued_item(counter: u64, path: &Path) -> BatchItem {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    BatchItem::new(format!("item-{counter}"), path.to_path_buf(), name)
}

/// Every supported image directly inside a folder, as queue items.
pub fn directory_items(start: u64, dir: &Path) -> Result<(Vec<BatchItem>, bool), String> {
    let scan = crate::browse::scan_folder(dir)?;
    let items = scan
        .entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| queued_item(start + index as u64, &entry.path))
        .collect();
    Ok((items, scan.truncated))
}

/// Pin the live workflow and describe one job over these items.
pub fn build_job(
    editor: &EditorCore,
    next_id: u64,
    settings: &BatchSettings,
    items: Vec<BatchItem>,
) -> Result<BatchJob, String> {
    if items.is_empty() {
        return Err("add at least one image to the queue".into());
    }
    if settings.output_dir.as_os_str().is_empty() {
        return Err("choose an output folder first".into());
    }
    if settings.quality == 0 {
        return Err("quality must be between 1 and 100".into());
    }
    let definition = WorkflowDefinition::new(
        "batch",
        "1.0.0",
        editor.graph().clone(),
        WorkflowMetadata::new("Batch"),
    )
    .map_err(|error| error.to_string())?;
    let workflow = PinnedWorkflow::new(definition, editor.graph().revision())
        .map_err(|error| error.to_string())?;
    let mut recipe = OutputRecipe::new(settings.format, settings.output_dir.clone());
    recipe.quality = settings.quality;
    BatchJob::new(
        format!("batch-{next_id}"),
        workflow,
        PinnedDependencies::default(),
        BTreeMap::new(),
        vec![recipe],
        CheckpointPolicy::AfterEachItem,
        items,
    )
    .map_err(|error| error.to_string())
}

/// Refuse a job the processor cannot run before any worker starts.
pub fn validate_job(job: &BatchJob) -> Result<(), String> {
    ImageFileProcessor
        .validate(&job.workflow, &job.dependencies)
        .map_err(|error| error.to_string())
}

/// Item counts and completion for one queue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub total: usize,
    pub waiting: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub finished: usize,
}

impl Progress {
    /// Whether every item has reached a terminal state.
    pub fn is_finished(self) -> bool {
        self.total == 0 || self.finished == self.total
    }

    /// The one-line readout printed under the queue.
    pub fn summary(self) -> String {
        format!(
            "{} queued · {} running · {} completed · {} failed",
            self.waiting, self.running, self.completed, self.failed
        )
    }
}

/// Count the queue's item states.
pub fn progress(job: &BatchJob) -> Progress {
    let mut progress = Progress {
        total: job.items.len(),
        ..Progress::default()
    };
    for item in &job.items {
        match item.state {
            ItemState::Waiting => progress.waiting += 1,
            ItemState::Running => progress.running += 1,
            ItemState::Completed => progress.completed += 1,
            ItemState::Failed => progress.failed += 1,
            ItemState::Skipped | ItemState::Cancelled => {}
        }
        if item.state.is_terminal() {
            progress.finished += 1;
        }
    }
    progress
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawweave_raw::{DeterministicCorpus, DeterministicDecoder};

    fn editor() -> EditorCore {
        EditorCore::new_with_raw_decoder(DeterministicDecoder::new(
            DeterministicCorpus::bayer_12_bit(),
        ))
    }

    fn settings() -> BatchSettings {
        BatchSettings {
            output_dir: std::env::temp_dir(),
            format: OutputFormat::Png,
            quality: 90,
        }
    }

    #[test]
    fn a_job_pins_the_live_workflow_and_reports_item_states() {
        let mut editor = editor();
        editor.reset_raw_image_graph().unwrap();
        let items = vec![queued_item(0, Path::new("/a/one.nef"))];
        let job = build_job(&editor, 1, &settings(), items).unwrap();
        assert_eq!(job.workflow.revision, editor.graph().revision());
        assert_eq!(job.workflow.hash, job.workflow.definition.hash());
        assert_eq!(job.items[0].display_name, "one.nef");
        assert_eq!(job.items[0].id, "item-0");
        assert_eq!(job.recipes[0].quality, 90);
        let mut job = job;
        job.items[0].state = ItemState::Failed;
        job.items[0].failure = Some("source unreadable".into());
        assert_eq!(
            progress(&job).summary(),
            "0 queued · 0 running · 0 completed · 1 failed"
        );
        assert!(progress(&job).is_finished());
        let mut job = job;
        job.items[0].state = ItemState::Waiting;
        assert!(!progress(&job).is_finished());
        assert_eq!(tone(ItemState::Failed), Tone::Failed);
        assert_eq!(tone(ItemState::Running), Tone::Working);
        assert_eq!(tone(ItemState::Completed), Tone::Fresh);
        assert_eq!(tone(ItemState::Cancelled), Tone::Held);
    }

    #[test]
    fn an_empty_queue_or_missing_output_folder_is_refused_before_any_work() {
        let editor = editor();
        assert!(build_job(&editor, 1, &settings(), vec![]).is_err());
        assert!(
            build_job(
                &editor,
                1,
                &BatchSettings::default(),
                vec![queued_item(0, Path::new("/a.png"))]
            )
            .is_err()
        );
        let mut bad = settings();
        bad.quality = 0;
        assert!(build_job(&editor, 1, &bad, vec![queued_item(0, Path::new("/a.png"))]).is_err());
    }

    #[test]
    fn queueing_a_folder_lists_supported_images_and_survives_an_unreadable_one() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("b.png"), b"x").unwrap();
        std::fs::write(dir.path().join("a.nef"), b"x").unwrap();
        std::fs::write(dir.path().join("skip.txt"), b"x").unwrap();
        let (items, truncated) = directory_items(7, dir.path()).unwrap();
        assert!(!truncated);
        assert_eq!(
            items
                .iter()
                .map(|item| item.display_name.as_str())
                .collect::<Vec<_>>(),
            ["a.nef", "b.png"]
        );
        assert_eq!(items[0].id, "item-7");
        assert_eq!(items[1].id, "item-8");
        assert!(directory_items(0, &dir.path().join("missing")).is_err());
    }

    #[test]
    fn an_ordinary_workflow_validates_and_a_missing_source_fails_cleanly() {
        let mut editor = editor();
        editor.reset_ordinary_image_graph().unwrap();
        let items = vec![queued_item(0, Path::new("/a/one.png"))];
        let job = build_job(&editor, 1, &settings(), items).unwrap();
        validate_job(&job).unwrap();
        let cancel = rawweave_batch::CancellationToken::new();
        assert!(
            ImageFileProcessor
                .process(&job.workflow, &job.items[0], &cancel)
                .is_err()
        );
        let mut decoder = editor;
        decoder.reset_raw_image_graph().unwrap();
        let job = build_job(
            &decoder,
            2,
            &settings(),
            vec![queued_item(0, Path::new("/a/one.nef"))],
        )
        .unwrap();
        validate_job(&job).unwrap();
    }
}
