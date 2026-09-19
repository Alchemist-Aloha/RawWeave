use crate::model::{BatchItem, BatchJob, BatchState, ItemState, OutputRecipe, PinnedWorkflow};
use crate::persistence::JobStore;
use crate::preflight::{PreflightOptions, PreflightReport, preflight};
use crate::recipe::{record_for_path, write_output};
use crate::{BatchError, OutputRecord};
use rawweave_image::{ColorDomain, Image, PixelFormat};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

/// Cooperative cancellation shared with a decoder/processor.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// The execution boundary for a batch item. Implementations should decode and
/// evaluate one item at a time; the engine drops the returned image before it
/// advances to another item.
pub trait BatchProcessor: Send + Sync {
    fn process(
        &self,
        workflow: &PinnedWorkflow,
        item: &BatchItem,
        cancel: &CancellationToken,
    ) -> Result<Image, BatchError>;
}

impl<F> BatchProcessor for F
where
    F: Fn(&PinnedWorkflow, &BatchItem, &CancellationToken) -> Result<Image, BatchError>
        + Send
        + Sync,
{
    fn process(
        &self,
        workflow: &PinnedWorkflow,
        item: &BatchItem,
        cancel: &CancellationToken,
    ) -> Result<Image, BatchError> {
        self(workflow, item, cancel)
    }
}

/// Processor for ordinary image files. Applications with RAW or plugin-backed
/// workflows can provide their own `BatchProcessor` at the same seam.
#[derive(Clone, Copy, Debug, Default)]
pub struct ImageFileProcessor;

impl BatchProcessor for ImageFileProcessor {
    fn process(
        &self,
        _workflow: &PinnedWorkflow,
        item: &BatchItem,
        cancel: &CancellationToken,
    ) -> Result<Image, BatchError> {
        if cancel.is_cancelled() {
            return Err(BatchError::Cancelled);
        }
        let decoded = image::ImageReader::open(&item.source_path)
            .map_err(|error| BatchError::Processor(format!("could not open source: {error}")))?
            .decode()
            .map_err(|error| BatchError::Processor(format!("could not decode source: {error}")))?
            .to_rgba32f();
        if cancel.is_cancelled() {
            return Err(BatchError::Cancelled);
        }
        let (width, height) = decoded.dimensions();
        let pixels = decoded.pixels().map(|pixel| pixel.0).collect::<Vec<_>>();
        Image::from_pixels_with_metadata(
            width,
            height,
            pixels,
            PixelFormat::Rgba32Float,
            ColorDomain::Srgb,
        )
        .map_err(|error| BatchError::Processor(format!("decoded image is invalid: {error}")))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DryRunResult {
    pub workflow_revision: u64,
    pub workflow_hash: String,
    pub item_ids: Vec<String>,
    pub recipes: Vec<OutputRecipe>,
}

impl DryRunResult {
    pub fn item_ids(&self) -> Vec<String> {
        self.item_ids.clone()
    }
}

pub fn dry_run(job: &BatchJob, subset: crate::DryRunSubset) -> Result<DryRunResult, BatchError> {
    job.validate()?;
    let items = job.selected_items(&subset)?;
    Ok(DryRunResult {
        workflow_revision: job.workflow.revision,
        workflow_hash: job.workflow.hash.clone(),
        item_ids: items.into_iter().map(|item| item.id.clone()).collect(),
        recipes: job.recipes.clone(),
    })
}

struct EngineState {
    running: bool,
    active_workers: usize,
}

struct EngineInner {
    job: Mutex<BatchJob>,
    store: JobStore,
    processor: Arc<dyn BatchProcessor>,
    max_workers: usize,
    cancel: CancellationToken,
    paused: AtomicBool,
    state: Mutex<EngineState>,
    wake: Condvar,
}

/// A restartable, bounded-concurrency batch runner.
pub struct BatchEngine {
    inner: Arc<EngineInner>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl BatchEngine {
    pub fn new<P>(
        job: BatchJob,
        store: JobStore,
        processor: Arc<P>,
        max_workers: usize,
    ) -> Result<Self, BatchError>
    where
        P: BatchProcessor + 'static,
    {
        if max_workers == 0 {
            return Err(BatchError::InvalidJob(
                "worker concurrency must be greater than zero".to_owned(),
            ));
        }
        job.validate()?;
        store.save(&job)?;
        Ok(Self {
            inner: Arc::new(EngineInner {
                job: Mutex::new(job),
                store,
                processor,
                max_workers,
                cancel: CancellationToken::new(),
                paused: AtomicBool::new(false),
                state: Mutex::new(EngineState {
                    running: false,
                    active_workers: 0,
                }),
                wake: Condvar::new(),
            }),
            workers: Mutex::new(Vec::new()),
        })
    }

    pub fn resume<P>(
        store: JobStore,
        processor: Arc<P>,
        max_workers: usize,
    ) -> Result<Self, BatchError>
    where
        P: BatchProcessor + 'static,
    {
        let mut job = store.load()?;
        for item in &mut job.items {
            if item.state == ItemState::Running {
                item.state = ItemState::Waiting;
            }
        }
        job.requeue_invalid_completed()?;
        job.refresh_state();
        store.save(&job)?;
        Self::new(job, store, processor, max_workers)
    }

    pub fn store(&self) -> JobStore {
        self.inner.store.clone()
    }

    pub fn snapshot(&self) -> Result<BatchJob, BatchError> {
        self.inner
            .job
            .lock()
            .map_err(|_| BatchError::Persistence("batch job is poisoned".to_owned()))
            .map(|job| job.clone())
    }

    pub fn preflight(&self, options: &PreflightOptions) -> Result<PreflightReport, BatchError> {
        Ok(preflight(&self.snapshot()?, options))
    }

    pub fn start(&self) -> Result<(), BatchError> {
        {
            let mut state = self
                .inner
                .state
                .lock()
                .map_err(|_| BatchError::Persistence("engine state is poisoned".to_owned()))?;
            if state.running {
                return Err(BatchError::AlreadyRunning);
            }
            state.running = true;
        }
        self.inner.cancel.reset();
        self.inner.paused.store(false, Ordering::Release);

        let ids = match self.prepare_start() {
            Ok(ids) => ids,
            Err(error) => {
                if let Ok(mut state) = self.inner.state.lock() {
                    state.running = false;
                }
                return Err(error);
            }
        };
        if ids.is_empty() {
            if let Ok(mut state) = self.inner.state.lock() {
                state.running = false;
            }
            self.persist_refresh()?;
            return Ok(());
        }

        let queue = Arc::new(Mutex::new(VecDeque::from(ids)));
        let worker_count = self.inner.max_workers.min(
            queue
                .lock()
                .map_err(|_| BatchError::Persistence("work queue is poisoned".to_owned()))?
                .len(),
        );
        {
            let mut state = self
                .inner
                .state
                .lock()
                .map_err(|_| BatchError::Persistence("engine state is poisoned".to_owned()))?;
            state.active_workers = worker_count;
        }

        let mut handles = self
            .workers
            .lock()
            .map_err(|_| BatchError::Persistence("worker list is poisoned".to_owned()))?;
        handles.clear();
        for _ in 0..worker_count {
            let inner = Arc::clone(&self.inner);
            let queue = Arc::clone(&queue);
            handles.push(thread::spawn(move || worker_loop(inner, queue)));
        }
        Ok(())
    }

    fn prepare_start(&self) -> Result<Vec<String>, BatchError> {
        let mut job = self
            .inner
            .job
            .lock()
            .map_err(|_| BatchError::Persistence("batch job is poisoned".to_owned()))?;
        job.requeue_invalid_completed()?;
        job.state = BatchState::Running;
        let ids = job
            .items
            .iter()
            .filter(|item| item.state == ItemState::Waiting)
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        self.inner.store.save(&job)?;
        Ok(ids)
    }

    pub fn pause(&self) -> Result<(), BatchError> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| BatchError::Persistence("engine state is poisoned".to_owned()))?;
        if !state.running {
            return Err(BatchError::NotRunning);
        }
        drop(state);
        self.inner.paused.store(true, Ordering::Release);
        let mut job = self
            .inner
            .job
            .lock()
            .map_err(|_| BatchError::Persistence("batch job is poisoned".to_owned()))?;
        job.state = BatchState::Paused;
        self.inner.store.save(&job)?;
        self.inner.wake.notify_all();
        Ok(())
    }

    pub fn resume_run(&self) -> Result<(), BatchError> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| BatchError::Persistence("engine state is poisoned".to_owned()))?;
        if !state.running {
            return Err(BatchError::NotRunning);
        }
        drop(state);
        self.inner.paused.store(false, Ordering::Release);
        let mut job = self
            .inner
            .job
            .lock()
            .map_err(|_| BatchError::Persistence("batch job is poisoned".to_owned()))?;
        if job.state == BatchState::Paused {
            job.state = BatchState::Running;
        }
        self.inner.store.save(&job)?;
        self.inner.wake.notify_all();
        Ok(())
    }

    pub fn cancel(&self) -> Result<(), BatchError> {
        self.inner.cancel.cancel();
        self.inner.paused.store(false, Ordering::Release);
        {
            let mut job = self
                .inner
                .job
                .lock()
                .map_err(|_| BatchError::Persistence("batch job is poisoned".to_owned()))?;
            let ids = job
                .items
                .iter()
                .filter(|item| item.state == ItemState::Waiting)
                .map(|item| item.id.clone())
                .collect::<Vec<_>>();
            for id in ids {
                let _ = job.transition_item(&id, ItemState::Cancelled);
            }
            self.inner.store.save(&job)?;
        }
        self.inner.wake.notify_all();
        Ok(())
    }

    pub fn retry_failed(&self) -> Result<usize, BatchError> {
        self.ensure_stopped()?;
        let mut job = self.lock_job()?;
        let count = job.retry_failed()?;
        self.inner.store.save(&job)?;
        Ok(count)
    }

    pub fn retry_selected(&self, ids: &[String]) -> Result<usize, BatchError> {
        self.ensure_stopped()?;
        let mut job = self.lock_job()?;
        let count = job.retry_selected(ids)?;
        self.inner.store.save(&job)?;
        Ok(count)
    }

    pub fn skip(&self, ids: &[String]) -> Result<usize, BatchError> {
        self.ensure_stopped()?;
        let mut job = self.lock_job()?;
        let count = job.skip(ids)?;
        self.inner.store.save(&job)?;
        Ok(count)
    }

    pub fn failed_item(&self, id: &str) -> Result<BatchItem, BatchError> {
        let job = self.snapshot()?;
        let item = job
            .items
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| BatchError::UnknownItem(id.to_owned()))?;
        if item.state != ItemState::Failed {
            return Err(BatchError::InvalidJob(format!(
                "batch item '{id}' is not failed"
            )));
        }
        Ok(item)
    }

    pub fn wait(&self) -> Result<(), BatchError> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| BatchError::Persistence("engine state is poisoned".to_owned()))?;
        while state.running {
            state = self
                .inner
                .wake
                .wait(state)
                .map_err(|_| BatchError::Persistence("engine state is poisoned".to_owned()))?;
        }
        drop(state);
        let mut handles = self
            .workers
            .lock()
            .map_err(|_| BatchError::Persistence("worker list is poisoned".to_owned()))?;
        for handle in handles.drain(..) {
            let _ = handle.join();
        }
        Ok(())
    }

    fn ensure_stopped(&self) -> Result<(), BatchError> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| BatchError::Persistence("engine state is poisoned".to_owned()))?;
        if state.running {
            Err(BatchError::AlreadyRunning)
        } else {
            Ok(())
        }
    }

    fn lock_job(&self) -> Result<std::sync::MutexGuard<'_, BatchJob>, BatchError> {
        self.inner
            .job
            .lock()
            .map_err(|_| BatchError::Persistence("batch job is poisoned".to_owned()))
    }

    fn persist_refresh(&self) -> Result<(), BatchError> {
        let mut job = self.lock_job()?;
        job.refresh_state();
        self.inner.store.save(&job)
    }
}

fn worker_loop(inner: Arc<EngineInner>, queue: Arc<Mutex<VecDeque<String>>>) {
    loop {
        if inner.cancel.is_cancelled() {
            break;
        }
        let id = {
            let mut queue_guard = match queue.lock() {
                Ok(queue_guard) => queue_guard,
                Err(_) => break,
            };
            queue_guard.pop_front()
        };
        let Some(id) = id else { break };

        let paused = inner.paused.load(Ordering::Acquire);
        if paused {
            let mut state = match inner.state.lock() {
                Ok(state) => state,
                Err(_) => break,
            };
            while inner.paused.load(Ordering::Acquire) && !inner.cancel.is_cancelled() {
                state = match inner.wake.wait(state) {
                    Ok(state) => state,
                    Err(_) => return,
                };
            }
            drop(state);
        }
        if inner.cancel.is_cancelled() {
            break;
        }
        process_item(&inner, &id);
    }
    finish_worker(&inner);
}

fn process_item(inner: &Arc<EngineInner>, id: &str) {
    let (workflow, item, recipes) = {
        let mut job = match inner.job.lock() {
            Ok(job) => job,
            Err(_) => return,
        };
        let Some(item) = job.items.iter().find(|item| item.id == id).cloned() else {
            return;
        };
        if item.state != ItemState::Waiting {
            return;
        }
        if job.transition_item(id, ItemState::Running).is_err() {
            return;
        }
        if inner.store.save(&job).is_err() {
            return;
        }
        (job.workflow.clone(), item, job.recipes.clone())
    };

    let result = inner.processor.process(&workflow, &item, &inner.cancel);
    let image = match result {
        Ok(image) if !inner.cancel.is_cancelled() => image,
        Ok(_) => {
            finish_cancelled(inner, id);
            return;
        }
        Err(BatchError::Cancelled) => {
            finish_cancelled(inner, id);
            return;
        }
        Err(error) => {
            finish_failed(inner, id, error.to_string());
            return;
        }
    };

    let mut outputs = Vec::<OutputRecord>::with_capacity(recipes.len());
    for (recipe_index, recipe) in recipes.iter().enumerate() {
        if inner.cancel.is_cancelled() {
            finish_cancelled(inner, id);
            return;
        }
        match write_output(&image, recipe, &item, recipe_index, None).and_then(record_for_path) {
            Ok(output) => outputs.push(output),
            Err(error) => {
                finish_failed(inner, id, error.to_string());
                return;
            }
        }
    }
    if inner.cancel.is_cancelled() {
        finish_cancelled(inner, id);
    } else {
        finish_completed(inner, id, outputs);
    }
}

fn finish_cancelled(inner: &Arc<EngineInner>, id: &str) {
    if let Ok(mut job) = inner.job.lock() {
        let _ = job.transition_item(id, ItemState::Cancelled);
        let _ = inner.store.save(&job);
    }
}

fn finish_failed(inner: &Arc<EngineInner>, id: &str, message: String) {
    if let Ok(mut job) = inner.job.lock() {
        let _ = job.fail_item(id, message);
        let _ = inner.store.save(&job);
    }
}

fn finish_completed(inner: &Arc<EngineInner>, id: &str, outputs: Vec<OutputRecord>) {
    if let Ok(mut job) = inner.job.lock() {
        let _ = job.complete_item(id, outputs);
        let _ = inner.store.save(&job);
    }
}

fn finish_worker(inner: &Arc<EngineInner>) {
    if let Ok(mut state) = inner.state.lock() {
        state.active_workers = state.active_workers.saturating_sub(1);
        if state.active_workers == 0 {
            state.running = false;
            if let Ok(mut job) = inner.job.lock() {
                job.refresh_state();
                let _ = inner.store.save(&job);
            }
            inner.wake.notify_all();
        }
    }
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, BatchError> {
    use sha2::{Digest, Sha256};
    let mut file = fs::File::open(path).map_err(|source| BatchError::Io {
        operation: "hash file",
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|source| BatchError::Io {
            operation: "read file while hashing",
            path: path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

use std::io::Read;
