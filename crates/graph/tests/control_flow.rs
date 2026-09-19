use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rawweave_core::NodeId;
use rawweave_core_values::register_nodes as register_value_nodes;
use rawweave_graph::{Graph, GraphError};
use rawweave_image::Image;
use rawweave_node_api::{
    EvaluationContext, ExecutionCapability, Inputs, LazyBranch, LazyCondition, LazyInputGate,
    NodeDescriptor, NodeError, NodeInstance, NodeRegistry, NodeResult, ParameterDescriptor,
    ParameterValue, Parameters, PortDescriptor, Value,
};

/// A test node whose `amount` parameter is not a static input port, so it can
/// only be driven through the generic connectable-parameter mechanism.
struct ParamSink {
    evaluations: Arc<AtomicUsize>,
}

impl NodeInstance for ParamSink {
    fn evaluate(
        &self,
        _inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        self.evaluations.fetch_add(1, Ordering::SeqCst);
        let amount = parameters
            .get("amount")
            .and_then(ParameterValue::as_float)
            .ok_or_else(|| NodeError::InvalidParameter("amount".to_owned()))?;
        Ok(NodeResult::single("value", Value::Float(amount)))
    }
}

/// An image node that records how many times it was evaluated, used to prove
/// lazy branch gating.
struct CountingProducer {
    evaluations: Arc<AtomicUsize>,
}

impl NodeInstance for CountingProducer {
    fn evaluate(
        &self,
        _inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        self.evaluations.fetch_add(1, Ordering::SeqCst);
        Ok(NodeResult::single("image", Value::Image(source_image())))
    }
}

/// A node with a wildcard input that records the raw value it received.
struct AnySink;
impl NodeInstance for AnySink {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        Ok(NodeResult::single(
            "value",
            inputs
                .get("value")
                .cloned()
                .ok_or_else(|| NodeError::MissingInput("value".to_owned()))?,
        ))
    }
}

/// A numeric node that reports the float it received, used to prove implicit
/// integer-to-float coercion.
struct FloatSink;

impl NodeInstance for FloatSink {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        match inputs.get("value") {
            Some(Value::Float(value)) => Ok(NodeResult::single("value", Value::Float(*value))),
            _ => Err(NodeError::MissingInput("value".to_owned())),
        }
    }
}

fn register_test_nodes(registry: &mut NodeRegistry, sink_evals: Arc<AtomicUsize>) {
    let mut param_sink = NodeDescriptor::new("test.param-sink", "Param Sink");
    param_sink
        .outputs
        .push(PortDescriptor::output("value", "Value", "value.Float"));
    param_sink.parameters.push(ParameterDescriptor::float(
        "amount", "Amount", 1.0, None, None,
    ));
    let counter = Arc::clone(&sink_evals);
    registry
        .register_factory(param_sink, move || {
            Box::new(ParamSink {
                evaluations: Arc::clone(&counter),
            })
        })
        .unwrap();

    let mut any_sink = NodeDescriptor::new("test.any-sink", "Any Sink");
    any_sink
        .inputs
        .push(PortDescriptor::input("value", "Value", "core.Any", true));
    any_sink
        .outputs
        .push(PortDescriptor::output("value", "Value", "core.Any"));
    registry.register(any_sink, || Box::new(AnySink)).unwrap();

    let mut float_sink = NodeDescriptor::new("test.float-sink", "Float Sink");
    float_sink
        .inputs
        .push(PortDescriptor::input("value", "Value", "value.Float", true));
    float_sink
        .outputs
        .push(PortDescriptor::output("value", "Value", "value.Float"));
    registry
        .register(float_sink, || Box::new(FloatSink))
        .unwrap();

    let mut camera_source = NodeDescriptor::new("test.camera-source", "Camera Source");
    camera_source.outputs.push(PortDescriptor::output(
        "camera",
        "Camera Metadata",
        "raw.CameraMetadata",
    ));
    registry
        .register(camera_source, || Box::new(CameraSource))
        .unwrap();
}

/// Emits camera metadata with no ISO so absent optional fields can be tested.
struct CameraSource;

impl NodeInstance for CameraSource {
    fn evaluate(
        &self,
        _inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        Ok(NodeResult::single(
            "camera",
            Value::CameraMetadata(rawweave_raw::CameraMetadata::default()),
        ))
    }
}

fn test_registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    register_value_nodes(&mut registry).unwrap();
    rawweave_core_image::register_nodes(&mut registry).unwrap();
    register_test_nodes(&mut registry, Arc::new(AtomicUsize::new(0)));
    registry
}

fn source_image() -> Image {
    Image::from_pixels(1, 1, vec![[0.25, 0.5, 0.75, 1.0]]).unwrap()
}

#[test]
fn exposed_parameter_uses_connection_then_restores_literal_on_disconnect() {
    let mut graph = Graph::new(test_registry());
    graph
        .add_node(NodeId::from("const"), "core.constant-float")
        .unwrap();
    graph
        .set_parameter(&NodeId::from("const"), "value", ParameterValue::Float(5.0))
        .unwrap();
    graph
        .add_node(NodeId::from("sink"), "test.param-sink")
        .unwrap();
    graph
        .expose_parameter(&NodeId::from("sink"), "amount")
        .unwrap();
    graph
        .connect(
            NodeId::from("const"),
            "value",
            NodeId::from("sink"),
            "amount",
        )
        .unwrap();

    let connected = graph
        .evaluate(
            &NodeId::from("sink"),
            "value",
            &EvaluationContext::default(),
        )
        .unwrap();
    assert_eq!(connected, Value::Float(5.0));

    let sink = graph.node(&NodeId::from("sink")).unwrap();
    assert_eq!(
        sink.parameters.get("amount"),
        Some(&ParameterValue::Float(1.0)),
        "the stored literal must survive a temporary connection"
    );

    graph
        .disconnect(
            NodeId::from("const"),
            "value",
            NodeId::from("sink"),
            "amount",
        )
        .unwrap();
    let restored = graph
        .evaluate(
            &NodeId::from("sink"),
            "value",
            &EvaluationContext::default(),
        )
        .unwrap();
    assert_eq!(restored, Value::Float(1.0));
}

#[test]
fn override_precedence_is_connected_then_override_then_literal() {
    let mut graph = Graph::new(test_registry());
    graph
        .add_node(NodeId::from("const"), "core.constant-float")
        .unwrap();
    graph
        .set_parameter(&NodeId::from("const"), "value", ParameterValue::Float(5.0))
        .unwrap();
    graph
        .add_node(NodeId::from("sink"), "test.param-sink")
        .unwrap();
    graph
        .expose_parameter(&NodeId::from("sink"), "amount")
        .unwrap();

    let override_context =
        EvaluationContext::default().with_parameter_override("sink", "amount", 7.0_f32);

    let with_override_only = graph
        .evaluate(&NodeId::from("sink"), "value", &override_context)
        .unwrap();
    assert_eq!(with_override_only, Value::Float(7.0));

    graph
        .connect(
            NodeId::from("const"),
            "value",
            NodeId::from("sink"),
            "amount",
        )
        .unwrap();
    let connected_beats_override = graph
        .evaluate(&NodeId::from("sink"), "value", &override_context)
        .unwrap();
    assert_eq!(connected_beats_override, Value::Float(5.0));

    let literal_context = EvaluationContext::default();
    graph
        .disconnect(
            NodeId::from("const"),
            "value",
            NodeId::from("sink"),
            "amount",
        )
        .unwrap();
    let literal = graph
        .evaluate(&NodeId::from("sink"), "value", &literal_context)
        .unwrap();
    assert_eq!(literal, Value::Float(1.0));
}

#[test]
fn unexposing_a_connected_parameter_drops_the_connection_and_restores_literal() {
    let mut graph = Graph::new(test_registry());
    graph
        .add_node(NodeId::from("const"), "core.constant-float")
        .unwrap();
    graph
        .set_parameter(&NodeId::from("const"), "value", ParameterValue::Float(9.0))
        .unwrap();
    graph
        .add_node(NodeId::from("sink"), "test.param-sink")
        .unwrap();
    graph
        .expose_parameter(&NodeId::from("sink"), "amount")
        .unwrap();
    graph
        .connect(
            NodeId::from("const"),
            "value",
            NodeId::from("sink"),
            "amount",
        )
        .unwrap();

    graph
        .unexpose_parameter(&NodeId::from("sink"), "amount")
        .unwrap();
    assert_eq!(
        graph
            .evaluate(
                &NodeId::from("sink"),
                "value",
                &EvaluationContext::default()
            )
            .unwrap(),
        Value::Float(1.0)
    );
    assert!(
        graph.edges().is_empty(),
        "unexposing must remove the parameter connection"
    );
    assert!(
        graph
            .connect(
                NodeId::from("const"),
                "value",
                NodeId::from("sink"),
                "amount",
            )
            .is_err(),
        "an unexposed parameter is no longer a valid connection target"
    );
}

#[test]
fn connect_rejects_type_mismatches_but_allows_numeric_and_wildcard_ports() {
    let mut graph = Graph::new(test_registry());
    graph
        .add_node(NodeId::from("string"), "core.constant-string")
        .unwrap();
    graph
        .add_node(NodeId::from("integer"), "core.constant-integer")
        .unwrap();
    graph
        .add_node(NodeId::from("sink"), "test.param-sink")
        .unwrap();
    graph
        .expose_parameter(&NodeId::from("sink"), "amount")
        .unwrap();

    assert!(matches!(
        graph.connect(
            NodeId::from("string"),
            "value",
            NodeId::from("sink"),
            "amount"
        ),
        Err(GraphError::TypeMismatch { .. })
    ));
    graph
        .connect(
            NodeId::from("integer"),
            "value",
            NodeId::from("sink"),
            "amount",
        )
        .unwrap();
    assert_eq!(
        graph
            .evaluate(
                &NodeId::from("sink"),
                "value",
                &EvaluationContext::default()
            )
            .unwrap(),
        Value::Float(0.0),
        "integer constants coerce to float parameters"
    );

    graph
        .add_node(NodeId::from("any"), "test.any-sink")
        .unwrap();
    graph
        .add_node(NodeId::from("other_string"), "core.constant-string")
        .unwrap();
    graph
        .connect(
            NodeId::from("other_string"),
            "value",
            NodeId::from("any"),
            "value",
        )
        .unwrap();
    assert_eq!(
        graph
            .evaluate(&NodeId::from("any"), "value", &EvaluationContext::default())
            .unwrap(),
        Value::String(String::new())
    );
}

#[test]
fn integer_outputs_coerce_into_float_inputs() {
    let mut graph = Graph::new(test_registry());
    graph
        .add_node(NodeId::from("integer"), "core.constant-integer")
        .unwrap();
    graph
        .set_parameter(
            &NodeId::from("integer"),
            "value",
            ParameterValue::Integer(42),
        )
        .unwrap();
    graph
        .add_node(NodeId::from("sink"), "test.float-sink")
        .unwrap();
    graph
        .connect(
            NodeId::from("integer"),
            "value",
            NodeId::from("sink"),
            "value",
        )
        .unwrap();
    assert_eq!(
        graph
            .evaluate(
                &NodeId::from("sink"),
                "value",
                &EvaluationContext::default()
            )
            .unwrap(),
        Value::Float(42.0)
    );
}

fn lazy_producer_registry(a_evals: Arc<AtomicUsize>, b_evals: Arc<AtomicUsize>) -> NodeRegistry {
    let mut registry = test_registry();

    let mut branch_a = NodeDescriptor::new("test.branch-a", "Branch A");
    branch_a
        .outputs
        .push(PortDescriptor::output("image", "Image", "core.Image"));
    branch_a.capabilities = vec![ExecutionCapability::Cpu];
    let a = Arc::clone(&a_evals);
    registry
        .register_factory(branch_a, move || {
            Box::new(CountingProducer {
                evaluations: Arc::clone(&a),
            })
        })
        .unwrap();

    let mut branch_b = NodeDescriptor::new("test.branch-b", "Branch B");
    branch_b
        .outputs
        .push(PortDescriptor::output("image", "Image", "core.Image"));
    branch_b.capabilities = vec![ExecutionCapability::Cpu];
    registry
        .register_factory(branch_b, move || {
            Box::new(CountingProducer {
                evaluations: Arc::clone(&b_evals),
            })
        })
        .unwrap();

    registry
}

#[test]
fn switch_only_evaluates_the_selected_image_branch() {
    let a_evals = Arc::new(AtomicUsize::new(0));
    let b_evals = Arc::new(AtomicUsize::new(0));
    let mut graph = Graph::new(lazy_producer_registry(
        Arc::clone(&a_evals),
        Arc::clone(&b_evals),
    ));
    graph
        .add_node(NodeId::from("condition"), "core.constant-boolean")
        .unwrap();
    graph
        .set_parameter(
            &NodeId::from("condition"),
            "value",
            ParameterValue::Boolean(true),
        )
        .unwrap();
    graph
        .add_node(NodeId::from("branch_a"), "test.branch-a")
        .unwrap();
    graph
        .add_node(NodeId::from("branch_b"), "test.branch-b")
        .unwrap();
    graph
        .add_node(NodeId::from("switch"), "core.switch")
        .unwrap();
    graph
        .add_node(NodeId::from("output"), "core.output")
        .unwrap();
    graph
        .connect(
            NodeId::from("condition"),
            "value",
            NodeId::from("switch"),
            "condition",
        )
        .unwrap();
    graph
        .connect(
            NodeId::from("branch_a"),
            "image",
            NodeId::from("switch"),
            "true",
        )
        .unwrap();
    graph
        .connect(
            NodeId::from("branch_b"),
            "image",
            NodeId::from("switch"),
            "false",
        )
        .unwrap();
    graph
        .connect(
            NodeId::from("switch"),
            "value",
            NodeId::from("output"),
            "image",
        )
        .unwrap();

    let produced = graph
        .evaluate(
            &NodeId::from("output"),
            "image",
            &EvaluationContext::default(),
        )
        .unwrap();
    assert!(matches!(produced, Value::Image(_)));
    assert_eq!(a_evals.load(Ordering::SeqCst), 1);
    assert_eq!(b_evals.load(Ordering::SeqCst), 0);

    graph
        .set_parameter(
            &NodeId::from("condition"),
            "value",
            ParameterValue::Boolean(false),
        )
        .unwrap();
    graph
        .evaluate(
            &NodeId::from("output"),
            "image",
            &EvaluationContext::default(),
        )
        .unwrap();
    assert_eq!(b_evals.load(Ordering::SeqCst), 1);
    assert_eq!(a_evals.load(Ordering::SeqCst), 1);
}

#[test]
fn workflow_round_trip_preserves_exposed_parameters_and_lazy_gates() {
    let mut graph = Graph::new(test_registry());
    graph
        .add_node(NodeId::from("sink"), "test.param-sink")
        .unwrap();
    graph
        .expose_parameter(&NodeId::from("sink"), "amount")
        .unwrap();
    graph
        .add_node(NodeId::from("switch"), "core.switch")
        .unwrap();

    let json = graph.to_json().unwrap();
    let reloaded = Graph::from_json(&json, test_registry()).unwrap();
    assert_eq!(
        reloaded
            .node(&NodeId::from("sink"))
            .unwrap()
            .exposed_parameters,
        BTreeSet::from(["amount".to_owned()])
    );
    let switch = reloaded.node(&NodeId::from("switch")).unwrap();
    assert_eq!(
        switch.descriptor.lazy_inputs,
        vec![LazyInputGate {
            selector: "condition".to_owned(),
            required: vec!["condition".to_owned()],
            branches: vec![
                LazyBranch {
                    condition: LazyCondition::True,
                    inputs: vec!["true".to_owned()],
                },
                LazyBranch {
                    condition: LazyCondition::False,
                    inputs: vec!["false".to_owned()],
                },
            ],
        }]
    );
}

#[test]
fn missing_metadata_produces_an_understandable_graph_error() {
    let mut graph = Graph::new(test_registry());
    graph
        .add_node(NodeId::from("camera"), "test.camera-source")
        .unwrap();
    graph
        .add_node(NodeId::from("metadata"), "core.metadata")
        .unwrap();
    graph
        .add_node(NodeId::from("sink"), "test.float-sink")
        .unwrap();
    graph
        .connect(
            NodeId::from("camera"),
            "camera",
            NodeId::from("metadata"),
            "camera",
        )
        .unwrap();
    // ISO is absent from the default camera metadata, so the metadata node
    // omits the output and the consumer reports exactly which port is missing.
    graph
        .connect(
            NodeId::from("metadata"),
            "iso",
            NodeId::from("sink"),
            "value",
        )
        .unwrap();

    let error = graph
        .evaluate(
            &NodeId::from("sink"),
            "value",
            &EvaluationContext::default(),
        )
        .unwrap_err();
    match error {
        GraphError::MissingOutput { node, port } => {
            assert_eq!(node, NodeId::from("metadata"));
            assert_eq!(port, "iso");
        }
        other => panic!("expected a MissingOutput error, got {other:?}"),
    }
}
