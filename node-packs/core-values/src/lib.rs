use rawweave_node_api::{
    EvaluationContext, Inputs, NodeDescriptor, NodeError, NodeInstance, NodePack, NodeRegistry,
    NodeResult, ParameterValue, Parameters, PortDescriptor, Value,
};

struct ConstantFloat;

impl NodeInstance for ConstantFloat {
    fn evaluate(
        &self,
        _inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let value = parameters
            .get("value")
            .and_then(ParameterValue::as_float)
            .ok_or_else(|| NodeError::InvalidParameter("value".to_owned()))?;
        Ok(NodeResult::single("value", Value::Float(value)))
    }
}

fn constant_float() -> Box<dyn NodeInstance> {
    Box::new(ConstantFloat)
}

fn descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.constant-float", "Constant Float");
    descriptor
        .outputs
        .push(PortDescriptor::output("value", "Value", "value.Float"));
    descriptor
        .parameters
        .push(rawweave_node_api::ParameterDescriptor::float(
            "value", "Value", 0.0, None, None,
        ));
    descriptor
}

pub fn register_nodes(registry: &mut NodeRegistry) -> Result<(), rawweave_node_api::RegistryError> {
    registry.register(descriptor(), constant_float)
}

pub struct CoreValuesPack;

impl NodePack for CoreValuesPack {
    fn id(&self) -> &'static str {
        "core-values"
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
    use rawweave_node_api::{EvaluationContext, NodeRegistry, Parameters, Value};

    #[test]
    fn constant_float_is_registered_through_the_node_api() {
        let mut registry = NodeRegistry::default();
        register_nodes(&mut registry).unwrap();
        let node = registry.instantiate("core.constant-float").unwrap();
        let parameters = [("value".to_owned(), 2.5_f32.into())]
            .into_iter()
            .collect::<Parameters>();
        let result = node
            .evaluate(
                &Default::default(),
                &parameters,
                &EvaluationContext::default(),
            )
            .unwrap();
        assert_eq!(result.outputs.get("value"), Some(&Value::Float(2.5)));
    }
}
