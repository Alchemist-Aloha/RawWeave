//! Provider-facing AI execution primitives for RawWeave.
//!
//! Providers own execution and transport. Workflows are serializable descriptions
//! of what to run, while [`AiProvider`] implementations describe where and how
//! to run it. The crate deliberately has no model runtime dependency.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::thread;
use std::time::Duration;

use rawweave_graph::{
    ArtifactStore, Checkpoint, CheckpointArtifact, CheckpointError, GenerationToken,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Stable identifier returned by a provider for one generation.
pub type TaskId = String;

/// The four initial AI checkpoint operations supported by the backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiOperation {
    Img2Img,
    Inpaint,
    GenerativeFill,
    Upscale,
}

/// Provider capabilities are data, not provider-specific implementation types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiCapabilities {
    #[serde(default)]
    pub operations: BTreeSet<AiOperation>,
    pub supports_cancel: bool,
    pub supports_progress: bool,
    pub max_image_bytes: usize,
    pub color_interchange: ColorInterchange,
}

impl Default for AiCapabilities {
    fn default() -> Self {
        Self {
            operations: BTreeSet::from([
                AiOperation::Img2Img,
                AiOperation::Inpaint,
                AiOperation::GenerativeFill,
                AiOperation::Upscale,
            ]),
            supports_cancel: false,
            supports_progress: false,
            max_image_bytes: 64 * 1024 * 1024,
            color_interchange: ColorInterchange::png_srgb(),
        }
    }
}

/// Provider job lifecycle. These values are safe to persist in project state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    #[default]
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStatus {
    pub state: TaskState,
    #[serde(default)]
    pub progress_percent: Option<u8>,
    #[serde(default)]
    pub message: Option<String>,
}

impl TaskStatus {
    pub fn queued() -> Self {
        Self {
            state: TaskState::Queued,
            ..Self::default()
        }
    }

    pub fn running() -> Self {
        Self {
            state: TaskState::Running,
            ..Self::default()
        }
    }

    pub fn succeeded() -> Self {
        Self {
            state: TaskState::Succeeded,
            progress_percent: Some(100),
            ..Self::default()
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            state: TaskState::Failed,
            message: Some(message.into()),
            ..Self::default()
        }
    }
}

/// Immutable information needed to reproduce or explain a generation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskProvenance {
    pub node_id: String,
    pub node_version: u32,
    pub dependency_hash: String,
    #[serde(default)]
    pub provider_id: Option<String>,
    pub workflow_id: String,
    pub workflow_version: String,
    #[serde(default)]
    pub workflow_hash: Option<String>,
    #[serde(default)]
    pub parameter_hash: Option<String>,
}

impl TaskProvenance {
    pub fn new(
        node_id: impl Into<String>,
        node_version: u32,
        dependency_hash: impl Into<String>,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            node_version,
            dependency_hash: dependency_hash.into(),
            ..Self::default()
        }
    }

    pub fn with_provider(mut self, provider_id: impl Into<String>) -> Self {
        self.provider_id = Some(provider_id.into());
        self
    }

    pub fn with_workflow_hash(mut self, workflow_hash: impl Into<String>) -> Self {
        self.workflow_hash = Some(workflow_hash.into());
        self
    }

    pub fn with_parameter_hash(mut self, parameter_hash: impl Into<String>) -> Self {
        self.parameter_hash = Some(parameter_hash.into());
        self
    }
}

/// Persistable task state. It intentionally contains references, not secrets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiTask {
    pub task_id: TaskId,
    pub provider_id: String,
    pub state: TaskState,
    pub provenance: TaskProvenance,
}

/// A binding from an editor-facing name to a workflow input.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowBinding {
    pub node: String,
    pub input: String,
}

impl WorkflowBinding {
    pub fn new(node: impl Into<String>, input: impl Into<String>) -> Self {
        Self {
            node: node.into(),
            input: input.into(),
        }
    }
}

/// Bindings stay with the workflow and are independent of its provider.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowBindings {
    #[serde(default)]
    pub image: Option<WorkflowBinding>,
    #[serde(default)]
    pub mask: Option<WorkflowBinding>,
    #[serde(default)]
    pub prompt: Option<WorkflowBinding>,
    #[serde(default)]
    pub parameters: BTreeMap<String, WorkflowBinding>,
    pub output: WorkflowBinding,
}

/// A provider-neutral workflow definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AiWorkflow {
    pub id: String,
    pub version: String,
    pub definition: Value,
    #[serde(default)]
    pub bindings: WorkflowBindings,
}

impl AiWorkflow {
    pub fn new(
        id: impl Into<String>,
        version: impl Into<String>,
        definition: Value,
        bindings: WorkflowBindings,
    ) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            definition,
            bindings,
        }
    }

    pub fn content_hash(&self) -> Result<String, ProviderError> {
        hash_json(&self.definition)
    }
}

/// Explicit color assumptions at the provider boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageFormat {
    Png,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorSpace {
    Srgb,
    SceneLinearSrgb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlphaMode {
    Straight,
    Premultiplied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorInterchange {
    pub format: ImageFormat,
    pub color_space: ColorSpace,
    pub alpha_mode: AlphaMode,
    pub lossless: bool,
}

impl ColorInterchange {
    pub const fn png_srgb() -> Self {
        Self {
            format: ImageFormat::Png,
            color_space: ColorSpace::Srgb,
            alpha_mode: AlphaMode::Straight,
            lossless: true,
        }
    }
}

/// Lossless provider image interchange. The bytes are kept opaque to the core.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiImage {
    pub bytes: Vec<u8>,
    pub dimensions: [u32; 2],
    pub color: ColorInterchange,
}

impl AiImage {
    pub fn new(
        bytes: Vec<u8>,
        dimensions: [u32; 2],
        color: ColorInterchange,
    ) -> Result<Self, ProviderError> {
        if bytes.is_empty() {
            return Err(ProviderError::InvalidRequest(
                "AI image interchange bytes cannot be empty".to_owned(),
            ));
        }
        if dimensions.contains(&0) {
            return Err(ProviderError::InvalidRequest(
                "AI image dimensions must be non-zero".to_owned(),
            ));
        }
        Ok(Self {
            bytes,
            dimensions,
            color,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiInput {
    Image(AiImage),
    Mask(AiImage),
    Text(String),
    Json(Value),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiOutput {
    Image(AiImage),
    Bytes { bytes: Vec<u8>, media_type: String },
}

impl AiOutput {
    pub fn image(image: AiImage) -> Self {
        Self::Image(image)
    }

    pub fn from_bytes(bytes: Vec<u8>, media_type: impl Into<String>) -> Self {
        Self::Bytes {
            bytes,
            media_type: media_type.into(),
        }
    }

    pub fn as_image(&self) -> Option<&AiImage> {
        match self {
            Self::Image(image) => Some(image),
            Self::Bytes { .. } => None,
        }
    }

    pub fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Image(image) => Some(&image.bytes),
            Self::Bytes { bytes, .. } => Some(bytes),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiResult {
    pub task_id: TaskId,
    pub output: AiOutput,
    pub provenance: TaskProvenance,
}

impl AiResult {
    pub fn from_bytes(
        task_id: impl Into<String>,
        bytes: Vec<u8>,
        media_type: impl Into<String>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            output: AiOutput::from_bytes(bytes, media_type),
            provenance: TaskProvenance::default(),
        }
    }

    pub fn bytes(&self) -> Option<&[u8]> {
        self.output.bytes()
    }

    pub fn with_provenance(mut self, provenance: TaskProvenance) -> Self {
        self.provenance = provenance;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SubmitRequest {
    pub workflow: AiWorkflow,
    #[serde(default)]
    pub inputs: BTreeMap<String, AiInput>,
    #[serde(default)]
    pub parameters: BTreeMap<String, Value>,
    pub provenance: TaskProvenance,
}

impl SubmitRequest {
    pub fn new(workflow: AiWorkflow, mut provenance: TaskProvenance) -> Self {
        provenance.workflow_id = workflow.id.clone();
        provenance.workflow_version = workflow.version.clone();
        Self {
            workflow,
            inputs: BTreeMap::new(),
            parameters: BTreeMap::new(),
            provenance,
        }
    }

    pub fn with_input(mut self, id: impl Into<String>, input: AiInput) -> Self {
        self.inputs.insert(id.into(), input);
        self
    }

    pub fn with_parameter(mut self, id: impl Into<String>, value: Value) -> Self {
        self.parameters.insert(id.into(), value);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitResponse {
    pub task_id: TaskId,
    pub status: TaskStatus,
    pub provenance: TaskProvenance,
}

impl SubmitResponse {
    pub fn queued(task_id: impl Into<String>, provenance: TaskProvenance) -> Self {
        Self {
            task_id: task_id.into(),
            status: TaskStatus::queued(),
            provenance,
        }
    }
}

/// Errors intentionally contain bounded, provider-safe messages only.
#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("invalid AI provider request: {0}")]
    InvalidRequest(String),
    #[error("AI provider transport failed: {0}")]
    Transport(String),
    #[error("AI provider response status {status}: {message}")]
    Remote { status: u16, message: String },
    #[error("AI provider response was invalid: {0}")]
    InvalidResponse(String),
    #[error("AI provider response exceeded {limit} bytes (received at least {actual})")]
    ResponseTooLarge { limit: usize, actual: usize },
    #[error("AI provider task '{0}' was not found")]
    UnknownTask(String),
    #[error("AI provider task failed: {0}")]
    TaskFailed(String),
    #[error("AI provider task was cancelled")]
    Cancelled,
    #[error("AI provider task reached a cancelled state")]
    TaskCancelled,
    #[error("AI provider does not support this operation: {0}")]
    Unsupported(String),
    #[error("AI provider registry error: {0}")]
    Registry(String),
    #[error("AI provider secret reference could not be resolved")]
    SecretUnavailable,
    #[error("invalid AI provider endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("invalid polling policy: {0}")]
    InvalidPolicy(String),
    #[error("AI provider polling limit of {0} was reached")]
    PollLimitExceeded(u32),
    #[error("checkpoint integration failed: {0}")]
    Checkpoint(#[from] CheckpointError),
}

impl ProviderError {
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Transport(_)
                | Self::Remote {
                    status: 408 | 425 | 429,
                    ..
                }
        ) || matches!(self, Self::Remote { status, .. } if *status >= 500)
    }
}

/// The only provider execution interface used by the rest of the backend.
pub trait AiProvider: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> AiCapabilities;
    fn submit(&self, request: &SubmitRequest) -> Result<SubmitResponse, ProviderError>;
    fn status(&self, task_id: &str) -> Result<TaskStatus, ProviderError>;
    fn result(&self, task_id: &str) -> Result<AiResult, ProviderError>;
    fn cancel(&self, task_id: &str) -> Result<(), ProviderError>;
}

/// Registry lookup keeps provider configuration separate from workflow data.
#[derive(Default)]
pub struct ProviderRegistry {
    providers: BTreeMap<String, Arc<dyn AiProvider>>,
}

impl fmt::Debug for ProviderRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderRegistry")
            .field("ids", &self.ids())
            .finish()
    }
}

impl ProviderRegistry {
    pub fn register<P>(&mut self, provider: P) -> Result<(), ProviderError>
    where
        P: AiProvider + 'static,
    {
        let id = provider.id().to_owned();
        if id.trim().is_empty() {
            return Err(ProviderError::Registry(
                "provider id cannot be empty".to_owned(),
            ));
        }
        if self.providers.contains_key(&id) {
            return Err(ProviderError::Registry(format!(
                "provider '{id}' is already registered"
            )));
        }
        self.providers.insert(id, Arc::new(provider));
        Ok(())
    }

    pub fn provider(&self, id: &str) -> Option<Arc<dyn AiProvider>> {
        self.providers.get(id).cloned()
    }

    pub fn require(&self, id: &str) -> Result<Arc<dyn AiProvider>, ProviderError> {
        self.provider(id)
            .ok_or_else(|| ProviderError::Registry(format!("provider '{id}' is not registered")))
    }

    pub fn ids(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }
}

/// Cooperative cancellation that is safe to share with a polling worker.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub multiplier: u32,
    pub max_attempts: u32,
}

impl RetryPolicy {
    /// Construct a policy from delay bounds, multiplier, and total attempts.
    pub const fn new(
        initial_delay_ms: u64,
        max_delay_ms: u64,
        multiplier: u32,
        max_attempts: u32,
    ) -> Self {
        Self {
            initial_delay_ms,
            max_delay_ms,
            multiplier,
            max_attempts,
        }
    }

    fn validate(self) -> Result<(), ProviderError> {
        if self.max_attempts == 0 || self.multiplier == 0 {
            return Err(ProviderError::InvalidPolicy(
                "retry attempts and multiplier must be greater than zero".to_owned(),
            ));
        }
        if self.initial_delay_ms > self.max_delay_ms {
            return Err(ProviderError::InvalidPolicy(
                "initial retry delay cannot exceed the maximum delay".to_owned(),
            ));
        }
        Ok(())
    }

    fn delay(self, retry_index: u32) -> Duration {
        let exponent = retry_index.saturating_sub(1);
        let factor = u64::from(self.multiplier).saturating_pow(exponent);
        Duration::from_millis(
            self.initial_delay_ms
                .saturating_mul(factor)
                .min(self.max_delay_ms),
        )
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new(250, 5_000, 2, 3)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollPolicy {
    pub max_polls: u32,
    pub poll_delay: RetryPolicy,
    pub request_retry: RetryPolicy,
}

impl Default for PollPolicy {
    fn default() -> Self {
        Self {
            max_polls: 120,
            poll_delay: RetryPolicy::new(500, 10_000, 2, 1),
            request_retry: RetryPolicy::default(),
        }
    }
}

/// Injectable sleeper makes retry and polling deterministic in tests.
pub trait Sleeper: Send + Sync {
    fn sleep(&self, delay: Duration) -> Result<(), ProviderError>;
}

#[derive(Debug, Default)]
pub struct ThreadSleeper;

impl Sleeper for ThreadSleeper {
    fn sleep(&self, delay: Duration) -> Result<(), ProviderError> {
        thread::sleep(delay);
        Ok(())
    }
}

fn retry<T, F>(
    policy: RetryPolicy,
    token: &CancellationToken,
    sleeper: &dyn Sleeper,
    mut operation: F,
) -> Result<T, ProviderError>
where
    F: FnMut() -> Result<T, ProviderError>,
{
    policy.validate()?;
    for attempt in 1..=policy.max_attempts {
        if token.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) if error.is_retryable() && attempt < policy.max_attempts => {
                sleeper.sleep(policy.delay(attempt))?;
            }
            Err(error) => return Err(error),
        }
    }
    Err(ProviderError::InvalidPolicy(
        "retry policy did not execute an attempt".to_owned(),
    ))
}

/// Poll authoritative provider status and fetch the result after completion.
pub fn poll_until_complete(
    provider: &dyn AiProvider,
    task_id: &str,
    policy: PollPolicy,
    token: &CancellationToken,
    sleeper: &dyn Sleeper,
) -> Result<AiResult, ProviderError> {
    if policy.max_polls == 0 {
        return Err(ProviderError::InvalidPolicy(
            "poll count must be greater than zero".to_owned(),
        ));
    }
    policy.poll_delay.validate()?;
    policy.request_retry.validate()?;

    for poll_number in 0..policy.max_polls {
        if token.is_cancelled() {
            let _ = provider.cancel(task_id);
            return Err(ProviderError::Cancelled);
        }
        let status = retry(policy.request_retry, token, sleeper, || {
            provider.status(task_id)
        })?;
        match status.state {
            TaskState::Succeeded => {
                return retry(policy.request_retry, token, sleeper, || {
                    provider.result(task_id)
                });
            }
            TaskState::Failed => {
                return Err(ProviderError::TaskFailed(
                    status
                        .message
                        .unwrap_or_else(|| "provider reported failure".to_owned()),
                ));
            }
            TaskState::Cancelled => return Err(ProviderError::TaskCancelled),
            TaskState::Queued | TaskState::Running => {
                if poll_number + 1 == policy.max_polls {
                    return Err(ProviderError::PollLimitExceeded(policy.max_polls));
                }
                sleeper.sleep(policy.poll_delay.delay(poll_number + 1))?;
            }
        }
    }
    Err(ProviderError::PollLimitExceeded(policy.max_polls))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeaderValue {
    Literal(String),
    /// Only a reference is serialized. The referenced secret is resolved at call time.
    SecretRef(String),
}

impl fmt::Debug for HeaderValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(value) => formatter.debug_tuple("Literal").field(value).finish(),
            Self::SecretRef(_) => formatter.write_str("SecretRef(REDACTED)"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpEndpoint {
    pub method: HttpMethod,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, HeaderValue>,
}

impl HttpEndpoint {
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpProviderManifest {
    pub id: String,
    pub name: String,
    pub submit: HttpEndpoint,
    #[serde(default)]
    pub status: Option<HttpEndpoint>,
    #[serde(default)]
    pub result: Option<HttpEndpoint>,
    #[serde(default)]
    pub cancel: Option<HttpEndpoint>,
    #[serde(default)]
    pub job_id_path: Option<String>,
    #[serde(default)]
    pub status_path: Option<String>,
    #[serde(default)]
    pub result_path: Option<String>,
    #[serde(default)]
    pub completion_states: BTreeSet<String>,
    #[serde(default)]
    pub failure_states: BTreeSet<String>,
    #[serde(default = "default_max_response_bytes")]
    pub max_response_bytes: usize,
    #[serde(default)]
    pub allow_insecure_http: bool,
}

fn default_max_response_bytes() -> usize {
    16 * 1024 * 1024
}

impl HttpProviderManifest {
    pub fn new(id: impl Into<String>, name: impl Into<String>, submit: HttpEndpoint) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            submit,
            status: None,
            result: None,
            cancel: None,
            job_id_path: None,
            status_path: None,
            result_path: None,
            completion_states: BTreeSet::from(["completed".to_owned(), "succeeded".to_owned()]),
            failure_states: BTreeSet::from(["failed".to_owned(), "error".to_owned()]),
            max_response_bytes: default_max_response_bytes(),
            allow_insecure_http: false,
        }
    }

    fn validate(&self) -> Result<(), ProviderError> {
        if self.id.trim().is_empty() {
            return Err(ProviderError::InvalidRequest(
                "HTTP provider id cannot be empty".to_owned(),
            ));
        }
        for endpoint in std::iter::once(&self.submit)
            .chain(self.status.iter())
            .chain(self.result.iter())
            .chain(self.cancel.iter())
        {
            validate_endpoint(endpoint, self.allow_insecure_http)?;
        }
        if self.max_response_bytes == 0 {
            return Err(ProviderError::InvalidRequest(
                "HTTP response limit must be greater than zero".to_owned(),
            ));
        }
        if self.job_id_path.is_some() && self.status.is_none() && self.result.is_none() {
            return Err(ProviderError::InvalidRequest(
                "asynchronous HTTP manifests need a status or result endpoint".to_owned(),
            ));
        }
        Ok(())
    }
}

fn sensitive_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "authorization"
        || lower == "proxy-authorization"
        || lower == "cookie"
        || lower == "set-cookie"
        || lower.contains("api-key")
        || lower.contains("apikey")
        || lower.contains("token")
        || lower.contains("secret")
}

fn validate_endpoint(
    endpoint: &HttpEndpoint,
    allow_insecure_http: bool,
) -> Result<(), ProviderError> {
    let url = endpoint.url.replace("$JOB_ID", "job-id");
    let https = url.starts_with("https://");
    let http = url.starts_with("http://");
    if !https && !(allow_insecure_http && http) {
        return Err(ProviderError::InvalidEndpoint(
            "endpoint must use HTTPS unless insecure HTTP is explicitly enabled".to_owned(),
        ));
    }
    if url.contains('@') || url.contains('\n') || url.contains('\r') {
        return Err(ProviderError::InvalidEndpoint(
            "endpoint contains credentials or control characters".to_owned(),
        ));
    }
    for (name, value) in &endpoint.headers {
        if sensitive_header(name) && matches!(value, HeaderValue::Literal(_)) {
            return Err(ProviderError::InvalidEndpoint(format!(
                "sensitive header '{name}' must use a secret reference"
            )));
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
    pub max_response_bytes: usize,
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let headers = self
            .headers
            .keys()
            .map(|name| {
                (
                    name,
                    if sensitive_header(name) {
                        "REDACTED"
                    } else {
                        "<present>"
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        formatter
            .debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &headers)
            .field("body_len", &self.body.len())
            .field("max_response_bytes", &self.max_response_bytes)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn bytes(status: u16, body: Vec<u8>, media_type: impl Into<String>) -> Self {
        Self {
            status,
            headers: BTreeMap::from([("content-type".to_owned(), media_type.into())]),
            body,
        }
    }

    pub fn json(status: u16, value: Value) -> Self {
        Self::bytes(
            status,
            serde_json::to_vec(&value).unwrap_or_default(),
            "application/json",
        )
    }

    fn media_type(&self) -> String {
        self.headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            .map(|(_, value)| value.split(';').next().unwrap_or(value).trim().to_owned())
            .unwrap_or_else(|| "application/octet-stream".to_owned())
    }
}

pub trait HttpTransport: Send + Sync {
    fn execute(&self, request: HttpRequest) -> Result<HttpResponse, ProviderError>;
}

type SecretResolver = Arc<dyn Fn(&str) -> Result<Option<String>, ProviderError> + Send + Sync>;

struct HttpTask {
    remote_job_id: Option<String>,
    inline_result: Option<AiResult>,
    provenance: TaskProvenance,
    state: TaskState,
}

/// Manifest-driven HTTP provider. The transport is injected so callers can use
/// a platform HTTP stack while tests remain offline and deterministic.
pub struct HttpProvider {
    manifest: HttpProviderManifest,
    transport: Arc<dyn HttpTransport>,
    secrets: SecretResolver,
    next_task_id: AtomicU64,
    tasks: Mutex<BTreeMap<TaskId, HttpTask>>,
}

impl fmt::Debug for HttpProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpProvider")
            .field("manifest", &self.manifest)
            .field(
                "task_count",
                &self.tasks.lock().map(|tasks| tasks.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl HttpProvider {
    pub fn new<F>(
        manifest: HttpProviderManifest,
        transport: Arc<dyn HttpTransport>,
        secrets: F,
    ) -> Result<Self, ProviderError>
    where
        F: Fn(&str) -> Result<Option<String>, ProviderError> + Send + Sync + 'static,
    {
        manifest.validate()?;
        Ok(Self {
            manifest,
            transport,
            secrets: Arc::new(secrets),
            next_task_id: AtomicU64::new(1),
            tasks: Mutex::new(BTreeMap::new()),
        })
    }

    fn local_task_id(&self) -> TaskId {
        format!(
            "{}-{}",
            self.manifest.id,
            self.next_task_id.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn endpoint_request(
        &self,
        endpoint: &HttpEndpoint,
        job_id: Option<&str>,
        body: Vec<u8>,
    ) -> Result<HttpRequest, ProviderError> {
        let url = endpoint.url.replace("$JOB_ID", job_id.unwrap_or(""));
        let mut headers = BTreeMap::new();
        for (name, value) in &endpoint.headers {
            let resolved = match value {
                HeaderValue::Literal(value) => value.clone(),
                HeaderValue::SecretRef(reference) => {
                    (self.secrets)(reference)?.ok_or(ProviderError::SecretUnavailable)?
                }
            };
            headers.insert(name.clone(), resolved);
        }
        if !body.is_empty()
            && !headers
                .keys()
                .any(|name| name.eq_ignore_ascii_case("content-type"))
        {
            headers.insert("Content-Type".to_owned(), "application/json".to_owned());
        }
        Ok(HttpRequest {
            method: endpoint.method,
            url,
            headers,
            body,
            max_response_bytes: self.manifest.max_response_bytes,
        })
    }

    fn execute(
        &self,
        endpoint: &HttpEndpoint,
        job_id: Option<&str>,
        body: Vec<u8>,
    ) -> Result<HttpResponse, ProviderError> {
        let request = self.endpoint_request(endpoint, job_id, body)?;
        let response = self.transport.execute(request)?;
        if response.body.len() > self.manifest.max_response_bytes {
            return Err(ProviderError::ResponseTooLarge {
                limit: self.manifest.max_response_bytes,
                actual: response.body.len(),
            });
        }
        if !(200..300).contains(&response.status) {
            return Err(ProviderError::Remote {
                status: response.status,
                message: bounded_message(&response.body),
            });
        }
        Ok(response)
    }

    fn task(&self, task_id: &str) -> Result<HttpTask, ProviderError> {
        self.tasks
            .lock()
            .map_err(|_| ProviderError::Transport("HTTP task state is poisoned".to_owned()))?
            .get(task_id)
            .map(|task| HttpTask {
                remote_job_id: task.remote_job_id.clone(),
                inline_result: task.inline_result.clone(),
                provenance: task.provenance.clone(),
                state: task.state,
            })
            .ok_or_else(|| ProviderError::UnknownTask(task_id.to_owned()))
    }

    fn json_response(response: &HttpResponse) -> Result<Value, ProviderError> {
        serde_json::from_slice(&response.body)
            .map_err(|error| ProviderError::InvalidResponse(format!("JSON body: {error}")))
    }
}

impl AiProvider for HttpProvider {
    fn id(&self) -> &str {
        &self.manifest.id
    }

    fn capabilities(&self) -> AiCapabilities {
        AiCapabilities {
            supports_cancel: self.manifest.cancel.is_some(),
            ..AiCapabilities::default()
        }
    }

    fn submit(&self, request: &SubmitRequest) -> Result<SubmitResponse, ProviderError> {
        let body = serde_json::to_vec(request)
            .map_err(|error| ProviderError::InvalidRequest(format!("request JSON: {error}")))?;
        let response = self.execute(&self.manifest.submit, None, body)?;
        let task_id = self.local_task_id();
        let workflow_hash = request.workflow.content_hash()?;
        let mut provenance = request.provenance.clone();
        provenance.provider_id = Some(self.id().to_owned());
        provenance.workflow_hash = Some(workflow_hash);
        let (remote_job_id, inline_result, state) = if let Some(path) = &self.manifest.job_id_path {
            let json = Self::json_response(&response)?;
            let remote_job_id = json_path(&json, path)?
                .as_str()
                .ok_or_else(|| {
                    ProviderError::InvalidResponse("job id must be a string".to_owned())
                })?
                .to_owned();
            (Some(remote_job_id), None, TaskState::Queued)
        } else {
            (
                None,
                Some(
                    AiResult::from_bytes(
                        task_id.clone(),
                        response.body.clone(),
                        response.media_type(),
                    )
                    .with_provenance(provenance.clone()),
                ),
                TaskState::Succeeded,
            )
        };
        self.tasks
            .lock()
            .map_err(|_| ProviderError::Transport("HTTP task state is poisoned".to_owned()))?
            .insert(
                task_id.clone(),
                HttpTask {
                    remote_job_id,
                    inline_result,
                    provenance: provenance.clone(),
                    state,
                },
            );
        Ok(SubmitResponse {
            task_id,
            status: TaskStatus {
                state,
                progress_percent: (state == TaskState::Succeeded).then_some(100),
                message: None,
            },
            provenance,
        })
    }

    fn status(&self, task_id: &str) -> Result<TaskStatus, ProviderError> {
        let task = self.task(task_id)?;
        if task.inline_result.is_some() {
            return Ok(TaskStatus::succeeded());
        }
        let job_id = task.remote_job_id.as_deref().ok_or_else(|| {
            ProviderError::InvalidResponse("HTTP task has no remote id".to_owned())
        })?;
        let endpoint = self.manifest.status.as_ref().ok_or_else(|| {
            ProviderError::Unsupported("HTTP status endpoint is not configured".to_owned())
        })?;
        let response = self.execute(endpoint, Some(job_id), Vec::new())?;
        let json = Self::json_response(&response)?;
        let state_value = json_path(
            &json,
            self.manifest.status_path.as_deref().unwrap_or("$.status"),
        )?;
        let state_name = state_value
            .as_str()
            .ok_or_else(|| {
                ProviderError::InvalidResponse("status state must be a string".to_owned())
            })?
            .to_ascii_lowercase();
        let state = if self.manifest.completion_states.contains(&state_name) {
            TaskState::Succeeded
        } else if self.manifest.failure_states.contains(&state_name) {
            TaskState::Failed
        } else {
            TaskState::Running
        };
        let status = TaskStatus {
            state,
            progress_percent: (state == TaskState::Succeeded).then_some(100),
            message: (state == TaskState::Failed).then_some(state_name),
        };
        if let Ok(mut tasks) = self.tasks.lock()
            && let Some(task) = tasks.get_mut(task_id)
        {
            task.state = state;
        }
        Ok(status)
    }

    fn result(&self, task_id: &str) -> Result<AiResult, ProviderError> {
        let task = self.task(task_id)?;
        if let Some(result) = task.inline_result {
            return Ok(result);
        }
        let job_id = task.remote_job_id.as_deref().ok_or_else(|| {
            ProviderError::InvalidResponse("HTTP task has no remote id".to_owned())
        })?;
        let endpoint = self.manifest.result.as_ref().ok_or_else(|| {
            ProviderError::Unsupported("HTTP result endpoint is not configured".to_owned())
        })?;
        let response = self.execute(endpoint, Some(job_id), Vec::new())?;
        let media_type = response.media_type();
        let bytes = if let Some(path) = &self.manifest.result_path {
            let json = Self::json_response(&response)?;
            let value = json_path(&json, path)?;
            if let Some(text) = value.as_str() {
                text.as_bytes().to_vec()
            } else {
                serde_json::to_vec(value)
                    .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?
            }
        } else {
            response.body
        };
        Ok(AiResult::from_bytes(task_id, bytes, media_type).with_provenance(task.provenance))
    }

    fn cancel(&self, task_id: &str) -> Result<(), ProviderError> {
        let task = self.task(task_id)?;
        if let Some(job_id) = task.remote_job_id.as_deref() {
            let endpoint = self.manifest.cancel.as_ref().ok_or_else(|| {
                ProviderError::Unsupported("HTTP cancellation is not configured".to_owned())
            })?;
            let _ = self.execute(endpoint, Some(job_id), Vec::new())?;
        }
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| ProviderError::Transport("HTTP task state is poisoned".to_owned()))?;
        if let Some(task) = tasks.get_mut(task_id) {
            task.state = TaskState::Cancelled;
        }
        Ok(())
    }
}

fn bounded_message(body: &[u8]) -> String {
    const MAX_MESSAGE_BYTES: usize = 256;
    let bounded = &body[..body.len().min(MAX_MESSAGE_BYTES)];
    String::from_utf8_lossy(bounded).replace('\n', " ")
}

fn json_path<'a>(value: &'a Value, path: &str) -> Result<&'a Value, ProviderError> {
    let path = path
        .strip_prefix("$.")
        .or_else(|| (path == "$").then_some(""))
        .ok_or_else(|| ProviderError::InvalidResponse(format!("unsupported JSON path '{path}'")))?;
    let mut current = value;
    if path.is_empty() {
        return Ok(current);
    }
    for segment in path.split('.') {
        current = current.get(segment).ok_or_else(|| {
            ProviderError::InvalidResponse(format!("JSON path '{path}' was not found"))
        })?;
    }
    Ok(current)
}

fn set_workflow_binding(
    definition: &mut Value,
    binding: &WorkflowBinding,
    value: Value,
) -> Result<(), ProviderError> {
    let object = definition.as_object_mut().ok_or_else(|| {
        ProviderError::InvalidRequest(
            "ComfyUI workflow definition must be a JSON object".to_owned(),
        )
    })?;
    let node = if object.get("nodes").and_then(Value::as_object).is_some() {
        object
            .get_mut("nodes")
            .and_then(Value::as_object_mut)
            .and_then(|nodes| nodes.get_mut(&binding.node))
            .and_then(Value::as_object_mut)
    } else {
        object.get_mut(&binding.node).and_then(Value::as_object_mut)
    }
    .ok_or_else(|| {
        ProviderError::InvalidRequest(format!(
            "ComfyUI workflow binding node '{}' was not found",
            binding.node
        ))
    })?;
    let inputs = node
        .get_mut("inputs")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            ProviderError::InvalidRequest(format!(
                "ComfyUI workflow binding node '{}' has no inputs",
                binding.node
            ))
        })?;
    inputs.insert(binding.input.clone(), value);
    Ok(())
}

fn join_url(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComfyUiConfig {
    #[serde(default = "default_provider_id")]
    pub provider_id: String,
    pub base_url: String,
    #[serde(default = "default_client_id")]
    pub client_id: String,
    #[serde(default = "default_max_response_bytes")]
    pub max_response_bytes: usize,
    #[serde(default = "default_true")]
    pub allow_insecure_http: bool,
}

fn default_client_id() -> String {
    "rawweave".to_owned()
}

fn default_provider_id() -> String {
    "comfyui".to_owned()
}

const fn default_true() -> bool {
    true
}

impl ComfyUiConfig {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            provider_id: default_provider_id(),
            base_url: base_url.into(),
            client_id: default_client_id(),
            max_response_bytes: default_max_response_bytes(),
            allow_insecure_http: true,
        }
    }

    pub fn with_client_id(mut self, client_id: impl Into<String>) -> Self {
        self.client_id = client_id.into();
        self
    }

    pub fn with_provider_id(mut self, provider_id: impl Into<String>) -> Self {
        self.provider_id = provider_id.into();
        self
    }
}

struct ComfyTask {
    prompt_id: String,
    provenance: TaskProvenance,
    output_node: Option<String>,
}

/// ComfyUI's HTTP adapter. It uses the same injected transport as the generic
/// provider, so local/LAN/VPN/remote operation is a configuration concern.
pub struct ComfyUiProvider {
    config: ComfyUiConfig,
    transport: Arc<dyn HttpTransport>,
    tasks: Mutex<BTreeMap<TaskId, ComfyTask>>,
}

impl fmt::Debug for ComfyUiProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ComfyUiProvider")
            .field("config", &self.config)
            .field(
                "task_count",
                &self.tasks.lock().map(|tasks| tasks.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl ComfyUiProvider {
    pub fn new(
        config: ComfyUiConfig,
        transport: Arc<dyn HttpTransport>,
    ) -> Result<Self, ProviderError> {
        let endpoint = HttpEndpoint::new(HttpMethod::Get, config.base_url.clone());
        validate_endpoint(&endpoint, config.allow_insecure_http)?;
        if config.max_response_bytes == 0 {
            return Err(ProviderError::InvalidRequest(
                "ComfyUI response limit must be greater than zero".to_owned(),
            ));
        }
        Ok(Self {
            config,
            transport,
            tasks: Mutex::new(BTreeMap::new()),
        })
    }

    fn execute(
        &self,
        method: HttpMethod,
        url: String,
        body: Vec<u8>,
    ) -> Result<HttpResponse, ProviderError> {
        let request = HttpRequest {
            method,
            url,
            headers: BTreeMap::from([(
                String::from("Content-Type"),
                String::from("application/json"),
            )]),
            body,
            max_response_bytes: self.config.max_response_bytes,
        };
        let response = self.transport.execute(request)?;
        if response.body.len() > self.config.max_response_bytes {
            return Err(ProviderError::ResponseTooLarge {
                limit: self.config.max_response_bytes,
                actual: response.body.len(),
            });
        }
        if !(200..300).contains(&response.status) {
            return Err(ProviderError::Remote {
                status: response.status,
                message: bounded_message(&response.body),
            });
        }
        Ok(response)
    }

    fn task(&self, task_id: &str) -> Result<ComfyTask, ProviderError> {
        self.tasks
            .lock()
            .map_err(|_| ProviderError::Transport("ComfyUI task state is poisoned".to_owned()))?
            .get(task_id)
            .map(|task| ComfyTask {
                prompt_id: task.prompt_id.clone(),
                provenance: task.provenance.clone(),
                output_node: task.output_node.clone(),
            })
            .ok_or_else(|| ProviderError::UnknownTask(task_id.to_owned()))
    }

    fn history(&self, prompt_id: &str) -> Result<Value, ProviderError> {
        let response = self.execute(
            HttpMethod::Get,
            join_url(&self.config.base_url, &format!("history/{prompt_id}")),
            Vec::new(),
        )?;
        HttpProvider::json_response(&response)
    }

    fn history_entry<'a>(history: &'a Value, prompt_id: &str) -> Result<&'a Value, ProviderError> {
        history.get(prompt_id).ok_or_else(|| {
            ProviderError::InvalidResponse("ComfyUI history did not contain task".to_owned())
        })
    }

    fn upload_asset(
        &self,
        bytes: &[u8],
        filename: &str,
        kind: &str,
    ) -> Result<String, ProviderError> {
        let boundary = "----rawweave-ai-boundary";
        let mut body = Vec::with_capacity(bytes.len().saturating_add(256));
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"image\"; filename=\"{filename}\"\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let request = HttpRequest {
            method: HttpMethod::Post,
            url: format!(
                "{}?type=input&overwrite=true&subfolder=rawweave-{kind}",
                join_url(&self.config.base_url, "upload/image")
            ),
            headers: BTreeMap::from([(
                "Content-Type".to_owned(),
                format!("multipart/form-data; boundary={boundary}"),
            )]),
            body,
            max_response_bytes: self.config.max_response_bytes,
        };
        let response = self.transport.execute(request)?;
        if response.body.len() > self.config.max_response_bytes {
            return Err(ProviderError::ResponseTooLarge {
                limit: self.config.max_response_bytes,
                actual: response.body.len(),
            });
        }
        if !(200..300).contains(&response.status) {
            return Err(ProviderError::Remote {
                status: response.status,
                message: bounded_message(&response.body),
            });
        }
        let uploaded = serde_json::from_slice::<Value>(&response.body)
            .ok()
            .and_then(|value| {
                let name = value.get("name").and_then(Value::as_str)?;
                let subfolder = value
                    .get("subfolder")
                    .and_then(Value::as_str)
                    .filter(|subfolder| !subfolder.is_empty());
                Some(match subfolder {
                    Some(subfolder) if !name.starts_with(&format!("{subfolder}/")) => {
                        format!("{subfolder}/{name}")
                    }
                    _ => name.to_owned(),
                })
            })
            .unwrap_or_else(|| filename.to_owned());
        Ok(uploaded)
    }

    fn prepare_workflow(&self, request: &SubmitRequest) -> Result<Value, ProviderError> {
        let mut definition = request.workflow.definition.clone();
        let mut set_input = |binding: &WorkflowBinding, value: Value| {
            set_workflow_binding(&mut definition, binding, value)
        };
        if let Some(binding) = request.workflow.bindings.image.as_ref()
            && let Some(AiInput::Image(image)) = request.inputs.get("image")
        {
            let filename = self.upload_asset(&image.bytes, "rawweave-image.png", "image")?;
            set_input(binding, Value::String(filename))?;
        }
        if let Some(binding) = request.workflow.bindings.mask.as_ref()
            && let Some(AiInput::Mask(mask)) = request.inputs.get("mask")
        {
            let filename = self.upload_asset(&mask.bytes, "rawweave-mask.png", "mask")?;
            set_input(binding, Value::String(filename))?;
        }
        if let Some(binding) = request.workflow.bindings.prompt.as_ref()
            && let Some(AiInput::Text(prompt)) = request.inputs.get("prompt")
        {
            set_input(binding, Value::String(prompt.clone()))?;
        }
        for (name, binding) in &request.workflow.bindings.parameters {
            if let Some(value) = request.parameters.get(name) {
                set_input(binding, value.clone())?;
            }
        }
        Ok(definition)
    }
}

impl AiProvider for ComfyUiProvider {
    fn id(&self) -> &str {
        &self.config.provider_id
    }

    fn capabilities(&self) -> AiCapabilities {
        AiCapabilities {
            supports_cancel: true,
            supports_progress: true,
            ..AiCapabilities::default()
        }
    }

    fn submit(&self, request: &SubmitRequest) -> Result<SubmitResponse, ProviderError> {
        let prompt = self.prepare_workflow(request)?;
        let payload = serde_json::json!({
            "prompt": prompt,
            "client_id": self.config.client_id,
        });
        let response = self.execute(
            HttpMethod::Post,
            join_url(&self.config.base_url, "prompt"),
            serde_json::to_vec(&payload)
                .map_err(|error| ProviderError::InvalidRequest(error.to_string()))?,
        )?;
        let json = HttpProvider::json_response(&response)?;
        let prompt_id = json
            .get("prompt_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ProviderError::InvalidResponse(
                    "ComfyUI response did not contain prompt_id".to_owned(),
                )
            })?
            .to_owned();
        let mut provenance = request.provenance.clone();
        provenance.provider_id = Some(self.id().to_owned());
        provenance.workflow_hash = Some(request.workflow.content_hash()?);
        self.tasks
            .lock()
            .map_err(|_| ProviderError::Transport("ComfyUI task state is poisoned".to_owned()))?
            .insert(
                prompt_id.clone(),
                ComfyTask {
                    prompt_id: prompt_id.clone(),
                    provenance: provenance.clone(),
                    output_node: (!request.workflow.bindings.output.node.is_empty())
                        .then(|| request.workflow.bindings.output.node.clone()),
                },
            );
        Ok(SubmitResponse::queued(prompt_id, provenance))
    }

    fn status(&self, task_id: &str) -> Result<TaskStatus, ProviderError> {
        let task = self.task(task_id)?;
        let history = self.history(&task.prompt_id)?;
        let Some(entry) = history.get(&task.prompt_id) else {
            return Ok(TaskStatus::running());
        };
        let status = entry.get("status").unwrap_or(&Value::Null);
        if status.get("completed").and_then(Value::as_bool) == Some(true) {
            return Ok(TaskStatus::succeeded());
        }
        let status_name = status
            .get("status_str")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(status_name.as_str(), "error" | "failed") {
            return Ok(TaskStatus::failed(status_name));
        }
        Ok(TaskStatus::running())
    }

    fn result(&self, task_id: &str) -> Result<AiResult, ProviderError> {
        let task = self.task(task_id)?;
        let history = self.history(&task.prompt_id)?;
        let entry = Self::history_entry(&history, &task.prompt_id)?;
        let image = entry
            .get("outputs")
            .and_then(Value::as_object)
            .and_then(|outputs| {
                task.output_node
                    .as_deref()
                    .and_then(|node| outputs.get(node))
                    .and_then(|output| output.get("images"))
                    .or_else(|| outputs.values().find_map(|output| output.get("images")))
            })
            .and_then(Value::as_array)
            .and_then(|images| images.first())
            .ok_or_else(|| {
                ProviderError::InvalidResponse("ComfyUI result has no output image".to_owned())
            })?;
        let filename = image
            .get("filename")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ProviderError::InvalidResponse("ComfyUI image has no filename".to_owned())
            })?;
        let subfolder = image.get("subfolder").and_then(Value::as_str).unwrap_or("");
        let output_type = image
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("output");
        let url = format!(
            "{}?filename={filename}&subfolder={subfolder}&type={output_type}",
            join_url(&self.config.base_url, "view")
        );
        let response = self.execute(HttpMethod::Get, url, Vec::new())?;
        let media_type = response.media_type();
        Ok(AiResult::from_bytes(task_id, response.body, media_type)
            .with_provenance(task.provenance))
    }

    fn cancel(&self, task_id: &str) -> Result<(), ProviderError> {
        let task = self.task(task_id)?;
        let payload = serde_json::json!({"prompt_id": task.prompt_id});
        let _ = self.execute(
            HttpMethod::Post,
            join_url(&self.config.base_url, "interrupt"),
            serde_json::to_vec(&payload)
                .map_err(|error| ProviderError::InvalidRequest(error.to_string()))?,
        )?;
        Ok(())
    }
}

/// A tokenized ManualCheckpoint generation. Late results cannot commit after
/// cancellation or another generation takes ownership of the checkpoint.
#[derive(Clone, Debug)]
pub struct CheckpointGeneration {
    pub token: GenerationToken,
    pub task: AiTask,
}

impl CheckpointGeneration {
    pub fn begin(checkpoint: &mut Checkpoint, task: AiTask) -> Result<Self, ProviderError> {
        let token = checkpoint.begin_generation_token()?;
        Ok(Self { token, task })
    }

    pub fn commit(
        &self,
        checkpoint: &mut Checkpoint,
        artifact: CheckpointArtifact,
        store: &ArtifactStore,
    ) -> Result<(), ProviderError> {
        checkpoint.commit_generation(self.token, artifact, store)?;
        Ok(())
    }

    pub fn cancel(&self, checkpoint: &mut Checkpoint) -> Result<(), ProviderError> {
        checkpoint.cancel_generation(&self.token)?;
        Ok(())
    }

    pub fn fail(
        &self,
        checkpoint: &mut Checkpoint,
        message: impl Into<String>,
    ) -> Result<(), ProviderError> {
        checkpoint.fail_generation(&self.token, message)?;
        Ok(())
    }
}

/// Submit a provider-neutral workflow as an explicit checkpoint generation.
pub fn submit_checkpoint_generation(
    registry: &ProviderRegistry,
    provider_id: &str,
    checkpoint: &mut Checkpoint,
    mut request: SubmitRequest,
) -> Result<CheckpointGeneration, ProviderError> {
    request.provenance.provider_id = Some(provider_id.to_owned());
    let provider = registry.require(provider_id)?;
    let response = provider.submit(&request)?;
    let task = AiTask {
        task_id: response.task_id,
        provider_id: provider_id.to_owned(),
        state: response.status.state,
        provenance: response.provenance,
    };
    CheckpointGeneration::begin(checkpoint, task)
}

fn hash_json(value: &Value) -> Result<String, ProviderError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| ProviderError::InvalidRequest(format!("workflow JSON: {error}")))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_headers_require_references() {
        let mut endpoint = HttpEndpoint::new(HttpMethod::Get, "https://example.test");
        endpoint.headers.insert(
            "Authorization".to_owned(),
            HeaderValue::Literal("secret".to_owned()),
        );
        let manifest = HttpProviderManifest::new("id", "name", endpoint);
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn json_paths_are_bounded_to_object_segments() {
        let value = serde_json::json!({"job": {"id": "one"}});
        assert_eq!(
            json_path(&value, "$.job.id").unwrap(),
            &Value::String("one".to_owned())
        );
        assert!(json_path(&value, "job.id").is_err());
    }
}
