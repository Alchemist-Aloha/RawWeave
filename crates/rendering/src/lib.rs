use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use rawweave_image::{Dimensions, Image, PixelFormat, Region};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GraphRevision(u64);

impl GraphRevision {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

impl From<u64> for GraphRevision {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileCoord {
    pub x: u32,
    pub y: u32,
}

impl TileCoord {
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewQuality {
    Draft,
    #[default]
    Preview,
    Final,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileRequest {
    pub region: Region,
    pub tile: TileCoord,
    pub mip_level: u8,
    pub quality: PreviewQuality,
}

impl TileRequest {
    pub const fn new(
        region: Region,
        tile: TileCoord,
        mip_level: u8,
        quality: PreviewQuality,
    ) -> Self {
        Self {
            region,
            tile,
            mip_level,
            quality,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CacheKey {
    pub node_id: String,
    pub implementation_version: u32,
    pub parameter_hash: u64,
    pub upstream_hash: u64,
    pub region: Region,
    pub tile: TileCoord,
    pub mip_level: u8,
    pub quality: PreviewQuality,
}

impl CacheKey {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        node_id: impl Into<String>,
        implementation_version: u32,
        parameter_hash: u64,
        upstream_hash: u64,
        region: Region,
        tile: TileCoord,
        mip_level: u8,
        quality: PreviewQuality,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            implementation_version,
            parameter_hash,
            upstream_hash,
            region,
            tile,
            mip_level,
            quality,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderResult {
    pub image: Arc<Image>,
    pub revision: GraphRevision,
}

impl RenderResult {
    pub fn new(image: Image, revision: GraphRevision) -> Self {
        Self {
            image: Arc::new(image),
            revision,
        }
    }

    pub fn is_current(&self, revision: GraphRevision) -> bool {
        self.revision == revision
    }

    pub fn into_image(self) -> Arc<Image> {
        self.image
    }
}

#[derive(Clone, Debug)]
pub struct MemoryRenderCache {
    capacity: usize,
    entries: HashMap<CacheKey, RenderResult>,
    order: VecDeque<CacheKey>,
}

impl MemoryRenderCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &CacheKey) -> Option<RenderResult> {
        self.entries.get(key).cloned()
    }

    pub fn insert(&mut self, key: CacheKey, result: RenderResult) {
        if self.capacity == 0 {
            return;
        }
        if !self.entries.contains_key(&key) {
            self.order.push_back(key.clone());
        }
        self.entries.insert(key, result);
        while self.entries.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }

    pub fn insert_if_current(
        &mut self,
        key: CacheKey,
        result: RenderResult,
        current_revision: GraphRevision,
    ) -> bool {
        if !result.is_current(current_revision) {
            return false;
        }
        self.insert(key, result);
        true
    }

    pub fn accepts_revision(&self, key: &CacheKey, revision: GraphRevision) -> bool {
        self.get(key)
            .is_some_and(|result| result.is_current(revision))
    }

    pub fn invalidate_node(&mut self, node_id: &str) -> usize {
        self.invalidate_where(|key| key.node_id == node_id)
    }

    pub fn invalidate_nodes<'a>(&mut self, node_ids: impl IntoIterator<Item = &'a str>) -> usize {
        let node_ids = node_ids
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        self.invalidate_where(|key| node_ids.contains(key.node_id.as_str()))
    }

    pub fn invalidate_revision(&mut self, revision: GraphRevision) -> usize {
        let keys = self
            .entries
            .iter()
            .filter(|(_, result)| result.revision == revision)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        let count = keys.len();
        for key in keys {
            self.entries.remove(&key);
        }
        self.order.retain(|key| self.entries.contains_key(key));
        count
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    fn invalidate_where(&mut self, predicate: impl Fn(&CacheKey) -> bool) -> usize {
        let keys = self
            .entries
            .keys()
            .filter(|key| predicate(key))
            .cloned()
            .collect::<Vec<_>>();
        let count = keys.len();
        for key in keys {
            self.entries.remove(&key);
        }
        self.order.retain(|key| self.entries.contains_key(key));
        count
    }
}

impl Default for MemoryRenderCache {
    fn default() -> Self {
        Self::new(64)
    }
}

#[derive(Clone, Debug)]
pub struct RenderContext {
    cache: Arc<Mutex<MemoryRenderCache>>,
    gpu: Option<GpuContext>,
}

impl Default for RenderContext {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderContext {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(MemoryRenderCache::default())),
            gpu: None,
        }
    }

    pub fn with_cache(cache: MemoryRenderCache) -> Self {
        Self {
            cache: Arc::new(Mutex::new(cache)),
            gpu: None,
        }
    }

    pub fn with_gpu(mut self, gpu: GpuContext) -> Self {
        self.gpu = Some(gpu);
        self
    }

    pub fn cache(&self) -> Arc<Mutex<MemoryRenderCache>> {
        Arc::clone(&self.cache)
    }

    pub fn gpu(&self) -> Option<&GpuContext> {
        self.gpu.as_ref()
    }

    pub fn gpu_available(&self) -> bool {
        self.gpu.as_ref().is_some_and(GpuContext::is_available)
    }
}

#[derive(Debug, Error)]
pub enum GpuError {
    #[error("no compatible wgpu adapter is available")]
    AdapterUnavailable,
    #[error("wgpu adapter request failed: {0}")]
    AdapterRequest(String),
    #[error("wgpu device request failed: {0}")]
    DeviceRequest(String),
    #[error("WGSL source cannot be empty")]
    EmptyShader,
    #[error("wgpu compute execution failed: {0}")]
    ComputeExecution(String),
}

#[derive(Clone, Debug)]
struct GpuInner {
    instance: Arc<wgpu::Instance>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

#[derive(Clone, Debug)]
pub struct GpuContext {
    inner: Arc<GpuInner>,
}

impl GpuContext {
    pub fn initialize() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|error| GpuError::AdapterRequest(error.to_string()))?;
        Self::from_adapter(Arc::new(instance), adapter)
    }

    pub fn initialize_or_cpu() -> Option<Self> {
        Self::initialize().ok()
    }

    pub fn from_adapter(
        instance: Arc<wgpu::Instance>,
        adapter: wgpu::Adapter,
    ) -> Result<Self, GpuError> {
        let descriptor = wgpu::DeviceDescriptor {
            label: Some("rawweave-rendering"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&descriptor))
            .map_err(|error| GpuError::DeviceRequest(error.to_string()))?;
        Ok(Self {
            inner: Arc::new(GpuInner {
                instance,
                adapter,
                device,
                queue,
            }),
        })
    }

    pub fn is_available(&self) -> bool {
        true
    }

    pub fn adapter_info(&self) -> wgpu::AdapterInfo {
        self.inner.adapter.get_info()
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.inner.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.inner.queue
    }

    pub fn instance(&self) -> &wgpu::Instance {
        &self.inner.instance
    }

    pub fn create_shader_module(&self, source: &str) -> Result<wgpu::ShaderModule, GpuError> {
        if source.trim().is_empty() {
            return Err(GpuError::EmptyShader);
        }
        Ok(self
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("rawweave-shader"),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)),
            }))
    }

    pub fn dispatch_compute(&self, source: &str, workgroups: [u32; 3]) -> Result<(), GpuError> {
        if workgroups.contains(&0) {
            return Err(GpuError::ComputeExecution(
                "compute workgroups must be non-zero".to_owned(),
            ));
        }
        let shader = self.create_shader_module(source)?;
        let pipeline = self
            .device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("rawweave-compute"),
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        let mut encoder = self
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rawweave-compute-encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rawweave-compute-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.dispatch_workgroups(workgroups[0], workgroups[1], workgroups[2]);
        }
        self.queue().submit(Some(encoder.finish()));
        self.device()
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| GpuError::ComputeExecution(error.to_string()))?;
        Ok(())
    }

    pub fn run_minimal_compute(&self) -> Result<(), GpuError> {
        self.dispatch_compute(MINIMAL_COMPUTE_WGSL, [1, 1, 1])
    }

    pub fn create_texture(
        &self,
        dimensions: Dimensions,
        pixel_format: PixelFormat,
    ) -> wgpu::Texture {
        self.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("rawweave-image"),
            size: wgpu::Extent3d {
                width: dimensions.width.max(1),
                height: dimensions.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture_format(pixel_format),
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct ShaderLibrary {
    sources: HashMap<String, String>,
}

impl ShaderLibrary {
    pub fn register(
        &mut self,
        name: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<(), GpuError> {
        let source = source.into();
        if source.trim().is_empty() {
            return Err(GpuError::EmptyShader);
        }
        self.sources.insert(name.into(), source);
        Ok(())
    }

    pub fn source(&self, name: &str) -> Option<&str> {
        self.sources.get(name).map(String::as_str)
    }

    pub fn create_module(
        &self,
        gpu: &GpuContext,
        name: &str,
    ) -> Result<wgpu::ShaderModule, GpuError> {
        let source = self.source(name).ok_or(GpuError::EmptyShader)?;
        gpu.create_shader_module(source)
    }
}

pub const COLOR_MATRIX_WGSL: &str = r#"
struct Params { matrix: mat4x4<f32>, offset: vec4<f32> };
@group(0) @binding(0) var<uniform> params: Params;
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    _ = id;
}
"#;

pub const MINIMAL_COMPUTE_WGSL: &str = r#"
@compute @workgroup_size(1, 1, 1)
fn main() {}
"#;

fn texture_format(pixel_format: PixelFormat) -> wgpu::TextureFormat {
    match pixel_format {
        PixelFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
        PixelFormat::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
        PixelFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
    }
}
