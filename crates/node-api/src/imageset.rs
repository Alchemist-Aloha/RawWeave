use std::collections::{BTreeMap, BTreeSet};

use rawweave_image::Image;
use serde::{Deserialize, Serialize, Serializer};
use thiserror::Error;

use crate::Metadata;

/// Maximum number of bytes in one member identity.
pub const MAX_IMAGE_SET_MEMBER_ID_BYTES: usize = 256;
/// Maximum number of images carried by one graph value.
pub const MAX_IMAGE_SET_MEMBERS: usize = 256;
/// Maximum aggregate pixel count accepted by one graph value.
pub const MAX_IMAGE_SET_PIXELS: usize = 64 * 1024 * 1024;
/// Maximum serialized metadata text held by one graph value.
pub const MAX_IMAGE_SET_METADATA_BYTES: usize = 1024 * 1024;
/// Maximum bytes retained for one persisted source path.
pub const MAX_IMAGE_SET_SOURCE_PATH_BYTES: usize = 4096;
/// Maximum bytes retained for one persisted source fingerprint.
pub const MAX_IMAGE_SET_SOURCE_FINGERPRINT_BYTES: usize = 256;
/// Maximum bytes retained for alignment provenance text.
pub const MAX_ALIGNMENT_PROVENANCE_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageSetOrder {
    #[default]
    Ordered,
    Unordered,
}

/// A deterministic integer-pixel translation from a member into the reference
/// image. `dx` and `dy` are source sampling offsets: a destination pixel at
/// `(x, y)` samples the member at `(x + dx, y + dy)`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlignmentTransform {
    pub dx: i32,
    pub dy: i32,
    #[serde(deserialize_with = "deserialize_finite_float")]
    pub error: f32,
}

impl AlignmentTransform {
    pub const fn identity() -> Self {
        Self {
            dx: 0,
            dy: 0,
            error: 0.0,
        }
    }

    pub fn new(dx: i32, dy: i32, error: f32) -> Option<Self> {
        error.is_finite().then_some(Self { dx, dy, error })
    }

    fn validate(&self) -> Result<(), ImageSetError> {
        if self.error.is_finite() && self.error >= 0.0 {
            Ok(())
        } else {
            Err(ImageSetError::InvalidAlignmentTransform)
        }
    }
}

/// The algorithm and bounded settings that produced an alignment result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlignmentProvenance {
    pub algorithm: String,
    pub version: u32,
    pub max_shift: u32,
}

impl AlignmentProvenance {
    pub fn new(algorithm: impl Into<String>, version: u32, max_shift: u32) -> Self {
        Self {
            algorithm: algorithm.into(),
            version,
            max_shift,
        }
    }

    fn validate(&self) -> Result<(), ImageSetError> {
        if self.algorithm.trim().is_empty() || self.algorithm.len() > MAX_ALIGNMENT_PROVENANCE_BYTES
        {
            return Err(ImageSetError::InvalidAlignmentProvenance);
        }
        Ok(())
    }
}

/// Alignment information shared by collection-level computational photography
/// nodes. An aligned value is only valid when every member has a validated
/// transform and the provenance identifies the registration algorithm.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum AlignmentState {
    #[default]
    Unaligned,
    Aligned {
        reference_member: String,
        #[serde(default)]
        transforms: BTreeMap<String, AlignmentTransform>,
        #[serde(default)]
        provenance: AlignmentProvenance,
    },
}

impl Default for AlignmentProvenance {
    fn default() -> Self {
        Self::new("legacy", 0, 0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageSetSourceDescriptor {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

impl ImageSetSourceDescriptor {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            fingerprint: None,
        }
    }

    pub fn with_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.fingerprint = Some(fingerprint.into());
        self
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    fn validate(&self) -> Result<(), ImageSetError> {
        if self.path.trim().is_empty() {
            return Err(ImageSetError::EmptySourcePath);
        }
        if self.path.len() > MAX_IMAGE_SET_SOURCE_PATH_BYTES {
            return Err(ImageSetError::SourcePathTooLong {
                actual: self.path.len(),
                limit: MAX_IMAGE_SET_SOURCE_PATH_BYTES,
            });
        }
        if self
            .fingerprint
            .as_ref()
            .is_some_and(|value| value.len() > MAX_IMAGE_SET_SOURCE_FINGERPRINT_BYTES)
        {
            return Err(ImageSetError::SourceFingerprintTooLong {
                limit: MAX_IMAGE_SET_SOURCE_FINGERPRINT_BYTES,
            });
        }
        Ok(())
    }
}

pub type ImageSetSource = ImageSetSourceDescriptor;
pub type ImageSetMemberSource = ImageSetSourceDescriptor;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageSetMember {
    pub id: String,
    pub image: Image,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ImageSetSourceDescriptor>,
}

impl ImageSetMember {
    pub fn new(id: impl Into<String>, image: Image, metadata: Metadata) -> Self {
        Self {
            id: id.into(),
            image,
            metadata,
            source: None,
        }
    }

    pub fn with_source(mut self, source: ImageSetSourceDescriptor) -> Self {
        self.source = Some(source);
        self
    }

    pub fn source(&self) -> Option<&ImageSetSourceDescriptor> {
        self.source.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImageSet {
    order: ImageSetOrder,
    members: Vec<ImageSetMember>,
    shared_metadata: Metadata,
    alignment: AlignmentState,
}

#[derive(Serialize)]
struct ImageSetDocument<'a> {
    order: ImageSetOrder,
    members: &'a [ImageSetMember],
    shared_metadata: &'a Metadata,
    alignment: &'a AlignmentState,
}

impl Serialize for ImageSet {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        ImageSetDocument {
            order: self.order,
            members: &self.members,
            shared_metadata: &self.shared_metadata,
            alignment: &self.alignment,
        }
        .serialize(serializer)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ImageSetError {
    #[error("image set must contain at least one member")]
    Empty,
    #[error("image set contains too many members: {actual} exceeds {limit}")]
    TooManyMembers { actual: usize, limit: usize },
    #[error("image set member id cannot be empty")]
    EmptyMemberId,
    #[error("image set member id exceeds {limit} bytes: {actual}")]
    MemberIdTooLong { actual: usize, limit: usize },
    #[error("image set member id '{0}' is duplicated")]
    DuplicateMemberId(String),
    #[error("image set member '{member}' has invalid pixel storage")]
    InvalidMemberPixels { member: String },
    #[error("image set exceeds the aggregate pixel limit of {limit}")]
    TooManyPixels { limit: usize },
    #[error("image set metadata exceeds the byte limit of {limit}")]
    MetadataTooLarge { limit: usize },
    #[error("image set source path cannot be empty")]
    EmptySourcePath,
    #[error("image set source path exceeds {limit} bytes: {actual}")]
    SourcePathTooLong { actual: usize, limit: usize },
    #[error("image set source fingerprint exceeds {limit} bytes")]
    SourceFingerprintTooLong { limit: usize },
    #[error("image set contains an invalid alignment transform")]
    InvalidAlignmentTransform,
    #[error("image set contains invalid alignment provenance")]
    InvalidAlignmentProvenance,
    #[error("image set alignment is missing a transform for member '{0}'")]
    MissingAlignmentTransform(String),
    #[error("image set alignment contains a transform for unknown member '{0}'")]
    UnknownAlignmentTransform(String),
    #[error("alignment reference member '{0}' is not in the image set")]
    MissingAlignmentReference(String),
}

impl ImageSet {
    pub fn new(
        order: ImageSetOrder,
        mut members: Vec<ImageSetMember>,
    ) -> Result<Self, ImageSetError> {
        // Unordered collections have no meaningful caller-provided order. Keep
        // one canonical order so serialization, cache keys, and checkpoint ids
        // do not depend on how the collection was assembled.
        if order == ImageSetOrder::Unordered {
            members.sort_by(|left, right| left.id.cmp(&right.id));
        }
        let set = Self {
            order,
            members,
            shared_metadata: Metadata::default(),
            alignment: AlignmentState::default(),
        };
        set.validate()?;
        Ok(set)
    }

    pub fn with_shared_metadata(mut self, shared_metadata: Metadata) -> Self {
        self.shared_metadata = shared_metadata;
        self
    }

    pub fn with_alignment(mut self, alignment: AlignmentState) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn order(&self) -> ImageSetOrder {
        self.order
    }

    pub fn is_ordered(&self) -> bool {
        self.order == ImageSetOrder::Ordered
    }

    pub fn members(&self) -> &[ImageSetMember] {
        &self.members
    }

    pub fn member(&self, id: &str) -> Option<&ImageSetMember> {
        self.members.iter().find(|member| member.id == id)
    }

    pub fn member_at(&self, index: usize) -> Option<&ImageSetMember> {
        self.members.get(index)
    }

    pub fn member_ids(&self) -> Vec<String> {
        self.members
            .iter()
            .map(|member| member.id.clone())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    pub fn shared_metadata(&self) -> &Metadata {
        &self.shared_metadata
    }

    pub fn alignment(&self) -> AlignmentState {
        self.alignment.clone()
    }

    pub fn validate(&self) -> Result<(), ImageSetError> {
        if self.members.is_empty() {
            return Err(ImageSetError::Empty);
        }
        if self.members.len() > MAX_IMAGE_SET_MEMBERS {
            return Err(ImageSetError::TooManyMembers {
                actual: self.members.len(),
                limit: MAX_IMAGE_SET_MEMBERS,
            });
        }
        let mut ids = BTreeSet::new();
        let mut total_pixels = 0_usize;
        for member in &self.members {
            if member.id.trim().is_empty() {
                return Err(ImageSetError::EmptyMemberId);
            }
            if member.id.len() > MAX_IMAGE_SET_MEMBER_ID_BYTES {
                return Err(ImageSetError::MemberIdTooLong {
                    actual: member.id.len(),
                    limit: MAX_IMAGE_SET_MEMBER_ID_BYTES,
                });
            }
            if !ids.insert(member.id.as_str()) {
                return Err(ImageSetError::DuplicateMemberId(member.id.clone()));
            }
            if let Some(source) = &member.source {
                source.validate()?;
            }
            let expected = member.image.dimensions().pixel_count().map_err(|_| {
                ImageSetError::InvalidMemberPixels {
                    member: member.id.clone(),
                }
            })?;
            if member.image.pixels().len() != expected
                || member
                    .image
                    .pixels()
                    .iter()
                    .flatten()
                    .any(|channel| !channel.is_finite())
            {
                return Err(ImageSetError::InvalidMemberPixels {
                    member: member.id.clone(),
                });
            }
            total_pixels =
                total_pixels
                    .checked_add(expected)
                    .ok_or(ImageSetError::TooManyPixels {
                        limit: MAX_IMAGE_SET_PIXELS,
                    })?;
            if total_pixels > MAX_IMAGE_SET_PIXELS {
                return Err(ImageSetError::TooManyPixels {
                    limit: MAX_IMAGE_SET_PIXELS,
                });
            }
        }
        match &self.alignment {
            AlignmentState::Aligned {
                reference_member, ..
            } if !ids.contains(reference_member.as_str()) => {
                return Err(ImageSetError::MissingAlignmentReference(
                    reference_member.clone(),
                ));
            }
            AlignmentState::Aligned {
                transforms,
                provenance,
                ..
            } => {
                provenance.validate()?;
                for member_id in &ids {
                    let transform = transforms.get(*member_id).ok_or_else(|| {
                        ImageSetError::MissingAlignmentTransform((*member_id).to_owned())
                    })?;
                    transform.validate()?;
                }
                if let Some(member_id) = transforms.keys().find(|id| !ids.contains(id.as_str())) {
                    return Err(ImageSetError::UnknownAlignmentTransform(member_id.clone()));
                }
            }
            AlignmentState::Unaligned => {}
        }
        if metadata_size(&self.shared_metadata).saturating_add(
            self.members
                .iter()
                .map(|member| metadata_size(&member.metadata))
                .sum::<usize>(),
        ) > MAX_IMAGE_SET_METADATA_BYTES
        {
            return Err(ImageSetError::MetadataTooLarge {
                limit: MAX_IMAGE_SET_METADATA_BYTES,
            });
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for ImageSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ImageSetDocument {
            order: ImageSetOrder,
            members: Vec<ImageSetMember>,
            #[serde(default)]
            shared_metadata: Metadata,
            #[serde(default)]
            alignment: AlignmentState,
        }

        let document = ImageSetDocument::deserialize(deserializer)?;
        let mut set =
            Self::new(document.order, document.members).map_err(serde::de::Error::custom)?;
        set.shared_metadata = document.shared_metadata;
        set.alignment = document.alignment;
        set.validate().map_err(serde::de::Error::custom)?;
        Ok(set)
    }
}

fn metadata_size(metadata: &Metadata) -> usize {
    metadata
        .make
        .len()
        .saturating_add(metadata.model.len())
        .saturating_add(metadata.lens.as_deref().map_or(0, str::len))
        .saturating_add(metadata.capture_time.as_deref().map_or(0, str::len))
        .saturating_add(metadata.orientation.as_deref().map_or(0, str::len))
        .saturating_add(
            metadata
                .tags
                .iter()
                .map(|(key, value)| key.len().saturating_add(value.len()))
                .sum::<usize>(),
        )
}

fn deserialize_finite_float<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| serde::de::Error::custom("alignment error must be finite"))
}
