use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use rawweave_external_protocol::{
    CURRENT_PROTOCOL_VERSION, Capabilities, DataBuffer, DataKind, FrameCodec, Message,
    ProtocolError, ProtocolRange, Request, RequestId, RequestPayload, Response, ResponsePayload,
};
use sha2::{Digest, Sha256};
use tempfile::{TempDir, tempdir};
use thiserror::Error;

static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error)]
pub enum HostError {
    #[error("invalid host configuration: {0}")]
    InvalidConfig(String),
    #[error("host I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("external protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("buffer is larger than the configured limit: {size} bytes > {limit} bytes")]
    BufferTooLarge { size: usize, limit: usize },
    #[error("invalid buffer path: {0}")]
    InvalidBufferPath(String),
    #[error("buffer file is missing: {0}")]
    BufferMissing(PathBuf),
    #[error("buffer size mismatch: descriptor says {expected} bytes, file contains {actual} bytes")]
    BufferSizeMismatch { expected: u64, actual: u64 },
    #[error("buffer hash mismatch: descriptor says {expected}, file hashes to {actual}")]
    BufferHashMismatch { expected: String, actual: String },
    #[error("request {request_id:?} timed out after {timeout:?}")]
    Timeout {
        request_id: Option<RequestId>,
        timeout: Duration,
    },
    #[error("external host process crashed with status {status:?}: {stderr}")]
    Crashed { status: Option<i32>, stderr: String },
    #[error("external host returned response for request {actual}, expected {expected}")]
    RequestIdMismatch {
        expected: RequestId,
        actual: RequestId,
    },
    #[error("external host rejected the requested capability: {0}")]
    CapabilityRejected(String),
    #[error("external host returned {code}: {message}")]
    Remote { code: String, message: String },
    #[error("external host emitted an unexpected message")]
    UnexpectedMessage,
    #[error("supervisor state is poisoned")]
    StatePoisoned,
    #[error("CLI argument '{name}' is invalid: {reason}")]
    InvalidArgument { name: String, reason: String },
    #[error("CLI input '{name}' is missing")]
    MissingInput { name: String },
    #[error("CLI output '{name}' was not created")]
    OutputMissing { name: String },
    #[error(
        "CLI output '{name}' is larger than the configured limit: {size} bytes > {limit} bytes"
    )]
    OutputTooLarge {
        name: String,
        size: usize,
        limit: usize,
    },
    #[error("CLI {stream} exceeded the configured limit of {limit} bytes")]
    StreamTooLarge { stream: &'static str, limit: usize },
    #[error("CLI exited with status {status}: {stderr}")]
    CliExit {
        status: i32,
        stdout: String,
        stderr: String,
    },
}

#[derive(Clone, Debug)]
pub struct ResourceLimits {
    pub request_timeout: Duration,
    pub max_frame_size: usize,
    pub max_buffer_bytes: usize,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
    pub max_output_bytes: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(30),
            max_frame_size: rawweave_external_protocol::DEFAULT_MAX_FRAME_SIZE,
            max_buffer_bytes: 256 * 1024 * 1024,
            max_stdout_bytes: 1024 * 1024,
            max_stderr_bytes: 1024 * 1024,
            max_output_bytes: 256 * 1024 * 1024,
        }
    }
}

impl ResourceLimits {
    fn validate(&self) -> Result<(), HostError> {
        if self.request_timeout.is_zero() {
            return Err(HostError::InvalidConfig(
                "request timeout must be greater than zero".into(),
            ));
        }
        if self.max_frame_size == 0
            || self.max_buffer_bytes == 0
            || self.max_stdout_bytes == 0
            || self.max_stderr_bytes == 0
            || self.max_output_bytes == 0
        {
            return Err(HostError::InvalidConfig(
                "resource limits must be greater than zero".into(),
            ));
        }
        Ok(())
    }
}

pub struct DataPlane {
    root: Arc<TempDir>,
    max_bytes: usize,
}

impl DataPlane {
    pub fn new(max_bytes: usize) -> Result<Self, HostError> {
        if max_bytes == 0 {
            return Err(HostError::InvalidConfig(
                "data-plane buffer limit must be greater than zero".into(),
            ));
        }
        Ok(Self {
            root: Arc::new(tempdir()?),
            max_bytes,
        })
    }

    pub fn root(&self) -> &Path {
        self.root.path()
    }

    pub fn create(&self, kind: DataKind, bytes: &[u8]) -> Result<ManagedBuffer, HostError> {
        if bytes.len() > self.max_bytes {
            return Err(HostError::BufferTooLarge {
                size: bytes.len(),
                limit: self.max_bytes,
            });
        }

        let id = format!("buffer-{}", NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed));
        let relative_path = PathBuf::from(format!("{id}.bin"));
        let path = self.root.path().join(&relative_path);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        if let Err(error) = file.write_all(bytes).and_then(|()| file.flush()) {
            let _ = fs::remove_file(&path);
            return Err(error.into());
        }

        let descriptor = DataBuffer::new(
            id,
            kind,
            relative_path.to_string_lossy(),
            bytes.len() as u64,
            sha256_hex(bytes),
        );
        Ok(ManagedBuffer {
            root: Arc::clone(&self.root),
            path,
            descriptor,
            cleaned: false,
        })
    }

    pub fn read(&self, descriptor: &DataBuffer) -> Result<Vec<u8>, HostError> {
        let path = self.resolve(descriptor)?;
        let metadata = fs::metadata(&path).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                HostError::BufferMissing(path.clone())
            } else {
                HostError::Io(error)
            }
        })?;
        if !metadata.is_file() {
            return Err(HostError::InvalidBufferPath(
                "buffer path is not a regular file".into(),
            ));
        }
        if metadata.len() > self.max_bytes as u64 {
            return Err(HostError::BufferTooLarge {
                size: usize::try_from(metadata.len()).unwrap_or(usize::MAX),
                limit: self.max_bytes,
            });
        }
        let file = File::open(&path)?;
        let mut bytes = Vec::new();
        let read_limit = self.max_bytes.saturating_add(1) as u64;
        file.take(read_limit).read_to_end(&mut bytes)?;
        if bytes.len() > self.max_bytes {
            return Err(HostError::BufferTooLarge {
                size: bytes.len(),
                limit: self.max_bytes,
            });
        }
        if descriptor.byte_len != bytes.len() as u64 {
            return Err(HostError::BufferSizeMismatch {
                expected: descriptor.byte_len,
                actual: bytes.len() as u64,
            });
        }
        let actual_hash = sha256_hex(&bytes);
        if descriptor.sha256 != actual_hash {
            return Err(HostError::BufferHashMismatch {
                expected: descriptor.sha256.clone(),
                actual: actual_hash,
            });
        }
        Ok(bytes)
    }

    pub fn cleanup(&self, descriptor: &DataBuffer) -> Result<(), HostError> {
        let path = self.resolve(descriptor)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn resolve(&self, descriptor: &DataBuffer) -> Result<PathBuf, HostError> {
        let relative = Path::new(&descriptor.relative_path);
        if relative.as_os_str().is_empty() || relative.is_absolute() {
            return Err(HostError::InvalidBufferPath(
                descriptor.relative_path.clone(),
            ));
        }
        if relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(HostError::InvalidBufferPath(
                descriptor.relative_path.clone(),
            ));
        }

        let root = self.root.path().canonicalize()?;
        let candidate = root.join(relative);
        let canonical = candidate.canonicalize().map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                HostError::BufferMissing(candidate.clone())
            } else {
                HostError::Io(error)
            }
        })?;
        if !canonical.starts_with(&root) {
            return Err(HostError::InvalidBufferPath(
                descriptor.relative_path.clone(),
            ));
        }
        Ok(canonical)
    }
}

pub struct ManagedBuffer {
    root: Arc<TempDir>,
    path: PathBuf,
    descriptor: DataBuffer,
    cleaned: bool,
}

impl ManagedBuffer {
    pub fn descriptor(&self) -> &DataBuffer {
        &self.descriptor
    }

    pub fn descriptor_mut(&mut self) -> &mut DataBuffer {
        &mut self.descriptor
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn cleanup(&mut self) -> Result<(), HostError> {
        if !self.cleaned {
            match fs::remove_file(&self.path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            self.cleaned = true;
        }
        Ok(())
    }

    pub fn into_descriptor(mut self) -> DataBuffer {
        let descriptor = self.descriptor.clone();
        let _ = self.cleanup();
        descriptor
    }
}

impl Drop for ManagedBuffer {
    fn drop(&mut self) {
        let _ = &self.root;
        let _ = fs::remove_file(&self.path);
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

#[derive(Clone, Debug)]
pub struct HostConfig {
    executable: PathBuf,
    args: Vec<OsString>,
    environment_allowlist: BTreeSet<OsString>,
    environment: BTreeMap<OsString, OsString>,
    limits: ResourceLimits,
    capabilities: Capabilities,
}

impl HostConfig {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            args: Vec::new(),
            environment_allowlist: BTreeSet::new(),
            environment: BTreeMap::new(),
            limits: ResourceLimits::default(),
            capabilities: Capabilities::default(),
        }
    }

    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_environment_allowlist<I>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = &'static str>,
    {
        self.environment_allowlist = names.into_iter().map(OsString::from).collect();
        self
    }

    pub fn with_environment(
        mut self,
        name: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> Self {
        self.environment.insert(name.into(), value.into());
        self
    }

    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn with_capabilities(mut self, capabilities: Capabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    fn validate(&self) -> Result<(), HostError> {
        if self.executable.as_os_str().is_empty() {
            return Err(HostError::InvalidConfig(
                "host executable must not be empty".into(),
            ));
        }
        self.limits.validate()
    }
}

struct RunningChild {
    child: Child,
    stdin: ChildStdin,
    stdout: Option<ChildStdout>,
    stderr: Arc<Mutex<Vec<u8>>>,
}

struct SupervisorState {
    child: Option<RunningChild>,
}

pub struct Supervisor {
    config: Arc<HostConfig>,
    state: Mutex<SupervisorState>,
    start_count: AtomicUsize,
    next_request_id: AtomicU64,
}

impl Supervisor {
    pub fn new(config: HostConfig) -> Result<Self, HostError> {
        config.validate()?;
        let config = Arc::new(config);
        let supervisor = Self {
            config: Arc::clone(&config),
            state: Mutex::new(SupervisorState { child: None }),
            start_count: AtomicUsize::new(0),
            next_request_id: AtomicU64::new(1),
        };
        let child = supervisor.spawn_child()?;
        supervisor
            .state
            .lock()
            .map_err(|_| HostError::StatePoisoned)?
            .child = Some(child);
        Ok(supervisor)
    }

    pub fn start_count(&self) -> usize {
        self.start_count.load(Ordering::Relaxed)
    }

    pub fn last_request_id(&self) -> Option<RequestId> {
        let next = self.next_request_id.load(Ordering::Relaxed);
        next.checked_sub(1).filter(|id| *id != 0)
    }

    pub fn request(&self, payload: RequestPayload) -> Result<ResponsePayload, HostError> {
        let response = self.request_response(payload)?;
        match response.payload {
            ResponsePayload::Error(error) => Err(HostError::Remote {
                code: error.code,
                message: error.message,
            }),
            payload => Ok(payload),
        }
    }

    pub fn request_response(&self, payload: RequestPayload) -> Result<Response, HostError> {
        let request_id = self.allocate_request_id();
        self.request_response_with_id(request_id, payload)
    }

    pub fn request_with_id(
        &self,
        request_id: RequestId,
        payload: RequestPayload,
    ) -> Result<ResponsePayload, HostError> {
        let response = self.request_response_with_id(request_id, payload)?;
        match response.payload {
            ResponsePayload::Error(error) => Err(HostError::Remote {
                code: error.code,
                message: error.message,
            }),
            payload => Ok(payload),
        }
    }

    pub fn cancel(&self, request_id: RequestId) -> Result<ResponsePayload, HostError> {
        self.request(RequestPayload::Cancel { request_id })
    }

    pub fn cancel_request(&self, request_id: RequestId) -> Result<ResponsePayload, HostError> {
        self.cancel(request_id)
    }

    fn request_response_with_id(
        &self,
        request_id: RequestId,
        payload: RequestPayload,
    ) -> Result<Response, HostError> {
        if request_id == 0 {
            return Err(HostError::InvalidConfig(
                "request IDs must be non-zero".into(),
            ));
        }
        if let RequestPayload::CapabilityQuery { required } = &payload {
            self.config
                .capabilities
                .check(required)
                .map_err(|error| match error {
                    ProtocolError::CapabilityRejected(message) => {
                        HostError::CapabilityRejected(message)
                    }
                    other => HostError::Protocol(other),
                })?;
        }

        let request = Request::with_id(request_id, payload);
        let frame = FrameCodec::encode(
            &Message::Request(request.clone()),
            self.config.limits.max_frame_size,
        )?;
        let mut state = self.state.lock().map_err(|_| HostError::StatePoisoned)?;
        let mut running = match state.child.take() {
            Some(running) => running,
            None => self.spawn_child()?,
        };

        if let Some(status) = running.child.try_wait()? {
            let stderr = stderr_snapshot(&running.stderr);
            self.stop_child(running);
            state.child = Some(self.spawn_child()?);
            return Err(HostError::Crashed {
                status: status.code(),
                stderr,
            });
        }

        if let Err(error) = running
            .stdin
            .write_all(&frame)
            .and_then(|()| running.stdin.flush())
        {
            let stderr = stderr_snapshot(&running.stderr);
            self.stop_child(running);
            state.child = Some(self.spawn_child()?);
            return Err(HostError::Io(error).into_with_context(stderr));
        }

        let stdout = running.stdout.take().ok_or(HostError::StatePoisoned)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let max_frame_size = self.config.limits.max_frame_size;
        thread::spawn(move || {
            let mut stdout = stdout;
            let result = FrameCodec::read(&mut stdout, max_frame_size);
            let _ = sender.send((stdout, result));
        });

        let received = match receiver.recv_timeout(self.config.limits.request_timeout) {
            Ok(received) => received,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.stop_child(running);
                state.child = Some(self.spawn_child()?);
                return Err(HostError::Timeout {
                    request_id: Some(request_id),
                    timeout: self.config.limits.request_timeout,
                });
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let stderr = stderr_snapshot(&running.stderr);
                let status = terminate_and_status(&mut running.child);
                state.child = Some(self.spawn_child()?);
                return Err(HostError::Crashed {
                    status: status.and_then(|value| value.code()),
                    stderr,
                });
            }
        };
        running.stdout = Some(received.0);
        match received.1 {
            Ok(Message::Response(response)) => {
                state.child = Some(running);
                if response.id != request_id {
                    return Err(HostError::RequestIdMismatch {
                        expected: request_id,
                        actual: response.id,
                    });
                }
                if response.protocol != CURRENT_PROTOCOL_VERSION {
                    return Err(HostError::Protocol(ProtocolError::VersionMismatch {
                        local: ProtocolRange::exact(CURRENT_PROTOCOL_VERSION),
                        peer: ProtocolRange::exact(response.protocol),
                    }));
                }
                Ok(response)
            }
            Ok(Message::Request(_)) => {
                self.stop_child(running);
                state.child = Some(self.spawn_child()?);
                Err(HostError::UnexpectedMessage)
            }
            Err(error) => {
                let stderr = stderr_snapshot(&running.stderr);
                let status = terminate_and_status(&mut running.child);
                state.child = Some(self.spawn_child()?);
                if matches!(error, ProtocolError::UnexpectedEof) {
                    if status.as_ref().and_then(ExitStatus::code) == Some(0) {
                        return Ok(Response {
                            id: request_id,
                            protocol: CURRENT_PROTOCOL_VERSION,
                            payload: ResponsePayload::Acknowledged,
                        });
                    }
                    Err(HostError::Crashed {
                        status: status.and_then(|value| value.code()),
                        stderr,
                    })
                } else {
                    Err(HostError::Protocol(error))
                }
            }
        }
    }

    fn allocate_request_id(&self) -> RequestId {
        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        if id == 0 {
            self.next_request_id.store(2, Ordering::Relaxed);
            1
        } else {
            id
        }
    }

    fn spawn_child(&self) -> Result<RunningChild, HostError> {
        let mut command = Command::new(&self.config.executable);
        command
            .args(&self.config.args)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for name in &self.config.environment_allowlist {
            if let Some(value) = env::var_os(name) {
                command.env(name, value);
            }
        }
        for (name, value) in &self.config.environment {
            command.env(name, value);
        }
        let mut child = command.spawn().map_err(|error| {
            HostError::Io(io::Error::new(
                error.kind(),
                format!(
                    "failed to start '{}': {error}",
                    self.config.executable.display()
                ),
            ))
        })?;
        let stdin = child.stdin.take().ok_or(HostError::StatePoisoned)?;
        let stdout = child.stdout.take().ok_or(HostError::StatePoisoned)?;
        let stderr_pipe = child.stderr.take().ok_or(HostError::StatePoisoned)?;
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let stderr_target = Arc::clone(&stderr);
        let stderr_limit = self.config.limits.max_stderr_bytes;
        thread::spawn(move || {
            if let (Ok(result), Ok(mut target)) = (
                read_limited(stderr_pipe, stderr_limit),
                stderr_target.lock(),
            ) {
                target.extend(result.bytes);
            }
        });
        self.start_count.fetch_add(1, Ordering::Relaxed);
        Ok(RunningChild {
            child,
            stdin,
            stdout: Some(stdout),
            stderr,
        })
    }

    fn stop_child(&self, mut running: RunningChild) {
        let _ = terminate_and_status(&mut running.child);
    }
}

trait ErrorContext {
    fn into_with_context(self, stderr: String) -> HostError;
}

impl ErrorContext for HostError {
    fn into_with_context(self, stderr: String) -> HostError {
        match self {
            HostError::Io(error) => HostError::Crashed {
                status: None,
                stderr: if stderr.is_empty() {
                    error.to_string()
                } else {
                    stderr
                },
            },
            error => error,
        }
    }
}

fn terminate_and_status(child: &mut Child) -> Option<ExitStatus> {
    match child.try_wait() {
        Ok(Some(status)) => Some(status),
        Ok(None) => {
            let _ = child.kill();
            child.wait().ok()
        }
        Err(_) => None,
    }
}

fn stderr_snapshot(stderr: &Arc<Mutex<Vec<u8>>>) -> String {
    stderr
        .lock()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliArgumentType {
    String,
    Integer,
    Float,
    Boolean,
    InputFile,
    OutputFile,
    Bytes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgumentSource {
    Value,
    Input,
    Output,
}

#[derive(Clone, Debug)]
pub struct CliArgument {
    pub name: String,
    pub argument_type: CliArgumentType,
    pub source: ArgumentSource,
}

impl CliArgument {
    pub fn new(
        name: impl Into<String>,
        argument_type: CliArgumentType,
        source: ArgumentSource,
    ) -> Self {
        Self {
            name: name.into(),
            argument_type,
            source,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CliValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Path(PathBuf),
    Bytes(Vec<u8>),
}

#[derive(Clone, Debug, Default)]
pub struct CliRequest {
    values: BTreeMap<String, CliValue>,
    inputs: BTreeMap<String, PathBuf>,
    outputs: BTreeSet<String>,
}

impl CliRequest {
    pub fn with_value(mut self, name: impl Into<String>, value: CliValue) -> Self {
        self.values.insert(name.into(), value);
        self
    }

    pub fn with_input(mut self, name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        self.inputs.insert(name.into(), path.into());
        self
    }

    pub fn with_output(mut self, name: impl Into<String>) -> Self {
        self.outputs.insert(name.into());
        self
    }
}

#[derive(Clone, Debug)]
pub struct CliOutput {
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct CliResult {
    pub status: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub outputs: BTreeMap<String, CliOutput>,
}

#[derive(Clone, Debug)]
pub struct CliHostConfig {
    executable: PathBuf,
    args: Vec<OsString>,
    arguments: Vec<CliArgument>,
    outputs: BTreeMap<String, usize>,
    environment_allowlist: BTreeSet<OsString>,
    environment: BTreeMap<OsString, OsString>,
    limits: ResourceLimits,
}

impl CliHostConfig {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            args: Vec::new(),
            arguments: Vec::new(),
            outputs: BTreeMap::new(),
            environment_allowlist: BTreeSet::new(),
            environment: BTreeMap::new(),
            limits: ResourceLimits::default(),
        }
    }

    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_argument(mut self, argument: CliArgument) -> Self {
        self.arguments.push(argument);
        self
    }

    pub fn with_output(mut self, name: impl Into<String>, max_bytes: usize) -> Self {
        self.outputs.insert(name.into(), max_bytes);
        self
    }

    pub fn with_environment_allowlist<I>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = &'static str>,
    {
        self.environment_allowlist = names.into_iter().map(OsString::from).collect();
        self
    }

    pub fn with_environment(
        mut self,
        name: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> Self {
        self.environment.insert(name.into(), value.into());
        self
    }

    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    fn validate(&self) -> Result<(), HostError> {
        if self.executable.as_os_str().is_empty() {
            return Err(HostError::InvalidConfig(
                "CLI executable must not be empty".into(),
            ));
        }
        self.limits.validate()?;
        let mut names = BTreeSet::new();
        for argument in &self.arguments {
            if argument.name.is_empty() || !names.insert(argument.name.clone()) {
                return Err(HostError::InvalidConfig(format!(
                    "CLI argument name '{}' is empty or duplicated",
                    argument.name
                )));
            }
            if argument.source == ArgumentSource::Output
                && argument.argument_type != CliArgumentType::OutputFile
            {
                return Err(HostError::InvalidConfig(format!(
                    "CLI output argument '{}' must use OutputFile type",
                    argument.name
                )));
            }
            if argument.source == ArgumentSource::Input
                && argument.argument_type != CliArgumentType::InputFile
            {
                return Err(HostError::InvalidConfig(format!(
                    "CLI input argument '{}' must use InputFile type",
                    argument.name
                )));
            }
        }
        if self.outputs.values().any(|limit| *limit == 0) {
            return Err(HostError::InvalidConfig(
                "CLI output limits must be greater than zero".into(),
            ));
        }
        Ok(())
    }
}

pub struct CliHost {
    config: CliHostConfig,
}

impl CliHost {
    pub fn new(config: CliHostConfig) -> Result<Self, HostError> {
        config.validate()?;
        Ok(Self { config })
    }

    pub fn execute(&self, request: CliRequest) -> Result<CliResult, HostError> {
        for name in &request.outputs {
            if !self.config.outputs.contains_key(name) {
                return Err(HostError::InvalidArgument {
                    name: name.clone(),
                    reason: "output is not declared by the CLI host".into(),
                });
            }
        }

        let workdir = tempdir()?;
        let mut output_paths = BTreeMap::new();
        for (index, name) in request.outputs.iter().enumerate() {
            let path = workdir.path().join(format!("output-{index}"));
            File::create_new(&path)?;
            output_paths.insert(name.clone(), path);
        }

        let mut command = Command::new(&self.config.executable);
        command
            .args(&self.config.args)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for name in &self.config.environment_allowlist {
            if let Some(value) = env::var_os(name) {
                command.env(name, value);
            }
        }
        for (name, value) in &self.config.environment {
            command.env(name, value);
        }

        for argument in &self.config.arguments {
            let value = match argument.source {
                ArgumentSource::Value => {
                    let value = request.values.get(&argument.name).ok_or_else(|| {
                        HostError::InvalidArgument {
                            name: argument.name.clone(),
                            reason: "value was not provided".into(),
                        }
                    })?;
                    format_value(&argument.name, argument.argument_type, value)?
                }
                ArgumentSource::Input => request
                    .inputs
                    .get(&argument.name)
                    .ok_or_else(|| HostError::MissingInput {
                        name: argument.name.clone(),
                    })?
                    .as_os_str()
                    .to_owned(),
                ArgumentSource::Output => output_paths
                    .get(&argument.name)
                    .ok_or_else(|| HostError::InvalidArgument {
                        name: argument.name.clone(),
                        reason: "output was not requested".into(),
                    })?
                    .as_os_str()
                    .to_owned(),
            };
            command.arg(value);
        }

        let mut child = command.spawn().map_err(|error| {
            HostError::Io(io::Error::new(
                error.kind(),
                format!(
                    "failed to start '{}': {error}",
                    self.config.executable.display()
                ),
            ))
        })?;
        let stdout = child.stdout.take().ok_or(HostError::StatePoisoned)?;
        let stderr = child.stderr.take().ok_or(HostError::StatePoisoned)?;
        let stdout_limit = self.config.limits.max_stdout_bytes;
        let stderr_limit = self.config.limits.max_stderr_bytes;
        let stdout_thread = thread::spawn(move || read_limited(stdout, stdout_limit));
        let stderr_thread = thread::spawn(move || read_limited(stderr, stderr_limit));

        let status = wait_with_timeout(&mut child, self.config.limits.request_timeout)?;
        let Some(status) = status else {
            return Err(HostError::Timeout {
                request_id: None,
                timeout: self.config.limits.request_timeout,
            });
        };
        let stdout = join_limited(stdout_thread)?;
        let stderr = join_limited(stderr_thread)?;
        if stdout.too_large {
            return Err(HostError::StreamTooLarge {
                stream: "stdout",
                limit: stdout_limit,
            });
        }
        if stderr.too_large {
            return Err(HostError::StreamTooLarge {
                stream: "stderr",
                limit: stderr_limit,
            });
        }

        let status_code = status.code().unwrap_or(-1);
        if !status.success() {
            return Err(HostError::CliExit {
                status: status_code,
                stdout: String::from_utf8_lossy(&stdout.bytes).into_owned(),
                stderr: String::from_utf8_lossy(&stderr.bytes).into_owned(),
            });
        }

        let mut outputs = BTreeMap::new();
        for (name, path) in output_paths {
            let configured_limit = self.config.outputs.get(&name).copied().ok_or_else(|| {
                HostError::InvalidArgument {
                    name: name.clone(),
                    reason: "output is not declared by the CLI host".into(),
                }
            })?;
            let limit = configured_limit.min(self.config.limits.max_output_bytes);
            let bytes = read_output(&path, limit, &name)?;
            outputs.insert(name, CliOutput { bytes });
        }

        Ok(CliResult {
            status: status_code,
            stdout: stdout.bytes,
            stderr: stderr.bytes,
            outputs,
        })
    }
}

fn format_value(
    name: &str,
    argument_type: CliArgumentType,
    value: &CliValue,
) -> Result<OsString, HostError> {
    let invalid = || HostError::InvalidArgument {
        name: name.to_owned(),
        reason: format!("value does not match {argument_type:?}"),
    };
    match (argument_type, value) {
        (CliArgumentType::String, CliValue::String(value)) => Ok(value.clone().into()),
        (CliArgumentType::Integer, CliValue::Integer(value)) => Ok(value.to_string().into()),
        (CliArgumentType::Float, CliValue::Float(value)) if value.is_finite() => {
            Ok(value.to_string().into())
        }
        (CliArgumentType::Boolean, CliValue::Boolean(value)) => {
            Ok(if *value { "true" } else { "false" }.to_owned().into())
        }
        (CliArgumentType::Bytes, CliValue::Bytes(value)) => String::from_utf8(value.clone())
            .map(Into::into)
            .map_err(|_| invalid()),
        _ => Err(invalid()),
    }
}

#[derive(Debug)]
struct LimitedRead {
    bytes: Vec<u8>,
    too_large: bool,
}

fn read_limited<R: Read>(mut reader: R, limit: usize) -> io::Result<LimitedRead> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut too_large = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        if bytes.len() < limit {
            let remaining = limit - bytes.len();
            let kept = count.min(remaining);
            bytes.extend_from_slice(&buffer[..kept]);
            if kept < count {
                too_large = true;
            }
        } else {
            too_large = true;
        }
    }
    Ok(LimitedRead { bytes, too_large })
}

fn join_limited(
    handle: thread::JoinHandle<io::Result<LimitedRead>>,
) -> Result<LimitedRead, HostError> {
    match handle.join() {
        Ok(result) => result.map_err(HostError::Io),
        Err(_) => Err(HostError::InvalidConfig(
            "CLI output reader thread panicked".into(),
        )),
    }
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> io::Result<Option<ExitStatus>> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(5).min(timeout.saturating_sub(started.elapsed())));
    }
}

fn read_output(path: &Path, limit: usize, name: &str) -> Result<Vec<u8>, HostError> {
    let metadata = fs::metadata(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            HostError::OutputMissing {
                name: name.to_owned(),
            }
        } else {
            HostError::Io(error)
        }
    })?;
    if !metadata.is_file() {
        return Err(HostError::OutputMissing {
            name: name.to_owned(),
        });
    }
    if metadata.len() > limit as u64 {
        return Err(HostError::OutputTooLarge {
            name: name.to_owned(),
            size: usize::try_from(metadata.len()).unwrap_or(usize::MAX),
            limit,
        });
    }
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(HostError::OutputTooLarge {
            name: name.to_owned(),
            size: bytes.len(),
            limit,
        });
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_plane_rejects_parent_paths() {
        let plane = DataPlane::new(16).expect("data plane");
        let descriptor = DataBuffer::new("bad", DataKind::Bytes, "../outside", 0, "");
        assert!(matches!(
            plane.read(&descriptor),
            Err(HostError::InvalidBufferPath(_))
        ));
    }

    #[test]
    fn cli_value_formatting_is_typed() {
        assert!(
            format_value("n", CliArgumentType::Integer, &CliValue::String("1".into())).is_err()
        );
        assert_eq!(
            format_value("n", CliArgumentType::Integer, &CliValue::Integer(1))
                .expect("integer")
                .to_string_lossy(),
            "1"
        );
    }
}
