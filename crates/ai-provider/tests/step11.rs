use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rawweave_ai_provider::{
    AiCapabilities, AiImage, AiOutput, AiProvider, AiResult, AiTask, AiWorkflow, AlphaMode,
    CancellationToken, CheckpointGeneration, ColorInterchange, ComfyUiConfig, ComfyUiProvider,
    HeaderValue, HttpEndpoint, HttpMethod, HttpProvider, HttpProviderManifest, HttpRequest,
    HttpResponse, HttpTransport, PollPolicy, ProviderError, ProviderRegistry, RetryPolicy, Sleeper,
    SubmitRequest, TaskProvenance, TaskState, TaskStatus, WorkflowBinding, WorkflowBindings,
    poll_until_complete,
};
use rawweave_graph::{
    ArtifactStore, Checkpoint, CheckpointArtifact, CheckpointPayload, GenerationMetadata,
    Provenance,
};
use serde_json::json;

fn workflow() -> AiWorkflow {
    AiWorkflow::new(
        "inpaint",
        "7",
        json!({"nodes": {"sampler": {"inputs": {"steps": 20}}}}),
        WorkflowBindings {
            image: Some(WorkflowBinding::new("image", "pixels")),
            mask: Some(WorkflowBinding::new("mask", "mask")),
            prompt: Some(WorkflowBinding::new("prompt", "text")),
            parameters: BTreeMap::from([(
                "steps".to_owned(),
                WorkflowBinding::new("sampler", "steps"),
            )]),
            output: WorkflowBinding::new("decode", "image"),
        },
    )
}

fn request(provider_id: &str) -> SubmitRequest {
    SubmitRequest::new(
        workflow(),
        TaskProvenance::new("node-1", 3, "dependency-hash")
            .with_provider(provider_id)
            .with_parameter_hash("parameter-hash"),
    )
}

#[derive(Clone)]
struct FixtureProvider {
    id: String,
    statuses: Arc<Mutex<Vec<Result<TaskStatus, ProviderError>>>>,
    result: AiResult,
    cancels: Arc<Mutex<usize>>,
}

impl FixtureProvider {
    fn new(id: &str, statuses: Vec<Result<TaskStatus, ProviderError>>) -> Self {
        Self {
            id: id.to_owned(),
            statuses: Arc::new(Mutex::new(statuses)),
            result: AiResult::from_bytes("task-1", vec![1, 2, 3], "image/png"),
            cancels: Arc::new(Mutex::new(0)),
        }
    }
}

impl AiProvider for FixtureProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn capabilities(&self) -> AiCapabilities {
        AiCapabilities::default()
    }

    fn submit(
        &self,
        request: &SubmitRequest,
    ) -> Result<rawweave_ai_provider::SubmitResponse, ProviderError> {
        Ok(rawweave_ai_provider::SubmitResponse::queued(
            "task-1",
            request.provenance.clone(),
        ))
    }

    fn status(&self, _task_id: &str) -> Result<TaskStatus, ProviderError> {
        self.statuses.lock().unwrap().remove(0)
    }

    fn result(&self, _task_id: &str) -> Result<AiResult, ProviderError> {
        Ok(self.result.clone())
    }

    fn cancel(&self, _task_id: &str) -> Result<(), ProviderError> {
        *self.cancels.lock().unwrap() += 1;
        Ok(())
    }
}

#[derive(Default)]
struct RecordingSleeper {
    delays: Mutex<Vec<Duration>>,
}

impl Sleeper for RecordingSleeper {
    fn sleep(&self, delay: Duration) -> Result<(), ProviderError> {
        self.delays.lock().unwrap().push(delay);
        Ok(())
    }
}

#[derive(Default)]
struct FixtureHttp {
    requests: Mutex<Vec<HttpRequest>>,
    responses: Mutex<Vec<HttpResponse>>,
}

impl FixtureHttp {
    fn with_responses(responses: Vec<HttpResponse>) -> Arc<Self> {
        Arc::new(Self {
            requests: Mutex::new(Vec::new()),
            responses: Mutex::new(responses),
        })
    }
}

impl HttpTransport for FixtureHttp {
    fn execute(&self, request: HttpRequest) -> Result<HttpResponse, ProviderError> {
        self.requests.lock().unwrap().push(request);
        self.responses.lock().unwrap().remove(0).pipe(Ok)
    }
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

#[test]
fn task_state_and_provenance_round_trip_without_provider_secrets() {
    let task = AiTask {
        task_id: "task-1".to_owned(),
        provider_id: "local-comfy".to_owned(),
        state: TaskState::Running,
        provenance: TaskProvenance::new("node-1", 3, "dependency-hash")
            .with_provider("local-comfy")
            .with_workflow_hash("workflow-hash"),
    };
    let encoded = serde_json::to_string(&task).unwrap();
    let decoded: AiTask = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, task);
    assert!(!encoded.contains("Authorization"));
    assert!(!encoded.contains("secret"));
}

#[test]
fn provider_registry_keeps_provider_and_workflow_as_separate_concepts() {
    let provider_a = FixtureProvider::new("provider-a", vec![]);
    let provider_b = FixtureProvider::new("provider-b", vec![]);
    let mut registry = ProviderRegistry::default();
    registry.register(provider_a).unwrap();
    registry.register(provider_b).unwrap();

    let same_workflow = workflow();
    let a = registry
        .provider("provider-a")
        .unwrap()
        .submit(&SubmitRequest::new(
            same_workflow.clone(),
            TaskProvenance::new("node", 1, "hash"),
        ))
        .unwrap();
    let b = registry
        .provider("provider-b")
        .unwrap()
        .submit(&SubmitRequest::new(
            same_workflow,
            TaskProvenance::new("node", 1, "hash"),
        ))
        .unwrap();
    assert_eq!(a.task_id, b.task_id);
    assert_eq!(registry.ids(), vec!["provider-a", "provider-b"]);
}

#[test]
fn polling_retries_transient_failures_with_bounded_exponential_backoff() {
    let provider = FixtureProvider::new(
        "fixture",
        vec![
            Err(ProviderError::Transport("temporary".to_owned())),
            Ok(TaskStatus::running()),
            Ok(TaskStatus::succeeded()),
        ],
    );
    let sleeper = RecordingSleeper::default();
    let policy = PollPolicy {
        max_polls: 3,
        poll_delay: RetryPolicy::new(10, 40, 2, 2),
        request_retry: RetryPolicy::new(2, 5, 10, 2),
    };
    let result = poll_until_complete(
        &provider,
        "task-1",
        policy,
        &CancellationToken::new(),
        &sleeper,
    )
    .unwrap();
    assert_eq!(result.task_id, "task-1");
    assert_eq!(
        sleeper.delays.lock().unwrap().as_slice(),
        &[Duration::from_millis(2), Duration::from_millis(10)]
    );
}

#[test]
fn polling_cancellation_calls_provider_cancel_and_never_unbounds_work() {
    let provider = FixtureProvider::new("fixture", vec![Ok(TaskStatus::running())]);
    let token = CancellationToken::new();
    token.cancel();
    let sleeper = RecordingSleeper::default();
    let error = poll_until_complete(&provider, "task-1", PollPolicy::default(), &token, &sleeper)
        .unwrap_err();
    assert!(matches!(error, ProviderError::Cancelled));
    assert_eq!(*provider.cancels.lock().unwrap(), 1);
}

#[test]
fn generic_http_provider_redacts_sensitive_headers_and_enforces_response_limits() {
    let mut manifest = HttpProviderManifest::new(
        "http-fixture",
        "HTTP Fixture",
        HttpEndpoint::new(HttpMethod::Post, "https://example.test/submit"),
    );
    manifest.submit.headers.insert(
        "Authorization".to_owned(),
        HeaderValue::SecretRef("api-key".to_owned()),
    );
    manifest.max_response_bytes = 3;
    let transport = FixtureHttp::with_responses(vec![HttpResponse::bytes(
        200,
        vec![1, 2, 3, 4],
        "application/octet-stream",
    )]);
    let provider = HttpProvider::new(manifest, transport, |_reference| {
        Ok(Some("do-not-log-this-token".to_owned()))
    })
    .unwrap();
    let debug = format!("{provider:?}");
    assert!(!debug.contains("do-not-log-this-token"));
    assert!(debug.contains("REDACTED"));
    let error = provider.submit(&request("http-fixture")).unwrap_err();
    assert!(matches!(
        error,
        ProviderError::ResponseTooLarge { limit: 3, .. }
    ));
}

#[test]
fn generic_http_provider_fixture_supports_submit_poll_and_result() {
    let transport = FixtureHttp::with_responses(vec![
        HttpResponse::json(202, json!({"id": "remote-1"})),
        HttpResponse::json(200, json!({"status": "completed"})),
        HttpResponse::bytes(200, vec![9, 8, 7], "image/png"),
    ]);
    let mut manifest = HttpProviderManifest::new(
        "http-fixture",
        "HTTP Fixture",
        HttpEndpoint::new(HttpMethod::Post, "https://example.test/submit"),
    );
    manifest.status = Some(HttpEndpoint::new(
        HttpMethod::Get,
        "https://example.test/jobs/$JOB_ID",
    ));
    manifest.result = Some(HttpEndpoint::new(
        HttpMethod::Get,
        "https://example.test/jobs/$JOB_ID/result",
    ));
    manifest.job_id_path = Some("$.id".to_owned());
    manifest.status_path = Some("$.status".to_owned());
    manifest.completion_states.insert("completed".to_owned());
    let provider = HttpProvider::new(manifest, transport, |_reference| Ok(None)).unwrap();
    let submitted = provider.submit(&request("http-fixture")).unwrap();
    assert_eq!(submitted.status.state, TaskState::Queued);
    assert_eq!(
        provider.status(&submitted.task_id).unwrap().state,
        TaskState::Succeeded
    );
    assert_eq!(
        provider.result(&submitted.task_id).unwrap().bytes(),
        Some(&[9, 8, 7][..])
    );
}

#[test]
fn comfyui_fixture_uses_authoritative_history_and_view_endpoints() {
    let transport = FixtureHttp::with_responses(vec![
        HttpResponse::json(200, json!({"prompt_id": "prompt-1"})),
        HttpResponse::json(
            200,
            json!({
                "prompt-1": {
                    "status": {"completed": true, "status_str": "success"},
                    "outputs": {"9": {"images": [{"filename": "result.png", "subfolder": "", "type": "output"}]}}
                }
            }),
        ),
        HttpResponse::json(
            200,
            json!({
                "prompt-1": {
                    "status": {"completed": true, "status_str": "success"},
                    "outputs": {"9": {"images": [{"filename": "result.png", "subfolder": "", "type": "output"}]}}
                }
            }),
        ),
        HttpResponse::bytes(200, vec![4, 5, 6], "image/png"),
    ]);
    let provider =
        ComfyUiProvider::new(ComfyUiConfig::new("http://127.0.0.1:8188"), transport).unwrap();
    let submitted = provider.submit(&request("comfyui")).unwrap();
    assert_eq!(submitted.status.state, TaskState::Queued);
    assert_eq!(
        provider.status(&submitted.task_id).unwrap().state,
        TaskState::Succeeded
    );
    assert_eq!(
        provider.result(&submitted.task_id).unwrap().bytes(),
        Some(&[4, 5, 6][..])
    );
}

#[test]
fn completed_provider_result_commits_through_manual_checkpoint_token() {
    let store = ArtifactStore::memory();
    let mut checkpoint = Checkpoint::new("node-1", 3);
    checkpoint.set_dependency_hash("dependency-hash");
    let task = AiTask {
        task_id: "task-1".to_owned(),
        provider_id: "fixture".to_owned(),
        state: TaskState::Succeeded,
        provenance: TaskProvenance::new("node-1", 3, "dependency-hash").with_provider("fixture"),
    };
    let generation = CheckpointGeneration::begin(&mut checkpoint, task).unwrap();
    let artifact = CheckpointArtifact::new(
        CheckpointPayload::SpatialData(vec![1, 2, 3]),
        "dependency-hash",
        Provenance::new("dependency-hash", 3),
        GenerationMetadata::new(1),
    )
    .unwrap();
    generation
        .commit(&mut checkpoint, artifact, &store)
        .unwrap();
    assert_eq!(checkpoint.state(), rawweave_graph::CheckpointState::Current);
}

#[test]
fn png_interchange_declares_lossless_color_and_alpha_assumptions() {
    let interchange = ColorInterchange::png_srgb();
    assert_eq!(interchange.format, rawweave_ai_provider::ImageFormat::Png);
    assert_eq!(interchange.alpha_mode, AlphaMode::Straight);
    assert!(interchange.lossless);
    let image = AiImage::new(vec![1, 2, 3], [1, 1], interchange).unwrap();
    assert!(AiOutput::image(image).as_image().is_some());
}

#[test]
fn comfyui_uploads_bound_assets_and_binds_the_uploaded_paths() {
    let transport = FixtureHttp::with_responses(vec![
        HttpResponse::json(
            200,
            json!({"name": "image.png", "subfolder": "rawweave-image", "type": "input"}),
        ),
        HttpResponse::json(
            200,
            json!({"name": "mask.png", "subfolder": "rawweave-mask", "type": "input"}),
        ),
        HttpResponse::json(200, json!({"prompt_id": "prompt-uploads"})),
    ]);
    let provider = ComfyUiProvider::new(
        ComfyUiConfig::new("http://127.0.0.1:8188"),
        transport.clone(),
    )
    .unwrap();
    let mut request = request("comfyui");
    request.workflow.definition = json!({
        "nodes": {
            "load": {"inputs": {"image": "old.png", "mask": "old-mask.png"}},
            "positive": {"inputs": {"text": "old prompt"}},
            "sampler": {"inputs": {"steps": 1}}
        }
    });
    request.workflow.bindings.image = Some(WorkflowBinding::new("load", "image"));
    request.workflow.bindings.mask = Some(WorkflowBinding::new("load", "mask"));
    request.workflow.bindings.prompt = Some(WorkflowBinding::new("positive", "text"));
    request.workflow.bindings.parameters =
        BTreeMap::from([("steps".to_owned(), WorkflowBinding::new("sampler", "steps"))]);
    let image = AiImage::new(vec![1, 2, 3], [1, 1], ColorInterchange::png_srgb()).unwrap();
    let mask = AiImage::new(vec![4, 5, 6], [1, 1], ColorInterchange::png_srgb()).unwrap();
    request.inputs.insert(
        "image".to_owned(),
        rawweave_ai_provider::AiInput::Image(image),
    );
    request
        .inputs
        .insert("mask".to_owned(), rawweave_ai_provider::AiInput::Mask(mask));
    request.inputs.insert(
        "prompt".to_owned(),
        rawweave_ai_provider::AiInput::Text("new prompt".to_owned()),
    );
    request.parameters.insert("steps".to_owned(), json!(30));

    provider.submit(&request).unwrap();

    let requests = transport.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].url.contains("upload/image"));
    assert!(
        requests[0]
            .body
            .windows(3)
            .any(|window| window == [1, 2, 3])
    );
    assert!(requests[1].url.contains("upload/image"));
    assert!(
        requests[1]
            .body
            .windows(3)
            .any(|window| window == [4, 5, 6])
    );
    let prompt: serde_json::Value = serde_json::from_slice(&requests[2].body).unwrap();
    assert_eq!(
        prompt["prompt"]["nodes"]["load"]["inputs"]["image"],
        "rawweave-image/image.png"
    );
    assert_eq!(
        prompt["prompt"]["nodes"]["load"]["inputs"]["mask"],
        "rawweave-mask/mask.png"
    );
    assert_eq!(
        prompt["prompt"]["nodes"]["positive"]["inputs"]["text"],
        "new prompt"
    );
    assert_eq!(prompt["prompt"]["nodes"]["sampler"]["inputs"]["steps"], 30);
}

#[test]
fn comfyui_missing_history_is_still_a_running_task() {
    let transport = FixtureHttp::with_responses(vec![
        HttpResponse::json(200, json!({"prompt_id": "prompt-pending"})),
        HttpResponse::json(200, json!({})),
    ]);
    let provider =
        ComfyUiProvider::new(ComfyUiConfig::new("http://127.0.0.1:8188"), transport).unwrap();
    let submitted = provider.submit(&request("comfyui")).unwrap();
    assert_eq!(
        provider.status(&submitted.task_id).unwrap().state,
        TaskState::Running
    );
}
