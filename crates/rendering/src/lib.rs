use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rawweave_image::{Dimensions, GpuImage, GpuImageResource, Image, Pixel, PixelFormat, Region};
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
        self.get_current(key, revision).is_some()
    }

    pub fn get_current(&self, key: &CacheKey, revision: GraphRevision) -> Option<RenderResult> {
        self.get(key).filter(|result| result.is_current(revision))
    }

    pub fn restamp_revision(&mut self, revision: GraphRevision) {
        for result in self.entries.values_mut() {
            result.revision = revision;
        }
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
    #[error("GPU image has no live runtime resource")]
    MissingImageResource,
    #[error("GPU image belongs to a different rendering context")]
    WrongImageContext,
}

#[derive(Debug)]
struct GpuInner {
    instance: Arc<wgpu::Instance>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    next_resource_id: AtomicU64,
}

#[derive(Clone, Debug)]
pub struct GpuContext {
    inner: Arc<GpuInner>,
}

struct WgpuImageResource {
    context: GpuContext,
    buffer: Arc<wgpu::Buffer>,
    resource_id: u64,
}

impl fmt::Debug for WgpuImageResource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WgpuImageResource")
            .field("resource_id", &self.resource_id)
            .field("context_id", &self.context.context_id())
            .finish_non_exhaustive()
    }
}

impl GpuImageResource for WgpuImageResource {
    fn resource_id(&self) -> u64 {
        self.resource_id
    }

    fn context_id(&self) -> u64 {
        self.context.context_id()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
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
                next_resource_id: AtomicU64::new(1),
            }),
        })
    }

    pub fn is_available(&self) -> bool {
        true
    }

    pub fn context_id(&self) -> u64 {
        Arc::as_ptr(&self.inner) as usize as u64
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

    pub fn upload_image(&self, image: &Image) -> Result<GpuImage, GpuError> {
        let bytes = pixels_to_bytes(image.pixels());
        let resource = self.create_buffer_resource(
            &bytes,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        Ok(GpuImage::from_resource_with_origin(
            image.dimensions(),
            image.origin(),
            image.pixel_format(),
            image.color_metadata(),
            image.revision(),
            resource,
        ))
    }

    pub fn apply_color_matrix(
        &self,
        image: &Image,
        matrix: [[f32; 4]; 4],
        offset: [f32; 4],
    ) -> Result<Image, GpuError> {
        if image.pixels().is_empty() {
            return Image::from_pixels_with_origin(
                image.dimensions(),
                image.origin(),
                Vec::new(),
                image.pixel_format(),
                image.color_metadata(),
            )
            .map_err(|error| GpuError::ComputeExecution(error.to_string()));
        }
        let input = self.upload_image(image)?;
        let input_resource = image_resource(&input)?;
        let output_bytes = vec![0_u8; std::mem::size_of_val(image.pixels())];
        let output_resource = self.create_buffer_resource(
            &output_bytes,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let output = GpuImage::from_resource_with_origin(
            image.dimensions(),
            image.origin(),
            image.pixel_format(),
            image.color_metadata(),
            image.revision(),
            Arc::clone(&output_resource),
        );
        let output_resource = image_resource(&output)?;
        self.dispatch_color_matrix(
            input_resource,
            output_resource,
            image.pixels().len() as u32,
            matrix,
            offset,
        )?;
        self.readback_image(&output)
    }

    pub fn readback_image(&self, image: &GpuImage) -> Result<Image, GpuError> {
        let resource = image_resource(image)?;
        if resource.context.context_id() != self.context_id() {
            return Err(GpuError::WrongImageContext);
        }
        let byte_len = image
            .dimensions
            .pixel_count()
            .map_err(|error| GpuError::ComputeExecution(error.to_string()))?
            * std::mem::size_of::<Pixel>();
        if byte_len == 0 {
            return Image::from_pixels_with_origin(
                image.dimensions,
                image.origin,
                Vec::new(),
                image.pixel_format,
                image.color_metadata,
            )
            .map_err(|error| GpuError::ComputeExecution(error.to_string()));
        }
        let staging = self.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("rawweave-image-readback"),
            size: byte_len as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rawweave-image-readback-encoder"),
            });
        encoder.copy_buffer_to_buffer(&resource.buffer, 0, &staging, 0, byte_len as u64);
        self.queue().submit(Some(encoder.finish()));
        let (sender, receiver) = std::sync::mpsc::channel();
        staging
            .slice(..byte_len as u64)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.device()
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| GpuError::ComputeExecution(error.to_string()))?;
        receiver
            .recv()
            .map_err(|error| GpuError::ComputeExecution(error.to_string()))?
            .map_err(|error| GpuError::ComputeExecution(format!("{error:?}")))?;
        let bytes = staging
            .slice(..byte_len as u64)
            .get_mapped_range()
            .map_err(|error| GpuError::ComputeExecution(error.to_string()))?
            .to_vec();
        staging.unmap();
        let pixels = bytes_to_pixels(&bytes)?;
        Image::from_pixels_with_origin(
            image.dimensions,
            image.origin,
            pixels,
            image.pixel_format,
            image.color_metadata,
        )
        .map_err(|error| GpuError::ComputeExecution(error.to_string()))
    }

    fn create_buffer_resource(
        &self,
        bytes: &[u8],
        usage: wgpu::BufferUsages,
    ) -> Arc<dyn GpuImageResource> {
        let buffer = self.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("rawweave-image-buffer"),
            size: bytes.len().max(16) as u64,
            usage,
            mapped_at_creation: false,
        });
        if !bytes.is_empty() {
            self.queue().write_buffer(&buffer, 0, bytes);
        }
        Arc::new(WgpuImageResource {
            context: self.clone(),
            buffer: Arc::new(buffer),
            resource_id: self.inner.next_resource_id.fetch_add(1, Ordering::Relaxed),
        })
    }

    fn dispatch_color_matrix(
        &self,
        input: &WgpuImageResource,
        output: &WgpuImageResource,
        count: u32,
        matrix: [[f32; 4]; 4],
        offset: [f32; 4],
    ) -> Result<(), GpuError> {
        let shader = self.create_shader_module(COLOR_MATRIX_WGSL)?;
        let layout = self
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("rawweave-color-matrix-bindings"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let pipeline_layout =
            self.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("rawweave-color-matrix-pipeline-layout"),
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                });
        let pipeline = self
            .device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("rawweave-color-matrix-pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        let params = color_matrix_params(matrix, offset, count);
        let params_buffer = self.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("rawweave-color-matrix-params"),
            size: params.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue().write_buffer(&params_buffer, 0, &params);
        let bytes_size = (count as usize * std::mem::size_of::<Pixel>()) as u64;
        let bind_group = self.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rawweave-color-matrix-bind-group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &input.buffer,
                        offset: 0,
                        size: Some(
                            std::num::NonZeroU64::new(bytes_size)
                                .expect("image buffer is non-empty"),
                        ),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &output.buffer,
                        offset: 0,
                        size: Some(
                            std::num::NonZeroU64::new(bytes_size)
                                .expect("image buffer is non-empty"),
                        ),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &params_buffer,
                        offset: 0,
                        size: Some(
                            std::num::NonZeroU64::new(params.len() as u64)
                                .expect("matrix parameters are non-empty"),
                        ),
                    }),
                },
            ],
        });
        let mut encoder = self
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rawweave-color-matrix-encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rawweave-color-matrix-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
        }
        self.queue().submit(Some(encoder.finish()));
        self.device()
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| GpuError::ComputeExecution(error.to_string()))?;
        Ok(())
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
struct Params {
    matrix: array<vec4<f32>, 4>,
    offset: vec4<f32>,
    count: u32,
    _padding: vec3<u32>,
};
struct Pixels { values: array<vec4<f32>> };
@group(0) @binding(0) var<storage, read> input_pixels: Pixels;
@group(0) @binding(1) var<storage, read_write> output_pixels: Pixels;
@group(0) @binding(2) var<uniform> params: Params;
@compute @workgroup_size(64, 1, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.count {
        return;
    }
    let pixel = input_pixels.values[id.x];
    var output = vec4<f32>(0.0);
    for (var row = 0u; row < 4u; row = row + 1u) {
        output[row] = params.offset[row] + dot(params.matrix[row], pixel);
    }
    output_pixels.values[id.x] = output;
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

fn image_resource(image: &GpuImage) -> Result<&WgpuImageResource, GpuError> {
    image
        .runtime_resource()
        .and_then(|resource| resource.as_any().downcast_ref::<WgpuImageResource>())
        .ok_or(GpuError::MissingImageResource)
}

fn pixels_to_bytes(pixels: &[Pixel]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(std::mem::size_of_val(pixels));
    for pixel in pixels {
        for channel in pixel {
            bytes.extend_from_slice(&channel.to_ne_bytes());
        }
    }
    bytes
}

fn bytes_to_pixels(bytes: &[u8]) -> Result<Vec<Pixel>, GpuError> {
    const PIXEL_BYTES: usize = std::mem::size_of::<Pixel>();
    if !bytes.len().is_multiple_of(PIXEL_BYTES) {
        return Err(GpuError::ComputeExecution(
            "GPU readback size is not a whole number of pixels".to_owned(),
        ));
    }
    Ok(bytes
        .as_chunks::<PIXEL_BYTES>()
        .0
        .iter()
        .map(|bytes| {
            std::array::from_fn(|channel| {
                let offset = channel * std::mem::size_of::<f32>();
                f32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap())
            })
        })
        .collect())
}

fn color_matrix_params(matrix: [[f32; 4]; 4], offset: [f32; 4], count: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(112);
    for row in matrix {
        for value in row {
            bytes.extend_from_slice(&value.to_ne_bytes());
        }
    }
    for value in offset {
        bytes.extend_from_slice(&value.to_ne_bytes());
    }
    bytes.extend_from_slice(&count.to_ne_bytes());
    bytes.resize(112, 0);
    bytes
}
