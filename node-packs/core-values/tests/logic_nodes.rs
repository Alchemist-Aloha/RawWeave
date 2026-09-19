use rawweave_node_api::{
    EvaluationContext, Inputs, Metadata, NodeError, NodeRegistry, ParameterValue, Parameters, Value,
};
use rawweave_raw::{CameraMetadata, ExifMetadata, Orientation};

fn registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    rawweave_core_values::register_nodes(&mut registry).unwrap();
    registry
}

fn result(type_id: &str, inputs: Inputs, parameters: Parameters) -> rawweave_node_api::Outputs {
    let node = registry().instantiate(type_id).unwrap();
    node.evaluate(&inputs, &parameters, &EvaluationContext::default())
        .unwrap()
        .outputs
}

fn evaluate(type_id: &str, inputs: Inputs, parameters: Parameters) -> Value {
    result(type_id, inputs, parameters)
        .into_iter()
        .next()
        .unwrap()
        .1
}

fn error(type_id: &str, inputs: Inputs, parameters: Parameters) -> NodeError {
    let node = registry().instantiate(type_id).unwrap();
    node.evaluate(&inputs, &parameters, &EvaluationContext::default())
        .unwrap_err()
}

fn inputs(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Inputs {
    pairs
        .into_iter()
        .map(|(id, value)| (id.to_owned(), value))
        .collect()
}

fn parameters(pairs: impl IntoIterator<Item = (&'static str, ParameterValue)>) -> Parameters {
    pairs
        .into_iter()
        .map(|(id, value)| (id.to_owned(), value))
        .collect()
}

fn camera() -> CameraMetadata {
    CameraMetadata {
        make: "Fujifilm".to_owned(),
        model: "X-T5".to_owned(),
        lens: Some("XF 35mm F1.4".to_owned()),
        iso: Some(3200),
        aperture: Some(1.4),
        shutter_seconds: Some(1.0 / 250.0),
        focal_length_mm: Some(35.0),
        capture_time: Some("2024-01-02T03:04:05".to_owned()),
        orientation: Orientation::Rotate90,
        dimensions: None,
    }
}

#[test]
fn constants_cover_the_scalar_types() {
    assert_eq!(
        evaluate(
            "core.constant-integer",
            Inputs::new(),
            parameters([("value", 4_i64.into())])
        ),
        Value::Integer(4)
    );
    assert_eq!(
        evaluate(
            "core.constant-boolean",
            Inputs::new(),
            parameters([("value", true.into())])
        ),
        Value::Boolean(true)
    );
    assert_eq!(
        evaluate(
            "core.constant-string",
            Inputs::new(),
            parameters([("value", "portrait".into())])
        ),
        Value::String("portrait".to_owned())
    );
}

#[test]
fn metadata_node_exposes_control_fields_and_the_full_value() {
    let metadata_inputs = inputs([
        ("camera", Value::CameraMetadata(camera())),
        ("exif", Value::ExifMetadata(ExifMetadata::default())),
    ]);
    let outputs = result("core.metadata", metadata_inputs, Parameters::new());

    assert_eq!(outputs.get("iso"), Some(&Value::Integer(3200)));
    assert_eq!(outputs.get("aperture"), Some(&Value::Float(1.4)));
    assert_eq!(
        outputs.get("camera_model"),
        Some(&Value::String("X-T5".to_owned()))
    );
    assert_eq!(
        outputs.get("lens_model"),
        Some(&Value::String("XF 35mm F1.4".to_owned()))
    );
    assert_eq!(
        outputs.get("orientation"),
        Some(&Value::String("rotate-90".to_owned()))
    );
    let Some(Value::Metadata(Metadata { iso, model, .. })) = outputs.get("metadata") else {
        panic!("expected metadata value");
    };
    assert_eq!(*iso, Some(3200));
    assert_eq!(model, "X-T5");
}

#[test]
fn metadata_node_omits_absent_optional_fields() {
    let outputs = result(
        "core.metadata",
        inputs([("camera", Value::CameraMetadata(CameraMetadata::default()))]),
        Parameters::new(),
    );
    assert!(!outputs.contains_key("iso"));
    assert!(!outputs.contains_key("aperture"));
    assert!(!outputs.contains_key("lens_model"));
    // Always-present descriptive fields remain available.
    assert!(outputs.contains_key("metadata"));
    assert!(outputs.contains_key("orientation"));
}

#[test]
fn metadata_node_requires_at_least_one_source() {
    assert_eq!(
        error("core.metadata", Inputs::new(), Parameters::new()),
        NodeError::MissingInput("camera".to_owned())
    );
}

#[test]
fn comparisons_produce_conditions() {
    assert_eq!(
        evaluate(
            "core.less-than",
            inputs([("a", Value::Float(0.5)), ("b", Value::Float(1.0))]),
            Parameters::new()
        ),
        Value::Condition(true)
    );
    assert_eq!(
        evaluate(
            "core.greater-than",
            inputs([("a", Value::Float(2.0)), ("b", Value::Float(1.0))]),
            Parameters::new()
        ),
        Value::Condition(true)
    );
    assert_eq!(
        evaluate(
            "core.equal",
            inputs([("a", Value::Float(2.0)), ("b", Value::Float(2.0))]),
            Parameters::new()
        ),
        Value::Condition(true)
    );
    assert_eq!(
        evaluate(
            "core.compare",
            inputs([("a", Value::Float(2.0)), ("b", Value::Float(2.0))]),
            parameters([("operation", ">=".into())])
        ),
        Value::Condition(true)
    );
}

#[test]
fn compare_rejects_unknown_operations() {
    let error = error(
        "core.compare",
        inputs([("a", Value::Float(1.0)), ("b", Value::Float(2.0))]),
        parameters([("operation", "~=".into())]),
    );
    assert!(matches!(error, NodeError::InvalidParameter(id) if id == "operation"));
}

#[test]
fn boolean_logic_nodes_combine_conditions() {
    assert_eq!(
        evaluate(
            "core.and",
            inputs([
                ("a", Value::Condition(true)),
                ("b", Value::Condition(false))
            ]),
            Parameters::new()
        ),
        Value::Condition(false)
    );
    assert_eq!(
        evaluate(
            "core.or",
            inputs([
                ("a", Value::Condition(true)),
                ("b", Value::Condition(false))
            ]),
            Parameters::new()
        ),
        Value::Condition(true)
    );
    assert_eq!(
        evaluate(
            "core.not",
            inputs([("value", Value::Condition(true))]),
            Parameters::new()
        ),
        Value::Condition(false)
    );
}

#[test]
fn switch_selects_the_matching_branch_from_typed_inputs() {
    assert_eq!(
        evaluate(
            "core.switch",
            inputs([
                ("condition", Value::Condition(true)),
                ("true", Value::Float(1.0)),
                ("false", Value::Float(2.0)),
            ]),
            Parameters::new(),
        ),
        Value::Float(1.0)
    );
    assert_eq!(
        evaluate(
            "core.switch",
            inputs([
                ("condition", Value::Condition(false)),
                ("true", Value::Float(1.0)),
                ("false", Value::Float(2.0)),
            ]),
            Parameters::new(),
        ),
        Value::Float(2.0)
    );
    assert_eq!(
        error(
            "core.switch",
            inputs([("condition", Value::Condition(true))]),
            Parameters::new()
        ),
        NodeError::MissingInput("true".to_owned())
    );
}

#[test]
fn select_routes_by_index_and_enum_select_by_key() {
    assert_eq!(
        evaluate(
            "core.select",
            inputs([
                ("index", Value::Integer(1)),
                ("a", Value::Float(10.0)),
                ("b", Value::Float(20.0)),
                ("c", Value::Float(30.0)),
            ]),
            Parameters::new(),
        ),
        Value::Float(20.0)
    );
    assert_eq!(
        evaluate(
            "core.enum-select",
            inputs([
                ("selector", Value::String("portrait".to_owned())),
                ("a", Value::Float(1.0)),
                ("b", Value::Float(2.0)),
            ]),
            parameters([
                ("match_a", "landscape".into()),
                ("match_b", "portrait".into())
            ]),
        ),
        Value::Float(2.0)
    );
}

#[test]
fn select_rejects_an_out_of_range_index() {
    let error = error(
        "core.select",
        inputs([("index", Value::Integer(9)), ("a", Value::Float(1.0))]),
        Parameters::new(),
    );
    assert!(matches!(error, NodeError::Message(_)));
}

#[test]
fn map_range_and_clamp_transform_scalars_deterministically() {
    assert_eq!(
        evaluate(
            "core.map-range",
            inputs([("value", Value::Float(5.0))]),
            parameters([
                ("in_min", 0.0.into()),
                ("in_max", 10.0.into()),
                ("out_min", 0.0.into()),
                ("out_max", 1.0.into()),
            ])
        ),
        Value::Float(0.5)
    );
    assert_eq!(
        evaluate(
            "core.map-range",
            inputs([("value", Value::Float(20.0))]),
            parameters([
                ("in_min", 0.0.into()),
                ("in_max", 10.0.into()),
                ("out_min", 0.0.into()),
                ("out_max", 1.0.into()),
                ("clamp", true.into()),
            ])
        ),
        Value::Float(1.0)
    );
    assert_eq!(
        evaluate(
            "core.clamp",
            inputs([("value", Value::Float(-3.0))]),
            parameters([("min", 0.0.into()), ("max", 1.0.into())])
        ),
        Value::Float(0.0)
    );
}

#[test]
fn map_range_rejects_a_degenerate_input_span() {
    let error = error(
        "core.map-range",
        inputs([("value", Value::Float(1.0))]),
        parameters([
            ("in_min", 2.0.into()),
            ("in_max", 2.0.into()),
            ("out_min", 0.0.into()),
            ("out_max", 1.0.into()),
        ]),
    );
    assert!(matches!(error, NodeError::InvalidParameter(id) if id == "in_max"));
}

#[test]
fn curve_interpolates_between_control_points() {
    assert_eq!(
        evaluate(
            "core.curve",
            inputs([("value", Value::Float(0.5))]),
            parameters([("points", "0,0;0.5,0.7;1,1".into())])
        ),
        Value::Float(0.7)
    );
    assert_eq!(
        evaluate(
            "core.curve",
            inputs([("value", Value::Float(0.25))]),
            parameters([("points", "0,0;0.5,0.7;1,1".into())])
        ),
        Value::Float(0.35)
    );
    assert_eq!(
        evaluate(
            "core.curve",
            inputs([("value", Value::Float(-1.0))]),
            parameters([("points", "0,0;0.5,0.7;1,1".into())])
        ),
        Value::Float(0.0)
    );
}

#[test]
fn curve_rejects_malformed_points() {
    for points in ["0,0;0.5", "x,y", ""] {
        let error = error(
            "core.curve",
            inputs([("value", Value::Float(0.5))]),
            parameters([("points", points.into())]),
        );
        assert!(
            matches!(error, NodeError::InvalidParameter(ref id) if id == "points"),
            "expected an invalid points error for {points:?}"
        );
    }
}

#[test]
fn expression_evaluates_arithmetic_with_input_variables() {
    assert_eq!(
        evaluate(
            "core.expression",
            inputs([("a", Value::Float(3.0)), ("b", Value::Float(4.0))]),
            parameters([("expression", "(a + b) * 2".into())])
        ),
        Value::Float(14.0)
    );
    assert_eq!(
        evaluate(
            "core.expression",
            Inputs::new(),
            parameters([("expression", "1 + 2 * 3".into())])
        ),
        Value::Float(7.0)
    );
    assert_eq!(
        evaluate(
            "core.expression",
            inputs([("a", Value::Float(10.0))]),
            parameters([("expression", "-a / 4".into())])
        ),
        Value::Float(-2.5)
    );
}

#[test]
fn expression_reports_syntax_and_evaluation_errors() {
    for expression in ["a +", "2 ** 3", "unknown", "1 / 0"] {
        let error = error(
            "core.expression",
            Inputs::new(),
            parameters([("expression", expression.into())]),
        );
        assert!(
            matches!(error, NodeError::InvalidParameter(ref id) if id == "expression"),
            "expected an expression error for {expression:?}, got {error:?}"
        );
    }
}

#[test]
fn string_match_supports_exact_and_case_insensitive_matching() {
    assert_eq!(
        evaluate(
            "core.string-match",
            inputs([("value", Value::String("Portrait".to_owned()))]),
            parameters([("pattern", "portrait".into())])
        ),
        Value::Condition(false)
    );
    assert_eq!(
        evaluate(
            "core.string-match",
            inputs([("value", Value::String("Portrait".to_owned()))]),
            parameters([
                ("pattern", "portrait".into()),
                ("case_sensitive", false.into())
            ])
        ),
        Value::Condition(true)
    );
}

#[test]
fn control_nodes_evaluate_deterministically() {
    let expression_inputs = inputs([("a", Value::Float(3.0)), ("b", Value::Float(4.0))]);
    let first = evaluate(
        "core.expression",
        expression_inputs.clone(),
        parameters([("expression", "(a + b) * 2".into())]),
    );
    let second = evaluate(
        "core.expression",
        expression_inputs,
        parameters([("expression", "(a + b) * 2".into())]),
    );
    assert_eq!(first, second);
    assert_eq!(
        evaluate(
            "core.map-range",
            inputs([("value", Value::Float(2.5))]),
            parameters([
                ("in_min", 0.0.into()),
                ("in_max", 10.0.into()),
                ("out_min", 0.0.into()),
                ("out_max", 1.0.into()),
            ])
        ),
        Value::Float(0.25)
    );
}

#[test]
fn parameter_values_used_by_logic_nodes_round_trip_as_json() {
    for value in [
        ParameterValue::Float(0.5),
        ParameterValue::Integer(2),
        ParameterValue::Boolean(true),
        ParameterValue::String("portrait".to_owned()),
    ] {
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(
            serde_json::from_str::<ParameterValue>(&json).unwrap(),
            value
        );
    }
}
