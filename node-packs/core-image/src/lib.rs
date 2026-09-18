use rawweave_image::Image;
use rawweave_node_api::{
    EvaluationContext, Inputs, NodeDescriptor, NodeError, NodeInstance, NodePack, NodeRegistry,
    NodeResult, ParameterDescriptor, ParameterValue, Parameters, PortDescriptor, Value,
};

fn image_input_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.image-input", "Image Input");
    descriptor
        .outputs
        .push(PortDescriptor::output("image", "Image", "core.Image"));
    descriptor
}

fn exposure_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.exposure", "Exposure");
    descriptor
        .inputs
        .push(PortDescriptor::input("image", "Image", "core.Image", true));
    descriptor.inputs.push(PortDescriptor::input(
        "exposure",
        "Exposure",
        "value.Float",
        false,
    ));
    descriptor
        .outputs
        .push(PortDescriptor::output("image", "Image", "core.Image"));
    descriptor.parameters.push(ParameterDescriptor::float(
        "exposure", "Exposure", 0.0, None, None,
    ));
    descriptor
}

fn invert_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.invert", "Invert");
    descriptor
        .inputs
        .push(PortDescriptor::input("image", "Image", "core.Image", true));
    descriptor
        .outputs
        .push(PortDescriptor::output("image", "Image", "core.Image"));
    descriptor
}

fn output_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.output", "Output");
    descriptor
        .inputs
        .push(PortDescriptor::input("image", "Image", "core.Image", true));
    descriptor
        .outputs
        .push(PortDescriptor::output("image", "Image", "core.Image"));
    descriptor
}

struct ImageInput;

impl NodeInstance for ImageInput {
    fn evaluate(
        &self,
        _inputs: &Inputs,
        _parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        context
            .source_image
            .clone()
            .map(|image| NodeResult::single("image", Value::Image(image)))
            .ok_or(NodeError::MissingSourceImage)
    }
}

struct Exposure;

impl NodeInstance for Exposure {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = image_input(inputs, "image")?;
        let exposure = inputs
            .get("exposure")
            .and_then(|value| match value {
                Value::Float(value) => Some(*value),
                _ => None,
            })
            .or_else(|| {
                parameters
                    .get("exposure")
                    .and_then(ParameterValue::as_float)
            })
            .ok_or_else(|| NodeError::InvalidParameter("exposure".to_owned()))?;
        let multiplier = 2.0_f32.powf(exposure);
        Ok(NodeResult::single(
            "image",
            Value::Image(image.map_pixels(|[red, green, blue, alpha]| {
                [
                    red * multiplier,
                    green * multiplier,
                    blue * multiplier,
                    alpha,
                ]
            })),
        ))
    }
}

struct Invert;

impl NodeInstance for Invert {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let image = image_input(inputs, "image")?;
        Ok(NodeResult::single(
            "image",
            Value::Image(image.map_pixels(|[red, green, blue, alpha]| {
                [1.0 - red, 1.0 - green, 1.0 - blue, alpha]
            })),
        ))
    }
}

struct Output;

impl NodeInstance for Output {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        Ok(NodeResult::single("image", image_value(inputs, "image")?))
    }
}

fn image_value(inputs: &Inputs, port: &str) -> Result<Value, NodeError> {
    match inputs.get(port) {
        Some(Value::Image(image)) => Ok(Value::Image(image.clone())),
        Some(_) => Err(NodeError::InvalidParameter(port.to_owned())),
        None => Err(NodeError::MissingInput(port.to_owned())),
    }
}

fn image_input(inputs: &Inputs, port: &str) -> Result<Image, NodeError> {
    match image_value(inputs, port)? {
        Value::Image(image) => Ok(image),
        Value::Float(_) => unreachable!("image_value only returns images"),
    }
}

fn image_input_factory() -> Box<dyn NodeInstance> {
    Box::new(ImageInput)
}

fn exposure_factory() -> Box<dyn NodeInstance> {
    Box::new(Exposure)
}

fn invert_factory() -> Box<dyn NodeInstance> {
    Box::new(Invert)
}

fn output_factory() -> Box<dyn NodeInstance> {
    Box::new(Output)
}

pub fn register_nodes(registry: &mut NodeRegistry) -> Result<(), rawweave_node_api::RegistryError> {
    registry.register(image_input_descriptor(), image_input_factory)?;
    registry.register(exposure_descriptor(), exposure_factory)?;
    registry.register(invert_descriptor(), invert_factory)?;
    registry.register(output_descriptor(), output_factory)
}

pub struct CoreImagePack;

impl NodePack for CoreImagePack {
    fn id(&self) -> &'static str {
        "core-image"
    }

    fn register(
        &self,
        registry: &mut NodeRegistry,
    ) -> Result<(), rawweave_node_api::RegistryError> {
        register_nodes(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::register_nodes;
    use rawweave_image::Image;
    use rawweave_node_api::{EvaluationContext, Inputs, NodeRegistry, Parameters, Value};

    #[test]
    fn exposure_and_invert_use_the_common_node_api() {
        let mut registry = NodeRegistry::default();
        register_nodes(&mut registry).unwrap();
        let exposure = registry.instantiate("core.exposure").unwrap();
        let image = Image::from_pixels(1, 1, vec![[0.25, 0.5, 0.75, 1.0]]).unwrap();
        let inputs = [("image".to_owned(), Value::Image(image))]
            .into_iter()
            .collect::<Inputs>();
        let parameters = [("exposure".to_owned(), 1.0_f32.into())]
            .into_iter()
            .collect::<Parameters>();
        let result = exposure
            .evaluate(&inputs, &parameters, &EvaluationContext::default())
            .unwrap();
        assert_eq!(
            result.outputs.get("image"),
            Some(&Value::Image(
                Image::from_pixels(1, 1, vec![[0.5, 1.0, 1.5, 1.0]]).unwrap()
            ))
        );
    }
}
