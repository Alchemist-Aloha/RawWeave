use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rawweave_ai_provider::{
    poll_until_complete, AiCapabilities, AiOperation, AiProvider, AiResult, AiTask,
    CancellationToken, ComfyUiConfig, ComfyUiProvider, HttpProvider, HttpProviderManifest,
    HttpRequest, HttpResponse, HttpTransport, PollPolicy, ProviderError, SubmitRequest,
    SubmitResponse, TaskState, TaskStatus, ThreadSleeper,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

const AI_PROVIDER_CONFIG_VERSION: u32 = 1;
const MAX_AI_PROVIDERS: usize = 64;
const MAX_AI_PROVIDER_CONFIG_BYTES: usize = 2 * 1024 * 1024;
const MAX_AI_PROVIDER_ID_BYTES: usize = 128;
const MAX_AI_PROVIDER_NAME_BYTES: usize = 256;
const MAX_AI_PROVIDER_URL_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderKind {
    ComfyUi,
    Http,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderStatus {
    #[default]
    Configured,
    Ready,
    Error,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderConfigDto {
    pub id: String,
    pub name: String,
    pub kind: AiProviderKind,
    pub base_url: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub manifest: Option<Value>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderColorInterchangeDto {
    pub format: String,
    pub color_space: String,
    pub alpha_mode: String,
    pub lossless: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderCapabilitiesDto {
    pub operations: Vec<AiOperation>,
    pub supports_cancel: bool,
    pub supports_progress: bool,
    pub max_image_bytes: usize,
    pub color_interchange: AiProviderColorInterchangeDto,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderDto {
    pub id: String,
    pub name: String,
    pub kind: AiProviderKind,
    pub base_url: String,
    pub client_id: Option<String>,
    pub status: AiProviderStatus,
    pub capabilities: Option<AiProviderCapabilitiesDto>,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
struct ProviderRuntime {
    status: AiProviderStatus,
    capabilities: Option<AiProviderCapabilitiesDto>,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct AiProviderStore {
    path: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredAiProviders {
    version: u32,
    providers: Vec<AiProviderConfigDto>,
}

impl AiProviderStore {
    fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
        }
    }

    fn load(&self) -> Result<Vec<AiProviderConfigDto>, String> {
        let Some(path) = &self.path else {
            return Ok(Vec::new());
        };
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(storage_error(path, error)),
        };
        let bytes = read_bounded(file, MAX_AI_PROVIDER_CONFIG_BYTES)
            .map_err(|error| storage_error(path, error))?;
        let stored: StoredAiProviders = serde_json::from_slice(&bytes).map_err(|error| {
            format!(
                "{}: invalid AI provider configuration: {error}",
                path.display()
            )
        })?;
        if stored.version != AI_PROVIDER_CONFIG_VERSION {
            return Err(format!(
                "{}: unsupported AI provider configuration version {}",
                path.display(),
                stored.version
            ));
        }
        let mut configs = BTreeMap::new();
        for config in stored.providers {
            let config = sanitize_config(config)?;
            validate_config(&config)?;
            if configs.insert(config.id.clone(), config).is_some() {
                return Err(format!("{}: duplicate AI provider id", path.display()));
            }
            if configs.len() > MAX_AI_PROVIDERS {
                return Err(format!(
                    "{}: at most {MAX_AI_PROVIDERS} AI providers are supported",
                    path.display()
                ));
            }
        }
        Ok(configs.into_values().collect())
    }

    fn save(&self, configs: &[AiProviderConfigDto]) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let stored = StoredAiProviders {
            version: AI_PROVIDER_CONFIG_VERSION,
            providers: configs
                .iter()
                .cloned()
                .map(sanitize_config)
                .collect::<Result<Vec<_>, _>>()?,
        };
        let bytes = serde_json::to_vec_pretty(&stored)
            .map_err(|error| format!("could not serialize AI provider configuration: {error}"))?;
        if bytes.len() > MAX_AI_PROVIDER_CONFIG_BYTES {
            return Err(format!(
                "{}: serialized configuration exceeds {MAX_AI_PROVIDER_CONFIG_BYTES} bytes",
                path.display()
            ));
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

fn storage_error(path: &Path, error: io::Error) -> String {
    format!("{}: {error}", path.display())
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

fn sanitize_config(mut config: AiProviderConfigDto) -> Result<AiProviderConfigDto, String> {
    config.id = config.id.trim().to_owned();
    config.name = config.name.trim().to_owned();
    config.base_url = config.base_url.trim().to_owned();
    config.client_id = config
        .client_id
        .map(|client_id| client_id.trim().to_owned())
        .filter(|client_id| !client_id.is_empty());
    validate_config(&config)?;
    Ok(config)
}

fn validate_text(value: &str, field: &str, max_bytes: usize) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("AI provider {field} cannot be empty"));
    }
    if value.len() > max_bytes {
        return Err(format!("AI provider {field} exceeds {max_bytes} bytes"));
    }
    if value.contains('\n') || value.contains('\r') {
        return Err(format!(
            "AI provider {field} cannot contain control characters"
        ));
    }
    Ok(())
}

fn validate_config(config: &AiProviderConfigDto) -> Result<(), String> {
    validate_text(&config.id, "id", MAX_AI_PROVIDER_ID_BYTES)?;
    validate_text(&config.name, "name", MAX_AI_PROVIDER_NAME_BYTES)?;
    validate_text(&config.base_url, "base URL", MAX_AI_PROVIDER_URL_BYTES)?;
    match config.kind {
        AiProviderKind::ComfyUi => {
            let provider = ComfyUiProvider::new(
                ComfyUiConfig::new(config.base_url.clone()).with_client_id(
                    config
                        .client_id
                        .clone()
                        .unwrap_or_else(|| "rawweave".to_owned()),
                ),
                Arc::new(ValidationTransport),
            )
            .map_err(|error| error.to_string())?;
            let _ = provider.capabilities();
        }
        AiProviderKind::Http => {
            let manifest = config
                .manifest
                .clone()
                .ok_or_else(|| "HTTP AI providers require a manifest".to_owned())?;
            let manifest: HttpProviderManifest = serde_json::from_value(manifest)
                .map_err(|error| format!("invalid HTTP AI provider manifest: {error}"))?;
            let provider =
                HttpProvider::new(manifest, Arc::new(ValidationTransport), |_reference| {
                    Ok::<Option<String>, ProviderError>(None)
                })
                .map_err(|error| error.to_string())?;
            let _ = provider.capabilities();
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct ValidationTransport;

impl HttpTransport for ValidationTransport {
    fn execute(&self, _request: HttpRequest) -> Result<HttpResponse, ProviderError> {
        Err(ProviderError::Transport(
            "provider configuration validation must not perform network requests".to_owned(),
        ))
    }
}

struct ReqwestTransport {
    client: reqwest::blocking::Client,
}

impl ReqwestTransport {
    fn new() -> Result<Self, ProviderError> {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|error| {
                ProviderError::Transport(format!("could not create HTTP client: {error}"))
            })?;
        Ok(Self { client })
    }
}

impl HttpTransport for ReqwestTransport {
    fn execute(&self, request: HttpRequest) -> Result<HttpResponse, ProviderError> {
        let method = match request.method {
            rawweave_ai_provider::HttpMethod::Get => reqwest::Method::GET,
            rawweave_ai_provider::HttpMethod::Post => reqwest::Method::POST,
            rawweave_ai_provider::HttpMethod::Put => reqwest::Method::PUT,
            rawweave_ai_provider::HttpMethod::Delete => reqwest::Method::DELETE,
        };
        let mut builder = self.client.request(method, &request.url);
        for (name, value) in request.headers {
            builder = builder.header(name, value);
        }
        let response = builder
            .body(request.body)
            .send()
            .map_err(|error| ProviderError::Transport(error.to_string()))?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                Some((name.as_str().to_owned(), value.to_str().ok()?.to_owned()))
            })
            .collect();
        let body = response
            .bytes()
            .map_err(|error| ProviderError::Transport(error.to_string()))?
            .to_vec();
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

struct ActiveProviderTask {
    provider: Arc<dyn AiProvider>,
    token: CancellationToken,
    task: AiTask,
}

fn capabilities_dto(capabilities: AiCapabilities) -> AiProviderCapabilitiesDto {
    let color = capabilities.color_interchange;
    AiProviderCapabilitiesDto {
        operations: capabilities.operations.into_iter().collect(),
        supports_cancel: capabilities.supports_cancel,
        supports_progress: capabilities.supports_progress,
        max_image_bytes: capabilities.max_image_bytes,
        color_interchange: AiProviderColorInterchangeDto {
            format: format!("{:?}", color.format),
            color_space: format!("{:?}", color.color_space),
            alpha_mode: format!("{:?}", color.alpha_mode),
            lossless: color.lossless,
        },
    }
}

#[derive(Clone)]
pub struct AiProviderManager {
    configs: Arc<Mutex<BTreeMap<String, AiProviderConfigDto>>>,
    runtime: Arc<Mutex<BTreeMap<String, ProviderRuntime>>>,
    providers: Arc<Mutex<BTreeMap<String, Arc<dyn AiProvider>>>>,
    tasks: Arc<Mutex<BTreeMap<String, ActiveProviderTask>>>,
    store: Arc<AiProviderStore>,
}

impl AiProviderManager {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn memory() -> Self {
        Self {
            configs: Arc::new(Mutex::new(BTreeMap::new())),
            runtime: Arc::new(Mutex::new(BTreeMap::new())),
            providers: Arc::new(Mutex::new(BTreeMap::new())),
            tasks: Arc::new(Mutex::new(BTreeMap::new())),
            store: Arc::new(AiProviderStore { path: None }),
        }
    }

    pub fn persistent(path: impl Into<PathBuf>) -> Result<Self, String> {
        let store = AiProviderStore::at(path);
        let configs = store
            .load()?
            .into_iter()
            .map(|config| (config.id.clone(), config))
            .collect();
        Ok(Self {
            configs: Arc::new(Mutex::new(configs)),
            runtime: Arc::new(Mutex::new(BTreeMap::new())),
            providers: Arc::new(Mutex::new(BTreeMap::new())),
            tasks: Arc::new(Mutex::new(BTreeMap::new())),
            store: Arc::new(store),
        })
    }

    pub fn add(&self, config: AiProviderConfigDto) -> Result<AiProviderDto, String> {
        let config = sanitize_config(config)?;
        let mut configs = self
            .configs
            .lock()
            .map_err(|_| "AI provider configuration state is unavailable".to_owned())?;
        if configs.contains_key(&config.id) {
            return Err(format!("AI provider '{}' already exists", config.id));
        }
        if configs.len() >= MAX_AI_PROVIDERS {
            return Err(format!(
                "at most {MAX_AI_PROVIDERS} AI providers are supported"
            ));
        }
        let mut next = configs.clone();
        next.insert(config.id.clone(), config.clone());
        self.persist(&next)?;
        *configs = next;
        self.runtime
            .lock()
            .map_err(|_| "AI provider runtime state is unavailable".to_owned())?
            .remove(&config.id);
        Ok(self.summary_from(&config))
    }

    pub fn remove(&self, provider_id: &str) -> Result<(), String> {
        let mut configs = self
            .configs
            .lock()
            .map_err(|_| "AI provider configuration state is unavailable".to_owned())?;
        if !configs.contains_key(provider_id) {
            return Err(format!("AI provider '{provider_id}' does not exist"));
        }
        let mut next = configs.clone();
        next.remove(provider_id);
        self.persist(&next)?;
        *configs = next;
        self.runtime
            .lock()
            .map_err(|_| "AI provider runtime state is unavailable".to_owned())?
            .remove(provider_id);
        self.providers
            .lock()
            .map_err(|_| "AI provider runtime state is unavailable".to_owned())?
            .remove(provider_id);
        self.tasks
            .lock()
            .map_err(|_| "AI provider task state is unavailable".to_owned())?
            .retain(|_, task| task.task.provider_id != provider_id);
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<AiProviderDto>, String> {
        let configs = self
            .configs
            .lock()
            .map_err(|_| "AI provider configuration state is unavailable".to_owned())?;
        configs
            .values()
            .map(|config| Ok(self.summary_from(config)))
            .collect()
    }

    pub fn test(&self, provider_id: &str) -> Result<AiProviderDto, String> {
        let config = self
            .configs
            .lock()
            .map_err(|_| "AI provider configuration state is unavailable".to_owned())?
            .get(provider_id)
            .cloned()
            .ok_or_else(|| format!("AI provider '{provider_id}' does not exist"))?;
        let runtime = match validate_config(&config) {
            Ok(()) => ProviderRuntime {
                status: AiProviderStatus::Ready,
                capabilities: Some(self.capabilities_for(&config)?),
                error: None,
            },
            Err(error) => ProviderRuntime {
                status: AiProviderStatus::Error,
                capabilities: None,
                error: Some(error),
            },
        };
        let mut runtimes = self
            .runtime
            .lock()
            .map_err(|_| "AI provider runtime state is unavailable".to_owned())?;
        runtimes.insert(provider_id.to_owned(), runtime);
        drop(runtimes);
        Ok(self.summary_from(&config))
    }

    fn provider_from_config(
        &self,
        config: &AiProviderConfigDto,
    ) -> Result<Arc<dyn AiProvider>, String> {
        let transport: Arc<dyn HttpTransport> =
            Arc::new(ReqwestTransport::new().map_err(|error| error.to_string())?);
        match config.kind {
            AiProviderKind::ComfyUi => {
                let comfy = ComfyUiProvider::new(
                    ComfyUiConfig::new(config.base_url.clone())
                        .with_provider_id(config.id.clone())
                        .with_client_id(
                            config
                                .client_id
                                .clone()
                                .unwrap_or_else(|| "rawweave".to_owned()),
                        ),
                    transport,
                )
                .map_err(|error| error.to_string())?;
                Ok(Arc::new(comfy))
            }
            AiProviderKind::Http => {
                let manifest = config
                    .manifest
                    .clone()
                    .ok_or_else(|| "HTTP AI providers require a manifest".to_owned())?;
                let manifest: HttpProviderManifest = serde_json::from_value(manifest)
                    .map_err(|error| format!("invalid HTTP AI provider manifest: {error}"))?;
                let provider = HttpProvider::new(manifest, transport, |_reference| Ok(None))
                    .map_err(|error| error.to_string())?;
                Ok(Arc::new(provider))
            }
        }
    }

    fn provider(&self, provider_id: &str) -> Result<Arc<dyn AiProvider>, String> {
        if let Some(provider) = self
            .providers
            .lock()
            .map_err(|_| "AI provider runtime state is unavailable".to_owned())?
            .get(provider_id)
            .cloned()
        {
            return Ok(provider);
        }
        let config = self
            .configs
            .lock()
            .map_err(|_| "AI provider configuration state is unavailable".to_owned())?
            .get(provider_id)
            .cloned()
            .ok_or_else(|| format!("AI provider '{provider_id}' does not exist"))?;
        let provider = self.provider_from_config(&config)?;
        self.providers
            .lock()
            .map_err(|_| "AI provider runtime state is unavailable".to_owned())?
            .insert(provider_id.to_owned(), Arc::clone(&provider));
        Ok(provider)
    }

    fn task_key(provider_id: &str, task_id: &str) -> String {
        format!("{provider_id}:{task_id}")
    }

    fn active_task(
        &self,
        provider_id: &str,
        task_id: &str,
    ) -> Result<(Arc<dyn AiProvider>, CancellationToken), String> {
        let key = Self::task_key(provider_id, task_id);
        let tasks = self
            .tasks
            .lock()
            .map_err(|_| "AI provider task state is unavailable".to_owned())?;
        let active = tasks
            .get(&key)
            .ok_or_else(|| format!("AI provider task '{task_id}' does not exist"))?;
        Ok((Arc::clone(&active.provider), active.token.clone()))
    }

    fn update_task_state(
        &self,
        provider_id: &str,
        task_id: &str,
        state: TaskState,
    ) -> Result<AiTask, String> {
        let key = Self::task_key(provider_id, task_id);
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| "AI provider task state is unavailable".to_owned())?;
        let active = tasks
            .get_mut(&key)
            .ok_or_else(|| format!("AI provider task '{task_id}' does not exist"))?;
        active.task.state = state;
        Ok(active.task.clone())
    }

    pub fn submit(&self, provider_id: &str, mut request: SubmitRequest) -> Result<AiTask, String> {
        let provider = self.provider(provider_id)?;
        request.provenance.provider_id = Some(provider_id.to_owned());
        let response: SubmitResponse = provider
            .submit(&request)
            .map_err(|error| error.to_string())?;
        let task = AiTask {
            task_id: response.task_id.clone(),
            provider_id: provider_id.to_owned(),
            state: response.status.state,
            provenance: response.provenance,
        };
        let key = Self::task_key(provider_id, &task.task_id);
        self.tasks
            .lock()
            .map_err(|_| "AI provider task state is unavailable".to_owned())?
            .insert(
                key,
                ActiveProviderTask {
                    provider,
                    token: CancellationToken::new(),
                    task: task.clone(),
                },
            );
        Ok(task)
    }

    pub fn status(&self, provider_id: &str, task_id: &str) -> Result<TaskStatus, String> {
        let (provider, _) = self.active_task(provider_id, task_id)?;
        let status = provider
            .status(task_id)
            .map_err(|error| error.to_string())?;
        self.update_task_state(provider_id, task_id, status.state)?;
        Ok(status)
    }

    pub fn result(&self, provider_id: &str, task_id: &str) -> Result<AiResult, String> {
        let (provider, _) = self.active_task(provider_id, task_id)?;
        provider.result(task_id).map_err(|error| error.to_string())
    }

    pub fn wait(
        &self,
        provider_id: &str,
        task_id: &str,
        policy: PollPolicy,
    ) -> Result<AiResult, String> {
        let (provider, token) = self.active_task(provider_id, task_id)?;
        let result =
            poll_until_complete(provider.as_ref(), task_id, policy, &token, &ThreadSleeper);
        match &result {
            Ok(_) => {
                self.update_task_state(provider_id, task_id, TaskState::Succeeded)?;
            }
            Err(ProviderError::Cancelled | ProviderError::TaskCancelled) => {
                self.update_task_state(provider_id, task_id, TaskState::Cancelled)?;
            }
            Err(_) => {
                self.update_task_state(provider_id, task_id, TaskState::Failed)?;
            }
        }
        result.map_err(|error| error.to_string())
    }

    pub fn cancel(&self, provider_id: &str, task_id: &str) -> Result<AiTask, String> {
        let (provider, token) = self.active_task(provider_id, task_id)?;
        token.cancel();
        provider
            .cancel(task_id)
            .map_err(|error| error.to_string())?;
        self.update_task_state(provider_id, task_id, TaskState::Cancelled)
    }

    fn capabilities_for(
        &self,
        config: &AiProviderConfigDto,
    ) -> Result<AiProviderCapabilitiesDto, String> {
        match config.kind {
            AiProviderKind::ComfyUi => Ok(capabilities_dto(AiCapabilities {
                supports_cancel: true,
                supports_progress: true,
                ..AiCapabilities::default()
            })),
            AiProviderKind::Http => {
                let manifest = config
                    .manifest
                    .clone()
                    .ok_or_else(|| "HTTP AI providers require a manifest".to_owned())?;
                let manifest: HttpProviderManifest = serde_json::from_value(manifest)
                    .map_err(|error| format!("invalid HTTP AI provider manifest: {error}"))?;
                let provider =
                    HttpProvider::new(manifest, Arc::new(ValidationTransport), |_reference| {
                        Ok::<Option<String>, ProviderError>(None)
                    })
                    .map_err(|error| error.to_string())?;
                Ok(capabilities_dto(provider.capabilities()))
            }
        }
    }

    fn summary_from(&self, config: &AiProviderConfigDto) -> AiProviderDto {
        let runtime = self
            .runtime
            .lock()
            .ok()
            .and_then(|runtimes| runtimes.get(&config.id).cloned());
        let (status, capabilities, error) = runtime
            .map(|runtime| (runtime.status, runtime.capabilities, runtime.error))
            .unwrap_or((AiProviderStatus::Configured, None, None));
        AiProviderDto {
            id: config.id.clone(),
            name: config.name.clone(),
            kind: config.kind,
            base_url: config.base_url.clone(),
            client_id: config.client_id.clone(),
            status,
            capabilities,
            error,
        }
    }

    fn persist(&self, configs: &BTreeMap<String, AiProviderConfigDto>) -> Result<(), String> {
        self.store
            .save(&configs.values().cloned().collect::<Vec<_>>())
    }
}

#[tauri::command]
pub fn list_ai_providers(
    state: State<'_, AiProviderManager>,
) -> Result<Vec<AiProviderDto>, String> {
    state.list()
}

#[tauri::command]
pub fn add_ai_provider(
    state: State<'_, AiProviderManager>,
    config: AiProviderConfigDto,
) -> Result<AiProviderDto, String> {
    state.add(config)
}

#[tauri::command]
pub fn remove_ai_provider(
    state: State<'_, AiProviderManager>,
    provider_id: String,
) -> Result<(), String> {
    state.remove(&provider_id)
}

#[tauri::command]
pub fn test_ai_provider(
    state: State<'_, AiProviderManager>,
    provider_id: String,
) -> Result<AiProviderDto, String> {
    state.test(&provider_id)
}

#[tauri::command]
pub fn submit_ai_task(
    state: State<'_, AiProviderManager>,
    provider_id: String,
    request: SubmitRequest,
) -> Result<AiTask, String> {
    state.submit(&provider_id, request)
}

#[tauri::command]
pub fn ai_task_status(
    state: State<'_, AiProviderManager>,
    provider_id: String,
    task_id: String,
) -> Result<TaskStatus, String> {
    state.status(&provider_id, &task_id)
}

#[tauri::command]
pub fn ai_task_result(
    state: State<'_, AiProviderManager>,
    provider_id: String,
    task_id: String,
) -> Result<AiResult, String> {
    state.result(&provider_id, &task_id)
}

#[tauri::command]
pub fn wait_ai_task(
    state: State<'_, AiProviderManager>,
    provider_id: String,
    task_id: String,
    policy: Option<PollPolicy>,
) -> Result<AiResult, String> {
    state.wait(&provider_id, &task_id, policy.unwrap_or_default())
}

#[tauri::command]
pub fn cancel_ai_task(
    state: State<'_, AiProviderManager>,
    provider_id: String,
    task_id: String,
) -> Result<AiTask, String> {
    state.cancel(&provider_id, &task_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn comfy(id: &str) -> AiProviderConfigDto {
        AiProviderConfigDto {
            id: id.to_owned(),
            name: format!("{id} provider"),
            kind: AiProviderKind::ComfyUi,
            base_url: "http://127.0.0.1:8188".to_owned(),
            client_id: Some("rawweave".to_owned()),
            manifest: None,
        }
    }

    #[test]
    fn provider_manager_keeps_provider_configuration_separate_and_redacts_secrets() {
        let manager = AiProviderManager::memory();
        manager.add(comfy("local-comfy")).unwrap();
        let providers = manager.list().unwrap();
        assert_eq!(providers[0].id, "local-comfy");
        assert_eq!(providers[0].status, AiProviderStatus::Configured);
        assert!(!serde_json::to_string(&providers)
            .unwrap()
            .contains("api_key"));
    }

    #[test]
    fn generic_http_provider_requires_a_manifest_and_secret_references() {
        let manager = AiProviderManager::memory();
        let mut config = comfy("http");
        config.kind = AiProviderKind::Http;
        assert!(manager.add(config.clone()).is_err());
        config.manifest = Some(json!({
            "id": "http",
            "name": "HTTP",
            "submit": { "method": "Post", "url": "https://example.test/submit", "headers": {
                "Authorization": { "Literal": "not-secret" }
            }}
        }));
        assert!(manager.add(config).is_err());
    }

    #[test]
    fn testing_a_provider_reports_capabilities_without_performing_network_io() {
        let manager = AiProviderManager::memory();
        manager.add(comfy("local-comfy")).unwrap();
        let provider = manager.test("local-comfy").unwrap();
        assert_eq!(provider.status, AiProviderStatus::Ready);
        assert!(provider.capabilities.unwrap().supports_cancel);
    }

    #[test]
    fn provider_configurations_round_trip_through_bounded_persistent_storage() {
        let path =
            std::env::temp_dir().join(format!("rawweave-ai-providers-{}.json", std::process::id()));
        let manager = AiProviderManager::persistent(&path).unwrap();
        manager.add(comfy("persisted")).unwrap();
        let loaded = AiProviderManager::persistent(&path).unwrap();
        assert_eq!(loaded.list().unwrap()[0].id, "persisted");
        let _ = fs::remove_file(path);
    }
}
