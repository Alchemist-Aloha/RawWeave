use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rawweave_image::{Image, Mask};
use rawweave_node_api::Value;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const CHECKPOINT_ARTIFACT_SCHEMA_VERSION: u32 = 1;

/// Lifecycle of a manual checkpoint generation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointState {
    #[default]
    Ungenerated,
    Generating,
    Current,
    Stale,
    Failed,
    Cancelled,
}

/// Whether a committed checkpoint can satisfy the current graph inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointAvailability {
    Fresh,
    Stale,
    Missing,
    Incompatible,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ArtifactId {
    Sha256(String),
}

impl ArtifactId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, CheckpointError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(CheckpointError::InvalidArtifactId(value));
        }
        Ok(Self::Sha256(value))
    }

    fn validate(&self) -> Result<(), CheckpointError> {
        Self::try_new(self.as_str()).map(|_| ())
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Sha256(value) => value,
        }
    }
}

impl Serialize for ArtifactId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_newtype_variant("ArtifactId", 0, "Sha256", self.as_str())
    }
}

impl<'de> Deserialize<'de> for ArtifactId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        enum WireArtifactId {
            Sha256(String),
        }

        match WireArtifactId::deserialize(deserializer)? {
            WireArtifactId::Sha256(value) => Self::try_new(value).map_err(de::Error::custom),
        }
    }
}

impl fmt::Display for ArtifactId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum CheckpointPayload {
    Image(Image),
    Mask(Mask),
    SpatialData(Vec<u8>),
}

impl CheckpointPayload {
    /// Project a committed artifact back into graph space for downstream
    /// automatic nodes.
    pub fn to_value(&self) -> Value {
        match self {
            Self::Image(image) => Value::Image(image.clone()),
            Self::Mask(mask) => Value::Mask(mask.clone()),
            Self::SpatialData(bytes) => Value::Bytes(bytes.clone()),
        }
    }
}

impl CheckpointPayload {
    fn validate(&self, limits: &ArtifactImportLimits) -> Result<(), CheckpointError> {
        match self {
            Self::Image(image) => {
                let dimensions = image.dimensions();
                let pixels = dimensions.pixel_count().map_err(|_| {
                    CheckpointError::InvalidPayload("image dimensions overflow".into())
                })?;
                if image.pixels().len() != pixels {
                    return Err(CheckpointError::InvalidPayload(format!(
                        "image dimensions require {pixels} pixels, got {}",
                        image.pixels().len()
                    )));
                }
                if dimensions.width.checked_add(image.origin().0).is_none()
                    || dimensions.height.checked_add(image.origin().1).is_none()
                {
                    return Err(CheckpointError::InvalidPayload(
                        "image origin overflows dimensions".into(),
                    ));
                }
                if pixels > limits.max_pixels {
                    return Err(CheckpointError::ImportLimitExceeded {
                        resource: "pixels",
                        limit: limits.max_pixels,
                    });
                }
                if image
                    .pixels()
                    .iter()
                    .flatten()
                    .any(|channel| !channel.is_finite())
                {
                    return Err(CheckpointError::InvalidPayload(
                        "image contains a non-finite channel".into(),
                    ));
                }
            }
            Self::Mask(mask) => {
                let dimensions = mask.dimensions();
                let pixels = dimensions.pixel_count().map_err(|_| {
                    CheckpointError::InvalidPayload("mask dimensions overflow".into())
                })?;
                if mask.tile_size() == 0 {
                    return Err(CheckpointError::InvalidPayload(
                        "mask tile size must be greater than zero".into(),
                    ));
                }
                if dimensions.width.checked_add(mask.origin().0).is_none()
                    || dimensions.height.checked_add(mask.origin().1).is_none()
                {
                    return Err(CheckpointError::InvalidPayload(
                        "mask origin overflows dimensions".into(),
                    ));
                }
                let tiles_per_row = dimensions.width.div_ceil(mask.tile_size());
                let tiles_per_column = dimensions.height.div_ceil(mask.tile_size());
                let expected_tiles = (tiles_per_row as usize)
                    .checked_mul(tiles_per_column as usize)
                    .ok_or_else(|| {
                        CheckpointError::InvalidPayload("mask tile count overflow".into())
                    })?;
                if mask.tiles().len() != expected_tiles {
                    return Err(CheckpointError::InvalidPayload(format!(
                        "mask dimensions require {expected_tiles} tiles, got {}",
                        mask.tiles().len()
                    )));
                }
                for (index, tile) in mask.tiles().iter().enumerate() {
                    let index = u32::try_from(index).map_err(|_| {
                        CheckpointError::InvalidPayload("mask tile index overflow".into())
                    })?;
                    let tile_x = index % tiles_per_row;
                    let tile_y = index / tiles_per_row;
                    let x = tile_x.checked_mul(mask.tile_size()).ok_or_else(|| {
                        CheckpointError::InvalidPayload("mask tile origin overflow".into())
                    })?;
                    let y = tile_y.checked_mul(mask.tile_size()).ok_or_else(|| {
                        CheckpointError::InvalidPayload("mask tile origin overflow".into())
                    })?;
                    let width = mask.tile_size().min(dimensions.width.saturating_sub(x));
                    let height = mask.tile_size().min(dimensions.height.saturating_sub(y));
                    let expected_origin = (
                        mask.origin().0.checked_add(x).ok_or_else(|| {
                            CheckpointError::InvalidPayload("mask tile origin overflow".into())
                        })?,
                        mask.origin().1.checked_add(y).ok_or_else(|| {
                            CheckpointError::InvalidPayload("mask tile origin overflow".into())
                        })?,
                    );
                    if tile.origin != expected_origin
                        || tile.dimensions != rawweave_image::Dimensions::new(width, height)
                    {
                        return Err(CheckpointError::InvalidPayload(
                            "mask tile geometry does not match mask dimensions".into(),
                        ));
                    }
                    let expected_values = (width as usize)
                        .checked_mul(height as usize)
                        .ok_or_else(|| {
                            CheckpointError::InvalidPayload("mask tile value count overflow".into())
                        })?;
                    if tile.values.len() != expected_values {
                        return Err(CheckpointError::InvalidPayload(format!(
                            "mask tile requires {expected_values} values, got {}",
                            tile.values.len()
                        )));
                    }
                }
                if pixels > limits.max_pixels {
                    return Err(CheckpointError::ImportLimitExceeded {
                        resource: "pixels",
                        limit: limits.max_pixels,
                    });
                }
                if mask
                    .tiles()
                    .iter()
                    .flat_map(|tile| tile.values.iter())
                    .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
                {
                    return Err(CheckpointError::InvalidPayload(
                        "mask contains an invalid value".into(),
                    ));
                }
            }
            Self::SpatialData(bytes) if bytes.len() > limits.max_spatial_bytes => {
                return Err(CheckpointError::ImportLimitExceeded {
                    resource: "spatial_data",
                    limit: limits.max_spatial_bytes,
                });
            }
            Self::SpatialData(_) => {}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalToolMetadata {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub parameters: BTreeMap<String, String>,
}

impl ExternalToolMetadata {
    pub fn new(id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Hash of all inputs that affect the generation. For a provenance built
    /// from individual upstream hashes this is computed with SHA-256.
    pub dependency_hash: String,
    #[serde(default)]
    pub upstream_hashes: BTreeMap<String, String>,
    pub node_version: u32,
    #[serde(default)]
    pub external_tool: Option<ExternalToolMetadata>,
}

impl Provenance {
    pub fn new(dependency_hash: impl Into<String>, node_version: u32) -> Self {
        Self {
            dependency_hash: dependency_hash.into(),
            upstream_hashes: BTreeMap::new(),
            node_version,
            external_tool: None,
        }
    }

    pub fn with_upstream_hashes(
        upstream_hashes: BTreeMap<String, String>,
        node_version: u32,
    ) -> Self {
        let dependency_hash = hash_upstream_inputs(&upstream_hashes, node_version);
        Self {
            dependency_hash,
            upstream_hashes,
            node_version,
            external_tool: None,
        }
    }

    pub fn with_external_tool(mut self, external_tool: ExternalToolMetadata) -> Self {
        self.external_tool = Some(external_tool);
        self
    }

    pub fn dependency_hash(&self) -> &str {
        &self.dependency_hash
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationMetadata {
    pub generation_revision: u64,
    #[serde(default)]
    pub generated_at: Option<String>,
    #[serde(default)]
    pub duration_millis: Option<u64>,
    #[serde(default)]
    pub generator: Option<String>,
}

impl GenerationMetadata {
    pub fn new(generation_revision: u64) -> Self {
        Self {
            generation_revision,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CheckpointArtifact {
    pub schema_version: u32,
    id: ArtifactId,
    pub payload: CheckpointPayload,
    pub dependency_hash: String,
    pub provenance: Provenance,
    pub generation: GenerationMetadata,
}

impl CheckpointArtifact {
    pub fn new(
        payload: CheckpointPayload,
        dependency_hash: impl Into<String>,
        provenance: Provenance,
        generation: GenerationMetadata,
    ) -> Result<Self, CheckpointError> {
        let dependency_hash = dependency_hash.into();
        if dependency_hash != provenance.dependency_hash {
            return Err(CheckpointError::InvalidProvenance(
                "artifact and provenance dependency hashes differ".into(),
            ));
        }
        let artifact = Self {
            schema_version: CHECKPOINT_ARTIFACT_SCHEMA_VERSION,
            id: ArtifactId::Sha256(String::new()),
            payload,
            dependency_hash,
            provenance,
            generation,
        };
        artifact.validate_payload()?;
        let id = artifact.compute_id()?;
        Ok(Self { id, ..artifact })
    }

    pub fn id(&self) -> &ArtifactId {
        &self.id
    }

    pub fn payload(&self) -> &CheckpointPayload {
        &self.payload
    }

    pub fn compute_id(&self) -> Result<ArtifactId, CheckpointError> {
        let document = ArtifactContent {
            schema_version: self.schema_version,
            payload: CanonicalPayload::from(&self.payload),
            dependency_hash: &self.dependency_hash,
            provenance: &self.provenance,
            generation: &self.generation,
        };
        let bytes = serde_json::to_vec(&document)?;
        Ok(ArtifactId::Sha256(hex_digest(&bytes)))
    }

    pub fn validate(&self) -> Result<(), CheckpointError> {
        self.id.validate()?;
        if self.schema_version != CHECKPOINT_ARTIFACT_SCHEMA_VERSION {
            return Err(CheckpointError::UnsupportedSchema(self.schema_version));
        }
        if self.dependency_hash != self.provenance.dependency_hash {
            return Err(CheckpointError::InvalidProvenance(
                "artifact and provenance dependency hashes differ".into(),
            ));
        }
        self.validate_payload()?;
        let expected = self.compute_id()?;
        if expected != self.id {
            return Err(CheckpointError::ArtifactIdMismatch {
                expected,
                actual: self.id.clone(),
            });
        }
        Ok(())
    }

    fn validate_payload(&self) -> Result<(), CheckpointError> {
        self.payload.validate(&ArtifactImportLimits::default())
    }

    fn validate_with_limits(&self, limits: &ArtifactImportLimits) -> Result<(), CheckpointError> {
        self.id.validate()?;
        if self.schema_version != CHECKPOINT_ARTIFACT_SCHEMA_VERSION {
            return Err(CheckpointError::UnsupportedSchema(self.schema_version));
        }
        if self.dependency_hash != self.provenance.dependency_hash {
            return Err(CheckpointError::InvalidProvenance(
                "artifact and provenance dependency hashes differ".into(),
            ));
        }
        if self.provenance.upstream_hashes.len() > limits.max_upstream_hashes {
            return Err(CheckpointError::ImportLimitExceeded {
                resource: "upstream_hashes",
                limit: limits.max_upstream_hashes,
            });
        }
        let metadata_bytes = serde_json::to_vec(&self.generation)?.len();
        if metadata_bytes > limits.max_metadata_bytes {
            return Err(CheckpointError::ImportLimitExceeded {
                resource: "metadata",
                limit: limits.max_metadata_bytes,
            });
        }
        self.payload.validate(limits)?;
        let expected = self.compute_id()?;
        if expected != self.id {
            return Err(CheckpointError::ArtifactIdMismatch {
                expected,
                actual: self.id.clone(),
            });
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct ArtifactContent<'a> {
    schema_version: u32,
    payload: CanonicalPayload<'a>,
    dependency_hash: &'a str,
    provenance: &'a Provenance,
    generation: &'a GenerationMetadata,
}

/// Runtime image and mask values contain process-local revisions used by the
/// render cache. They are intentionally excluded from artifact identity so
/// reconstructing the same content after a restart produces the same id.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CanonicalPayload<'a> {
    Image(CanonicalImage<'a>),
    Mask(CanonicalMask<'a>),
    SpatialData { bytes: &'a [u8] },
}

#[derive(Serialize)]
struct CanonicalImage<'a> {
    dimensions: rawweave_image::Dimensions,
    origin: (u32, u32),
    pixel_format: rawweave_image::PixelFormat,
    color_metadata: rawweave_image::ColorMetadata,
    pixels: &'a [[f32; 4]],
}

#[derive(Serialize)]
struct CanonicalMask<'a> {
    dimensions: rawweave_image::Dimensions,
    origin: (u32, u32),
    tile_size: u32,
    tiles: &'a [rawweave_image::MaskTile],
}

impl<'a> From<&'a CheckpointPayload> for CanonicalPayload<'a> {
    fn from(payload: &'a CheckpointPayload) -> Self {
        match payload {
            CheckpointPayload::Image(image) => Self::Image(CanonicalImage {
                dimensions: image.dimensions(),
                origin: image.origin(),
                pixel_format: image.pixel_format(),
                color_metadata: image.color_metadata(),
                pixels: image.pixels(),
            }),
            CheckpointPayload::Mask(mask) => Self::Mask(CanonicalMask {
                dimensions: mask.dimensions(),
                origin: mask.origin(),
                tile_size: mask.tile_size(),
                tiles: mask.tiles(),
            }),
            CheckpointPayload::SpatialData(bytes) => Self::SpatialData { bytes },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArtifactImportLimits {
    pub max_bytes: usize,
    pub max_pixels: usize,
    pub max_spatial_bytes: usize,
    pub max_metadata_bytes: usize,
    pub max_upstream_hashes: usize,
}

impl Default for ArtifactImportLimits {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_pixels: 16_777_216,
            max_spatial_bytes: 16 * 1024 * 1024,
            max_metadata_bytes: 1024 * 1024,
            max_upstream_hashes: 10_000,
        }
    }
}

#[derive(Debug, Error)]
pub enum CheckpointError {
    #[error("checkpoint artifact schema version {0} is unsupported")]
    UnsupportedSchema(u32),
    #[error("checkpoint artifact id mismatch: expected {expected}, got {actual}")]
    ArtifactIdMismatch {
        expected: ArtifactId,
        actual: ArtifactId,
    },
    #[error("invalid checkpoint payload: {0}")]
    InvalidPayload(String),
    #[error("invalid artifact id '{0}': expected exactly 64 lowercase sha256 hex characters")]
    InvalidArtifactId(String),
    #[error("invalid checkpoint provenance: {0}")]
    InvalidProvenance(String),
    #[error("checkpoint import exceeds the {resource} limit of {limit}")]
    ImportLimitExceeded {
        resource: &'static str,
        limit: usize,
    },
    #[error("checkpoint artifact '{0}' was not found")]
    MissingArtifact(ArtifactId),
    #[error("checkpoint node version mismatch: expected {expected}, got {actual}")]
    NodeVersionMismatch { expected: u32, actual: u32 },
    #[error("checkpoint dependency hash mismatch: expected {expected}, got {actual}")]
    DependencyHashMismatch { expected: String, actual: String },
    #[error("checkpoint is already generating")]
    AlreadyGenerating,
    #[error("checkpoint generation token is no longer active")]
    GenerationNotActive,
    #[error("checkpoint generation id is exhausted")]
    GenerationIdExhausted,
    #[error("checkpoint has no committed artifact")]
    NoCommittedArtifact,
    #[error("checkpoint store is poisoned")]
    StorePoisoned,
    #[error("checkpoint persistence failed while trying to {operation} '{path}': {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("checkpoint serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Clone, Debug)]
enum ArtifactStoreBackend {
    Memory(Arc<Mutex<BTreeMap<ArtifactId, CheckpointArtifact>>>),
    File(Arc<PathBuf>),
}

/// Durable content-addressed storage for committed checkpoint artifacts.
#[derive(Clone, Debug)]
pub struct ArtifactStore {
    backend: ArtifactStoreBackend,
    limits: ArtifactImportLimits,
}

impl Default for ArtifactStore {
    fn default() -> Self {
        Self::memory()
    }
}

impl ArtifactStore {
    pub fn memory() -> Self {
        Self {
            backend: ArtifactStoreBackend::Memory(Arc::new(Mutex::new(BTreeMap::new()))),
            limits: ArtifactImportLimits::default(),
        }
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            backend: ArtifactStoreBackend::File(Arc::new(root.into())),
            limits: ArtifactImportLimits::default(),
        }
    }

    pub fn with_limits(mut self, limits: ArtifactImportLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn put(&self, artifact: &CheckpointArtifact) -> Result<ArtifactId, CheckpointError> {
        let bytes = serde_json::to_vec_pretty(artifact)?;
        if bytes.len() > self.limits.max_bytes {
            return Err(CheckpointError::ImportLimitExceeded {
                resource: "bytes",
                limit: self.limits.max_bytes,
            });
        }
        artifact.validate_with_limits(&self.limits)?;
        match &self.backend {
            ArtifactStoreBackend::Memory(artifacts) => {
                let mut artifacts = artifacts
                    .lock()
                    .map_err(|_| CheckpointError::StorePoisoned)?;
                artifacts.insert(artifact.id.clone(), artifact.clone());
            }
            ArtifactStoreBackend::File(root) => {
                let path = artifact_path(root, &artifact.id)?;
                write_atomically(&path, &bytes)?;
            }
        }
        Ok(artifact.id.clone())
    }

    pub fn get(&self, id: &ArtifactId) -> Result<Option<CheckpointArtifact>, CheckpointError> {
        id.validate()?;
        let artifact = match &self.backend {
            ArtifactStoreBackend::Memory(artifacts) => artifacts
                .lock()
                .map_err(|_| CheckpointError::StorePoisoned)?
                .get(id)
                .cloned(),
            ArtifactStoreBackend::File(root) => {
                let path = artifact_path(root, id)?;
                if !path.is_file() {
                    None
                } else {
                    let bytes = read_bounded(&path, self.limits.max_bytes)?;
                    Some(serde_json::from_slice::<CheckpointArtifact>(&bytes)?)
                }
            }
        };
        if let Some(artifact) = &artifact {
            artifact.validate_with_limits(&self.limits)?;
        }
        Ok(artifact)
    }

    pub fn require(&self, id: &ArtifactId) -> Result<CheckpointArtifact, CheckpointError> {
        self.get(id)?
            .ok_or_else(|| CheckpointError::MissingArtifact(id.clone()))
    }

    pub fn validate(&self, id: &ArtifactId) -> Result<bool, CheckpointError> {
        Ok(self.get(id)?.is_some())
    }

    pub fn export(&self, id: &ArtifactId) -> Result<Vec<u8>, CheckpointError> {
        let artifact = self.require(id)?;
        Ok(serde_json::to_vec_pretty(&artifact)?)
    }

    pub fn import(&self, bytes: &[u8]) -> Result<ArtifactId, CheckpointError> {
        if bytes.len() > self.limits.max_bytes {
            return Err(CheckpointError::ImportLimitExceeded {
                resource: "bytes",
                limit: self.limits.max_bytes,
            });
        }
        let artifact = serde_json::from_slice::<CheckpointArtifact>(bytes)?;
        artifact.validate_with_limits(&self.limits)?;
        self.put(&artifact)
    }
}

fn artifact_path(root: &Path, id: &ArtifactId) -> Result<PathBuf, CheckpointError> {
    id.validate()?;
    Ok(root.join(format!("{}.json", id.as_str())))
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), CheckpointError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| CheckpointError::Io {
        operation: "create artifact directory",
        path: parent.to_path_buf(),
        source,
    })?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("artifact"),
        std::process::id()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|source| CheckpointError::Io {
                operation: "create temporary artifact",
                path: temporary.clone(),
                source,
            })?;
        file.write_all(bytes)
            .map_err(|source| CheckpointError::Io {
                operation: "write temporary artifact",
                path: temporary.clone(),
                source,
            })?;
        file.sync_all().map_err(|source| CheckpointError::Io {
            operation: "sync temporary artifact",
            path: temporary.clone(),
            source,
        })?;
        fs::rename(&temporary, path).map_err(|source| CheckpointError::Io {
            operation: "install artifact",
            path: path.to_path_buf(),
            source,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn read_bounded(path: &Path, max_bytes: usize) -> Result<Vec<u8>, CheckpointError> {
    let metadata = fs::metadata(path).map_err(|source| CheckpointError::Io {
        operation: "inspect artifact",
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.len() > max_bytes as u64 {
        return Err(CheckpointError::ImportLimitExceeded {
            resource: "bytes",
            limit: max_bytes,
        });
    }
    let file = File::open(path).map_err(|source| CheckpointError::Io {
        operation: "open artifact",
        path: path.to_path_buf(),
        source,
    })?;
    let capacity = usize::try_from(metadata.len()).unwrap_or(max_bytes);
    let mut bytes = Vec::with_capacity(capacity);
    file.take((max_bytes as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| CheckpointError::Io {
            operation: "read artifact",
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > max_bytes {
        return Err(CheckpointError::ImportLimitExceeded {
            resource: "bytes",
            limit: max_bytes,
        });
    }
    Ok(bytes)
}

/// Opaque handle for one checkpoint generation attempt.
///
/// A token is intentionally not serialized with a checkpoint. It identifies
/// an in-memory generation attempt and therefore cannot be reused after a
/// workflow restart or after another attempt becomes active.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GenerationToken(u64);

impl GenerationToken {
    pub fn generation_id(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    pub node_id: String,
    pub node_version: u32,
    current_dependency_hash: Option<String>,
    committed_dependency_hash: Option<String>,
    committed_artifact_id: Option<ArtifactId>,
    state: CheckpointState,
    #[serde(default)]
    pub generation: Option<GenerationMetadata>,
    #[serde(default)]
    pub failure: Option<String>,
    #[serde(skip)]
    next_generation_id: u64,
    #[serde(skip)]
    active_generation: Option<GenerationToken>,
}

impl PartialEq for Checkpoint {
    fn eq(&self, other: &Self) -> bool {
        self.node_id == other.node_id
            && self.node_version == other.node_version
            && self.current_dependency_hash == other.current_dependency_hash
            && self.committed_dependency_hash == other.committed_dependency_hash
            && self.committed_artifact_id == other.committed_artifact_id
            && self.state == other.state
            && self.generation == other.generation
            && self.failure == other.failure
    }
}

impl Checkpoint {
    pub fn new(node_id: impl Into<String>, node_version: u32) -> Self {
        Self {
            node_id: node_id.into(),
            node_version,
            current_dependency_hash: None,
            committed_dependency_hash: None,
            committed_artifact_id: None,
            state: CheckpointState::Ungenerated,
            generation: None,
            failure: None,
            next_generation_id: 0,
            active_generation: None,
        }
    }

    pub fn state(&self) -> CheckpointState {
        self.state
    }

    pub fn current_dependency_hash(&self) -> Option<&str> {
        self.current_dependency_hash.as_deref()
    }

    pub fn committed_dependency_hash(&self) -> Option<&str> {
        self.committed_dependency_hash.as_deref()
    }

    pub fn committed_artifact_id(&self) -> Option<&ArtifactId> {
        self.committed_artifact_id.as_ref()
    }

    pub fn set_dependency_hash(&mut self, dependency_hash: impl Into<String>) {
        let dependency_hash = dependency_hash.into();
        self.current_dependency_hash = Some(dependency_hash.clone());
        if self.committed_dependency_hash.as_deref() == Some(dependency_hash.as_str()) {
            if self.state != CheckpointState::Generating {
                self.state = CheckpointState::Current;
            }
        } else if self.committed_artifact_id.is_some() && self.state != CheckpointState::Generating
        {
            self.state = CheckpointState::Stale;
        } else if self.committed_artifact_id.is_none() && self.state != CheckpointState::Generating
        {
            self.state = CheckpointState::Ungenerated;
        }
    }

    pub fn availability(&self) -> CheckpointAvailability {
        match (
            self.committed_artifact_id.is_some(),
            self.current_dependency_hash.as_deref(),
            self.committed_dependency_hash.as_deref(),
        ) {
            (false, _, _) => CheckpointAvailability::Missing,
            (true, Some(current), Some(committed)) if current == committed => {
                CheckpointAvailability::Fresh
            }
            (true, _, _) => CheckpointAvailability::Stale,
        }
    }

    pub fn availability_with_store(
        &self,
        store: &ArtifactStore,
    ) -> Result<CheckpointAvailability, CheckpointError> {
        let Some(id) = self.committed_artifact_id.as_ref() else {
            return Ok(CheckpointAvailability::Missing);
        };
        let Some(artifact) = store.get(id)? else {
            return Ok(CheckpointAvailability::Missing);
        };
        if artifact.provenance.node_version != self.node_version {
            return Ok(CheckpointAvailability::Incompatible);
        }
        Ok(self.availability())
    }

    pub fn begin_generation_token(&mut self) -> Result<GenerationToken, CheckpointError> {
        if self.state == CheckpointState::Generating || self.active_generation.is_some() {
            return Err(CheckpointError::AlreadyGenerating);
        }
        self.next_generation_id = self
            .next_generation_id
            .checked_add(1)
            .ok_or(CheckpointError::GenerationIdExhausted)?;
        let token = GenerationToken(self.next_generation_id);
        self.active_generation = Some(token);
        self.state = CheckpointState::Generating;
        self.failure = None;
        Ok(token)
    }

    /// Backwards-compatible start operation for callers that do not need to
    /// retain a token. New asynchronous callers should use
    /// [`Self::begin_generation_token`] and commit with that token.
    pub fn begin_generation(&mut self) -> Result<(), CheckpointError> {
        self.begin_generation_token().map(|_| ())
    }

    pub fn commit(
        &mut self,
        artifact: CheckpointArtifact,
        store: &ArtifactStore,
    ) -> Result<(), CheckpointError> {
        if let Some(token) = self.active_generation {
            return self.commit_generation(token, artifact, store);
        }
        if matches!(
            self.state,
            CheckpointState::Generating | CheckpointState::Failed | CheckpointState::Cancelled
        ) {
            return Err(CheckpointError::GenerationNotActive);
        }
        self.commit_artifact(artifact, store, false)
    }

    pub fn commit_generation(
        &mut self,
        token: GenerationToken,
        artifact: CheckpointArtifact,
        store: &ArtifactStore,
    ) -> Result<(), CheckpointError> {
        self.require_active_generation(token)?;
        self.commit_artifact(artifact, store, true)
    }

    fn commit_artifact(
        &mut self,
        artifact: CheckpointArtifact,
        store: &ArtifactStore,
        tokenized: bool,
    ) -> Result<(), CheckpointError> {
        if artifact.provenance.node_version != self.node_version {
            if tokenized {
                self.finish_failed_generation(
                    "generated artifact has an incompatible node version",
                );
            }
            return Err(CheckpointError::NodeVersionMismatch {
                expected: self.node_version,
                actual: artifact.provenance.node_version,
            });
        }
        let expected = self
            .current_dependency_hash
            .clone()
            .ok_or(CheckpointError::NoCommittedArtifact)?;
        if artifact.dependency_hash.as_str() != expected.as_str() {
            // The upstream graph changed while this generation was running.
            // Keep the committed artifact available, but expose that it no
            // longer represents the current inputs.
            self.finish_generation();
            self.state = if self.committed_artifact_id.is_some() {
                CheckpointState::Stale
            } else {
                CheckpointState::Ungenerated
            };
            return Err(CheckpointError::DependencyHashMismatch {
                expected: expected.clone(),
                actual: artifact.dependency_hash.clone(),
            });
        }
        store.put(&artifact)?;
        self.committed_dependency_hash = Some(artifact.dependency_hash.clone());
        self.committed_artifact_id = Some(artifact.id.clone());
        self.generation = Some(artifact.generation.clone());
        self.finish_generation();
        self.state = CheckpointState::Current;
        self.failure = None;
        Ok(())
    }

    pub fn cancel_generation(&mut self, token: &GenerationToken) -> Result<(), CheckpointError> {
        self.require_active_generation(*token)?;
        self.finish_generation();
        self.state = CheckpointState::Cancelled;
        Ok(())
    }

    pub fn fail_generation(
        &mut self,
        token: &GenerationToken,
        message: impl Into<String>,
    ) -> Result<(), CheckpointError> {
        self.require_active_generation(*token)?;
        self.finish_generation();
        self.failure = Some(message.into());
        self.state = CheckpointState::Failed;
        Ok(())
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.finish_generation();
        self.failure = Some(message.into());
        self.state = CheckpointState::Failed;
    }

    pub fn cancel(&mut self) {
        self.finish_generation();
        self.state = CheckpointState::Cancelled;
    }

    fn require_active_generation(&self, token: GenerationToken) -> Result<(), CheckpointError> {
        if self.state != CheckpointState::Generating || self.active_generation != Some(token) {
            return Err(CheckpointError::GenerationNotActive);
        }
        Ok(())
    }

    fn finish_generation(&mut self) {
        self.active_generation = None;
    }

    fn finish_failed_generation(&mut self, message: &str) {
        self.finish_generation();
        self.failure = Some(message.to_owned());
        self.state = CheckpointState::Failed;
    }

    pub fn committed_artifact(
        &self,
        store: &ArtifactStore,
    ) -> Result<Option<CheckpointArtifact>, CheckpointError> {
        self.committed_artifact_id
            .as_ref()
            .map_or(Ok(None), |id| store.get(id))
    }
}

pub fn hash_upstream_inputs(
    upstream_hashes: &BTreeMap<String, String>,
    node_version: u32,
) -> String {
    #[derive(Serialize)]
    struct DependencyDocument<'a> {
        node_version: u32,
        upstream_hashes: &'a BTreeMap<String, String>,
    }
    let document = DependencyDocument {
        node_version,
        upstream_hashes,
    };
    let bytes = serde_json::to_vec(&document)
        .unwrap_or_else(|_| b"rawweave-invalid-checkpoint-dependencies".to_vec());
    hex_digest(&bytes)
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
