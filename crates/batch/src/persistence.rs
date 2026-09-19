use crate::{BatchError, BatchJob};
use rawweave_graph::ArtifactStore;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
enum JobStoreBackend {
    File(Arc<PathBuf>),
    Memory(Arc<Mutex<Option<BatchJob>>>),
}

/// Durable storage for a batch job.
///
/// File-backed saves are written to a sibling temporary file, flushed, synced,
/// and renamed into place. The in-memory backend is useful for callers that
/// already own a higher-level persistence boundary and for tests.
#[derive(Clone)]
pub struct JobStore {
    backend: JobStoreBackend,
}

impl JobStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            backend: JobStoreBackend::File(Arc::new(path.into())),
        }
    }

    pub fn memory() -> Self {
        Self {
            backend: JobStoreBackend::Memory(Arc::new(Mutex::new(None))),
        }
    }

    pub fn path(&self) -> Option<&Path> {
        match &self.backend {
            JobStoreBackend::File(path) => Some(path.as_path()),
            JobStoreBackend::Memory(_) => None,
        }
    }

    /// Return the artifact store paired with this job's persistence boundary.
    /// File-backed jobs keep committed checkpoint payloads beside the job state;
    /// memory-backed jobs remain fully in memory for callers and tests.
    pub fn artifact_store(&self) -> ArtifactStore {
        match &self.backend {
            JobStoreBackend::File(path) => ArtifactStore::new(path.with_extension("artifacts")),
            JobStoreBackend::Memory(_) => ArtifactStore::memory(),
        }
    }

    pub fn exists(&self) -> bool {
        self.path().is_some_and(Path::exists)
    }

    pub fn save(&self, job: &BatchJob) -> Result<(), BatchError> {
        job.validate()?;
        match &self.backend {
            JobStoreBackend::Memory(job_slot) => job_slot
                .lock()
                .map_err(|_| BatchError::Persistence("memory store is poisoned".to_owned()))
                .map(|mut slot| {
                    *slot = Some(job.clone());
                }),
            JobStoreBackend::File(path) => save_file(path, job),
        }
    }

    pub fn load(&self) -> Result<BatchJob, BatchError> {
        match &self.backend {
            JobStoreBackend::Memory(job_slot) => job_slot
                .lock()
                .map_err(|_| BatchError::Persistence("memory store is poisoned".to_owned()))?
                .clone()
                .ok_or_else(|| BatchError::Persistence("memory store is empty".to_owned())),
            JobStoreBackend::File(path) => load_file(path),
        }
    }

    pub fn clear(&self) -> Result<(), BatchError> {
        match &self.backend {
            JobStoreBackend::Memory(job_slot) => {
                *job_slot.lock().map_err(|_| {
                    BatchError::Persistence("memory store is poisoned".to_owned())
                })? = None;
                Ok(())
            }
            JobStoreBackend::File(path) => match fs::remove_file(path.as_path()) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(source) => Err(BatchError::Io {
                    operation: "remove persisted batch job",
                    path: path.as_ref().clone(),
                    source,
                }),
            },
        }
    }
}

fn save_file(path: &Path, job: &BatchJob) -> Result<(), BatchError> {
    let bytes = serde_json::to_vec_pretty(job)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| BatchError::Io {
        operation: "create batch state directory",
        path: parent.to_path_buf(),
        source,
    })?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("batch.json");
    let temporary = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|source| BatchError::Io {
                operation: "create temporary batch state",
                path: temporary.clone(),
                source,
            })?;
        file.write_all(&bytes).map_err(|source| BatchError::Io {
            operation: "write temporary batch state",
            path: temporary.clone(),
            source,
        })?;
        file.sync_all().map_err(|source| BatchError::Io {
            operation: "sync temporary batch state",
            path: temporary.clone(),
            source,
        })?;
        fs::rename(&temporary, path).map_err(|source| BatchError::Io {
            operation: "install batch state atomically",
            path: path.to_path_buf(),
            source,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn load_file(path: &Path) -> Result<BatchJob, BatchError> {
    let mut file = File::open(path).map_err(|source| BatchError::Io {
        operation: "open persisted batch state",
        path: path.to_path_buf(),
        source,
    })?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|source| BatchError::Io {
            operation: "read persisted batch state",
            path: path.to_path_buf(),
            source,
        })?;
    let job: BatchJob = serde_json::from_slice(&bytes)?;
    job.validate()?;
    Ok(job)
}
