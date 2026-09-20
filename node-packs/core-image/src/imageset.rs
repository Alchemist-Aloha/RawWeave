use std::collections::BTreeMap;

use rawweave_image::Image;
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
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let set = input_set(inputs)?;
        let reference = &set.members()[0];
        let dimensions = reference.image.dimensions();
        let pixel_count = dimensions
            .pixel_count()
            .map_err(|error| NodeError::Message(error.to_string()))?;
        let mut pixels = vec![[0.0; 4]; pixel_count];
        let count = set.len() as f32;
        for member in set.members() {
            if member.image.dimensions() != dimensions
                || member.image.origin() != reference.image.origin()
            {
                return Err(NodeError::Message(format!(
                    "HDR member '{}' geometry does not match member '{}'",
                    member.id, reference.id
                )));
            }
            for (output, input) in pixels.iter_mut().zip(member.image.pixels()) {
                for (channel, value) in output.iter_mut().zip(input) {
                    *channel += *value / count;
                    if !channel.is_finite() {
                        return Err(NodeError::Message(format!(
                            "HDR member '{}' produced a non-finite output",
                            member.id
                        )));
                    }
                }
            }
        }
        let image = Image::from_pixels_with_origin(
            dimensions,
            reference.image.origin(),
            pixels,
            reference.image.pixel_format(),
            reference.image.color_metadata(),
        )
        .map_err(|error| NodeError::Message(format!("HDR merge output is invalid: {error}")))?;
        Ok(image_result(image, reference.id.clone()))
    }
}

struct FocusStack;

impl NodeInstance for FocusStack {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let set = input_set(inputs)?;
        let reference = &set.members()[0];
        let dimensions = reference.image.dimensions();
        for member in set.members().iter().skip(1) {
            if member.image.dimensions() != dimensions
                || member.image.origin() != reference.image.origin()
            {
                return Err(NodeError::Message(format!(
                    "focus-stack member '{}' geometry does not match member '{}'",
                    member.id, reference.id
                )));
            }
        }
        let pixels = dimensions
            .pixel_count()
            .map_err(|error| NodeError::Message(error.to_string()))?;
        let mut output = Vec::with_capacity(pixels);
        for index in 0..pixels {
            let x = index as u32 % dimensions.width;
            let y = index as u32 / dimensions.width;
            let best = set
                .members()
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| {
                    focus_score(&left.image, x, y)
                        .partial_cmp(&focus_score(&right.image, x, y))
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| right.id.cmp(&left.id))
                })
                .ok_or_else(|| NodeError::Message("focus-stack set is empty".to_owned()))?;
            output.push(best.1.image.pixels()[index]);
        }
        let image = Image::from_pixels_with_origin(
            dimensions,
            reference.image.origin(),
            output,
            reference.image.pixel_format(),
            reference.image.color_metadata(),
        )
        .map_err(|error| NodeError::Message(format!("focus stack output is invalid: {error}")))?;
        Ok(image_result(image, reference.id.clone()))
    }
}

fn focus_score(image: &Image, x: u32, y: u32) -> f32 {
    let center = luminance(image.pixel(x, y).unwrap_or([0.0; 4]));
    let mut score = 0.0;
    for (neighbor_x, neighbor_y) in [
        (x.saturating_sub(1), y),
        (x.saturating_add(1), y),
        (x, y.saturating_sub(1)),
        (x, y.saturating_add(1)),
    ] {
        if let Some(pixel) = image.pixel(neighbor_x, neighbor_y) {
            score += (center - luminance(pixel)).abs();
        }
    }
    score
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
    registry.register(
        image_set_to_image_descriptor("core.hdr-merge", "HDR Merge"),
        hdr_factory,
    )?;
    registry.register(
        image_set_to_image_descriptor("core.focus-stack", "Focus Stack"),
        focus_factory,
    )?;

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
