use std::collections::{HashMap, VecDeque};
use std::io::Cursor;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use png::{BitDepth, ColorType, Encoder};
use rawweave_color::{DisplayTransform, SrgbDisplayTransform, WorkingSpace};
use rawweave_image::{Dimensions, Image, Region};
use rawweave_node_api::{EvaluationContext, Value};
use rawweave_project::EditorCore;
use rawweave_rendering::{PreviewQuality, TileCoord, TileRequest};
use serde::{Deserialize, Serialize};
use tauri::http::{Request, Response};

const PREVIEW_MIME_TYPE: &str = "image/png";

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewQualityRequest {
    Draft,
    Preview,
    Final,
}

impl From<PreviewQualityRequest> for PreviewQuality {
    fn from(value: PreviewQualityRequest) -> Self {
        match value {
            PreviewQualityRequest::Draft => Self::Draft,
            PreviewQualityRequest::Preview => Self::Preview,
            PreviewQualityRequest::Final => Self::Final,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaskDisplayRequest {
    #[default]
    Grayscale,
    Overlay,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRegionRequest {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl From<PreviewRegionRequest> for Region {
    fn from(value: PreviewRegionRequest) -> Self {
        Self::new(value.x, value.y, value.width, value.height)
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTileRequest {
    pub x: u32,
    pub y: u32,
}

impl From<PreviewTileRequest> for TileCoord {
    fn from(value: PreviewTileRequest) -> Self {
        Self::new(value.x, value.y)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    pub request_id: String,
    pub revision: u64,
    pub node_id: String,
    pub output_port: String,
    pub quality: PreviewQualityRequest,
    pub region: PreviewRegionRequest,
    pub tile: PreviewTileRequest,
    pub mip: u8,
    pub mask_display: MaskDisplayRequest,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewMetadata {
    pub request_id: String,
    pub revision: u64,
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub full_width: u32,
    pub full_height: u32,
    pub mime_type: &'static str,
    pub origin_x: u32,
    pub origin_y: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenImageMetadata {
    pub kind: crate::SourceKind,
    pub width: u32,
    pub height: u32,
    pub revision: u64,
    pub metadata: Option<crate::OpenMetadataSummary>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewProgressEvent {
    pub request_id: String,
    pub revision: u64,
    pub progress: f32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewErrorEvent {
    pub request_id: String,
    pub revision: u64,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCancelledEvent {
    pub request_id: String,
    pub revision: u64,
}

#[derive(Clone, Debug)]
struct StoredPreview {
    revision: u64,
    bytes: Vec<u8>,
}

pub struct PreviewStore {
    previews: Mutex<HashMap<String, StoredPreview>>,
    order: Mutex<VecDeque<String>>,
    max_entries: usize,
    max_bytes: usize,
    bytes: Mutex<usize>,
}

impl Default for PreviewStore {
    fn default() -> Self {
        Self::with_limits(32, 64 * 1024 * 1024)
    }
}

impl PreviewStore {
    pub fn with_limits(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            previews: Mutex::new(HashMap::new()),
            order: Mutex::new(VecDeque::new()),
            max_entries,
            max_bytes,
            bytes: Mutex::new(0),
        }
    }

    pub fn len(&self) -> usize {
        self.previews
            .lock()
            .map(|previews| previews.len())
            .unwrap_or(0)
    }

    pub fn byte_len(&self) -> usize {
        self.bytes.lock().map(|bytes| *bytes).unwrap_or(0)
    }

    pub fn insert(&self, path: String, revision: u64, bytes: Vec<u8>) -> Result<(), String> {
        if self.max_entries == 0 || bytes.len() > self.max_bytes {
            return Err("preview exceeds the configured store budget".to_owned());
        }
        let mut previews = self
            .previews
            .lock()
            .map_err(|_| "preview store is unavailable".to_owned())?;
        let mut order = self
            .order
            .lock()
            .map_err(|_| "preview store is unavailable".to_owned())?;
        let mut total_bytes = self
            .bytes
            .lock()
            .map_err(|_| "preview store is unavailable".to_owned())?;
        if let Some(previous) = previews.remove(&path) {
            *total_bytes = total_bytes.saturating_sub(previous.bytes.len());
        }
        order.retain(|candidate| candidate != &path);
        *total_bytes = total_bytes.saturating_add(bytes.len());
        previews.insert(path.clone(), StoredPreview { revision, bytes });
        order.push_back(path);
        while previews.len() > self.max_entries || *total_bytes > self.max_bytes {
            let Some(oldest) = order.pop_front() else {
                break;
            };
            if let Some(previous) = previews.remove(&oldest) {
                *total_bytes = total_bytes.saturating_sub(previous.bytes.len());
            }
        }
        Ok(())
    }

    pub fn get(&self, path: &str) -> Option<Vec<u8>> {
        let bytes = self
            .previews
            .lock()
            .ok()
            .and_then(|previews| previews.get(path).map(|preview| preview.bytes.clone()));
        if bytes.is_some() {
            self.touch(path);
        }
        bytes
    }

    pub fn take(&self, path: &str) -> Option<Vec<u8>> {
        let bytes = self
            .previews
            .lock()
            .ok()
            .and_then(|mut previews| previews.remove(path).map(|preview| preview.bytes));
        if let Some(ref bytes) = bytes {
            if let Ok(mut total_bytes) = self.bytes.lock() {
                *total_bytes = total_bytes.saturating_sub(bytes.len());
            }
            if let Ok(mut order) = self.order.lock() {
                order.retain(|candidate| candidate != path);
            }
        }
        bytes
    }

    pub fn revision(&self, path: &str) -> Option<u64> {
        self.previews
            .lock()
            .ok()
            .and_then(|previews| previews.get(path).map(|preview| preview.revision))
    }

    pub fn remove(&self, path: &str) {
        let _ = self.take(path);
    }

    pub fn clear(&self) {
        if let Ok(mut previews) = self.previews.lock() {
            previews.clear();
        }
        if let Ok(mut order) = self.order.lock() {
            order.clear();
        }
        if let Ok(mut bytes) = self.bytes.lock() {
            *bytes = 0;
        }
    }

    fn touch(&self, path: &str) {
        if let Ok(mut order) = self.order.lock() {
            order.retain(|candidate| candidate != path);
            order.push_back(path.to_owned());
        }
    }
}

#[derive(Default)]
pub struct PreviewManager {
    pub store: PreviewStore,
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl PreviewManager {
    pub fn begin(&self, request_id: &str) -> Arc<AtomicBool> {
        if let Ok(mut jobs) = self.jobs.lock() {
            if let Some(token) = jobs.get(request_id) {
                // Keep a cancelled request cancelled until its worker cleans it up.
                // This is also the worker-side lookup after dispatch registration.
                return Arc::clone(token);
            }
            let token = Arc::new(AtomicBool::new(false));
            jobs.insert(request_id.to_owned(), Arc::clone(&token));
            return token;
        }
        Arc::new(AtomicBool::new(false))
    }

    pub fn is_cancelled(&self, request_id: &str) -> bool {
        self.jobs
            .lock()
            .ok()
            .and_then(|jobs| jobs.get(request_id).cloned())
            .is_some_and(|token| token.load(Ordering::Acquire))
    }

    pub fn cancel(&self, request_id: &str) -> bool {
        let cancelled = self
            .jobs
            .lock()
            .ok()
            .and_then(|jobs| jobs.get(request_id).cloned())
            .map(|token| {
                token.store(true, Ordering::Release);
                true
            })
            .unwrap_or(false);
        self.store.remove(&preview_path(request_id));
        cancelled
    }

    pub fn cancel_all(&self) {
        if let Ok(jobs) = self.jobs.lock() {
            for token in jobs.values() {
                token.store(true, Ordering::Release);
            }
        }
        self.store.clear();
    }

    pub fn discard(&self, request_id: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.remove(request_id);
        }
        self.store.remove(&preview_path(request_id));
    }

    pub fn release(&self, url: &str) -> Result<(), String> {
        let path = url
            .strip_prefix("rawweave-preview://localhost")
            .unwrap_or(url);
        if !path.starts_with("/preview/") {
            return Err("invalid preview URL".to_owned());
        }
        self.store.remove(path);
        Ok(())
    }

    pub fn finish_job(&self, request_id: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.remove(request_id);
        }
    }

    pub fn response(&self, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
        let path = request.uri().path();
        match self.store.take(path) {
            Some(bytes) => Response::builder()
                .status(200)
                .header("Content-Type", PREVIEW_MIME_TYPE)
                .header("Cache-Control", "no-store")
                .body(bytes)
                .expect("valid preview response"),
            None => Response::builder()
                .status(404)
                .header("Content-Type", "text/plain; charset=utf-8")
                .body(b"preview not found".to_vec())
                .expect("valid missing preview response"),
        }
    }
}

pub fn decode_image_file(path: impl AsRef<Path>) -> Result<Image, String> {
    let path = path.as_ref();
    let decoded = image::ImageReader::open(path)
        .map_err(|error| format!("could not open image '{}': {error}", path.display()))?
        .decode()
        .map_err(|error| format!("could not decode image '{}': {error}", path.display()))?;
    let rgba = decoded.to_rgba32f();
    Image::from_pixels(
        rgba.width(),
        rgba.height(),
        rgba.pixels().map(|pixel| pixel.0).collect(),
    )
    .map_err(|error| format!("could not create rawweave image: {error}"))
}

pub fn preview_path(request_id: &str) -> String {
    format!("/preview/{request_id}.png")
}

pub fn preview_url(request_id: &str) -> String {
    format!("rawweave-preview://localhost{}", preview_path(request_id))
}

pub fn encode_png(image: &Image) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(Cursor::new(&mut bytes), image.width(), image.height());
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|error| format!("could not encode preview PNG: {error}"))?;
    let rgba = image
        .pixels()
        .iter()
        .flat_map(|pixel| pixel.map(channel_to_byte))
        .collect::<Vec<_>>();
    writer
        .write_image_data(&rgba)
        .map_err(|error| format!("could not write preview PNG: {error}"))?;
    writer
        .finish()
        .map_err(|error| format!("could not finish preview PNG: {error}"))?;
    Ok(bytes)
}

pub fn select_preview_image(
    image: &Image,
    requested_region: Region,
    mip: u8,
) -> Result<Image, String> {
    let region = image
        .global_region()
        .intersection(requested_region)
        .ok_or_else(|| "preview region is outside the evaluated image".to_owned())?;
    let scale = 1_u32.checked_shl(u32::from(mip)).unwrap_or(u32::MAX);
    let width = region.width.saturating_add(scale.saturating_sub(1)) / scale;
    let height = region.height.saturating_add(scale.saturating_sub(1)) / scale;
    let mut pixels = Vec::with_capacity((width as usize).saturating_mul(height as usize));
    for y in 0..height {
        for x in 0..width {
            let source_x = region.x + (x.saturating_mul(scale)).min(region.width - 1);
            let source_y = region.y + (y.saturating_mul(scale)).min(region.height - 1);
            pixels.push(
                image
                    .pixel_global(source_x, source_y)
                    .ok_or_else(|| "preview region pixel is unavailable".to_owned())?,
            );
        }
    }
    Image::from_pixels_with_origin(
        Dimensions::new(width, height),
        (region.x, region.y),
        pixels,
        image.pixel_format(),
        image.color_metadata(),
    )
    .map_err(|error| format!("could not create preview region: {error}"))
}

fn channel_to_byte(value: f32) -> u8 {
    if !value.is_finite() {
        return 0;
    }
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn evaluation_context(source: &crate::SourceAsset) -> EvaluationContext {
    match source {
        crate::SourceAsset::Ordinary(image) => EvaluationContext::with_source_image(image.clone()),
        crate::SourceAsset::Raw { bytes, path } => EvaluationContext::default()
            .with_source_bytes(bytes.as_ref().clone())
            .with_source_path(path),
    }
}

const LABEL_PREVIEW_COLORS: [[f32; 3]; 8] = [
    [0.12, 0.42, 0.78],
    [0.92, 0.32, 0.18],
    [0.22, 0.68, 0.38],
    [0.72, 0.32, 0.78],
    [0.88, 0.66, 0.16],
    [0.14, 0.68, 0.72],
    [0.78, 0.24, 0.46],
    [0.46, 0.52, 0.2],
];

fn display_image(
    dimensions: Dimensions,
    origin: (u32, u32),
    pixels: Vec<[f32; 4]>,
) -> Result<Image, String> {
    Image::from_pixels_with_origin(
        dimensions,
        origin,
        pixels,
        Default::default(),
        Default::default(),
    )
    .map_err(|error| format!("could not create display preview image: {error}"))
}

fn mask_pixel(mask: &rawweave_image::Mask, x: u32, y: u32) -> f32 {
    mask.pixel_global(x, y).unwrap_or(0.0)
}

fn mask_preview_pixels(
    mask: &rawweave_image::Mask,
    mask_display: MaskDisplayRequest,
) -> (Dimensions, (u32, u32), Vec<[f32; 4]>) {
    let dimensions = mask.dimensions();
    let origin = mask.origin();
    let pixels = (0..dimensions.height)
        .flat_map(|y| {
            (0..dimensions.width).map(move |x| {
                let value = mask_pixel(mask, origin.0 + x, origin.1 + y);
                match mask_display {
                    MaskDisplayRequest::Grayscale => [value, value, value, 1.0],
                    MaskDisplayRequest::Overlay => [1.0, 0.25, 0.1, value],
                }
            })
        })
        .collect();
    (dimensions, origin, pixels)
}

fn union_regions<I>(regions: I) -> Result<Option<Region>, String>
where
    I: IntoIterator<Item = Region>,
{
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for region in regions {
        if region.width == 0 || region.height == 0 {
            continue;
        }
        let end_x = region
            .end_x()
            .ok_or_else(|| "spatial preview region x overflowed".to_owned())?;
        let end_y = region
            .end_y()
            .ok_or_else(|| "spatial preview region y overflowed".to_owned())?;
        bounds = Some(match bounds {
            Some((min_x, min_y, max_x, max_y)) => (
                min_x.min(region.x),
                min_y.min(region.y),
                max_x.max(end_x),
                max_y.max(end_y),
            ),
            None => (region.x, region.y, end_x, end_y),
        });
    }
    Ok(bounds.map(|(min_x, min_y, max_x, max_y)| {
        Region::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }))
}

fn mask_set_preview(
    masks: &rawweave_image::MaskSet,
    mask_display: MaskDisplayRequest,
) -> Result<Image, String> {
    let bounds = union_regions(masks.iter().map(|mask| mask.global_region()))?
        .ok_or_else(|| "cannot preview an empty MaskSet".to_owned())?;
    let mut pixels =
        Vec::with_capacity((bounds.width as usize).saturating_mul(bounds.height as usize));
    for y in 0..bounds.height {
        for x in 0..bounds.width {
            let global_x = bounds.x + x;
            let global_y = bounds.y + y;
            let mut strongest = 0.0;
            let mut strongest_index = 0;
            for (index, mask) in masks.iter().enumerate() {
                let value = mask_pixel(mask, global_x, global_y);
                if value > strongest {
                    strongest = value;
                    strongest_index = index;
                }
            }
            pixels.push(match mask_display {
                MaskDisplayRequest::Grayscale => [strongest, strongest, strongest, 1.0],
                MaskDisplayRequest::Overlay => {
                    let color = LABEL_PREVIEW_COLORS[strongest_index % LABEL_PREVIEW_COLORS.len()];
                    [color[0], color[1], color[2], strongest]
                }
            });
        }
    }
    display_image(bounds.dimensions(), (bounds.x, bounds.y), pixels)
}

fn label_map_preview(label_map: &rawweave_image::LabelMap) -> Result<Image, String> {
    let dimensions = label_map.dimensions();
    let origin = label_map.origin();
    let pixels = (0..dimensions.height)
        .flat_map(|y| {
            (0..dimensions.width).map(move |x| {
                let value = label_map
                    .pixel_global(origin.0 + x, origin.1 + y)
                    .unwrap_or(0);
                if value == 0 {
                    [0.0, 0.0, 0.0, 1.0]
                } else {
                    let color =
                        LABEL_PREVIEW_COLORS[(value as usize - 1) % LABEL_PREVIEW_COLORS.len()];
                    [color[0], color[1], color[2], 1.0]
                }
            })
        })
        .collect();
    display_image(dimensions, origin, pixels)
}

fn depth_map_preview(depth_map: &rawweave_image::DepthMap) -> Result<Image, String> {
    let dimensions = depth_map.dimensions();
    let origin = depth_map.origin();
    let (minimum, maximum) = depth_map
        .values()
        .iter()
        .copied()
        .fold(None, |range: Option<(f64, f64)>, value| {
            let value = f64::from(value);
            Some(match range {
                Some((minimum, maximum)) => (minimum.min(value), maximum.max(value)),
                None => (value, value),
            })
        })
        .ok_or_else(|| "cannot preview an empty DepthMap".to_owned())?;
    let span = maximum - minimum;
    let pixels = (0..dimensions.height)
        .flat_map(|y| {
            (0..dimensions.width).map(move |x| {
                let value = f64::from(
                    depth_map
                        .pixel_global(origin.0 + x, origin.1 + y)
                        .unwrap_or(minimum as f32),
                );
                let normalized = if span == 0.0 {
                    0.5
                } else {
                    ((value - minimum) / span) as f32
                };
                [normalized, normalized, normalized, 1.0]
            })
        })
        .collect();
    display_image(dimensions, origin, pixels)
}

fn region_set_preview(region_set: &rawweave_image::RegionSet) -> Result<Image, String> {
    let Some(bounds) = union_regions(region_set.regions().iter().copied())? else {
        return display_image(Dimensions::new(1, 1), (0, 0), vec![[0.0, 0.0, 0.0, 0.0]]);
    };
    let mut pixels =
        vec![[0.0, 0.0, 0.0, 0.0]; (bounds.width as usize).saturating_mul(bounds.height as usize)];
    for (index, region) in region_set.regions().iter().enumerate() {
        let Some(region) = region.intersection(bounds) else {
            continue;
        };
        let color = LABEL_PREVIEW_COLORS[index % LABEL_PREVIEW_COLORS.len()];
        for y in region.y..region.y + region.height {
            for x in region.x..region.x + region.width {
                let local_x = (x - bounds.x) as usize;
                let local_y = (y - bounds.y) as usize;
                let pixel_index = local_y * bounds.width as usize + local_x;
                let border = x == region.x
                    || y == region.y
                    || x + 1 == region.x + region.width
                    || y + 1 == region.y + region.height;
                pixels[pixel_index] = [
                    color[0],
                    color[1],
                    color[2],
                    if border { 1.0 } else { 0.35 },
                ];
            }
        }
    }
    display_image(bounds.dimensions(), (bounds.x, bounds.y), pixels)
}

fn color_value_to_image(value: Value, mask_display: MaskDisplayRequest) -> Result<Image, String> {
    match value {
        Value::Image(image) => Ok(image),
        Value::Mask(mask) => {
            let (dimensions, origin, pixels) = mask_preview_pixels(&mask, mask_display);
            display_image(dimensions, origin, pixels)
        }
        Value::MaskSet(masks) => mask_set_preview(&masks, mask_display),
        Value::LabelMap(label_map) => label_map_preview(&label_map),
        Value::ConfidenceMap(confidence_map) => {
            let (dimensions, origin, pixels) =
                mask_preview_pixels(confidence_map.mask(), MaskDisplayRequest::Grayscale);
            display_image(dimensions, origin, pixels)
        }
        Value::DepthMap(depth_map) => depth_map_preview(&depth_map),
        Value::RegionSet(region_set) => region_set_preview(&region_set),
        Value::DisplayRGB(display) => display_image(
            display.dimensions(),
            (0, 0),
            display
                .pixels()
                .iter()
                .map(|pixel| [pixel[0], pixel[1], pixel[2], 1.0])
                .collect(),
        ),
        Value::SceneLinearRGB(scene) => {
            let display = SrgbDisplayTransform
                .transform(&scene)
                .or_else(|_| {
                    SrgbDisplayTransform.transform(&scene.with_working_space(WorkingSpace::Srgb))
                })
                .map_err(|error| {
                    format!("could not convert scene preview to display RGB: {error}")
                })?;
            display_image(
                display.dimensions(),
                (0, 0),
                display
                    .pixels()
                    .iter()
                    .map(|pixel| [pixel[0], pixel[1], pixel[2], 1.0])
                    .collect(),
            )
        }
        other => Err(format!(
            "preview output has unsupported data type {}",
            other.data_type()
        )),
    }
}

pub fn render_preview(
    manager: &PreviewManager,
    editor: &EditorCore,
    current_editor: &Arc<Mutex<EditorCore>>,
    source: Option<crate::SourceAsset>,
    request: PreviewRequest,
) -> Result<PreviewMetadata, String> {
    manager.begin(&request.request_id);
    let result = (|| {
        if manager.is_cancelled(&request.request_id) {
            return Err("preview cancelled".to_owned());
        }
        let current_revision = current_editor
            .lock()
            .map_err(|_| "editor state is unavailable".to_owned())?
            .graph()
            .revision();
        if request.revision != current_revision {
            return Err(format!(
                "stale preview request: graph revision is {current_revision}, requested {}",
                request.revision
            ));
        }
        let source = source.ok_or_else(|| {
            "preview source image unavailable; open an image before rendering".to_owned()
        })?;
        let full_frame = editor
            .evaluate(
                &request.node_id,
                &request.output_port,
                evaluation_context(&source),
            )
            .map_err(|error| format!("preview full-frame evaluation failed: {error}"))?;
        let full_frame =
            color_value_to_image(full_frame, request.mask_display).map_err(|error| {
                format!(
                    "preview output '{}:{}' is not displayable: {error}",
                    request.node_id, request.output_port
                )
            })?;
        let full_width = full_frame.width();
        let full_height = full_frame.height();
        let full_origin = full_frame.origin();
        let requested_region = Region::new(
            full_origin
                .0
                .checked_add(request.region.x)
                .ok_or_else(|| "preview region x overflowed the output origin".to_owned())?,
            full_origin
                .1
                .checked_add(request.region.y)
                .ok_or_else(|| "preview region y overflowed the output origin".to_owned())?,
            request.region.width,
            request.region.height,
        );
        if manager.is_cancelled(&request.request_id) {
            return Err("preview cancelled".to_owned());
        }
        let context = evaluation_context(&source).with_tile_request(TileRequest::new(
            requested_region,
            request.tile.into(),
            request.mip,
            request.quality.into(),
        ));
        let value = editor
            .evaluate(&request.node_id, &request.output_port, context)
            .map_err(|error| format!("preview evaluation failed: {error}"))?;
        if manager.is_cancelled(&request.request_id) {
            return Err("preview cancelled".to_owned());
        }
        let image = color_value_to_image(value, request.mask_display).map_err(|error| {
            format!(
                "preview output '{}:{}' is not displayable: {error}",
                request.node_id, request.output_port
            )
        })?;
        let image = select_preview_image(&image, requested_region, request.mip)?;
        let latest_revision = current_editor
            .lock()
            .map_err(|_| "editor state is unavailable".to_owned())?
            .graph()
            .revision();
        if request.revision != latest_revision {
            return Err(format!(
                "stale preview result: graph revision is {latest_revision}, requested {}",
                request.revision
            ));
        }
        let bytes = encode_png(&image)?;
        if manager.is_cancelled(&request.request_id) {
            return Err("preview cancelled".to_owned());
        }
        manager
            .store
            .insert(preview_path(&request.request_id), request.revision, bytes)?;
        if manager.is_cancelled(&request.request_id) {
            manager.store.remove(&preview_path(&request.request_id));
            return Err("preview cancelled".to_owned());
        }
        Ok(PreviewMetadata {
            request_id: request.request_id.clone(),
            revision: request.revision,
            url: preview_url(&request.request_id),
            width: image.width(),
            height: image.height(),
            full_width,
            full_height,
            mime_type: PREVIEW_MIME_TYPE,
            origin_x: image.origin().0,
            origin_y: image.origin().1,
        })
    })();
    match result {
        Ok(metadata) => {
            manager.finish_job(&request.request_id);
            Ok(metadata)
        }
        Err(error) => {
            manager.discard(&request.request_id);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawweave_image::{
        ConfidenceMap, DepthMap, Dimensions, Image, LabelMap, Mask, MaskSet, Region, RegionSet,
    };

    #[test]
    fn encodes_float_rgba_pixels_as_a_browser_png() {
        let image = Image::from_pixels(
            2,
            1,
            vec![[0.0, 0.5, 1.0, 1.0], [1.2, -0.1, f32::NAN, 0.25]],
        )
        .unwrap();
        let bytes = encode_png(&image).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");

        let decoder = png::Decoder::new(Cursor::new(bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut output = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut output).unwrap();
        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!(&output[..8], &[0, 128, 255, 255, 255, 0, 0, 64]);
    }

    #[test]
    fn converts_spatial_checkpoint_values_into_inspectable_preview_images() {
        let dimensions = Dimensions::new(2, 1);
        let origin = (4, 7);
        let mask = Mask::from_values_with_origin(dimensions, origin, vec![0.0, 0.75]).unwrap();
        let labels = std::collections::BTreeMap::from([
            ("background".to_owned(), 0_u16),
            ("sky".to_owned(), 1_u16),
        ]);
        let values = [
            (
                Value::MaskSet(MaskSet::new(vec![mask.clone()])),
                dimensions,
                origin,
            ),
            (
                Value::LabelMap(
                    LabelMap::from_values_with_labels(dimensions, origin, vec![0, 1], labels)
                        .unwrap(),
                ),
                dimensions,
                origin,
            ),
            (
                Value::ConfidenceMap(ConfidenceMap::from_mask(mask)),
                dimensions,
                origin,
            ),
            (
                Value::DepthMap(DepthMap::from_values(dimensions, origin, vec![1.0, 2.0]).unwrap()),
                dimensions,
                origin,
            ),
            (
                Value::RegionSet(RegionSet::new(vec![Region::new(4, 7, 2, 1)])),
                dimensions,
                origin,
            ),
        ];

        for (value, expected_dimensions, expected_origin) in values {
            let image = color_value_to_image(value, MaskDisplayRequest::Grayscale).unwrap();

            assert_eq!(image.dimensions(), expected_dimensions);
            assert_eq!(image.origin(), expected_origin);
            assert!(image.pixels().iter().any(|pixel| pixel[3] > 0.0));
        }
    }

    #[test]
    fn stores_preview_bytes_with_the_graph_revision() {
        let store = PreviewStore::default();
        store
            .insert("/preview/request.png".to_owned(), 12, vec![1, 2, 3])
            .unwrap();
        assert_eq!(store.revision("/preview/request.png"), Some(12));
        assert_eq!(store.get("/preview/request.png"), Some(vec![1, 2, 3]));
        store.remove("/preview/request.png");
        assert!(store.get("/preview/request.png").is_none());
    }

    #[test]
    fn evicts_least_recently_used_previews_by_count_and_bytes() {
        let store = PreviewStore::with_limits(2, 5);
        store
            .insert("/preview/a.png".to_owned(), 1, vec![1, 2, 3])
            .unwrap();
        store
            .insert("/preview/b.png".to_owned(), 1, vec![4, 5])
            .unwrap();
        assert_eq!(store.get("/preview/a.png"), Some(vec![1, 2, 3]));

        store
            .insert("/preview/c.png".to_owned(), 1, vec![6, 7])
            .unwrap();

        assert!(store.get("/preview/b.png").is_none());
        assert_eq!(store.get("/preview/a.png"), Some(vec![1, 2, 3]));
        assert_eq!(store.get("/preview/c.png"), Some(vec![6, 7]));
        assert_eq!(store.len(), 2);
        assert_eq!(store.byte_len(), 5);
    }

    #[test]
    fn taking_a_preview_releases_its_bytes_without_cloning_them_for_protocol_reads() {
        let store = PreviewStore::default();
        store
            .insert("/preview/request.png".to_owned(), 12, vec![1, 2, 3])
            .unwrap();

        assert_eq!(store.take("/preview/request.png"), Some(vec![1, 2, 3]));
        assert!(store.get("/preview/request.png").is_none());
        assert_eq!(store.byte_len(), 0);
    }

    #[test]
    fn releasing_a_preview_url_removes_its_stored_bytes() {
        let manager = PreviewManager::default();
        let path = preview_path("request");
        manager
            .store
            .insert(path.clone(), 12, vec![1, 2, 3])
            .unwrap();

        manager.release(&preview_url("request")).unwrap();

        assert!(manager.store.get(&path).is_none());
        assert_eq!(manager.store.byte_len(), 0);
        assert!(manager.release("not-a-preview-url").is_err());
    }

    #[test]
    fn selects_the_requested_region_and_mip_for_preview_encoding() {
        let image = Image::new(4, 3).unwrap();

        let region =
            select_preview_image(&image, rawweave_image::Region::new(1, 1, 2, 2), 1).unwrap();

        assert_eq!(region.width(), 1);
        assert_eq!(region.height(), 1);
        assert_eq!(region.origin(), (1, 1));
    }

    #[test]
    fn opens_a_generated_png_and_renders_it_through_the_image_graph() {
        let path = std::env::temp_dir().join(format!(
            "rawweave-preview-open-render-{}.png",
            std::process::id()
        ));
        let mut png_bytes = Vec::new();
        let mut encoder = png::Encoder::new(Cursor::new(&mut png_bytes), 2, 2);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ])
            .unwrap();
        writer.finish().unwrap();
        std::fs::write(&path, png_bytes).unwrap();

        let source = decode_image_file(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!((source.width(), source.height()), (2, 2));

        let mut editor = EditorCore::default();
        editor.add_node("input", "core.image-input").unwrap();
        editor.add_node("output", "core.output").unwrap();
        editor.connect("input", "image", "output", "image").unwrap();
        let current_editor = Arc::new(Mutex::new(editor.clone()));
        let manager = PreviewManager::default();
        let request = PreviewRequest {
            request_id: "open-render".to_owned(),
            revision: editor.graph().revision(),
            node_id: "output".to_owned(),
            output_port: "image".to_owned(),
            quality: PreviewQualityRequest::Preview,
            region: PreviewRegionRequest {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            tile: PreviewTileRequest { x: 0, y: 0 },
            mip: 0,
            mask_display: MaskDisplayRequest::Grayscale,
        };

        let metadata = render_preview(
            &manager,
            &editor,
            &current_editor,
            Some(crate::SourceAsset::Ordinary(source)),
            request,
        )
        .unwrap();

        assert_eq!((metadata.width, metadata.height), (2, 2));
        assert_eq!((metadata.full_width, metadata.full_height), (2, 2));
        assert!(manager.store.get(&preview_path("open-render")).is_some());
    }

    #[test]
    fn renders_raw_display_output_from_source_bytes_as_png() {
        let decoder = rawweave_raw::DeterministicDecoder::new(
            rawweave_raw::DeterministicCorpus::bayer_12_bit(),
        );
        let mut editor = rawweave_project::EditorCore::new_with_raw_decoder(decoder);
        crate::build_raw_workflow(&mut editor).unwrap();
        let current_editor = Arc::new(Mutex::new(editor.clone()));
        let manager = PreviewManager::default();
        let request = PreviewRequest {
            request_id: "raw-open-render".to_owned(),
            revision: editor.graph().revision(),
            node_id: "display-transform".to_owned(),
            output_port: "display".to_owned(),
            quality: PreviewQualityRequest::Preview,
            region: PreviewRegionRequest {
                x: 0,
                y: 0,
                width: 4,
                height: 2,
            },
            tile: PreviewTileRequest { x: 0, y: 0 },
            mip: 0,
            mask_display: MaskDisplayRequest::Grayscale,
        };

        let metadata = render_preview(
            &manager,
            &editor,
            &current_editor,
            Some(crate::SourceAsset::Raw {
                bytes: Arc::new(vec![1, 2, 3]),
                path: std::path::PathBuf::from("fixture.dng"),
            }),
            request,
        )
        .unwrap();

        assert_eq!((metadata.full_width, metadata.full_height), (4, 2));
        let bytes = manager.store.get(&preview_path("raw-open-render")).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn ordinary_image_preview_still_uses_the_existing_source_path() {
        let source = Image::new(4, 3).unwrap();
        let manager = PreviewManager::default();

        let cases = [
            ("exposure", "core.exposure", vec![("exposure", 1.0_f32)]),
            (
                "crop",
                "core.crop",
                vec![("x", 1.0), ("y", 1.0), ("width", 2.0), ("height", 2.0)],
            ),
            (
                "resize",
                "core.resize",
                vec![("width", 8.0), ("height", 6.0)],
            ),
        ];

        for (request_id, type_id, parameters) in cases {
            let mut editor = EditorCore::default();
            editor.add_node("input", "core.image-input").unwrap();
            editor.add_node("target", type_id).unwrap();
            editor.connect("input", "image", "target", "image").unwrap();
            for (parameter_id, value) in parameters {
                editor
                    .set_node_parameter("target", parameter_id, value.into())
                    .unwrap();
            }
            let revision = editor.graph().revision();
            let request = PreviewRequest {
                request_id: request_id.to_owned(),
                revision,
                node_id: "target".to_owned(),
                output_port: "image".to_owned(),
                quality: PreviewQualityRequest::Preview,
                region: match request_id {
                    "resize" => PreviewRegionRequest {
                        x: 2,
                        y: 1,
                        width: 4,
                        height: 3,
                    },
                    _ => PreviewRegionRequest {
                        x: 0,
                        y: 0,
                        width: 2,
                        height: 2,
                    },
                },
                tile: PreviewTileRequest { x: 0, y: 0 },
                mip: 0,
                mask_display: MaskDisplayRequest::Grayscale,
            };
            let current_editor = Arc::new(Mutex::new(editor.clone()));
            let metadata = render_preview(
                &manager,
                &editor,
                &current_editor,
                Some(crate::SourceAsset::Ordinary(source.clone())),
                request,
            )
            .unwrap();

            match request_id {
                "exposure" => {
                    assert_eq!((metadata.full_width, metadata.full_height), (4, 3));
                    assert_eq!((metadata.width, metadata.height), (2, 2));
                }
                "crop" => {
                    assert_eq!((metadata.full_width, metadata.full_height), (2, 2));
                    assert_eq!((metadata.width, metadata.height), (2, 2));
                }
                "resize" => {
                    assert_eq!((metadata.full_width, metadata.full_height), (8, 6));
                    assert_eq!((metadata.width, metadata.height), (4, 3));
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn cancellation_marks_the_active_job_and_removes_its_stored_bytes() {
        let manager = PreviewManager::default();
        let token = manager.begin("request");
        manager
            .store
            .insert(preview_path("request"), 1, vec![1])
            .unwrap();

        assert!(manager.cancel("request"));
        assert!(token.load(Ordering::Acquire));
        assert!(manager.is_cancelled("request"));
        let same_request = manager.begin("request");
        assert!(Arc::ptr_eq(&token, &same_request));
        assert!(same_request.load(Ordering::Acquire));
        assert!(manager.store.get(&preview_path("request")).is_none());
        manager.finish_job("request");
        assert!(!manager.is_cancelled("request"));
        let replacement = manager.begin("request");
        assert!(!Arc::ptr_eq(&token, &replacement));
        assert!(!replacement.load(Ordering::Acquire));
    }

    #[test]
    fn queued_worker_keeps_cancellation_and_does_not_repopulate_the_store() {
        let mut editor = EditorCore::default();
        editor.add_node("input", "core.image-input").unwrap();
        editor.add_node("output", "core.output").unwrap();
        editor.connect("input", "image", "output", "image").unwrap();
        let revision = editor.graph().revision();
        let current_editor = Arc::new(Mutex::new(editor.clone()));
        let manager = PreviewManager::default();
        let request_id = "queued-worker";
        let request = PreviewRequest {
            request_id: request_id.to_owned(),
            revision,
            node_id: "output".to_owned(),
            output_port: "image".to_owned(),
            quality: PreviewQualityRequest::Preview,
            region: PreviewRegionRequest {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            tile: PreviewTileRequest { x: 0, y: 0 },
            mip: 0,
            mask_display: MaskDisplayRequest::Grayscale,
        };

        manager.begin(request_id);
        manager
            .store
            .insert(preview_path(request_id), revision, vec![1, 2, 3])
            .unwrap();
        manager.cancel_all();

        let error = render_preview(
            &manager,
            &editor,
            &current_editor,
            Some(crate::SourceAsset::Ordinary(Image::new(1, 1).unwrap())),
            request,
        )
        .unwrap_err();

        assert_eq!(error, "preview cancelled");
        assert!(manager.store.get(&preview_path(request_id)).is_none());
    }

    #[test]
    fn downloaded_common_image_dataset_decodes() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../test-data/images/common");
        let files = [
            "beatles-press-conference.jpg",
            "gracie-allen-portrait.jpg",
            "pngsuite-rgb8.png",
            "pngsuite-rgba8.png",
            "pngsuite-gray-alpha16.png",
            "pngsuite-indexed-transparency.png",
            "pngsuite-interlaced-odd.png",
            "pngsuite-1x1.png",
        ];

        for filename in files {
            let image = decode_image_file(root.join(filename).to_string_lossy().as_ref())
                .unwrap_or_else(|error| panic!("{filename} failed to decode: {error}"));
            assert!(image.width() > 0, "{filename} has zero width");
            assert!(image.height() > 0, "{filename} has zero height");
        }
    }

    #[test]
    fn serves_stored_png_bytes_over_the_preview_protocol() {
        let manager = PreviewManager::default();
        let path = preview_path("request");
        let bytes = vec![1, 2, 3];
        manager
            .store
            .insert(path.clone(), 12, bytes.clone())
            .unwrap();

        let request = Request::builder().uri(path).body(Vec::new()).unwrap();
        let response = manager.response(&request);

        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            PREVIEW_MIME_TYPE
        );
        assert_eq!(response.body(), &bytes);
    }

    #[test]
    fn renders_mask_values_as_grayscale_or_colored_overlay_pngs() {
        let source = Image::new(3, 1).unwrap();
        let mut editor = EditorCore::default();
        editor
            .add_node("mask", "core.mask-linear-gradient")
            .unwrap();
        let revision = editor.graph().revision();
        let current_editor = Arc::new(Mutex::new(editor.clone()));
        let manager = PreviewManager::default();
        let request = |request_id: &str, mask_display| PreviewRequest {
            request_id: request_id.to_owned(),
            revision,
            node_id: "mask".to_owned(),
            output_port: "mask".to_owned(),
            quality: PreviewQualityRequest::Preview,
            region: PreviewRegionRequest {
                x: 0,
                y: 0,
                width: 3,
                height: 1,
            },
            tile: PreviewTileRequest { x: 0, y: 0 },
            mip: 0,
            mask_display,
        };

        let grayscale = render_preview(
            &manager,
            &editor,
            &current_editor,
            Some(crate::SourceAsset::Ordinary(source.clone())),
            request("mask-gray", MaskDisplayRequest::Grayscale),
        )
        .unwrap();
        let overlay = render_preview(
            &manager,
            &editor,
            &current_editor,
            Some(crate::SourceAsset::Ordinary(source)),
            request("mask-overlay", MaskDisplayRequest::Overlay),
        )
        .unwrap();

        assert_eq!((grayscale.full_width, grayscale.full_height), (3, 1));
        assert_eq!((overlay.full_width, overlay.full_height), (3, 1));
        assert_eq!(overlay.mime_type, PREVIEW_MIME_TYPE);
        let gray_bytes = manager.store.get(&preview_path("mask-gray")).unwrap();
        let overlay_bytes = manager.store.get(&preview_path("mask-overlay")).unwrap();
        assert_ne!(gray_bytes, overlay_bytes);
        assert_eq!(&gray_bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&overlay_bytes[..8], b"\x89PNG\r\n\x1a\n");
    }
}
