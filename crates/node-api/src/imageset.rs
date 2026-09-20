use std::collections::BTreeSet;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageSetOrder {
    #[default]
    Ordered,
    Unordered,
}

/// Alignment information shared by collection-level computational photography
/// nodes. Transform matrices are intentionally left to the geometry layer; the
/// graph still records whether an alignment pass has completed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum AlignmentState {
    #[default]
    Unaligned,
    Aligned {
        reference_member: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageSetMember {
    pub id: String,
    pub image: Image,
    #[serde(default)]
    pub metadata: Metadata,
}

impl ImageSetMember {
    pub fn new(id: impl Into<String>, image: Image, metadata: Metadata) -> Self {
        Self {
            id: id.into(),
            image,
            metadata,
        }
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
            AlignmentState::Aligned { reference_member }
                if !ids.contains(reference_member.as_str()) =>
            {
                return Err(ImageSetError::MissingAlignmentReference(
                    reference_member.clone(),
                ));
            }
            AlignmentState::Unaligned | AlignmentState::Aligned { .. } => {}
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
