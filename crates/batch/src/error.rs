use crate::model::{BitDepth, OutputFormat};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BatchError {
    #[error("unsupported batch schema version {0}")]
    UnsupportedSchema(u32),
    #[error("invalid batch job: {0}")]
    InvalidJob(String),
    #[error("invalid output recipe: {0}")]
    InvalidRecipe(String),
    #[error("workflow is invalid: {0}")]
    Workflow(String),
    #[error("pinned workflow hash mismatch: expected {expected}, got {actual}")]
    WorkflowHashMismatch { expected: String, actual: String },
    #[error("unknown batch item '{0}'")]
    UnknownItem(String),
    #[error("invalid state transition for '{item_id}': {from:?} -> {to:?}")]
    InvalidTransition {
        item_id: String,
        from: crate::ItemState,
        to: crate::ItemState,
    },
    #[error("I/O while trying to {operation} '{}': {source}", path.display())]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("JSON serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("output format {format:?} does not support {bit_depth:?}")]
    UnsupportedBitDepth {
        format: OutputFormat,
        bit_depth: BitDepth,
    },
    #[error("unsupported color transform: {0}")]
    UnsupportedColorTransform(String),
    #[error("unsupported workflow output: {0}")]
    UnsupportedWorkflowOutput(String),
    #[error("processor failed: {0}")]
    Processor(String),
    #[error("checkpoint policy failed: {0}")]
    CheckpointPolicy(String),
    #[error("checkpoint operation failed: {0}")]
    Checkpoint(#[from] rawweave_graph::CheckpointError),
    #[error("batch was cancelled")]
    Cancelled,
    #[error("batch is already running")]
    AlreadyRunning,
    #[error("batch is not running")]
    NotRunning,
    #[error("persistence failure: {0}")]
    Persistence(String),
    #[error("output collision at '{0}'")]
    OutputCollision(PathBuf),
    #[error("output directory '{0}' is not writable")]
    OutputDirectory(PathBuf),
    #[error("diagnostic error: {0}")]
    Diagnostic(String),
}
