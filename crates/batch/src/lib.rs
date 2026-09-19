//! Production-oriented batch processing primitives for RawWeave.
//!
//! The crate deliberately keeps decoding/evaluation behind `BatchProcessor` so
//! the scheduler can enforce bounded image lifetimes without knowing whether a
//! source is an ordinary image, a RAW file, or a plugin-backed asset.

mod engine;
pub mod model;
mod persistence;
mod preflight;
pub mod recipe;

pub use engine::{
    BatchEngine, BatchProcessor, CancellationToken, DryRunResult, ImageFileProcessor, dry_run,
};
pub use model::{
    BATCH_JOB_SCHEMA_VERSION, BatchItem, BatchJob, BatchState, BitDepth, CheckpointPolicy,
    CollisionPolicy, ColorSpace, Compression, DryRunSubset, ItemState, MetadataPolicy,
    OutputFormat, OutputRecipe, OutputRecord, OutputSharpening, PinnedDependencies, PinnedWorkflow,
    Resolution,
};
pub use persistence::JobStore;
pub use preflight::{Diagnostic, DiagnosticSeverity, PreflightOptions, PreflightReport, preflight};
pub use recipe::write_output;

use std::path::Path;

pub(crate) fn sha256_file(path: &Path) -> Result<String, BatchError> {
    engine::sha256_file(path)
}

pub use crate::error::BatchError;
mod error;
