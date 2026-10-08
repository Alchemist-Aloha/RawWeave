//! Bounded analysis of display-encoded BGRA previews, not scene-linear exposure or gamut measurement.
use crate::CancellationToken;
use rawweave_image::Dimensions;
use thiserror::Error;

pub const RESOLUTION: usize = 256;
pub const HIGHLIGHT: u8 = 1;
pub const SHADOW: u8 = 2;
const MAX_SAMPLES: u32 = 100_000;
const MAX_EDGE: u32 = 8192;
const MAX_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ScopeError {
    #[error("Invalid scope dimensions or buffer length")]
    InvalidBuffer,
    #[error("Scope analysis exceeds resource limits")]
    ResourceLimit,
    #[error("Scope analysis cancelled")]
    Cancelled,
}
#[derive(Clone, Copy, Debug)]
pub struct AnalysisOptions {
    pub max_samples: u32,
    pub inspector_size: u32,
}
impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            max_samples: MAX_SAMPLES,
            inspector_size: 256,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Raster {
    pub dimensions: Dimensions,
    pub bgra: Vec<u8>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScopeKind {
    #[default]
    Histogram,
    Waveform,
    Parade,
    Vectorscope,
    FalseColor,
    GamutWarning,
    PixelInspector,
    Zebra,
}
impl ScopeKind {
    pub const ALL: [Self; 8] = [
        Self::Histogram,
        Self::Waveform,
        Self::Parade,
        Self::Vectorscope,
        Self::FalseColor,
        Self::GamutWarning,
        Self::PixelInspector,
        Self::Zebra,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Histogram => "Histogram",
            Self::Waveform => "Waveform",
            Self::Parade => "RGB Parade",
            Self::Vectorscope => "Vectorscope",
            Self::FalseColor => "False Color",
            Self::GamutWarning => "Gamut Warning",
            Self::PixelInspector => "Pixel Inspector",
            Self::Zebra => "Zebra",
        }
    }
}
#[derive(Clone, Debug)]
pub struct PixelSample {
    pub x: u32,
    pub y: u32,
    pub rgba: [u8; 4],
    pub luma: f64,
}
impl PixelSample {
    pub fn hex(&self) -> String {
        format!(
            "#{:02x}{:02x}{:02x}",
            self.rgba[0], self.rgba[1], self.rgba[2]
        )
    }
}
#[derive(Debug)]
pub struct ScopeAnalysis {
    dimensions: Dimensions,
    pub sample_count: u32,
    /// RGB and display-code luma, in that order.
    pub histogram: [[u32; RESOLUTION]; 4],
    /// Column-major: column * 256 + channel level (0 black, 255 white).
    pub waveform: [Vec<u32>; 4],
    pub vectorscope: Vec<u32>,
    pub clipping: Vec<u8>,
    pub highlight_count: u32,
    pub shadow_count: u32,
    pub inspector: Raster,
    pub false_color: Raster,
    pub gamut_warning: Vec<u8>,
    pub gamut_warning_count: u32,
    pub zebra: Vec<u8>,
    pub zebra_count: u32,
}
fn byte_count(dimensions: Dimensions, edge: u32) -> Result<usize, ScopeError> {
    if dimensions.width == 0 || dimensions.height == 0 {
        return Err(ScopeError::InvalidBuffer);
    }
    if dimensions.width > edge || dimensions.height > edge {
        return Err(ScopeError::ResourceLimit);
    }
    dimensions
        .pixel_count()
        .map_err(|_| ScopeError::ResourceLimit)?
        .checked_mul(4)
        .filter(|bytes| *bytes <= MAX_BYTES)
        .ok_or(ScopeError::ResourceLimit)
}
fn luma(r: u8, g: u8, b: u8) -> f64 {
    0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b)
}
fn flags(r: u8, g: u8, b: u8) -> u8 {
    let max = r.max(g).max(b);
    (u8::from(max >= 250) * HIGHLIGHT) | (u8::from(max <= 5) * SHADOW)
}
fn pixel(data: &[u8], width: u32, x: u32, y: u32) -> Option<[u8; 4]> {
    let index = u64::from(y)
        .checked_mul(u64::from(width))?
        .checked_add(u64::from(x))?
        .checked_mul(4)?;
    let index = usize::try_from(index).ok()?;
    let [b, g, r, a] = <[u8; 4]>::try_from(data.get(index..index.checked_add(4)?)?).ok()?;
    Some([r, g, b, a])
}
fn endpoint(index: u32, from: u32, to: u32) -> u32 {
    let numerator = u64::from(index) * u64::from(to.saturating_sub(1));
    let denominator = u64::from(from.saturating_sub(1).max(1));
    u32::try_from((numerator + denominator / 2) / denominator).unwrap_or_default()
}
fn false_color(r: u8, g: u8, b: u8) -> [u8; 3] {
    const STOPS: [(f64, [u8; 3]); 7] = [
        (0.0, [26, 12, 72]),
        (0.2, [25, 104, 220]),
        (0.4, [20, 190, 200]),
        (0.6, [72, 210, 74]),
        (0.8, [245, 211, 42]),
        (0.95, [226, 65, 42]),
        (1.0, [255, 255, 255]),
    ];
    let level = luma(r, g, b) / 255.0;
    for pair in STOPS.windows(2) {
        let [(start, a), (end, b)] = pair else {
            continue;
        };
        if level > *end {
            continue;
        }
        let amount = ((level - start) / (end - start)).clamp(0.0, 1.0);
        return std::array::from_fn(|i| {
            (f64::from(a[i]) + (f64::from(b[i]) - f64::from(a[i])) * amount)
                .round()
                .clamp(0.0, 255.0) as u8
        });
    }
    [255; 3]
}
pub fn analyze_bgra(
    dimensions: Dimensions,
    data: &[u8],
    options: AnalysisOptions,
    cancellation: &CancellationToken,
) -> Result<ScopeAnalysis, ScopeError> {
    let bytes = byte_count(dimensions, MAX_EDGE)?;
    if data.len() != bytes {
        return Err(ScopeError::InvalidBuffer);
    }
    if options.max_samples == 0
        || options.max_samples > MAX_SAMPLES
        || options.inspector_size == 0
        || options.inspector_size > 1024
    {
        return Err(ScopeError::ResourceLimit);
    }
    if cancellation.is_cancelled() {
        return Err(ScopeError::Cancelled);
    }
    let mut step = 1_u32;
    while u64::from(dimensions.width.div_ceil(step)) * u64::from(dimensions.height.div_ceil(step))
        > u64::from(options.max_samples)
    {
        step += 1;
    }
    let scale = (f64::from(options.inspector_size) / f64::from(dimensions.width))
        .min(f64::from(options.inspector_size) / f64::from(dimensions.height))
        .min(1.0);
    let inspector_dimensions = Dimensions::new(
        (f64::from(dimensions.width) * scale).round().max(1.0) as u32,
        (f64::from(dimensions.height) * scale).round().max(1.0) as u32,
    );
    let inspector_bytes = byte_count(inspector_dimensions, 1024)?;
    let mut result = ScopeAnalysis {
        dimensions,
        sample_count: 0,
        histogram: [[0; RESOLUTION]; 4],
        waveform: std::array::from_fn(|_| vec![0; RESOLUTION * RESOLUTION]),
        vectorscope: vec![0; RESOLUTION * RESOLUTION],
        clipping: vec![0; bytes / 4],
        highlight_count: 0,
        shadow_count: 0,
        inspector: Raster {
            dimensions: inspector_dimensions,
            bgra: Vec::with_capacity(inspector_bytes),
        },
        false_color: Raster {
            dimensions: inspector_dimensions,
            bgra: Vec::with_capacity(inspector_bytes),
        },
        gamut_warning: Vec::with_capacity(inspector_bytes / 4),
        gamut_warning_count: 0,
        zebra: Vec::with_capacity(inspector_bytes / 4),
        zebra_count: 0,
    };
    let step = usize::try_from(step).map_err(|_| ScopeError::ResourceLimit)?;
    for y in (0..dimensions.height).step_by(step) {
        if cancellation.is_cancelled() {
            return Err(ScopeError::Cancelled);
        }
        for x in (0..dimensions.width).step_by(step) {
            let [r, g, b, _] =
                pixel(data, dimensions.width, x, y).ok_or(ScopeError::InvalidBuffer)?;
            let gray = luma(r, g, b);
            let column = usize::try_from(u64::from(x) * 256 / u64::from(dimensions.width))
                .map_err(|_| ScopeError::ResourceLimit)?;
            for ((histogram, waveform), level) in
                result.histogram.iter_mut().zip(&mut result.waveform).zip([
                    usize::from(r),
                    usize::from(g),
                    usize::from(b),
                    gray.round() as usize,
                ])
            {
                if let Some(bin) = histogram.get_mut(level) {
                    *bin += 1;
                }
                if let Some(bin) = waveform.get_mut(column * 256 + level) {
                    *bin += 1;
                }
            }
            let chroma_x = (((f64::from(b) - gray) / 255.0 + 0.5) * 255.0)
                .round()
                .clamp(0.0, 255.0) as usize;
            let chroma_y = ((0.5 - (f64::from(r) - gray) / 255.0) * 255.0)
                .round()
                .clamp(0.0, 255.0) as usize;
            if let Some(bin) = result.vectorscope.get_mut(chroma_y * 256 + chroma_x) {
                *bin += 1;
            }
            result.sample_count += 1;
        }
    }
    let width = usize::try_from(dimensions.width).map_err(|_| ScopeError::ResourceLimit)?;
    for (index, (pixel, mask)) in data
        .as_chunks::<4>()
        .0
        .iter()
        .zip(&mut result.clipping)
        .enumerate()
    {
        if index % width == 0 && cancellation.is_cancelled() {
            return Err(ScopeError::Cancelled);
        }
        let [b, g, r, _] = *pixel;
        *mask = flags(r, g, b);
        result.highlight_count += u32::from(*mask & HIGHLIGHT != 0);
        result.shadow_count += u32::from(*mask & SHADOW != 0);
    }
    for y in 0..inspector_dimensions.height {
        if cancellation.is_cancelled() {
            return Err(ScopeError::Cancelled);
        }
        for x in 0..inspector_dimensions.width {
            let sx = endpoint(x, inspector_dimensions.width, dimensions.width);
            let sy = endpoint(y, inspector_dimensions.height, dimensions.height);
            let [r, g, b, a] =
                pixel(data, dimensions.width, sx, sy).ok_or(ScopeError::InvalidBuffer)?;
            result.inspector.bgra.extend([b, g, r, a]);
            let [fr, fg, fb] = false_color(r, g, b);
            result.false_color.bgra.extend([fb, fg, fr, 255]);
            // Display saturation heuristic only: this cannot detect out-of-gamut scene values after clipping.
            let gamut = u8::from(r.max(g).max(b) >= 250 && r.min(g).min(b) < 250);
            result.gamut_warning.push(gamut);
            result.gamut_warning_count += u32::from(gamut);
            let zebra = flags(r, g, b);
            result.zebra.push(zebra);
            result.zebra_count += u32::from(zebra != 0);
        }
    }
    Ok(result)
}
impl ScopeAnalysis {
    pub fn dimensions(&self) -> Dimensions {
        self.dimensions
    }
    pub fn sample_pixel(&self, x: u32, y: u32) -> Option<PixelSample> {
        let x = x.min(self.dimensions.width.saturating_sub(1));
        let y = y.min(self.dimensions.height.saturating_sub(1));
        let ix = endpoint(x, self.dimensions.width, self.inspector.dimensions.width);
        let iy = u32::try_from(
            u64::from(y) * u64::from(self.inspector.dimensions.height)
                / u64::from(self.dimensions.height),
        )
        .ok()?
        .min(self.inspector.dimensions.height.saturating_sub(1));
        let rgba = pixel(
            &self.inspector.bgra,
            self.inspector.dimensions.width,
            ix,
            iy,
        )?;
        Some(PixelSample {
            x,
            y,
            rgba,
            luma: luma(rgba[0], rgba[1], rgba[2]),
        })
    }
    pub fn clipping_overlay(&self) -> Result<Raster, ScopeError> {
        let bytes = byte_count(self.dimensions, MAX_EDGE)?;
        if self.clipping.len() != bytes / 4 {
            return Err(ScopeError::InvalidBuffer);
        }
        let mut bgra = Vec::with_capacity(bytes);
        for flags in &self.clipping {
            bgra.extend(match *flags {
                0 => [0, 0, 0, 0],
                SHADOW => [255, 124, 34, 170],
                _ => [52, 52, 255, 170],
            });
        }
        Ok(Raster {
            dimensions: self.dimensions,
            bgra,
        })
    }
    /// Source-over the clipping marks in straight-alpha display codes, for display comparison.
    pub fn overlay_clipping(&self, source: &[u8]) -> Result<Raster, ScopeError> {
        let mut overlay = self.clipping_overlay()?;
        if source.len() != overlay.bgra.len() {
            return Err(ScopeError::InvalidBuffer);
        }
        for (output, source) in overlay
            .bgra
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(source.as_chunks::<4>().0)
        {
            let mark_alpha = f64::from(output[3]) / 255.0;
            let source_alpha = f64::from(source[3]) / 255.0;
            let alpha = mark_alpha + source_alpha * (1.0 - mark_alpha);
            for (channel, value) in output.iter_mut().take(3).zip(source) {
                *channel = if alpha > 0.0 {
                    ((f64::from(*channel) * mark_alpha
                        + f64::from(*value) * source_alpha * (1.0 - mark_alpha))
                        / alpha)
                        .round()
                        .clamp(0.0, 255.0) as u8
                } else {
                    0
                };
            }
            output[3] = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        Ok(overlay)
    }
    pub fn render(&self, kind: ScopeKind, dimensions: Dimensions) -> Result<Raster, ScopeError> {
        let mut raster = Raster::ground(dimensions)?;
        match kind {
            ScopeKind::Histogram => {
                let maximum = self
                    .histogram
                    .iter()
                    .flatten()
                    .copied()
                    .max()
                    .unwrap_or(1)
                    .max(1);
                for (channel, color) in self.histogram.iter().zip([
                    [239, 115, 125],
                    [124, 222, 156],
                    [126, 175, 255],
                    [240, 240, 240],
                ]) {
                    let mut previous = None;
                    for (index, value) in channel.iter().enumerate() {
                        let x = (index as f64 / 255.0
                            * f64::from(dimensions.width.saturating_sub(1)))
                        .round() as i32;
                        let y = (f64::from(dimensions.height.saturating_sub(1))
                            - (f64::from(*value) / f64::from(maximum))
                                * f64::from(dimensions.height.saturating_sub(8))
                            - 4.0)
                            .round()
                            .max(0.0) as i32;
                        if let Some(start) = previous {
                            raster.line(start, (x, y), color);
                        }
                        previous = Some((x, y));
                    }
                }
            }
            ScopeKind::Waveform => {
                if let Some(channel) = self.waveform.get(3) {
                    raster.density(channel, 1, 0, [240; 3], true);
                }
            }
            ScopeKind::Parade => {
                for (index, (channel, color)) in self
                    .waveform
                    .iter()
                    .take(3)
                    .zip([[239, 115, 125], [124, 222, 156], [126, 175, 255]])
                    .enumerate()
                {
                    raster.density(channel, 3, index as u32, color, true);
                }
            }
            ScopeKind::Vectorscope => {
                let cx = f64::from(dimensions.width) / 2.0;
                let cy = f64::from(dimensions.height) / 2.0;
                let radius = f64::from(dimensions.width.min(dimensions.height)) * 0.4;
                let mut previous = None;
                for step in 0..=256 {
                    let angle = f64::from(step) * std::f64::consts::TAU / 256.0;
                    let point = (
                        (cx + radius * angle.cos()).round() as i32,
                        (cy + radius * angle.sin()).round() as i32,
                    );
                    if let Some(start) = previous {
                        raster.line(start, point, [58; 3]);
                    }
                    previous = Some(point);
                }
                raster.density(&self.vectorscope, 1, 0, [240; 3], false);
            }
            ScopeKind::FalseColor | ScopeKind::GamutWarning | ScopeKind::Zebra => {
                for y in 0..dimensions.height {
                    for x in 0..dimensions.width {
                        let sx = u32::try_from(
                            u64::from(x) * u64::from(self.inspector.dimensions.width)
                                / u64::from(dimensions.width),
                        )
                        .map_err(|_| ScopeError::ResourceLimit)?;
                        let sy = u32::try_from(
                            u64::from(y) * u64::from(self.inspector.dimensions.height)
                                / u64::from(dimensions.height),
                        )
                        .map_err(|_| ScopeError::ResourceLimit)?;
                        let index = usize::try_from(
                            u64::from(sy) * u64::from(self.inspector.dimensions.width)
                                + u64::from(sx),
                        )
                        .map_err(|_| ScopeError::ResourceLimit)?;
                        let color = match kind {
                            ScopeKind::FalseColor => pixel(
                                &self.false_color.bgra,
                                self.false_color.dimensions.width,
                                sx,
                                sy,
                            )
                            .map(|p| [p[0], p[1], p[2]]),
                            ScopeKind::GamutWarning
                                if self.gamut_warning.get(index).is_some_and(|v| *v != 0) =>
                            {
                                Some([217, 74, 56])
                            }
                            ScopeKind::Zebra if self.zebra.get(index).is_some_and(|v| *v != 0) => {
                                Some([232, 163, 61])
                            }
                            _ => None,
                        };
                        if let Some(color) = color {
                            raster.set(x, y, color);
                        }
                    }
                }
            }
            ScopeKind::PixelInspector => {}
        }
        Ok(raster)
    }
}
impl Raster {
    fn ground(dimensions: Dimensions) -> Result<Self, ScopeError> {
        let bytes = byte_count(dimensions, 1024)?;
        let mut bgra = vec![20; bytes];
        for pixel in bgra.as_chunks_mut::<4>().0 {
            if let Some(alpha) = pixel.get_mut(3) {
                *alpha = 255;
            }
        }
        Ok(Self { dimensions, bgra })
    }
    fn set(&mut self, x: u32, y: u32, [r, g, b]: [u8; 3]) {
        if x >= self.dimensions.width || y >= self.dimensions.height {
            return;
        }
        let index = u64::from(y) * u64::from(self.dimensions.width) + u64::from(x);
        if let Some(index) = index.checked_mul(4).and_then(|n| usize::try_from(n).ok())
            && let Some(pixel) = self.bgra.get_mut(index..index.saturating_add(4))
        {
            pixel.copy_from_slice(&[b, g, r, 255]);
        }
    }
    fn line(&mut self, mut start: (i32, i32), end: (i32, i32), color: [u8; 3]) {
        let dx = (end.0 - start.0).abs();
        let sx = if start.0 < end.0 { 1 } else { -1 };
        let dy = -(end.1 - start.1).abs();
        let sy = if start.1 < end.1 { 1 } else { -1 };
        let mut error = dx + dy;
        loop {
            if let (Ok(x), Ok(y)) = (u32::try_from(start.0), u32::try_from(start.1)) {
                self.set(x, y, color);
            }
            if start == end {
                break;
            }
            let twice = 2 * error;
            if twice >= dy {
                error += dy;
                start.0 += sx;
            }
            if twice <= dx {
                error += dx;
                start.1 += sy;
            }
        }
    }
    fn density(
        &mut self,
        values: &[u32],
        panels: u32,
        panel: u32,
        color: [u8; 3],
        column_major: bool,
    ) {
        let maximum = values.iter().copied().max().unwrap_or(1).max(1);
        for column in 0_u32..256 {
            for level in 0_u32..256 {
                let index = if column_major {
                    column * 256 + level
                } else {
                    level * 256 + column
                };
                let Some(value) = usize::try_from(index)
                    .ok()
                    .and_then(|i| values.get(i))
                    .copied()
                    .filter(|v| *v != 0)
                else {
                    continue;
                };
                let display_y = if column_major { 255 - level } else { level };
                let start_x = (panel * 256 + column) * self.dimensions.width / (256 * panels);
                let end_x = ((panel * 256 + column + 1) * self.dimensions.width / (256 * panels))
                    .max(start_x + 1)
                    .min(self.dimensions.width);
                let start_y = display_y * self.dimensions.height / 256;
                let end_y = ((display_y + 1) * self.dimensions.height / 256)
                    .max(start_y + 1)
                    .min(self.dimensions.height);
                let intensity = (0.12 + f64::from(value) / f64::from(maximum)).min(1.0);
                let color = color.map(|v| (20.0 + (f64::from(v) - 20.0) * intensity).round() as u8);
                for y in start_y..end_y {
                    for x in start_x..end_x {
                        self.set(x, y, color);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CancellationToken;
    use rawweave_image::Dimensions;

    #[test]
    fn histogram_clipping_and_display_warnings_match_reference_fixture() {
        let pixels = [
            255, 255, 255, 255, 0, 0, 0, 255, 0, 0, 255, 255, 32, 32, 32, 255,
        ];
        let analysis = analyze_bgra(
            Dimensions::new(4, 1),
            &pixels,
            AnalysisOptions::default(),
            &CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(analysis.sample_count, 4);
        assert_eq!(analysis.histogram[0][255], 2);
        assert_eq!(analysis.histogram[1][0], 2);
        assert_eq!(analysis.highlight_count, 2);
        assert_eq!(analysis.shadow_count, 1);
        assert_eq!(analysis.clipping, [HIGHLIGHT, SHADOW, HIGHLIGHT, 0]);
        assert_eq!(analysis.gamut_warning_count, 1);
        assert_eq!(analysis.zebra_count, 3);
        let sample = analysis.sample_pixel(2, 0).unwrap();
        assert_eq!(sample.rgba, [255, 0, 0, 255]);
        assert_eq!(sample.hex(), "#ff0000");
        assert!((sample.luma - 54.213).abs() < 0.001);
    }

    #[test]
    fn thresholds_apply_to_maximum_channel_and_do_not_invent_hdr_gamut() {
        let pixels = [0, 0, 249, 255, 0, 0, 250, 255, 5, 5, 5, 0, 6, 6, 6, 255];
        let analysis = analyze_bgra(
            Dimensions::new(4, 1),
            &pixels,
            AnalysisOptions::default(),
            &CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(analysis.clipping, [0, HIGHLIGHT, SHADOW, 0]);
        assert_eq!(analysis.gamut_warning_count, 1);
        let overlay = analysis.clipping_overlay().unwrap();
        assert_eq!(&overlay.bgra[4..8], &[52, 52, 255, 170]);
        assert_eq!(&overlay.bgra[8..12], &[255, 124, 34, 170]);
    }

    #[test]
    fn clipping_composition_preserves_straight_alpha_for_comparison() {
        let pixels = [255, 255, 255, 255, 0, 0, 0, 0];
        let analysis = analyze_bgra(
            Dimensions::new(2, 1),
            &pixels,
            AnalysisOptions::default(),
            &CancellationToken::new(),
        )
        .unwrap();
        let composed = analysis.overlay_clipping(&pixels).unwrap();
        assert_eq!(composed.bgra, [120, 120, 255, 255, 255, 124, 34, 170]);
        assert!(analysis.overlay_clipping(&[]).is_err());
    }
    #[test]
    fn scope_and_inspector_sampling_are_bounded_and_keep_endpoint_pixels() {
        let mut pixels = Vec::new();
        for value in 0_u8..=255 {
            pixels.extend([40, 20, value, 255]);
        }
        let analysis = analyze_bgra(
            Dimensions::new(16, 16),
            &pixels,
            AnalysisOptions {
                max_samples: 20,
                inspector_size: 8,
            },
            &CancellationToken::new(),
        )
        .unwrap();
        assert!(analysis.sample_count <= 20);
        assert_eq!(analysis.inspector.dimensions, Dimensions::new(8, 8));
        assert_eq!(analysis.sample_pixel(15, 15).unwrap().rgba[0], 255);
        assert_eq!(
            analysis.histogram[0].iter().sum::<u32>(),
            analysis.sample_count
        );
        assert_eq!(
            analysis.waveform[0].iter().sum::<u32>(),
            analysis.sample_count
        );
        assert_eq!(
            analysis.vectorscope.iter().sum::<u32>(),
            analysis.sample_count
        );
    }

    #[test]
    fn waveform_preserves_spatial_column_and_level_axes() {
        let analysis = analyze_bgra(
            Dimensions::new(2, 1),
            &[0, 0, 0, 255, 255, 255, 255, 255],
            AnalysisOptions::default(),
            &CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(analysis.waveform[3][0], 1);
        assert_eq!(analysis.waveform[3][128 * 256 + 255], 1);
        let raster = analysis
            .render(ScopeKind::Waveform, Dimensions::new(256, 256))
            .unwrap();
        assert_ne!(
            &raster.bgra[255 * 256 * 4..255 * 256 * 4 + 3],
            &[20, 20, 20]
        );
        assert_ne!(&raster.bgra[128 * 4..128 * 4 + 3], &[20, 20, 20]);
    }

    #[test]
    fn all_scope_views_render_valid_bounded_bgra_and_validate_inputs() {
        let analysis = analyze_bgra(
            Dimensions::new(1, 1),
            &[0, 0, 255, 128],
            AnalysisOptions::default(),
            &CancellationToken::new(),
        )
        .unwrap();
        for kind in ScopeKind::ALL {
            let raster = analysis.render(kind, Dimensions::new(320, 140)).unwrap();
            assert_eq!(raster.bgra.len(), 320 * 140 * 4);
            assert!(raster.bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        }
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        assert!(
            analyze_bgra(
                Dimensions::new(1, 1),
                &[0; 4],
                AnalysisOptions::default(),
                &cancelled
            )
            .is_err()
        );
        for dimensions in [Dimensions::new(0, 1), Dimensions::new(8193, 1)] {
            assert!(
                analyze_bgra(
                    dimensions,
                    &[],
                    AnalysisOptions::default(),
                    &CancellationToken::new()
                )
                .is_err()
            );
        }
        assert!(
            analyze_bgra(
                Dimensions::new(1, 1),
                &[],
                AnalysisOptions::default(),
                &CancellationToken::new()
            )
            .is_err()
        );
        assert!(
            analyze_bgra(
                Dimensions::new(1, 1),
                &[0; 4],
                AnalysisOptions {
                    max_samples: 0,
                    inspector_size: 256
                },
                &CancellationToken::new()
            )
            .is_err()
        );
        assert!(
            analysis
                .render(ScopeKind::Histogram, Dimensions::new(8192, 8192))
                .is_err()
        );
    }
}
