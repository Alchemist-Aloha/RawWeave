//! Production-oriented batch processing primitives for RawWeave.
//!
//! The crate deliberately keeps decoding/evaluation behind `BatchProcessor` so
//! the scheduler can enforce bounded image lifetimes without knowing whether a
//! source is an ordinary image, a RAW file, or a plugin-backed asset.

mod checkpoint;
mod engine;
pub mod model;
mod ordinary;
mod persistence;
mod preflight;
pub mod recipe;

pub use checkpoint::{CheckpointResolution, CheckpointRuntime};
pub use engine::{
    BatchEngine, BatchProcessor, CancellationToken, DryRunResult, ImageFileProcessor,
    MAX_BATCH_WORKERS, dry_run,
};
pub use model::{
    BATCH_JOB_SCHEMA_VERSION, BatchItem, BatchJob, BatchState, BitDepth, CheckpointPolicy,
    CollisionPolicy, ColorSpace, Compression, DryRunSubset, ItemState, MetadataPolicy,
    OutputFormat, OutputRecipe, OutputRecord, OutputSharpening, PinnedDependencies, PinnedWorkflow,
    Resolution,
};
pub use ordinary::{
    MAX_ORDINARY_ENCODED_BYTES, MAX_ORDINARY_IMAGE_EDGE, MAX_ORDINARY_IMAGE_PIXELS,
    MAX_ORDINARY_RGBA32F_BYTES, OrdinaryDecodeError, decode_ordinary_bytes, decode_ordinary_file,
    validate_dimensions,
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
