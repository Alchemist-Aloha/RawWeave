use std::collections::BTreeMap;

use rawweave_image::{ConfidenceMap, Image, Mask, Pixel};
use rawweave_node_api::{
    AlignmentProvenance, AlignmentState, AlignmentTransform, EvaluationContext,
    ExecutionCapability, ImageSet, ImageSetMember, ImageSetOrder, Inputs, MAX_IMAGE_SET_MEMBERS,
    Metadata, NodeDescriptor, NodeError, NodeInstance, NodeRegistry, NodeResult,
    ParameterDescriptor, ParameterValue, Parameters, PortDescriptor, Value,
};

const IMAGE_SET_PORT: &str = "images";
const IMAGE_SET_ALIAS_PORT: &str = "set";
const IMAGE_PORT: &str = "image";
const MEMBER_ID_PORT: &str = "member_id";
const MAX_ALIGNMENT_SHIFT: i64 = 64;
const DEFAULT_HIGHLIGHT_THRESHOLD: f32 = 0.95;
const DEFAULT_DEGHOST_THRESHOLD: f32 = 0.35;
const DEFAULT_BLEND_FLOOR: f32 = 0.05;
const EPSILON: f32 = 1.0e-6;

fn collection_descriptor(type_id: &str, name: &str) -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(type_id, name);
    descriptor.inputs.push(PortDescriptor::input(
        IMAGE_SET_PORT,
        "Image Set",
        "core.ImageSet",
        false,
    ));
    descriptor.inputs.push(PortDescriptor::input(
        IMAGE_SET_ALIAS_PORT,
        "Image Set (alias)",
        "core.ImageSet",
        false,
    ));
    descriptor.outputs.push(PortDescriptor::output(
        IMAGE_SET_PORT,
        "Image Set",
        "core.ImageSet",
    ));
    descriptor.outputs.push(PortDescriptor::output(
        IMAGE_SET_ALIAS_PORT,
        "Image Set (alias)",
        "core.ImageSet",
    ));
    descriptor.capabilities = vec![ExecutionCapability::Cpu, ExecutionCapability::FullFrame];
    descriptor
}

fn image_set_input_descriptor(type_id: &str, name: &str) -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(type_id, name);
    descriptor.outputs.push(PortDescriptor::output(
        IMAGE_SET_PORT,
        "Image Set",
        "core.ImageSet",
    ));
    descriptor.outputs.push(PortDescriptor::output(
        IMAGE_SET_ALIAS_PORT,
        "Image Set (alias)",
        "core.ImageSet",
    ));
    descriptor.capabilities = vec![ExecutionCapability::Cpu, ExecutionCapability::FullFrame];
    descriptor
}

fn image_set_collector_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.imageset-collect", "Collect Images");
    for index in 0..MAX_IMAGE_SET_MEMBERS {
        descriptor.inputs.push(PortDescriptor::input(
            format!("image_{index}"),
            format!("Image {index}"),
            "core.Image",
            false,
        ));
    }
    descriptor.outputs.push(PortDescriptor::output(
        IMAGE_SET_PORT,
        "Image Set",
        "core.ImageSet",
    ));
    descriptor.outputs.push(PortDescriptor::output(
        IMAGE_SET_ALIAS_PORT,
        "Image Set (alias)",
        "core.ImageSet",
    ));
    descriptor.parameters.push(ParameterDescriptor::string(
        "id_prefix",
        "Member ID Prefix",
        "member",
    ));
    descriptor.parameters.push(ParameterDescriptor::boolean(
        "ordered",
        "Preserve Input Order",
        true,
    ));
    descriptor.capabilities = vec![ExecutionCapability::Cpu, ExecutionCapability::FullFrame];
    descriptor
}

fn image_set_to_image_descriptor(type_id: &str, name: &str) -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(type_id, name);
    descriptor.inputs.push(PortDescriptor::input(
        IMAGE_SET_PORT,
        "Image Set",
        "core.ImageSet",
        false,
    ));
    descriptor.inputs.push(PortDescriptor::input(
        IMAGE_SET_ALIAS_PORT,
        "Image Set (alias)",
        "core.ImageSet",
        false,
    ));
    descriptor
        .outputs
        .push(PortDescriptor::output(IMAGE_PORT, "Image", "core.Image"));
    descriptor.outputs.push(PortDescriptor::output(
        MEMBER_ID_PORT,
        "Member ID",
        "value.String",
    ));
    descriptor.capabilities = vec![ExecutionCapability::Cpu, ExecutionCapability::FullFrame];
    descriptor
}

fn hdr_descriptor() -> NodeDescriptor {
    let mut descriptor = image_set_to_image_descriptor("core.hdr-merge", "HDR Merge");
    descriptor.outputs.push(PortDescriptor::output(
        "highlight_mask",
        "Highlight Rejection Mask",
        "core.Mask",
    ));
    descriptor.outputs.push(PortDescriptor::output(
        "deghost_mask",
        "Deghost Agreement Mask",
        "core.Mask",
    ));
    descriptor.outputs.push(PortDescriptor::output(
        "mask",
        "HDR Diagnostic Mask",
        "core.Mask",
    ));
    descriptor.parameters.push(ParameterDescriptor::float(
        "highlight_threshold",
        "Highlight Threshold",
        0.95,
        Some(0.5),
        Some(1.0),
    ));
    descriptor.parameters.push(ParameterDescriptor::float(
        "deghost_threshold",
        "Deghost Threshold",
        0.35,
        Some(0.01),
        Some(4.0),
    ));
    descriptor
}

fn focus_descriptor() -> NodeDescriptor {
    let mut descriptor = image_set_to_image_descriptor("core.focus-stack", "Focus Stack");
    descriptor.outputs.push(PortDescriptor::output(
        "selection_mask",
        "Focus Selection Mask",
        "core.Mask",
    ));
    descriptor.outputs.push(PortDescriptor::output(
        "mask",
        "Focus Selection Mask (alias)",
        "core.Mask",
    ));
    descriptor.parameters.push(ParameterDescriptor::float(
        "blend_floor",
        "Blend Floor",
        0.05,
        Some(0.0),
        Some(1.0),
    ));
    descriptor
}

fn alignment_descriptor() -> NodeDescriptor {
    let mut descriptor = collection_descriptor("core.alignment", "Alignment");
    descriptor.parameters.push(ParameterDescriptor::string(
        "reference",
        "Reference Member",
        "",
    ));
    descriptor.parameters.push(ParameterDescriptor::integer(
        "max_shift",
        "Maximum Translation",
        8,
    ));
    descriptor
}

fn input_set(inputs: &Inputs) -> Result<ImageSet, NodeError> {
    let value = inputs
        .get(IMAGE_SET_PORT)
        .or_else(|| inputs.get(IMAGE_SET_ALIAS_PORT))
        .ok_or_else(|| NodeError::MissingInput(IMAGE_SET_PORT.to_owned()))?;
    match value {
        Value::ImageSet(set) => {
            set.validate()
                .map_err(|error| NodeError::Message(error.to_string()))?;
            Ok(set.clone())
        }
        _ => Err(NodeError::InvalidParameter(IMAGE_SET_PORT.to_owned())),
    }
}

fn set_result(set: ImageSet) -> NodeResult {
    NodeResult::new(
        [
            (IMAGE_SET_PORT.to_owned(), Value::ImageSet(set.clone())),
            (IMAGE_SET_ALIAS_PORT.to_owned(), Value::ImageSet(set)),
        ]
        .into_iter()
        .collect(),
    )
}

fn image_result(image: Image, member_id: impl Into<String>) -> NodeResult {
    NodeResult::new(
        [
            (IMAGE_PORT.to_owned(), Value::Image(image)),
            (MEMBER_ID_PORT.to_owned(), Value::String(member_id.into())),
        ]
        .into_iter()
        .collect(),
    )
}

fn finite_parameter(
    parameters: &Parameters,
    id: &'static str,
    default: f32,
    minimum: f32,
    maximum: f32,
) -> Result<f32, NodeError> {
    let value = parameters
        .get(id)
        .and_then(ParameterValue::as_float)
        .unwrap_or(default);
    if !value.is_finite() || !(minimum..=maximum).contains(&value) {
        return Err(NodeError::InvalidParameter(id.to_owned()));
    }
    Ok(value)
}

fn aligned_frames(
    set: &ImageSet,
    operation: &str,
) -> Result<(String, BTreeMap<String, AlignmentTransform>), NodeError> {
    let alignment = set.alignment();
    let AlignmentState::Aligned {
        reference_member,
        transforms,
        ..
    } = alignment
    else {
        return Err(NodeError::Message(format!(
            "{operation} requires an aligned image set; run Alignment first"
        )));
    };
    if transforms.len() != set.len() {
        return Err(NodeError::Message(format!(
            "{operation} alignment does not cover every member: {}",
            set.member_ids().join(", ")
        )));
    }
    if set.member(&reference_member).is_none() {
        return Err(NodeError::Message(format!(
            "{operation} alignment reference member '{reference_member}' is unavailable"
        )));
    }
    Ok((reference_member, transforms))
}

fn validate_compatible_frames(
    set: &ImageSet,
    reference: &ImageSetMember,
    operation: &str,
) -> Result<(), NodeError> {
    for member in set.members() {
        if member.image.dimensions() != reference.image.dimensions()
            || member.image.origin() != reference.image.origin()
        {
            return Err(NodeError::Message(format!(
                "{operation} member '{}' dimensions/origin do not match reference '{}'",
                member.id, reference.id
            )));
        }
        if member.image.pixel_format() != reference.image.pixel_format() {
            return Err(NodeError::Message(format!(
                "{operation} member '{}' pixel format does not match reference '{}'",
                member.id, reference.id
            )));
        }
        if member.image.color_metadata() != reference.image.color_metadata() {
            return Err(NodeError::Message(format!(
                "{operation} member '{}' color format does not match reference '{}'",
                member.id, reference.id
            )));
        }
    }
    Ok(())
}

fn aligned_pixel(image: &Image, transform: AlignmentTransform, x: u32, y: u32) -> Option<Pixel> {
    let source_x = i64::from(x).checked_add(i64::from(transform.dx))?;
    let source_y = i64::from(y).checked_add(i64::from(transform.dy))?;
    if source_x < 0
        || source_y < 0
        || source_x >= i64::from(image.width())
        || source_y >= i64::from(image.height())
    {
        return None;
    }
    image.pixel(source_x as u32, source_y as u32)
}

fn mask_from_values(
    dimensions: rawweave_image::Dimensions,
    origin: (u32, u32),
    values: Vec<f32>,
    operation: &str,
) -> Result<Mask, NodeError> {
    Mask::from_values_with_origin(dimensions, origin, values)
        .map_err(|error| NodeError::Message(format!("{operation} mask is invalid: {error}")))
}

fn confidence_from_values(
    dimensions: rawweave_image::Dimensions,
    origin: (u32, u32),
    values: Vec<f32>,
    operation: &str,
) -> Result<ConfidenceMap, NodeError> {
    Ok(ConfidenceMap::from_mask(mask_from_values(
        dimensions, origin, values, operation,
    )?))
}

fn add_diagnostics(
    mut result: NodeResult,
    confidence: ConfidenceMap,
    diagnostics: String,
) -> NodeResult {
    result
        .outputs
        .insert("confidence".to_owned(), Value::ConfidenceMap(confidence));
    result
        .outputs
        .insert("diagnostics".to_owned(), Value::String(diagnostics));
    result
}

fn highlight_weight(pixel: Pixel, threshold: f32) -> f32 {
    let peak = pixel[0].max(pixel[1]).max(pixel[2]);
    if peak <= threshold {
        1.0
    } else if peak >= 1.0 {
        0.0
    } else {
        let distance = (1.0 - peak) / (1.0 - threshold).max(EPSILON);
        distance * distance
    }
}

fn deghost_weight(luminance: f32, estimate: f32, threshold: f32) -> f32 {
    if estimate <= EPSILON {
        return 1.0;
    }
    let relative_error = (luminance - estimate).abs() / estimate.max(EPSILON);
    (-(relative_error / threshold).powi(2))
        .exp()
        .clamp(0.0, 1.0)
}

fn exposure_factor(member: &ImageSetMember) -> Result<f32, NodeError> {
    let iso = member.metadata.iso.ok_or_else(|| {
        NodeError::Message(format!(
            "HDR member '{}' is missing ISO metadata",
            member.id
        ))
    })?;
    let aperture = member.metadata.aperture.ok_or_else(|| {
        NodeError::Message(format!(
            "HDR member '{}' is missing aperture metadata",
            member.id
        ))
    })?;
    let shutter = member.metadata.shutter_seconds.ok_or_else(|| {
        NodeError::Message(format!(
            "HDR member '{}' is missing shutter metadata",
            member.id
        ))
    })?;
    if iso == 0 {
        return Err(NodeError::Message(format!(
            "HDR member '{}' has invalid ISO metadata",
            member.id
        )));
    }
    if !aperture.is_finite() || aperture <= 0.0 {
        return Err(NodeError::Message(format!(
            "HDR member '{}' has invalid aperture metadata",
            member.id
        )));
    }
    if !shutter.is_finite() || shutter <= 0.0 {
        return Err(NodeError::Message(format!(
            "HDR member '{}' has invalid shutter metadata",
            member.id
        )));
    }
    let factor = shutter * iso as f32 / aperture.powi(2);
    if !factor.is_finite() || factor <= 0.0 {
        return Err(NodeError::Message(format!(
            "HDR member '{}' has non-finite exposure metadata",
            member.id
        )));
    }
    Ok(factor)
}

fn focus_score_aligned(
    member: &ImageSetMember,
    transform: AlignmentTransform,
    x: u32,
    y: u32,
) -> Result<Option<f32>, NodeError> {
    let Some(center) = aligned_pixel(&member.image, transform, x, y) else {
        return Ok(None);
    };
    let center_luminance = luminance(center);
    let mut score = 0.0_f32;
    let width = member.image.width();
    let height = member.image.height();
    for (offset_x, offset_y) in [(-1_i32, 0_i32), (1, 0), (0, -1), (0, 1)] {
        let neighbor_x = i64::from(x) + i64::from(offset_x);
        let neighbor_y = i64::from(y) + i64::from(offset_y);
        if neighbor_x < 0
            || neighbor_y < 0
            || neighbor_x >= i64::from(width)
            || neighbor_y >= i64::from(height)
        {
            continue;
        }
        if let Some(neighbor) = aligned_pixel(
            &member.image,
            transform,
            neighbor_x as u32,
            neighbor_y as u32,
        ) {
            score += (center_luminance - luminance(neighbor)).abs();
        }
    }
    if score.is_finite() {
        Ok(Some(score))
    } else {
        Err(NodeError::Message(format!(
            "focus-stack member '{}' produced a non-finite focus score",
            member.id
        )))
    }
}

fn focus_weight(score: f32, maximum_score: f32, member_count: usize, blend_floor: f32) -> f32 {
    let normalized = if maximum_score <= EPSILON {
        1.0 / member_count as f32
    } else {
        (score / maximum_score).clamp(0.0, 1.0)
    };
    blend_floor + (1.0 - blend_floor) * normalized
}

struct ImageSetInput;

impl NodeInstance for ImageSetInput {
    fn evaluate(
        &self,
        _inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let set = context
            .source_image_set
            .clone()
            .or_else(|| match context.external_inputs.get(IMAGE_SET_PORT) {
                Some(Value::ImageSet(set)) => Some(set.clone()),
                _ => None,
            })
            .ok_or_else(|| NodeError::Message("image set input is unavailable".to_owned()))?;
        set.validate()
            .map_err(|error| NodeError::Message(error.to_string()))?;
        Ok(set_result(set))
    }
}

struct ImageSetCollector;

impl NodeInstance for ImageSetCollector {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let prefix = parameters
            .get("id_prefix")
            .and_then(ParameterValue::as_string)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("member");
        let order = if parameters
            .get("ordered")
            .and_then(ParameterValue::as_boolean)
            .unwrap_or(true)
        {
            ImageSetOrder::Ordered
        } else {
            ImageSetOrder::Unordered
        };
        let mut members = Vec::new();
        for index in 0..MAX_IMAGE_SET_MEMBERS {
            let port = format!("image_{index}");
            let Some(value) = inputs.get(&port) else {
                continue;
            };
            let Value::Image(image) = value else {
                return Err(NodeError::InvalidParameter(port));
            };
            let member_id = match inputs.get(&format!("member_id_{index}")) {
                None => format!("{prefix}-{index}"),
                Some(Value::String(id)) if !id.trim().is_empty() => id.clone(),
                Some(_) => return Err(NodeError::InvalidParameter(format!("member_id_{index}"))),
            };
            members.push(ImageSetMember::new(
                member_id,
                image.clone(),
                Metadata::default(),
            ));
        }
        if members.is_empty() {
            return Err(NodeError::Message(
                "image-set collector requires at least one image".to_owned(),
            ));
        }
        let set =
            ImageSet::new(order, members).map_err(|error| NodeError::Message(error.to_string()))?;
        Ok(set_result(set))
    }
}

struct Alignment;

impl NodeInstance for Alignment {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let set = input_set(inputs)?;
        let reference = parameters
            .get("reference")
            .and_then(ParameterValue::as_string)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| set.members()[0].id.clone());
        if set.member(&reference).is_none() {
            return Err(NodeError::Message(format!(
                "alignment reference member '{reference}' is not present; members: {}",
                set.member_ids().join(", ")
            )));
        }
        let max_shift = parameters
            .get("max_shift")
            .and_then(ParameterValue::as_integer)
            .unwrap_or(8);
        if !(0..=MAX_ALIGNMENT_SHIFT).contains(&max_shift) {
            return Err(NodeError::InvalidParameter("max_shift".to_owned()));
        }
        let reference_image = &set.member(&reference).expect("validated reference").image;
        let mut transforms = BTreeMap::new();
        for member in set.members() {
            if member.image.dimensions() != reference_image.dimensions() {
                return Err(NodeError::Message(format!(
                    "alignment member '{}' dimensions do not match reference '{}'",
                    member.id, reference
                )));
            }
            let transform = if member.id == reference {
                AlignmentTransform::identity()
            } else {
                register_translation(reference_image, &member.image, max_shift as u32)?
            };
            transforms.insert(member.id.clone(), transform);
        }
        let aligned = set.with_alignment(AlignmentState::Aligned {
            reference_member: reference.clone(),
            transforms,
            provenance: AlignmentProvenance::new(
                "translation-ssd",
                1,
                u32::try_from(max_shift).unwrap_or(8),
            ),
        });
        aligned
            .validate()
            .map_err(|error| NodeError::Message(error.to_string()))?;
        Ok(set_result(aligned))
    }
}

fn register_translation(
    reference: &Image,
    member: &Image,
    max_shift: u32,
) -> Result<AlignmentTransform, NodeError> {
    let width = reference.width();
    let height = reference.height();
    let limit = i32::try_from(max_shift)
        .map_err(|_| NodeError::InvalidParameter("max_shift".to_owned()))?;
    let mut best: Option<(f32, u32, i32, i32)> = None;
    for dy in -limit..=limit {
        for dx in -limit..=limit {
            let mut error = 0.0_f32;
            let mut samples = 0_usize;
            for y in 0..height {
                let source_y = y as i32 + dy;
                if !(0..height as i32).contains(&source_y) {
                    continue;
                }
                for x in 0..width {
                    let source_x = x as i32 + dx;
                    if !(0..width as i32).contains(&source_x) {
                        continue;
                    }
                    let reference_pixel = reference.pixel(x, y).ok_or_else(|| {
                        NodeError::Message("reference pixel is unavailable".to_owned())
                    })?;
                    let member_pixel =
                        member
                            .pixel(source_x as u32, source_y as u32)
                            .ok_or_else(|| {
                                NodeError::Message("member pixel is unavailable".to_owned())
                            })?;
                    for channel in 0..3 {
                        let difference = reference_pixel[channel] - member_pixel[channel];
                        error += difference * difference;
                    }
                    samples = samples.saturating_add(1);
                }
            }
            if samples == 0 {
                continue;
            }
            let mean_error = error / (samples as f32 * 3.0);
            if !mean_error.is_finite() {
                return Err(NodeError::Message(
                    "alignment produced a non-finite registration error".to_owned(),
                ));
            }
            let distance = dx.unsigned_abs().saturating_add(dy.unsigned_abs());
            let replace = best.as_ref().is_none_or(|current| {
                mean_error < current.0
                    || (mean_error == current.0
                        && (distance, dx, dy) < (current.1, current.2, current.3))
            });
            if replace {
                best = Some((mean_error, distance, dx, dy));
            }
        }
    }
    let (error, _, dx, dy) = best.ok_or_else(|| {
        NodeError::Message("alignment could not find an overlapping translation".to_owned())
    })?;
    AlignmentTransform::new(dx, dy, error)
        .ok_or_else(|| NodeError::Message("alignment transform is invalid".to_owned()))
}

struct ExposureSet;

impl NodeInstance for ExposureSet {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let set = input_set(inputs)?;
        let sort_by_exposure = parameters
            .get("sort_by_exposure")
            .and_then(ParameterValue::as_boolean)
            .unwrap_or(true);
        if !sort_by_exposure {
            return Ok(set_result(set));
        }
        let mut members = set.members().to_vec();
        members.sort_by(|left, right| {
            left.metadata
                .shutter_seconds
                .partial_cmp(&right.metadata.shutter_seconds)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.id.cmp(&right.id))
        });
        let reordered = ImageSet::new(ImageSetOrder::Ordered, members)
            .map_err(|error| NodeError::Message(error.to_string()))?
            .with_shared_metadata(set.shared_metadata().clone())
            .with_alignment(set.alignment());
        Ok(set_result(reordered))
    }
}

struct HdrMerge;

impl NodeInstance for HdrMerge {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let set = input_set(inputs)?;
        let (reference_id, transforms) = aligned_frames(&set, "HDR merge")?;
        let reference = set.member(&reference_id).ok_or_else(|| {
            NodeError::Message(format!(
                "HDR merge reference member '{reference_id}' is unavailable"
            ))
        })?;
        validate_compatible_frames(&set, reference, "HDR merge")?;
        let highlight_threshold = finite_parameter(
            parameters,
            "highlight_threshold",
            DEFAULT_HIGHLIGHT_THRESHOLD,
            0.5,
            1.0,
        )?;
        let deghost_threshold = finite_parameter(
            parameters,
            "deghost_threshold",
            DEFAULT_DEGHOST_THRESHOLD,
            0.01,
            4.0,
        )?;
        let mut frames = Vec::with_capacity(set.len());
        for member in set.members() {
            let transform = transforms.get(&member.id).copied().ok_or_else(|| {
                NodeError::Message(format!(
                    "HDR merge alignment is missing member '{}', preserving member identity",
                    member.id
                ))
            })?;
            frames.push((member, transform, exposure_factor(member)?));
        }

        let dimensions = reference.image.dimensions();
        let pixel_count = dimensions
            .pixel_count()
            .map_err(|error| NodeError::Message(error.to_string()))?;
        let mut pixels = Vec::with_capacity(pixel_count);
        let mut confidence_values = Vec::with_capacity(pixel_count);
        let mut highlight_values = Vec::with_capacity(pixel_count);
        let mut deghost_values = Vec::with_capacity(pixel_count);
        let mut rejected_highlights = 0_usize;
        let mut rejected_deghost = 0_usize;

        for index in 0..pixel_count {
            let x = (index % dimensions.width as usize) as u32;
            let y = (index / dimensions.width as usize) as u32;
            let mut provisional = [0.0_f32; 3];
            let mut base_total = 0.0_f32;
            let mut valid_count = 0_usize;
            let mut highlight_total = 0.0_f32;
            let mut fallback = (0_usize, f32::MAX);
            for (frame_index, (member, transform, exposure)) in frames.iter().enumerate() {
                let Some(pixel) = aligned_pixel(&member.image, *transform, x, y) else {
                    continue;
                };
                valid_count = valid_count.saturating_add(1);
                let peak = pixel[0].max(pixel[1]).max(pixel[2]);
                if peak < fallback.1 {
                    fallback = (frame_index, peak);
                }
                let base = highlight_weight(pixel, highlight_threshold);
                highlight_total += base;
                if base > 0.0 {
                    let normalized = [
                        pixel[0] / exposure,
                        pixel[1] / exposure,
                        pixel[2] / exposure,
                    ];
                    if normalized.iter().any(|value| !value.is_finite()) {
                        return Err(NodeError::Message(format!(
                            "HDR member '{}' produced a non-finite exposure-normalized pixel",
                            member.id
                        )));
                    }
                    base_total += base;
                    for channel in 0..3 {
                        provisional[channel] += normalized[channel] * base;
                    }
                }
            }
            if valid_count == 0 {
                return Err(NodeError::Message(format!(
                    "HDR merge has no overlapping aligned samples at ({x}, {y}); reference member '{reference_id}'"
                )));
            }
            if base_total <= EPSILON {
                let (fallback_index, _) = fallback;
                let (member, transform, exposure) = frames.get(fallback_index).ok_or_else(|| {
                    NodeError::Message(format!(
                        "HDR merge could not choose a fallback for reference member '{reference_id}'"
                    ))
                })?;
                let pixel = aligned_pixel(&member.image, *transform, x, y).ok_or_else(|| {
                    NodeError::Message(format!(
                        "HDR member '{}' lost its fallback aligned sample at ({x}, {y})",
                        member.id
                    ))
                })?;
                let normalized = [
                    pixel[0] / exposure,
                    pixel[1] / exposure,
                    pixel[2] / exposure,
                ];
                if normalized.iter().any(|value| !value.is_finite()) {
                    return Err(NodeError::Message(format!(
                        "HDR member '{}' produced a non-finite fallback pixel",
                        member.id
                    )));
                }
                provisional = normalized;
                base_total = 1.0;
            }
            let provisional_luminance = luminance([
                provisional[0] / base_total,
                provisional[1] / base_total,
                provisional[2] / base_total,
                1.0,
            ]);
            let mut merged = [0.0_f32; 4];
            let mut weight_total = 0.0_f32;
            let mut deghost_total = 0.0_f32;
            let mut effective_base_total = 0.0_f32;
            for (frame_index, (member, transform, exposure)) in frames.iter().enumerate() {
                let Some(pixel) = aligned_pixel(&member.image, *transform, x, y) else {
                    continue;
                };
                let mut base = highlight_weight(pixel, highlight_threshold);
                if highlight_total <= EPSILON && frame_index == fallback.0 {
                    base = 1.0;
                }
                let normalized = [
                    pixel[0] / exposure,
                    pixel[1] / exposure,
                    pixel[2] / exposure,
                ];
                let sample_luminance =
                    luminance([normalized[0], normalized[1], normalized[2], 1.0]);
                let deghost =
                    deghost_weight(sample_luminance, provisional_luminance, deghost_threshold);
                let weight = base * deghost;
                effective_base_total += base;
                weight_total += weight;
                deghost_total += base * deghost;
                for channel in 0..3 {
                    merged[channel] += normalized[channel] * weight;
                }
                merged[3] += pixel[3] * weight;
                if base < 0.5 {
                    rejected_highlights = rejected_highlights.saturating_add(1);
                }
                if deghost < 0.5 {
                    rejected_deghost = rejected_deghost.saturating_add(1);
                }
            }
            if weight_total <= EPSILON {
                return Err(NodeError::Message(format!(
                    "HDR merge rejected every sample at ({x}, {y}); reference member '{reference_id}'"
                )));
            }
            for channel in &mut merged {
                *channel /= weight_total;
                if !channel.is_finite() {
                    return Err(NodeError::Message(format!(
                        "HDR merge produced a non-finite output at ({x}, {y})"
                    )));
                }
            }
            pixels.push(merged);
            let valid_count = valid_count as f32;
            confidence_values.push((weight_total / valid_count).clamp(0.0, 1.0));
            highlight_values.push((1.0 - highlight_total / valid_count).clamp(0.0, 1.0));
            deghost_values.push(if effective_base_total > EPSILON {
                (deghost_total / effective_base_total).clamp(0.0, 1.0)
            } else {
                0.0
            });
        }

        let image = Image::from_pixels_with_origin(
            dimensions,
            reference.image.origin(),
            pixels,
            reference.image.pixel_format(),
            reference.image.color_metadata(),
        )
        .map_err(|error| NodeError::Message(format!("HDR merge output is invalid: {error}")))?;
        let confidence = confidence_from_values(
            dimensions,
            reference.image.origin(),
            confidence_values,
            "HDR merge",
        )?;
        let highlight_mask = mask_from_values(
            dimensions,
            reference.image.origin(),
            highlight_values,
            "HDR merge highlight",
        )?;
        let deghost_mask = mask_from_values(
            dimensions,
            reference.image.origin(),
            deghost_values,
            "HDR merge deghost",
        )?;
        let diagnostics = format!(
            "HDR merge: reference={reference_id}; members={}; aligned=true; highlight-rejections={rejected_highlights}; deghost-rejections={rejected_deghost}",
            frames.len()
        );
        let mut result =
            add_diagnostics(image_result(image, reference_id), confidence, diagnostics);
        result.outputs.insert(
            "highlight_mask".to_owned(),
            Value::Mask(highlight_mask.clone()),
        );
        result
            .outputs
            .insert("deghost_mask".to_owned(), Value::Mask(deghost_mask));
        result
            .outputs
            .insert("mask".to_owned(), Value::Mask(highlight_mask));
        Ok(result)
    }
}

struct FocusStack;

impl NodeInstance for FocusStack {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let set = input_set(inputs)?;
        let (reference_id, transforms) = aligned_frames(&set, "focus stack")?;
        let reference = set.member(&reference_id).ok_or_else(|| {
            NodeError::Message(format!(
                "focus stack reference member '{reference_id}' is unavailable"
            ))
        })?;
        validate_compatible_frames(&set, reference, "focus stack")?;
        let blend_floor =
            finite_parameter(parameters, "blend_floor", DEFAULT_BLEND_FLOOR, 0.0, 1.0)?;
        let dimensions = reference.image.dimensions();
        let pixel_count = dimensions
            .pixel_count()
            .map_err(|error| NodeError::Message(error.to_string()))?;
        let mut output = Vec::with_capacity(pixel_count);
        let mut confidence_values = Vec::with_capacity(pixel_count);
        let mut selection_values = Vec::with_capacity(pixel_count);
        let mut blended_pixels = 0_usize;
        let mut fallback_pixels = 0_usize;

        for index in 0..pixel_count {
            let x = (index % dimensions.width as usize) as u32;
            let y = (index / dimensions.width as usize) as u32;
            let mut samples = Vec::with_capacity(set.len());
            for member in set.members() {
                let transform = transforms.get(&member.id).copied().ok_or_else(|| {
                    NodeError::Message(format!(
                        "focus stack alignment is missing member '{}', preserving member identity",
                        member.id
                    ))
                })?;
                let Some(pixel) = aligned_pixel(&member.image, transform, x, y) else {
                    continue;
                };
                let score = focus_score_aligned(member, transform, x, y)?.ok_or_else(|| {
                    NodeError::Message(format!(
                        "focus stack member '{}' lost its aligned sample at ({x}, {y})",
                        member.id
                    ))
                })?;
                samples.push((member, pixel, score));
            }
            if samples.is_empty() {
                return Err(NodeError::Message(format!(
                    "focus stack has no overlapping aligned samples at ({x}, {y}); reference member '{reference_id}'"
                )));
            }
            let maximum_score = samples
                .iter()
                .map(|(_, _, score)| *score)
                .fold(0.0_f32, f32::max);
            let mut output_pixel = [0.0_f32; 4];
            let mut total_weight = 0.0_f32;
            let mut best_weight = 0.0_f32;
            let mut second_weight = 0.0_f32;
            for (_, pixel, score) in &samples {
                let weight = focus_weight(*score, maximum_score, samples.len(), blend_floor);
                total_weight += weight;
                if weight >= best_weight {
                    second_weight = best_weight;
                    best_weight = weight;
                } else if weight > second_weight {
                    second_weight = weight;
                }
                for channel in 0..4 {
                    output_pixel[channel] += pixel[channel] * weight;
                }
            }
            if !total_weight.is_finite() || total_weight <= EPSILON {
                return Err(NodeError::Message(format!(
                    "focus stack produced an invalid blend at ({x}, {y})"
                )));
            }
            for channel in &mut output_pixel {
                *channel /= total_weight;
                if !channel.is_finite() {
                    return Err(NodeError::Message(format!(
                        "focus stack produced a non-finite output at ({x}, {y})"
                    )));
                }
            }
            output.push(output_pixel);
            let confidence = if maximum_score <= EPSILON {
                1.0 / samples.len() as f32
            } else {
                (best_weight / total_weight).clamp(0.0, 1.0)
            };
            let selection = if best_weight <= EPSILON {
                0.0
            } else {
                ((best_weight - second_weight) / best_weight).clamp(0.0, 1.0)
            };
            confidence_values.push(confidence);
            selection_values.push(selection);
            if samples.len() > 1 {
                blended_pixels = blended_pixels.saturating_add(1);
            }
            if maximum_score <= EPSILON {
                fallback_pixels = fallback_pixels.saturating_add(1);
            }
        }
        let image = Image::from_pixels_with_origin(
            dimensions,
            reference.image.origin(),
            output,
            reference.image.pixel_format(),
            reference.image.color_metadata(),
        )
        .map_err(|error| NodeError::Message(format!("focus stack output is invalid: {error}")))?;
        let confidence = confidence_from_values(
            dimensions,
            reference.image.origin(),
            confidence_values,
            "focus stack",
        )?;
        let selection_mask = mask_from_values(
            dimensions,
            reference.image.origin(),
            selection_values,
            "focus stack selection",
        )?;
        let diagnostics = format!(
            "Focus stack: reference={reference_id}; members={}; aligned=true; blended-pixels={blended_pixels}; flat-score-pixels={fallback_pixels}",
            set.len()
        );
        let mut result =
            add_diagnostics(image_result(image, reference_id), confidence, diagnostics);
        result.outputs.insert(
            "selection_mask".to_owned(),
            Value::Mask(selection_mask.clone()),
        );
        result
            .outputs
            .insert("mask".to_owned(), Value::Mask(selection_mask));
        Ok(result)
    }
}

fn luminance(pixel: [f32; 4]) -> f32 {
    0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2]
}

struct ImageSetSelect;

impl NodeInstance for ImageSetSelect {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let set = input_set(inputs)?;
        let requested_id = parameters
            .get("member_id")
            .and_then(ParameterValue::as_string)
            .filter(|value| !value.trim().is_empty());
        let member = if let Some(requested_id) = requested_id {
            set.member(requested_id).ok_or_else(|| {
                NodeError::Message(format!(
                    "image-set member id '{requested_id}' is unavailable; members: {}",
                    set.member_ids().join(", ")
                ))
            })?
        } else {
            let index = parameters
                .get("index")
                .and_then(ParameterValue::as_integer)
                .ok_or_else(|| NodeError::InvalidParameter("index".to_owned()))?;
            let index = usize::try_from(index).map_err(|_| {
                NodeError::Message(format!(
                    "image-set member index {index} is invalid; members: {}",
                    set.member_ids().join(", ")
                ))
            })?;
            set.member_at(index).ok_or_else(|| {
                NodeError::Message(format!(
                    "image-set member index {index} is out of range; members: {}",
                    set.member_ids().join(", ")
                ))
            })?
        };
        Ok(image_result(member.image.clone(), member.id.clone()))
    }
}

struct ImageSetFilter;

impl NodeInstance for ImageSetFilter {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let set = input_set(inputs)?;
        let tag = parameters
            .get("tag")
            .and_then(ParameterValue::as_string)
            .unwrap_or("");
        let value = parameters
            .get("value")
            .and_then(ParameterValue::as_string)
            .unwrap_or("");
        if tag.trim().is_empty() {
            return Ok(set_result(set));
        }
        let members = set
            .members()
            .iter()
            .filter(|member| member.metadata.tags.get(tag) == Some(&value.to_owned()))
            .cloned()
            .collect::<Vec<_>>();
        if members.is_empty() {
            return Err(NodeError::Message(format!(
                "image-set filter removed every member for {tag}={value}; members: {}",
                set.member_ids().join(", ")
            )));
        }
        let filtered = ImageSet::new(set.order(), members)
            .map_err(|error| NodeError::Message(error.to_string()))?
            .with_shared_metadata(set.shared_metadata().clone())
            .with_alignment(set.alignment());
        Ok(set_result(filtered))
    }
}

struct ImageSetMap;

impl NodeInstance for ImageSetMap {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let set = input_set(inputs)?;
        let exposure = parameters
            .get("exposure")
            .and_then(ParameterValue::as_float)
            .unwrap_or(0.0);
        if !exposure.is_finite() {
            return Err(NodeError::InvalidParameter("exposure".to_owned()));
        }
        let multiplier = 2.0_f32.powf(exposure);
        if !multiplier.is_finite() {
            return Err(NodeError::InvalidParameter("exposure".to_owned()));
        }
        let members = set
            .members()
            .iter()
            .map(|member| {
                let pixels = member
                    .image
                    .pixels()
                    .iter()
                    .map(|[red, green, blue, alpha]| {
                        [
                            red * multiplier,
                            green * multiplier,
                            blue * multiplier,
                            *alpha,
                        ]
                    })
                    .collect::<Vec<_>>();
                if pixels.iter().flatten().any(|channel| !channel.is_finite()) {
                    return Err(NodeError::Message(format!(
                        "image-set member '{}' produced a non-finite output",
                        member.id
                    )));
                }
                let image = Image::from_pixels_with_origin(
                    member.image.dimensions(),
                    member.image.origin(),
                    pixels,
                    member.image.pixel_format(),
                    member.image.color_metadata(),
                )
                .map_err(|error| {
                    NodeError::Message(format!("image-set member '{}' failed: {error}", member.id))
                })?;
                let mapped = ImageSetMember::new(member.id.clone(), image, member.metadata.clone());
                Ok(match member.source.clone() {
                    Some(source) => mapped.with_source(source),
                    None => mapped,
                })
            })
            .collect::<Result<Vec<_>, NodeError>>()?;
        let mapped = ImageSet::new(set.order(), members)
            .map_err(|error| NodeError::Message(error.to_string()))?
            .with_shared_metadata(set.shared_metadata().clone())
            .with_alignment(set.alignment());
        Ok(set_result(mapped))
    }
}

struct ImageSetGroup;

impl NodeInstance for ImageSetGroup {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let set = input_set(inputs)?;
        let tag = parameters
            .get("tag")
            .and_then(ParameterValue::as_string)
            .unwrap_or("");
        if tag.trim().is_empty() {
            return Ok(set_result(set));
        }
        let group = parameters
            .get("value")
            .and_then(ParameterValue::as_string)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                set.members()
                    .first()
                    .and_then(|member| member.metadata.tags.get(tag).cloned())
            })
            .ok_or_else(|| NodeError::Message(format!("group tag '{tag}' is unavailable")))?;
        let members = set
            .members()
            .iter()
            .filter(|member| member.metadata.tags.get(tag) == Some(&group))
            .cloned()
            .collect::<Vec<_>>();
        if members.is_empty() {
            return Err(NodeError::Message(format!(
                "image-set group '{tag}={group}' has no members; members: {}",
                set.member_ids().join(", ")
            )));
        }
        let grouped = ImageSet::new(set.order(), members)
            .map_err(|error| NodeError::Message(error.to_string()))?
            .with_shared_metadata(set.shared_metadata().clone())
            .with_alignment(set.alignment());
        let mut result = set_result(grouped);
        result
            .outputs
            .insert("group".to_owned(), Value::String(group));
        Ok(result)
    }
}

fn input_factory() -> Box<dyn NodeInstance> {
    Box::new(ImageSetInput)
}
fn collector_factory() -> Box<dyn NodeInstance> {
    Box::new(ImageSetCollector)
}
fn alignment_factory() -> Box<dyn NodeInstance> {
    Box::new(Alignment)
}
fn exposure_set_factory() -> Box<dyn NodeInstance> {
    Box::new(ExposureSet)
}
fn hdr_factory() -> Box<dyn NodeInstance> {
    Box::new(HdrMerge)
}
fn focus_factory() -> Box<dyn NodeInstance> {
    Box::new(FocusStack)
}
fn select_factory() -> Box<dyn NodeInstance> {
    Box::new(ImageSetSelect)
}
fn filter_factory() -> Box<dyn NodeInstance> {
    Box::new(ImageSetFilter)
}
fn map_factory() -> Box<dyn NodeInstance> {
    Box::new(ImageSetMap)
}
fn group_factory() -> Box<dyn NodeInstance> {
    Box::new(ImageSetGroup)
}

pub fn register_nodes(registry: &mut NodeRegistry) -> Result<(), rawweave_node_api::RegistryError> {
    registry.register(
        image_set_input_descriptor("core.imageset-input", "ImageSet Input"),
        input_factory,
    )?;
    registry.register(image_set_collector_descriptor(), collector_factory)?;
    registry.register(alignment_descriptor(), alignment_factory)?;
    let mut exposure = collection_descriptor("core.exposure-set", "Exposure Set");
    exposure.parameters.push(ParameterDescriptor::boolean(
        "sort_by_exposure",
        "Sort by Exposure",
        true,
    ));
    registry.register(exposure, exposure_set_factory)?;
    registry.register(hdr_descriptor(), hdr_factory)?;
    registry.register(focus_descriptor(), focus_factory)?;

    let mut select = image_set_to_image_descriptor("core.imageset-select", "Select");
    select
        .parameters
        .push(ParameterDescriptor::integer("index", "Member Index", 0));
    select
        .parameters
        .push(ParameterDescriptor::string("member_id", "Member ID", ""));
    registry.register(select, select_factory)?;
    let mut filter = collection_descriptor("core.imageset-filter", "Filter");
    filter
        .parameters
        .push(ParameterDescriptor::string("tag", "Tag", ""));
    filter
        .parameters
        .push(ParameterDescriptor::string("value", "Value", ""));
    registry.register(filter, filter_factory)?;
    let mut map = collection_descriptor("core.imageset-map", "Map");
    map.parameters.push(ParameterDescriptor::float(
        "exposure", "Exposure", 0.0, None, None,
    ));
    registry.register(map, map_factory)?;
    let mut group = collection_descriptor("core.imageset-group", "Group");
    group
        .parameters
        .push(ParameterDescriptor::string("tag", "Tag", ""));
    group
        .parameters
        .push(ParameterDescriptor::string("value", "Value", ""));
    group
        .outputs
        .push(PortDescriptor::output("group", "Group", "value.String"));
    registry.register(group, group_factory)
}
