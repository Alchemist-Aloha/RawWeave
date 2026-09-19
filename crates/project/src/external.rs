use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rawweave_external_host::{DataPlane, HostConfig, HostError, ManagedBuffer, Supervisor};
use rawweave_external_protocol::{
    Capabilities, DataBuffer, DataKind, DataMetadata, ExternalNodeDescriptor, ExternalParameter,
    ExternalPort, ExternalValue, PixelFormat as ExternalPixelFormat, RequestPayload,
    ResponsePayload,
};
use rawweave_image::{ColorDomain, ColorMetadata, Dimensions, Image, Mask, Pixel, PixelFormat};
use rawweave_node_api::{
    EvaluationContext, ExecutionCapability, Inputs, NodeDescriptor, NodeError, NodeInstance,
    NodePack, NodeRegistry, NodeResult, ParameterDescriptor, ParameterValue, Parameters,
    PortDescriptor, RegistryError, Value,
};
use thiserror::Error;

static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error)]
pub enum ExternalError {
    #[error(transparent)]
    Host(#[from] HostError),
    #[error("invalid external node descriptor: {0}")]
    InvalidDescriptor(String),
    #[error("external host returned an unexpected response")]
    UnexpectedResponse,
}

struct ExternalHostInner {
    id: String,
    config: HostConfig,
    data_plane: Arc<DataPlane>,
    supervisor: Mutex<Option<Arc<Supervisor>>>,
    last_error: Mutex<Option<String>>,
}

/// A lazily-started external node host and its request data plane.
///
/// Construction only validates local configuration and creates the temporary
/// data directory. The executable is started on the first request so a saved
/// project can describe an unavailable host without failing to load.
#[derive(Clone)]
pub struct ExternalHost {
    inner: Arc<ExternalHostInner>,
}

impl fmt::Debug for ExternalHost {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExternalHost")
            .field("id", &self.inner.id)
            .field("executable", &self.inner.config.executable())
            .finish_non_exhaustive()
    }
}

impl ExternalHost {
    pub fn connect(id: impl Into<String>, config: HostConfig) -> Result<Self, HostError> {
        let id = id.into();
        if id.is_empty() {
            return Err(HostError::InvalidConfig(
                "external host id must not be empty".into(),
            ));
        }
        validate_config_without_launch(&config)?;
        let data_plane = Arc::new(DataPlane::new(config.limits().max_buffer_bytes)?);
        // The host process is intentionally given only the data-plane root. The
        // supervisor itself clears the inherited environment before spawning it.
        let config = config.with_environment(
            "RAWWEAVE_EXTERNAL_DATA_ROOT",
            data_plane.root().as_os_str().to_owned(),
        );
        Ok(Self {
            inner: Arc::new(ExternalHostInner {
                id,
                config,
                data_plane,
                supervisor: Mutex::new(None),
                last_error: Mutex::new(None),
            }),
        })
    }

    pub fn id(&self) -> &str {
        &self.inner.id
    }

    pub fn data_plane(&self) -> Arc<DataPlane> {
        Arc::clone(&self.inner.data_plane)
    }

    pub fn request(&self, payload: RequestPayload) -> Result<ResponsePayload, HostError> {
        let supervisor = match self.supervisor() {
            Ok(supervisor) => supervisor,
            Err(error) => {
                self.record_error(&error);
                return Err(error);
            }
        };
        match supervisor.request(payload) {
            Ok(response) => Ok(response),
            Err(error) => {
                self.record_error(&error);
                Err(error)
            }
        }
    }

    /// Return a human-readable status without launching the configured process.
    /// Request failures are retained and included after a failed lazy launch.
    pub fn diagnostics(&self) -> ExternalHostDiagnostics {
        let detail = self
            .inner
            .last_error
            .lock()
            .ok()
            .and_then(|error| error.clone())
            .unwrap_or_else(|| "not started".to_owned());
        ExternalHostDiagnostics {
            host_id: self.inner.id.clone(),
            executable: self.inner.config.executable().to_owned(),
            detail,
        }
    }

    fn supervisor(&self) -> Result<Arc<Supervisor>, HostError> {
        let mut slot = self
            .inner
            .supervisor
            .lock()
            .map_err(|_| HostError::StatePoisoned)?;
        if let Some(supervisor) = slot.as_ref() {
            return Ok(Arc::clone(supervisor));
        }
        let supervisor = Arc::new(Supervisor::new(self.inner.config.clone())?);
        *slot = Some(Arc::clone(&supervisor));
        Ok(supervisor)
    }

    fn record_error(&self, error: &HostError) {
        if let Ok(mut last_error) = self.inner.last_error.lock() {
            *last_error = Some(error.to_string());
        }
    }
}

fn validate_config_without_launch(config: &HostConfig) -> Result<(), HostError> {
    if config.executable().as_os_str().is_empty() {
        return Err(HostError::InvalidConfig(
            "host executable must not be empty".into(),
        ));
    }
    let limits = config.limits();
    if limits.request_timeout.is_zero()
        || limits.max_frame_size == 0
        || limits.max_buffer_bytes == 0
        || limits.max_stdout_bytes == 0
        || limits.max_stderr_bytes == 0
        || limits.max_output_bytes == 0
    {
        return Err(HostError::InvalidConfig(
            "resource limits must be greater than zero".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalHostDiagnostics {
    pub host_id: String,
    pub executable: PathBuf,
    pub detail: String,
}

impl fmt::Display for ExternalHostDiagnostics {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "external host '{}' executable '{}': {}",
            self.host_id,
            self.executable.display(),
            self.detail
        )
    }
}

/// A node pack discovered from one external host.
#[derive(Clone, Debug)]
pub struct ExternalNodePack {
    host: ExternalHost,
    package_id: String,
    descriptors: Vec<DiscoveredDescriptor>,
    capabilities: Capabilities,
}

#[derive(Clone, Debug)]
struct DiscoveredDescriptor {
    external_type_id: String,
    descriptor: NodeDescriptor,
}

impl ExternalNodePack {
    pub fn discover(host: ExternalHost) -> Result<Self, ExternalError> {
        let response = host.request(RequestPayload::Discover)?;
        let ResponsePayload::Discovered {
            descriptors,
            capabilities,
        } = response
        else {
            return Err(ExternalError::UnexpectedResponse);
        };
        let package_id = format!("external.{}", host.id());
        let descriptors = descriptors
            .into_iter()
            .map(|descriptor| {
                let external_type_id = descriptor.type_id.clone();
                let descriptor = convert_descriptor(&package_id, descriptor)?;
                Ok(DiscoveredDescriptor {
                    external_type_id,
                    descriptor,
                })
            })
            .collect::<Result<Vec<_>, ExternalError>>()?;
        Ok(Self {
            host,
            package_id,
            descriptors,
            capabilities,
        })
    }

    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    pub fn host(&self) -> &ExternalHost {
        &self.host
    }

    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    pub fn descriptors(&self) -> Vec<NodeDescriptor> {
        self.descriptors
            .iter()
            .map(|item| item.descriptor.clone())
            .collect()
    }

    pub fn register_into(&self, registry: &mut NodeRegistry) -> Result<(), RegistryError> {
        for item in &self.descriptors {
            let host = self.host.clone();
            let external_type_id = item.external_type_id.clone();
            let descriptor = item.descriptor.clone();
            registry.register_factory(item.descriptor.clone(), move || {
                Box::new(ExternalNode {
                    host: host.clone(),
                    external_type_id: external_type_id.clone(),
                    descriptor: descriptor.clone(),
                    instance_id: next_instance_id(),
                })
            })?;
        }
        Ok(())
    }
}

impl NodePack for ExternalNodePack {
    fn id(&self) -> &'static str {
        "external"
    }

    fn register(&self, registry: &mut NodeRegistry) -> Result<(), RegistryError> {
        self.register_into(registry)
    }
}

fn next_instance_id() -> String {
    format!(
        "rawweave-external-{}",
        NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed)
    )
}

fn convert_descriptor(
    package_id: &str,
    descriptor: ExternalNodeDescriptor,
) -> Result<NodeDescriptor, ExternalError> {
    if descriptor.type_id.is_empty() || descriptor.name.is_empty() {
        return Err(ExternalError::InvalidDescriptor(
            "type id and name must not be empty".into(),
        ));
    }
    let mut converted = NodeDescriptor::new(
        format!("{package_id}.{}", descriptor.type_id),
        descriptor.name,
    );
    converted.version = descriptor.version;
    converted.inputs = descriptor
        .inputs
        .into_iter()
        .map(convert_port)
        .collect::<Result<Vec<_>, _>>()?;
    converted.outputs = descriptor
        .outputs
        .into_iter()
        .map(convert_output_port)
        .collect::<Result<Vec<_>, _>>()?;
    converted.parameters = descriptor
        .parameters
        .into_iter()
        .map(convert_parameter)
        .collect::<Result<Vec<_>, _>>()?;
    converted.capabilities = execution_capabilities(&descriptor.capabilities);
    Ok(converted)
}

fn convert_port(port: ExternalPort) -> Result<PortDescriptor, ExternalError> {
    if port.id.is_empty() || port.data_type.is_empty() {
        return Err(ExternalError::InvalidDescriptor(
            "port id and data type must not be empty".into(),
        ));
    }
    Ok(PortDescriptor::input(
        port.id,
        port.name,
        port.data_type,
        port.required,
    ))
}

fn convert_output_port(port: ExternalPort) -> Result<PortDescriptor, ExternalError> {
    if port.id.is_empty() || port.data_type.is_empty() {
        return Err(ExternalError::InvalidDescriptor(
            "port id and data type must not be empty".into(),
        ));
    }
    Ok(PortDescriptor::output(port.id, port.name, port.data_type))
}

fn convert_parameter(parameter: ExternalParameter) -> Result<ParameterDescriptor, ExternalError> {
    if parameter.id.is_empty() || parameter.data_type.is_empty() {
        return Err(ExternalError::InvalidDescriptor(
            "parameter id and data type must not be empty".into(),
        ));
    }
    let data_type = parameter.data_type.to_ascii_lowercase();
    match (&data_type[..], parameter.default) {
        ("float" | "value.float", ExternalValue::Float(value)) => {
            let value = finite_f32(value, &parameter.id)?;
            Ok(ParameterDescriptor::float(
                parameter.id,
                parameter.name,
                value,
                None,
                None,
            ))
        }
        ("integer" | "value.integer", ExternalValue::Integer(value)) => Ok(
            ParameterDescriptor::integer(parameter.id, parameter.name, value),
        ),
        ("boolean" | "value.boolean", ExternalValue::Boolean(value)) => Ok(
            ParameterDescriptor::boolean(parameter.id, parameter.name, value),
        ),
        ("string" | "value.string", ExternalValue::String(value))
        | ("enum" | "value.enum", ExternalValue::Enum(value)) => Ok(ParameterDescriptor::string(
            parameter.id,
            parameter.name,
            value,
        )),
        (_, ExternalValue::Float(value)) => {
            let default = finite_f32(value, &parameter.id)?;
            Ok(ParameterDescriptor::float(
                parameter.id,
                parameter.name,
                default,
                None,
                None,
            ))
        }
        (_, ExternalValue::Integer(value)) => Ok(ParameterDescriptor::integer(
            parameter.id,
            parameter.name,
            value,
        )),
        (_, ExternalValue::Boolean(value)) => Ok(ParameterDescriptor::boolean(
            parameter.id,
            parameter.name,
            value,
        )),
        (_, ExternalValue::String(value) | ExternalValue::Enum(value)) => Ok(
            ParameterDescriptor::string(parameter.id, parameter.name, value),
        ),
        (_, value) => Err(ExternalError::InvalidDescriptor(format!(
            "parameter '{}' has unsupported default {value:?}",
            parameter.id
        ))),
    }
}

fn finite_f32(value: f64, parameter_id: &str) -> Result<f32, ExternalError> {
    if !value.is_finite() || value < f32::MIN as f64 || value > f32::MAX as f64 {
        return Err(ExternalError::InvalidDescriptor(format!(
            "parameter '{parameter_id}' default is not a finite f32"
        )));
    }
    Ok(value as f32)
}

fn execution_capabilities(capabilities: &Capabilities) -> Vec<ExecutionCapability> {
    let mut result = vec![ExecutionCapability::Cpu];
    if capabilities.gpu {
        result.push(ExecutionCapability::Gpu);
    }
    if capabilities.roi {
        result.push(ExecutionCapability::RegionAware);
    }
    if capabilities.full_frame {
        result.push(ExecutionCapability::FullFrame);
    }
    result
}

struct ExternalNode {
    host: ExternalHost,
    external_type_id: String,
    descriptor: NodeDescriptor,
    instance_id: String,
}

impl NodeInstance for ExternalNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let plane = self.host.data_plane();
        let mut buffers = Vec::new();
        let external_inputs = inputs
            .iter()
            .map(|(id, value)| {
                encode_value(value, &plane, &mut buffers)
                    .map(|value| (id.clone(), value))
                    .map_err(NodeError::Message)
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let external_parameters = parameters
            .iter()
            .map(|(id, value)| {
                parameter_to_external(value)
                    .map(|value| (id.clone(), value))
                    .map_err(NodeError::Message)
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        request_expected(
            &self.host,
            RequestPayload::Instantiate {
                type_id: self.external_type_id.clone(),
                instance_id: self.instance_id.clone(),
            },
            "instantiate",
        )
        .map_err(NodeError::Message)?;
        request_expected(
            &self.host,
            RequestPayload::SetParameters {
                instance_id: self.instance_id.clone(),
                parameters: external_parameters.clone(),
            },
            "set parameters",
        )
        .map_err(NodeError::Message)?;
        let response = self
            .host
            .request(RequestPayload::Evaluate {
                instance_id: self.instance_id.clone(),
                inputs: external_inputs,
                parameters: external_parameters,
            })
            .map_err(|error| NodeError::Message(error.to_string()))?;
        let ResponsePayload::Evaluated { outputs } = response else {
            return Err(NodeError::Message(
                "external host returned an unexpected evaluation response".into(),
            ));
        };

        eprintln!("external outputs: {outputs:?}");
        let mut result = BTreeMap::new();
        let mut output_descriptors = BTreeSet::new();
        for output in &self.descriptor.outputs {
            let value = outputs.get(&output.id).ok_or_else(|| {
                NodeError::Message(format!(
                    "external host did not produce output '{}'",
                    output.id
                ))
            })?;
            if let ExternalValue::Buffer(buffer) = value {
                output_descriptors.insert(buffer.relative_path.clone());
            }
            let decoded = decode_value(value, &output.data_type, &plane).map_err(|error| {
                NodeError::Message(format!(
                    "{error}; buffer root={}, inputs_alive={}",
                    plane.root().display(),
                    buffers.len()
                ))
            })?;
            result.insert(output.id.clone(), decoded);
        }
        // Hosts may alias one request-owned buffer across multiple outputs.
        for relative_path in output_descriptors {
            if let Some(buffer) = outputs.values().find_map(|value| match value {
                ExternalValue::Buffer(buffer) if buffer.relative_path == relative_path => {
                    Some(buffer)
                }
                _ => None,
            }) {
                plane
                    .cleanup(buffer)
                    .map_err(|error| NodeError::Message(error.to_string()))?;
            }
        }
        // Keep request-owned inputs alive until every aliased output has been decoded.
        drop(buffers);
        let _ = self.host.request(RequestPayload::Destroy {
            instance_id: self.instance_id.clone(),
        });
        Ok(NodeResult::new(result))
    }
}

fn request_expected(
    host: &ExternalHost,
    payload: RequestPayload,
    operation: &str,
) -> Result<(), String> {
    match host.request(payload).map_err(|error| error.to_string())? {
        ResponsePayload::Acknowledged | ResponsePayload::Instantiated { .. } => Ok(()),
        _ => Err(format!(
            "external host returned an unexpected {operation} response"
        )),
    }
}

fn parameter_to_external(value: &ParameterValue) -> Result<ExternalValue, String> {
    match value {
        ParameterValue::Float(value) if value.is_finite() => {
            Ok(ExternalValue::Float(f64::from(*value)))
        }
        ParameterValue::Float(_) => Err("parameter float must be finite".into()),
        ParameterValue::Integer(value) => Ok(ExternalValue::Integer(*value)),
        ParameterValue::Boolean(value) => Ok(ExternalValue::Boolean(*value)),
        ParameterValue::String(value) => Ok(ExternalValue::String(value.clone())),
    }
}

fn encode_value(
    value: &Value,
    plane: &DataPlane,
    buffers: &mut Vec<ManagedBuffer>,
) -> Result<ExternalValue, String> {
    match value {
        Value::Image(image) => {
            let bytes = image_bytes(image);
            let mut buffer = plane
                .create(DataKind::Image, &bytes)
                .map_err(|error| error.to_string())?;
            buffer.descriptor_mut().metadata = DataMetadata {
                dimensions: Some([image.width(), image.height()]),
                origin: Some([image.origin().0, image.origin().1]),
                pixel_format: Some(ExternalPixelFormat::Rgba32Float),
                color_domain: Some(format!("{:?}", image.color_domain())),
                region: Some([
                    image.origin().0,
                    image.origin().1,
                    image.width(),
                    image.height(),
                ]),
            };
            let descriptor = buffer.descriptor().clone();
            buffers.push(buffer);
            Ok(ExternalValue::Buffer(descriptor))
        }
        Value::Mask(mask) => {
            let bytes = mask_bytes(mask);
            let mut buffer = plane
                .create(DataKind::Mask, &bytes)
                .map_err(|error| error.to_string())?;
            buffer.descriptor_mut().metadata = DataMetadata {
                dimensions: Some([mask.width(), mask.height()]),
                origin: Some([mask.origin().0, mask.origin().1]),
                pixel_format: Some(ExternalPixelFormat::Mask32Float),
                color_domain: None,
                region: Some([
                    mask.origin().0,
                    mask.origin().1,
                    mask.width(),
                    mask.height(),
                ]),
            };
            let descriptor = buffer.descriptor().clone();
            buffers.push(buffer);
            Ok(ExternalValue::Buffer(descriptor))
        }
        Value::Float(value) if value.is_finite() => Ok(ExternalValue::Float(f64::from(*value))),
        Value::Integer(value) => Ok(ExternalValue::Integer(*value)),
        Value::Boolean(value) | Value::Condition(value) => Ok(ExternalValue::Boolean(*value)),
        Value::String(value) => Ok(ExternalValue::String(value.clone())),
        Value::Enum(value) => Ok(ExternalValue::Enum(value.clone())),
        Value::Color(value) => Ok(ExternalValue::Color([
            value.red,
            value.green,
            value.blue,
            value.alpha,
        ])),
        Value::Bytes(bytes) => {
            let buffer = plane
                .create(DataKind::Bytes, bytes)
                .map_err(|error| error.to_string())?;
            let descriptor = buffer.descriptor().clone();
            buffers.push(buffer);
            Ok(ExternalValue::Buffer(descriptor))
        }
        Value::Float(_) => Err("value float must be finite".into()),
        other => Err(format!(
            "unsupported external input type {}",
            other.data_type()
        )),
    }
}

fn decode_value(
    value: &ExternalValue,
    expected_type: &str,
    plane: &DataPlane,
) -> Result<Value, String> {
    match expected_type {
        "core.Image" => match value {
            ExternalValue::Buffer(buffer) => decode_image(buffer, plane),
            _ => Err("external image output is not a data buffer".into()),
        },
        "core.Mask" => match value {
            ExternalValue::Buffer(buffer) => decode_mask(buffer, plane),
            _ => Err("external mask output is not a data buffer".into()),
        },
        "core.Bytes" => match value {
            ExternalValue::Buffer(buffer) if buffer.kind == DataKind::Bytes => plane
                .read(buffer)
                .map(Value::Bytes)
                .map_err(|error| error.to_string()),
            _ => Err("external bytes output is not a byte buffer".into()),
        },
        "value.Float" => match value {
            ExternalValue::Float(value) if value.is_finite() => Ok(Value::Float(*value as f32)),
            ExternalValue::Integer(value) => Ok(Value::Float(*value as f32)),
            _ => Err("external float output has the wrong type".into()),
        },
        "value.Integer" => match value {
            ExternalValue::Integer(value) => Ok(Value::Integer(*value)),
            _ => Err("external integer output has the wrong type".into()),
        },
        "value.Boolean" | "value.Condition" => match value {
            ExternalValue::Boolean(value) => {
                if expected_type == "value.Condition" {
                    Ok(Value::Condition(*value))
                } else {
                    Ok(Value::Boolean(*value))
                }
            }
            _ => Err("external boolean output has the wrong type".into()),
        },
        "value.String" => match value {
            ExternalValue::String(value) | ExternalValue::Enum(value) => {
                Ok(Value::String(value.clone()))
            }
            _ => Err("external string output has the wrong type".into()),
        },
        "value.Enum" => match value {
            ExternalValue::String(value) | ExternalValue::Enum(value) => {
                Ok(Value::Enum(value.clone()))
            }
            _ => Err("external enum output has the wrong type".into()),
        },
        _ => Err(format!(
            "unsupported external output type '{expected_type}'"
        )),
    }
}

fn image_bytes(image: &Image) -> Vec<u8> {
    image
        .pixels()
        .iter()
        .flat_map(|pixel| pixel.iter().flat_map(|value| value.to_le_bytes()))
        .collect()
}

fn mask_bytes(mask: &Mask) -> Vec<u8> {
    mask.values()
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn dimensions_from_buffer(
    buffer: &DataBuffer,
    bytes: &[u8],
    bytes_per_pixel: usize,
) -> Result<Dimensions, String> {
    let [width, height] = buffer
        .metadata
        .dimensions
        .ok_or_else(|| "external buffer is missing dimensions".to_owned())?;
    let pixels = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| "external buffer dimensions overflow".to_owned())?;
    let expected = pixels
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| "external buffer byte length overflow".to_owned())?;
    if expected != bytes.len() {
        return Err(format!(
            "external buffer dimensions require {expected} bytes, got {}",
            bytes.len()
        ));
    }
    Ok(Dimensions::new(width, height))
}

fn decode_image(buffer: &DataBuffer, plane: &DataPlane) -> Result<Value, String> {
    if buffer.kind != DataKind::Image {
        return Err("external image output has the wrong buffer kind".into());
    }
    if buffer.metadata.pixel_format != Some(ExternalPixelFormat::Rgba32Float) {
        return Err("external image output is not RGBA32 float data".into());
    }
    let bytes = plane.read(buffer).map_err(|error| error.to_string())?;
    let dimensions = dimensions_from_buffer(buffer, &bytes, 16)?;
    let pixels = bytes
        .as_chunks::<16>()
        .0
        .iter()
        .map(|chunk| {
            let mut pixel = [0.0; 4];
            for (channel, bytes) in pixel.iter_mut().zip(chunk.as_chunks::<4>().0) {
                *channel = f32::from_le_bytes(*bytes);
            }
            pixel
        })
        .collect::<Vec<Pixel>>();
    let origin = buffer
        .metadata
        .origin
        .map(|origin| (origin[0], origin[1]))
        .unwrap_or((0, 0));
    let color_metadata = ColorMetadata {
        domain: color_domain(buffer.metadata.color_domain.as_deref()),
        ..ColorMetadata::default()
    };
    Image::from_pixels_with_origin(
        dimensions,
        origin,
        pixels,
        PixelFormat::Rgba32Float,
        color_metadata,
    )
    .map(Value::Image)
    .map_err(|error| error.to_string())
}

fn decode_mask(buffer: &DataBuffer, plane: &DataPlane) -> Result<Value, String> {
    if buffer.kind != DataKind::Mask {
        return Err("external mask output has the wrong buffer kind".into());
    }
    if buffer.metadata.pixel_format != Some(ExternalPixelFormat::Mask32Float) {
        return Err("external mask output is not mask32 float data".into());
    }
    let bytes = plane.read(buffer).map_err(|error| error.to_string())?;
    let dimensions = dimensions_from_buffer(buffer, &bytes, 4)?;
    let values = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect();
    let origin = buffer
        .metadata
        .origin
        .map(|origin| (origin[0], origin[1]))
        .unwrap_or((0, 0));
    Mask::from_values_with_origin(dimensions, origin, values)
        .map(Value::Mask)
        .map_err(|error| error.to_string())
}

fn color_domain(value: Option<&str>) -> ColorDomain {
    match value {
        Some("Srgb") => ColorDomain::Srgb,
        Some("DisplayP3") => ColorDomain::DisplayP3,
        Some("Unknown") => ColorDomain::Unknown,
        _ => ColorDomain::LinearSrgb,
    }
}
