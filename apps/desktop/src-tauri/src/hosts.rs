use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use rawweave_external_host::{HostConfig, HostError};
use rawweave_external_protocol::{
    Capabilities, PixelFormat, RequestPayload, ResponsePayload, CURRENT_PROTOCOL_VERSION,
};
use rawweave_project::{EditorCore, ExternalHost, ExternalNodePack};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const HOST_CONFIG_VERSION: u32 = 1;
pub const MAX_HOSTS: usize = 64;
pub const MAX_HOST_ID_BYTES: usize = 128;
pub const MAX_EXECUTABLE_BYTES: usize = 4096;
pub const MAX_ARGS: usize = 64;
pub const MAX_ARG_BYTES: usize = 4096;
pub const MAX_ENV_ALLOWLIST: usize = 32;
pub const MAX_ENV_NAME_BYTES: usize = 128;
pub const MAX_ENV_VALUE_BYTES: usize = 4096;
pub const MAX_HOST_CONFIG_BYTES: usize = 1024 * 1024;
pub const MAX_ERROR_BYTES: usize = 4096;

#[derive(Debug, Error)]
pub enum HostManagerError {
    #[error("host configuration is invalid: {0}")]
    InvalidConfig(String),
    #[error("host '{0}' is not configured")]
    MissingHost(String),
    #[error("host storage error: {0}")]
    Storage(String),
    #[error(transparent)]
    Host(#[from] HostError),
    #[error(transparent)]
    Project(#[from] rawweave_project::ProjectError),
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExternalHostConfigDto {
    pub id: String,
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env_allowlist: Vec<String>,
    /// Explicit values are optional and are only used when the key is also in
    /// `env_allowlist`. They are filtered before storage and never returned.
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HostNodeDto {
    pub type_id: String,
    pub name: String,
    pub version: u32,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HostCapabilitiesDto {
    pub pixel_formats: Vec<String>,
    pub roi: bool,
    pub full_frame: bool,
    pub multi_input: bool,
    pub multi_output: bool,
    pub thread_safety: String,
    pub gpu: bool,
    pub custom_ui: bool,
    pub deterministic: bool,
    pub data_plane: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HostProtocolVersionDto {
    pub major: u16,
    pub minor: u16,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExternalHostDto {
    pub id: String,
    pub executable: String,
    pub args: Vec<String>,
    pub env_allowlist: Vec<String>,
    pub status: String,
    pub protocol_version: Option<HostProtocolVersionDto>,
    pub capabilities: Option<HostCapabilitiesDto>,
    pub nodes: Vec<HostNodeDto>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExternalHostDiagnosticsDto {
    pub host_id: String,
    pub executable: String,
    pub status: String,
    pub protocol_version: Option<HostProtocolVersionDto>,
    pub capabilities: Option<HostCapabilitiesDto>,
    pub node_count: usize,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredHosts {
    version: u32,
    hosts: Vec<ExternalHostConfigDto>,
}

#[derive(Clone, Debug)]
pub struct HostStore {
    path: Option<PathBuf>,
}

impl HostStore {
    pub fn memory() -> Self {
        Self { path: None }
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
        }
    }

    pub fn default_path() -> PathBuf {
        if let Some(path) = env::var_os("RAWWEAVE_HOSTS_CONFIG") {
            return PathBuf::from(path);
        }
        let root = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        root.join("rawweave").join("external-hosts.json")
    }

    fn load(&self) -> Result<Vec<ExternalHostConfigDto>, HostManagerError> {
        let Some(path) = &self.path else {
            return Ok(Vec::new());
        };
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(storage_error(path, error)),
        };
        let bytes = read_bounded(file, MAX_HOST_CONFIG_BYTES)
            .map_err(|error| storage_error(path, error))?;
        let stored: StoredHosts = serde_json::from_slice(&bytes)
            .map_err(|error| HostManagerError::Storage(format!("{}: {error}", path.display())))?;
        if stored.version != HOST_CONFIG_VERSION {
            return Err(HostManagerError::Storage(format!(
                "{}: unsupported host configuration version {}",
                path.display(),
                stored.version
            )));
        }
        let mut configs = BTreeMap::new();
        for config in stored.hosts {
            let config = sanitize_config(config)?;
            validate_config(&config)?;
            if configs.insert(config.id.clone(), config).is_some() {
                return Err(HostManagerError::Storage(format!(
                    "{}: duplicate host id",
                    path.display()
                )));
            }
            if configs.len() > MAX_HOSTS {
                return Err(HostManagerError::Storage(format!(
                    "{}: too many hosts",
                    path.display()
                )));
            }
        }
        Ok(configs.into_values().collect())
    }

    fn save(&self, configs: &[ExternalHostConfigDto]) -> Result<(), HostManagerError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let stored = StoredHosts {
            version: HOST_CONFIG_VERSION,
            hosts: configs
                .iter()
                .cloned()
                .map(sanitize_config)
                .collect::<Result<Vec<_>, _>>()?,
        };
        let bytes = serde_json::to_vec_pretty(&stored)
            .map_err(|error| HostManagerError::Storage(error.to_string()))?;
        if bytes.len() > MAX_HOST_CONFIG_BYTES {
            return Err(HostManagerError::Storage(format!(
                "{}: serialized configuration exceeds {} bytes",
                path.display(),
                MAX_HOST_CONFIG_BYTES
            )));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| storage_error(path, error))?;
        }
        let temporary = path.with_extension("json.tmp");
        let result = (|| {
            let mut file = File::create(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, path)
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(storage_error(path, error));
        }
        Ok(())
    }
}

fn storage_error(path: &Path, error: io::Error) -> HostManagerError {
    HostManagerError::Storage(format!("{}: {error}", path.display()))
}

fn read_bounded<R: Read>(reader: R, limit: usize) -> io::Result<Vec<u8>> {
    let mut reader = reader.take(limit.saturating_add(1) as u64);
    let mut bytes = Vec::with_capacity(limit.min(8192));
    reader.read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("configuration exceeds {limit} bytes"),
        ));
    }
    Ok(bytes)
}

pub fn is_secret_like_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "secret",
        "password",
        "passwd",
        "token",
        "api_key",
        "apikey",
        "auth",
        "credential",
        "private_key",
        "access_key",
        "refresh",
        "cookie",
        "session",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

pub fn is_secret_like_value(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "secret",
        "password",
        "passwd",
        "token=",
        "api_key",
        "apikey",
        "bearer ",
        "private key",
        "-----begin",
    ]
    .iter()
    .any(|needle| value.contains(needle))
}

pub fn redact_text(text: &str, values: impl IntoIterator<Item = String>) -> String {
    let mut redacted = text.to_owned();
    for value in values {
        if value.len() >= 3 {
            redacted = redacted.replace(&value, "[REDACTED]");
        }
    }
    truncate_text(&redacted, MAX_ERROR_BYTES)
}

fn truncate_text(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

pub fn sanitize_config(
    mut config: ExternalHostConfigDto,
) -> Result<ExternalHostConfigDto, HostManagerError> {
    config.env_allowlist.retain(|key| !is_secret_like_key(key));
    config.environment.retain(|key, value| {
        config.env_allowlist.iter().any(|allowed| allowed == key)
            && !is_secret_like_key(key)
            && !is_secret_like_value(value)
    });
    validate_config(&config)?;
    Ok(config)
}

pub fn validate_config(config: &ExternalHostConfigDto) -> Result<(), HostManagerError> {
    if config.id.is_empty() || config.id.len() > MAX_HOST_ID_BYTES {
        return Err(HostManagerError::InvalidConfig(format!(
            "host id must be 1..={} bytes",
            MAX_HOST_ID_BYTES
        )));
    }
    if config.executable.is_empty() || config.executable.len() > MAX_EXECUTABLE_BYTES {
        return Err(HostManagerError::InvalidConfig(format!(
            "executable must be 1..={} bytes",
            MAX_EXECUTABLE_BYTES
        )));
    }
    if config.args.len() > MAX_ARGS {
        return Err(HostManagerError::InvalidConfig(format!(
            "at most {MAX_ARGS} arguments are allowed"
        )));
    }
    if config.args.iter().any(|arg| arg.len() > MAX_ARG_BYTES) {
        return Err(HostManagerError::InvalidConfig(format!(
            "arguments must be at most {MAX_ARG_BYTES} bytes"
        )));
    }
    if config.env_allowlist.len() > MAX_ENV_ALLOWLIST {
        return Err(HostManagerError::InvalidConfig(format!(
            "at most {MAX_ENV_ALLOWLIST} environment names are allowed"
        )));
    }
    let mut names = BTreeSet::new();
    for name in &config.env_allowlist {
        if name.is_empty() || name.len() > MAX_ENV_NAME_BYTES {
            return Err(HostManagerError::InvalidConfig(format!(
                "environment names must be 1..={} bytes",
                MAX_ENV_NAME_BYTES
            )));
        }
        if !name.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphanumeric() && (index > 0 || !byte.is_ascii_digit())
        }) {
            return Err(HostManagerError::InvalidConfig(format!(
                "environment name '{name}' is invalid"
            )));
        }
        if !names.insert(name) {
            return Err(HostManagerError::InvalidConfig(format!(
                "environment name '{name}' is duplicated"
            )));
        }
    }
    if config
        .environment
        .keys()
        .any(|key| !names.contains(key) || key.len() > MAX_ENV_NAME_BYTES)
    {
        return Err(HostManagerError::InvalidConfig(
            "explicit environment values must use the allowlist".into(),
        ));
    }
    if config
        .environment
        .values()
        .any(|value| value.len() > MAX_ENV_VALUE_BYTES)
    {
        return Err(HostManagerError::InvalidConfig(format!(
            "environment values must be at most {MAX_ENV_VALUE_BYTES} bytes"
        )));
    }
    Ok(())
}

fn host_config(config: &ExternalHostConfigDto) -> HostConfig {
    let mut host = HostConfig::new(PathBuf::from(&config.executable))
        .with_args(config.args.clone())
        .with_environment_allowlist(config.env_allowlist.clone());
    for (key, value) in &config.environment {
        host = host.with_environment(key.clone(), value.clone());
    }
    host
}

fn protocol_version() -> HostProtocolVersionDto {
    HostProtocolVersionDto {
        major: CURRENT_PROTOCOL_VERSION.major,
        minor: CURRENT_PROTOCOL_VERSION.minor,
    }
}

fn capabilities_dto(capabilities: &Capabilities) -> HostCapabilitiesDto {
    HostCapabilitiesDto {
        pixel_formats: capabilities
            .pixel_formats
            .iter()
            .map(pixel_format_name)
            .collect(),
        roi: capabilities.roi,
        full_frame: capabilities.full_frame,
        multi_input: capabilities.multi_input,
        multi_output: capabilities.multi_output,
        thread_safety: format!("{:?}", capabilities.thread_safety),
        gpu: capabilities.gpu,
        custom_ui: capabilities.custom_ui,
        deterministic: capabilities.deterministic,
        data_plane: capabilities.data_plane,
    }
}

fn pixel_format_name(format: &PixelFormat) -> String {
    format!("{format:?}")
}

struct RuntimeHost {
    host: ExternalHost,
    pack: Option<ExternalNodePack>,
    status: String,
    protocol_version: Option<HostProtocolVersionDto>,
    capabilities: Option<HostCapabilitiesDto>,
    nodes: Vec<HostNodeDto>,
    error: Option<String>,
}

pub struct HostManager {
    store: HostStore,
    configs: BTreeMap<String, ExternalHostConfigDto>,
    runtime: BTreeMap<String, RuntimeHost>,
}

impl Default for HostManager {
    fn default() -> Self {
        Self::memory()
    }
}

impl HostManager {
    pub fn memory() -> Self {
        Self {
            store: HostStore::memory(),
            configs: BTreeMap::new(),
            runtime: BTreeMap::new(),
        }
    }

    pub fn load_default() -> Result<Self, HostManagerError> {
        Self::with_store(HostStore::at(HostStore::default_path()))
    }

    pub fn with_store(store: HostStore) -> Result<Self, HostManagerError> {
        let configs = store
            .load()?
            .into_iter()
            .map(|config| (config.id.clone(), config))
            .collect();
        Ok(Self {
            store,
            configs,
            runtime: BTreeMap::new(),
        })
    }

    pub fn add(
        &mut self,
        config: ExternalHostConfigDto,
    ) -> Result<ExternalHostDto, HostManagerError> {
        let config = sanitize_config(config)?;
        let id = config.id.clone();
        let mut next = self.configs.clone();
        next.insert(id.clone(), config);
        if next.len() > MAX_HOSTS {
            return Err(HostManagerError::InvalidConfig(format!(
                "at most {MAX_HOSTS} hosts are allowed"
            )));
        }
        self.persist(&next)?;
        self.configs = next;
        self.runtime.remove(&id);
        Ok(self.summary(&id))
    }

    pub fn remove(&mut self, id: &str) -> Result<(), HostManagerError> {
        if !self.configs.contains_key(id) {
            return Err(HostManagerError::MissingHost(id.to_owned()));
        }
        let mut next = self.configs.clone();
        next.remove(id);
        self.persist(&next)?;
        self.configs = next;
        self.runtime.remove(id);
        Ok(())
    }

    pub fn list(&self) -> Vec<ExternalHostDto> {
        self.configs.keys().map(|id| self.summary(id)).collect()
    }

    pub fn test(&mut self, id: &str) -> Result<ExternalHostDto, HostManagerError> {
        let response = {
            let runtime = self.ensure_runtime(id)?;
            runtime.host.request(RequestPayload::Discover)
        };
        match response {
            Ok(ResponsePayload::Discovered {
                descriptors,
                capabilities,
            }) => {
                let nodes = descriptors
                    .into_iter()
                    .map(|descriptor| HostNodeDto {
                        type_id: descriptor.type_id,
                        name: descriptor.name,
                        version: descriptor.version,
                    })
                    .collect();
                self.update_discovered(id, nodes, capabilities, "ready");
                Ok(self.summary(id))
            }
            Ok(_) => self.set_error(id, "external host returned an unexpected discover response"),
            Err(error) => self.set_error(id, &error.to_string()),
        }
    }

    pub fn discover(
        &mut self,
        id: &str,
        editor: &mut EditorCore,
    ) -> Result<ExternalHostDto, HostManagerError> {
        let host = self.ensure_runtime(id)?.host.clone();
        let pack = match ExternalNodePack::discover(host) {
            Ok(pack) => pack,
            Err(error) => return self.set_error(id, &error.to_string()),
        };
        let should_register = self
            .runtime
            .get(id)
            .and_then(|runtime| runtime.pack.as_ref())
            .is_none();
        if should_register {
            if let Err(error) = editor.register_external_node_pack(pack.clone()) {
                return self.set_error(id, &error.to_string());
            }
        }
        let nodes = pack
            .descriptors()
            .into_iter()
            .map(|descriptor| HostNodeDto {
                type_id: descriptor.type_id,
                name: descriptor.name,
                version: descriptor.version,
            })
            .collect();
        self.update_discovered(id, nodes, pack.capabilities().clone(), "ready");
        if let Some(runtime) = self.runtime.get_mut(id) {
            runtime.pack = Some(pack);
        }
        Ok(self.summary(id))
    }

    pub fn discover_all(
        &mut self,
        editor: &mut EditorCore,
    ) -> Result<Vec<ExternalHostDto>, HostManagerError> {
        let ids = self.configs.keys().cloned().collect::<Vec<_>>();
        let mut result = Vec::with_capacity(ids.len());
        for id in ids {
            let _ = self.discover(&id, editor);
            result.push(self.summary(&id));
        }
        Ok(result)
    }

    pub fn diagnostics(
        &mut self,
        id: &str,
    ) -> Result<ExternalHostDiagnosticsDto, HostManagerError> {
        if !self.configs.contains_key(id) {
            return Err(HostManagerError::MissingHost(id.to_owned()));
        }
        let summary = self.summary(id);
        Ok(ExternalHostDiagnosticsDto {
            host_id: summary.id,
            executable: summary.executable,
            status: summary.status,
            protocol_version: summary.protocol_version,
            capabilities: summary.capabilities,
            node_count: summary.nodes.len(),
            error: summary.error,
        })
    }

    fn ensure_runtime(&mut self, id: &str) -> Result<&mut RuntimeHost, HostManagerError> {
        let config = self
            .configs
            .get(id)
            .ok_or_else(|| HostManagerError::MissingHost(id.to_owned()))?
            .clone();
        if !self.runtime.contains_key(id) {
            let host = ExternalHost::connect(id.to_owned(), host_config(&config))?;
            self.runtime.insert(
                id.to_owned(),
                RuntimeHost {
                    host,
                    pack: None,
                    status: "configured".into(),
                    protocol_version: None,
                    capabilities: None,
                    nodes: Vec::new(),
                    error: None,
                },
            );
        }
        self.runtime
            .get_mut(id)
            .ok_or_else(|| HostManagerError::MissingHost(id.to_owned()))
    }

    fn update_discovered(
        &mut self,
        id: &str,
        nodes: Vec<HostNodeDto>,
        capabilities: Capabilities,
        status: &str,
    ) {
        if let Some(runtime) = self.runtime.get_mut(id) {
            runtime.status = status.to_owned();
            runtime.protocol_version = Some(protocol_version());
            runtime.capabilities = Some(capabilities_dto(&capabilities));
            runtime.nodes = nodes;
            runtime.error = None;
        }
    }

    fn set_error(&mut self, id: &str, error: &str) -> Result<ExternalHostDto, HostManagerError> {
        let values = self
            .configs
            .get(id)
            .into_iter()
            .flat_map(|config| config.environment.values().cloned())
            .collect::<Vec<_>>();
        let error = redact_text(error, values);
        if let Some(runtime) = self.runtime.get_mut(id) {
            runtime.status = "error".into();
            runtime.error = Some(error);
        }
        Ok(self.summary(id))
    }

    fn summary(&self, id: &str) -> ExternalHostDto {
        let config = self.configs.get(id);
        let runtime = self.runtime.get(id);
        ExternalHostDto {
            id: id.to_owned(),
            executable: config
                .map(|config| config.executable.clone())
                .unwrap_or_default(),
            args: config.map(|config| config.args.clone()).unwrap_or_default(),
            env_allowlist: config
                .map(|config| config.env_allowlist.clone())
                .unwrap_or_default(),
            status: runtime
                .map(|runtime| runtime.status.clone())
                .unwrap_or_else(|| "configured".into()),
            protocol_version: runtime.and_then(|runtime| runtime.protocol_version.clone()),
            capabilities: runtime.and_then(|runtime| runtime.capabilities.clone()),
            nodes: runtime
                .map(|runtime| runtime.nodes.clone())
                .unwrap_or_default(),
            error: runtime.and_then(|runtime| runtime.error.clone()),
        }
    }

    fn persist(
        &self,
        configs: &BTreeMap<String, ExternalHostConfigDto>,
    ) -> Result<(), HostManagerError> {
        self.store
            .save(&configs.values().cloned().collect::<Vec<_>>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::tempdir;

    fn config() -> ExternalHostConfigDto {
        ExternalHostConfigDto {
            id: "fixture".into(),
            executable: "/bin/sh".into(),
            args: vec!["-c".into(), "exit 0".into()],
            env_allowlist: vec!["PATH".into(), "RAWWEAVE_TOKEN".into()],
            environment: [
                ("PATH".into(), "/usr/bin".into()),
                ("RAWWEAVE_TOKEN".into(), "secret-token".into()),
            ]
            .into_iter()
            .collect(),
        }
    }

    #[test]
    fn bounds_reject_unbounded_host_configurations() {
        let mut value = config();
        value.args = vec!["x".into(); MAX_ARGS + 1];
        assert!(validate_config(&value).is_err());
        value.args = vec!["x".repeat(MAX_ARG_BYTES + 1)];
        assert!(validate_config(&value).is_err());
    }

    #[test]
    fn missing_hosts_are_reported_without_launching_a_process() {
        let mut manager = HostManager::memory();
        assert!(matches!(
            manager.test("missing"),
            Err(HostManagerError::MissingHost(id)) if id == "missing"
        ));
    }

    #[test]
    fn secret_environment_keys_and_values_are_dropped_from_storage_and_responses() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("hosts.json");
        let mut manager = HostManager::with_store(HostStore::at(&path)).unwrap();
        let summary = manager.add(config()).unwrap();
        assert_eq!(summary.env_allowlist, vec!["PATH"]);
        let bytes = fs::read(&path).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("RAWWEAVE_TOKEN"));
        assert!(!text.contains("secret-token"));
    }

    #[test]
    fn persisted_host_configurations_are_loaded_with_the_same_sanitized_shape() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("hosts.json");
        let config = config();
        let mut writer = HostManager::with_store(HostStore::at(&path)).unwrap();
        writer.add(config).unwrap();

        let reader = HostManager::with_store(HostStore::at(&path)).unwrap();
        let hosts = reader.list();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].id, "fixture");
        assert_eq!(hosts[0].executable, "/bin/sh");
        assert_eq!(hosts[0].args, vec!["-c", "exit 0"]);
        assert_eq!(hosts[0].env_allowlist, vec!["PATH"]);
    }

    #[test]
    fn discovery_registers_external_nodes_in_the_editor_library() {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../scripts/fixture_external_host.py");
        let mut manager = HostManager::memory();
        manager
            .add(ExternalHostConfigDto {
                id: "fixture".into(),
                executable: "python3".into(),
                args: vec![script.to_string_lossy().into_owned()],
                env_allowlist: Vec::new(),
                environment: BTreeMap::new(),
            })
            .unwrap();
        let mut editor = EditorCore::default();

        let host = manager.discover("fixture", &mut editor).unwrap();

        assert_eq!(host.status, "ready");
        assert_eq!(host.nodes.len(), 2);
        assert!(host
            .nodes
            .iter()
            .any(|node| node.type_id == "external.fixture.fixture.image-pass"));
        assert!(editor
            .node_descriptors()
            .iter()
            .any(|descriptor| descriptor.type_id == "external.fixture.fixture.image-pass"));
    }

    #[test]
    fn bounded_storage_rejects_oversized_files() {
        let oversized = vec![b'x'; MAX_HOST_CONFIG_BYTES + 1];
        assert!(read_bounded(Cursor::new(oversized), MAX_HOST_CONFIG_BYTES).is_err());
    }

    #[test]
    fn redaction_removes_explicit_environment_values_from_diagnostics() {
        assert_eq!(
            redact_text("host failed: safe-secret", ["safe-secret".into()]),
            "host failed: [REDACTED]"
        );
    }
}
