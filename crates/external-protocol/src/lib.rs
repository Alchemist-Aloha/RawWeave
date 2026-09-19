use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROTOCOL_NAME: &str = "rawweave.external-node";
pub const CURRENT_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion { major: 1, minor: 0 };
pub const DEFAULT_MAX_FRAME_SIZE: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

impl ProtocolVersion {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolRange {
    pub min: ProtocolVersion,
    pub max: ProtocolVersion,
}

impl ProtocolRange {
    pub const fn exact(version: ProtocolVersion) -> Self {
        Self {
            min: version,
            max: version,
        }
    }

    pub const fn current() -> Self {
        Self::exact(CURRENT_PROTOCOL_VERSION)
    }

    fn contains(self, version: ProtocolVersion) -> bool {
        self.min <= version && version <= self.max
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NegotiatedProtocol {
    pub name: &'static str,
    pub version: ProtocolVersion,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("protocol versions are incompatible: local {local:?}, peer {peer:?}")]
    VersionMismatch {
        local: ProtocolRange,
        peer: ProtocolRange,
    },
    #[error("protocol capability is not supported: {0}")]
    CapabilityRejected(String),
    #[error("frame length {length} exceeds limit {limit}")]
    FrameTooLarge { length: usize, limit: usize },
    #[error("frame length must be greater than zero")]
    EmptyFrame,
    #[error("framed message ended unexpectedly")]
    UnexpectedEof,
    #[error("malformed framed message: {0}")]
    Malformed(String),
    #[error("unsupported protocol name '{0}'")]
    UnsupportedProtocol(String),
    #[error("I/O error while transferring protocol frame: {0}")]
    Io(String),
}

impl From<io::Error> for ProtocolError {
    fn from(error: io::Error) -> Self {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            Self::UnexpectedEof
        } else {
            Self::Io(error.to_string())
        }
    }
}

pub fn negotiate(
    local: ProtocolRange,
    peer: ProtocolRange,
) -> Result<NegotiatedProtocol, ProtocolError> {
    let min_major = local.min.major.max(peer.min.major);
    let max_major = local.max.major.min(peer.max.major);
    if min_major > max_major {
        return Err(ProtocolError::VersionMismatch { local, peer });
    }
    let major = max_major;
    let local_minor = if local.max.major == major {
        local.max.minor
    } else {
        u16::MAX
    };
    let peer_minor = if peer.max.major == major {
        peer.max.minor
    } else {
        u16::MAX
    };
    let minor = local_minor.min(peer_minor);
    let candidate = ProtocolVersion::new(major, minor);
    if !local.contains(candidate) || !peer.contains(candidate) {
        return Err(ProtocolError::VersionMismatch { local, peer });
    }
    Ok(NegotiatedProtocol {
        name: PROTOCOL_NAME,
        version: candidate,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PixelFormat {
    Rgba8Unorm,
    Rgba16Float,
    Rgba32Float,
    Mask32Float,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ThreadSafety {
    SingleThreaded,
    Reentrant,
    Parallel,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub pixel_formats: BTreeSet<PixelFormat>,
    pub roi: bool,
    pub full_frame: bool,
    pub multi_input: bool,
    pub multi_output: bool,
    pub thread_safety: ThreadSafety,
    pub gpu: bool,
    pub custom_ui: bool,
    pub deterministic: bool,
    pub data_plane: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            pixel_formats: BTreeSet::new(),
            roi: false,
            full_frame: true,
            multi_input: false,
            multi_output: false,
            thread_safety: ThreadSafety::SingleThreaded,
            gpu: false,
            custom_ui: false,
            deterministic: false,
            data_plane: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRequirement {
    pub pixel_format: Option<PixelFormat>,
    pub roi: Option<bool>,
    pub full_frame: Option<bool>,
    pub multi_input: Option<bool>,
    pub multi_output: Option<bool>,
    pub gpu: Option<bool>,
    pub custom_ui: Option<bool>,
    pub deterministic: Option<bool>,
    pub data_plane: Option<bool>,
}

impl Capabilities {
    pub fn check(&self, required: &CapabilityRequirement) -> Result<(), ProtocolError> {
        if let Some(format) = required.pixel_format
            && !self.pixel_formats.contains(&format)
        {
            return Err(ProtocolError::CapabilityRejected(format!(
                "pixel format {format:?}"
            )));
        }
        for (name, requested, available) in [
            ("roi", required.roi, self.roi),
            ("full_frame", required.full_frame, self.full_frame),
            ("multi_input", required.multi_input, self.multi_input),
            ("multi_output", required.multi_output, self.multi_output),
            ("gpu", required.gpu, self.gpu),
            ("custom_ui", required.custom_ui, self.custom_ui),
            ("deterministic", required.deterministic, self.deterministic),
            ("data_plane", required.data_plane, self.data_plane),
        ] {
            if requested == Some(true) && !available {
                return Err(ProtocolError::CapabilityRejected(name.to_owned()));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataKind {
    Bytes,
    Image,
    Mask,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BufferOwnership {
    Sender,
    Receiver,
    Shared,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BufferLifetime {
    Request,
    Session,
    Explicit,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataMetadata {
    pub dimensions: Option<[u32; 2]>,
    pub origin: Option<[u32; 2]>,
    pub pixel_format: Option<PixelFormat>,
    pub color_domain: Option<String>,
    pub region: Option<[u32; 4]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataBuffer {
    pub id: String,
    pub kind: DataKind,
    pub relative_path: String,
    pub byte_len: u64,
    pub sha256: String,
    pub ownership: BufferOwnership,
    pub lifetime: BufferLifetime,
    #[serde(default)]
    pub metadata: DataMetadata,
}

impl DataBuffer {
    pub fn new(
        id: impl Into<String>,
        kind: DataKind,
        relative_path: impl Into<String>,
        byte_len: u64,
        sha256: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            relative_path: relative_path.into(),
            byte_len,
            sha256: sha256.into(),
            ownership: BufferOwnership::Sender,
            lifetime: BufferLifetime::Request,
            metadata: DataMetadata::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ExternalValue {
    Null,
    Float(f64),
    Integer(i64),
    Boolean(bool),
    String(String),
    Enum(String),
    Color([f32; 4]),
    Buffer(DataBuffer),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalPort {
    pub id: String,
    pub name: String,
    pub data_type: String,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExternalParameter {
    pub id: String,
    pub name: String,
    pub data_type: String,
    pub default: ExternalValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExternalNodeDescriptor {
    pub type_id: String,
    pub name: String,
    pub version: u32,
    pub inputs: Vec<ExternalPort>,
    pub outputs: Vec<ExternalPort>,
    pub parameters: Vec<ExternalParameter>,
    pub capabilities: Capabilities,
}

pub type RequestId = u64;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: RequestId,
    pub protocol: ProtocolVersion,
    pub payload: RequestPayload,
}

impl Request {
    pub fn new(payload: RequestPayload) -> Self {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self::with_id(
            NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            payload,
        )
    }

    pub fn with_id(id: RequestId, payload: RequestPayload) -> Self {
        Self {
            id,
            protocol: CURRENT_PROTOCOL_VERSION,
            payload,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", content = "data")]
pub enum RequestPayload {
    Discover,
    Describe {
        type_id: String,
    },
    Instantiate {
        type_id: String,
        instance_id: String,
    },
    SetParameters {
        instance_id: String,
        parameters: BTreeMap<String, ExternalValue>,
    },
    Evaluate {
        instance_id: String,
        inputs: BTreeMap<String, ExternalValue>,
        parameters: BTreeMap<String, ExternalValue>,
    },
    Status {
        instance_id: String,
        request_id: Option<RequestId>,
    },
    Result {
        instance_id: String,
        request_id: RequestId,
    },
    Cancel {
        request_id: RequestId,
    },
    SerializeState {
        instance_id: String,
    },
    Destroy {
        instance_id: String,
    },
    CapabilityQuery {
        required: CapabilityRequirement,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub id: RequestId,
    pub protocol: ProtocolVersion,
    pub payload: ResponsePayload,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", content = "data")]
pub enum ResponsePayload {
    Discovered {
        descriptors: Vec<ExternalNodeDescriptor>,
        capabilities: Capabilities,
    },
    Described {
        descriptor: ExternalNodeDescriptor,
    },
    Instantiated {
        instance_id: String,
    },
    Evaluated {
        outputs: BTreeMap<String, ExternalValue>,
    },
    Status {
        state: String,
        progress: Option<f32>,
    },
    SerializedState {
        bytes: Vec<u8>,
    },
    Capabilities {
        capabilities: Capabilities,
    },
    Acknowledged,
    Error(ExternalError),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(default)]
    pub details: BTreeMap<String, String>,
}

impl ExternalError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: false,
            details: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Message {
    Request(Request),
    Response(Response),
}

pub struct FrameCodec;

impl FrameCodec {
    pub fn encode(message: &Message, max_frame_size: usize) -> Result<Vec<u8>, ProtocolError> {
        let payload = serde_json::to_vec(message)
            .map_err(|error| ProtocolError::Malformed(error.to_string()))?;
        if payload.is_empty() {
            return Err(ProtocolError::EmptyFrame);
        }
        if payload.len() > max_frame_size || payload.len() > u32::MAX as usize {
            return Err(ProtocolError::FrameTooLarge {
                length: payload.len(),
                limit: max_frame_size.min(u32::MAX as usize),
            });
        }
        let length = u32::try_from(payload.len()).map_err(|_| ProtocolError::FrameTooLarge {
            length: payload.len(),
            limit: max_frame_size,
        })?;
        let mut frame = Vec::with_capacity(payload.len() + 4);
        frame.extend_from_slice(&length.to_be_bytes());
        frame.extend_from_slice(&payload);
        Ok(frame)
    }

    pub fn write<W: Write>(
        writer: &mut W,
        message: &Message,
        max_frame_size: usize,
    ) -> Result<(), ProtocolError> {
        writer.write_all(&Self::encode(message, max_frame_size)?)?;
        writer.flush()?;
        Ok(())
    }

    pub fn read<R: Read>(reader: &mut R, max_frame_size: usize) -> Result<Message, ProtocolError> {
        let mut length_bytes = [0_u8; 4];
        reader.read_exact(&mut length_bytes)?;
        let length = u32::from_be_bytes(length_bytes) as usize;
        if length == 0 {
            return Err(ProtocolError::EmptyFrame);
        }
        if length > max_frame_size {
            return Err(ProtocolError::FrameTooLarge {
                length,
                limit: max_frame_size,
            });
        }
        let mut payload = vec![0_u8; length];
        reader.read_exact(&mut payload)?;
        serde_json::from_slice(&payload)
            .map_err(|error| ProtocolError::Malformed(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn request_round_trips_through_framed_json() {
        let request = Request::with_id(
            41,
            RequestPayload::Evaluate {
                instance_id: "node-1".into(),
                parameters: [("strength".into(), ExternalValue::Float(0.5))]
                    .into_iter()
                    .collect(),
                inputs: [("image".into(), ExternalValue::Integer(3))]
                    .into_iter()
                    .collect(),
            },
        );
        let encoded = FrameCodec::encode(&Message::Request(request.clone()), 4096).unwrap();
        let decoded = FrameCodec::read(&mut Cursor::new(encoded), 4096).unwrap();
        assert_eq!(decoded, Message::Request(request));
    }

    #[test]
    fn version_negotiation_rejects_incompatible_major_versions() {
        let local = ProtocolRange::exact(ProtocolVersion::new(1, 2));
        let peer = ProtocolRange::exact(ProtocolVersion::new(2, 0));
        assert!(matches!(
            negotiate(local, peer),
            Err(ProtocolError::VersionMismatch { .. })
        ));
    }

    #[test]
    fn capability_negotiation_rejects_missing_required_features() {
        let available = Capabilities::default();
        let required = CapabilityRequirement {
            roi: Some(true),
            ..Default::default()
        };
        assert!(available.check(&required).is_err());
    }

    #[test]
    fn malformed_and_oversized_frames_are_rejected_before_allocation() {
        assert!(matches!(
            FrameCodec::read(&mut Cursor::new(vec![0, 0, 0, 2, b'{']), 16),
            Err(ProtocolError::UnexpectedEof)
        ));
        let oversized = [0, 0, 1, 0];
        assert!(matches!(
            FrameCodec::read(&mut Cursor::new(oversized), 16),
            Err(ProtocolError::FrameTooLarge { .. })
        ));
    }

    #[test]
    fn structured_errors_and_data_buffers_are_serializable() {
        let response = Message::Response(Response {
            id: 7,
            protocol: CURRENT_PROTOCOL_VERSION,
            payload: ResponsePayload::Error(ExternalError::new("bad_input", "invalid image")),
        });
        let buffer = DataBuffer::new(
            "image-1",
            DataKind::Image,
            "buffers/image-1.bin",
            12,
            "00".repeat(32),
        );
        let json = serde_json::to_string(&(response, buffer)).unwrap();
        assert!(json.contains("bad_input"));
    }
}
