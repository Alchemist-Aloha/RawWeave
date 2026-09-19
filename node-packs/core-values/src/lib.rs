//! Scalar, metadata, and logic nodes built on the common node API.
//!
//! These nodes operate on small typed control values. They never allocate
//! image buffers and are intentionally cheap to evaluate.

use std::collections::BTreeMap;

use rawweave_node_api::{
    EvaluationContext, ExecutionCapability, Inputs, LazyBranch, LazyCondition, LazyInputGate,
    Metadata, NodeDescriptor, NodeError, NodeInstance, NodePack, NodeRegistry, NodeResult,
    ParameterDescriptor, ParameterValue, Parameters, PortDescriptor, RegistryError, Value,
};
use rawweave_raw::ExifMetadata;

fn cpu_capability(descriptor: &mut NodeDescriptor) {
    descriptor.capabilities = vec![ExecutionCapability::Cpu];
}

fn scalar_output(descriptor: &mut NodeDescriptor, data_type: &str) {
    descriptor
        .outputs
        .push(PortDescriptor::output("value", "Value", data_type));
}

fn float_parameter(parameters: &Parameters, id: &str) -> Result<f32, NodeError> {
    parameters
        .get(id)
        .and_then(ParameterValue::as_float)
        .filter(|value| value.is_finite())
        .ok_or_else(|| NodeError::InvalidParameter(id.to_owned()))
}

fn boolean_parameter(parameters: &Parameters, id: &str) -> Result<bool, NodeError> {
    parameters
        .get(id)
        .and_then(ParameterValue::as_boolean)
        .ok_or_else(|| NodeError::InvalidParameter(id.to_owned()))
}

fn integer_parameter(parameters: &Parameters, id: &str) -> Result<i64, NodeError> {
    parameters
        .get(id)
        .and_then(ParameterValue::as_integer)
        .ok_or_else(|| NodeError::InvalidParameter(id.to_owned()))
}

fn string_parameter(parameters: &Parameters, id: &str) -> Result<String, NodeError> {
    parameters
        .get(id)
        .and_then(ParameterValue::as_string)
        .map(str::to_owned)
        .ok_or_else(|| NodeError::InvalidParameter(id.to_owned()))
}

fn float_input(inputs: &Inputs, id: &str) -> Result<f32, NodeError> {
    match inputs.get(id) {
        Some(Value::Float(value)) if value.is_finite() => Ok(*value),
        Some(Value::Integer(value)) => Ok(*value as f32),
        _ => Err(NodeError::MissingInput(id.to_owned())),
    }
}

fn condition_input(inputs: &Inputs, id: &str) -> Result<bool, NodeError> {
    match inputs.get(id) {
        Some(Value::Condition(value)) | Some(Value::Boolean(value)) => Ok(*value),
        _ => Err(NodeError::MissingInput(id.to_owned())),
    }
}

fn string_input(inputs: &Inputs, id: &str) -> Result<String, NodeError> {
    match inputs.get(id) {
        Some(Value::String(value)) | Some(Value::Enum(value)) => Ok(value.clone()),
        _ => Err(NodeError::MissingInput(id.to_owned())),
    }
}

fn integer_input(inputs: &Inputs, id: &str) -> Result<i64, NodeError> {
    match inputs.get(id) {
        Some(Value::Integer(value)) => Ok(*value),
        Some(Value::Float(value)) if value.fract() == 0.0 => Ok(*value as i64),
        _ => Err(NodeError::MissingInput(id.to_owned())),
    }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

struct ConstantFloat;

impl NodeInstance for ConstantFloat {
    fn evaluate(
        &self,
        _inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let value = float_parameter(parameters, "value")?;
        Ok(NodeResult::single("value", Value::Float(value)))
    }
}

struct ConstantInteger;

impl NodeInstance for ConstantInteger {
    fn evaluate(
        &self,
        _inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let value = integer_parameter(parameters, "value")?;
        Ok(NodeResult::single("value", Value::Integer(value)))
    }
}

struct ConstantBoolean;

impl NodeInstance for ConstantBoolean {
    fn evaluate(
        &self,
        _inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let value = boolean_parameter(parameters, "value")?;
        Ok(NodeResult::single("value", Value::Boolean(value)))
    }
}

struct ConstantString;

impl NodeInstance for ConstantString {
    fn evaluate(
        &self,
        _inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let value = string_parameter(parameters, "value")?;
        Ok(NodeResult::single("value", Value::String(value)))
    }
}

fn constant_float_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.constant-float", "Constant Float");
    scalar_output(&mut descriptor, "value.Float");
    descriptor.parameters.push(ParameterDescriptor::float(
        "value", "Value", 0.0, None, None,
    ));
    cpu_capability(&mut descriptor);
    descriptor
}

fn constant_integer_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.constant-integer", "Constant Integer");
    scalar_output(&mut descriptor, "value.Integer");
    descriptor
        .parameters
        .push(ParameterDescriptor::integer("value", "Value", 0));
    cpu_capability(&mut descriptor);
    descriptor
}

fn constant_boolean_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.constant-boolean", "Constant Boolean");
    scalar_output(&mut descriptor, "value.Boolean");
    descriptor
        .parameters
        .push(ParameterDescriptor::boolean("value", "Value", false));
    cpu_capability(&mut descriptor);
    descriptor
}

fn constant_string_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.constant-string", "Constant String");
    scalar_output(&mut descriptor, "value.String");
    descriptor
        .parameters
        .push(ParameterDescriptor::string("value", "Value", ""));
    cpu_capability(&mut descriptor);
    descriptor
}

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

struct MetadataNode;

impl NodeInstance for MetadataNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let camera = match inputs.get("camera") {
            Some(Value::CameraMetadata(camera)) => Some(camera.clone()),
            Some(_) => None,
            None => None,
        };
        let exif = match inputs.get("exif") {
            Some(Value::ExifMetadata(exif)) => exif.clone(),
            _ => ExifMetadata::default(),
        };
        if camera.is_none() && !inputs.contains_key("exif") {
            return Err(NodeError::MissingInput("camera".to_owned()));
        }
        let camera = camera.unwrap_or_default();
        let metadata = Metadata::from_sources(&camera, &exif);
        let mut outputs = BTreeMap::new();
        outputs.insert("metadata".to_owned(), Value::Metadata(metadata.clone()));
        outputs.insert("make".to_owned(), Value::String(metadata.make.clone()));
        outputs.insert("model".to_owned(), Value::String(metadata.model.clone()));
        outputs.insert(
            "camera_model".to_owned(),
            Value::String(metadata.model.clone()),
        );
        if let Some(lens) = &metadata.lens {
            outputs.insert("lens_model".to_owned(), Value::String(lens.clone()));
        }
        if let Some(iso) = metadata.iso {
            outputs.insert("iso".to_owned(), Value::Integer(i64::from(iso)));
        }
        if let Some(aperture) = metadata.aperture {
            outputs.insert("aperture".to_owned(), Value::Float(aperture));
        }
        if let Some(shutter) = metadata.shutter_seconds {
            outputs.insert("shutter_seconds".to_owned(), Value::Float(shutter));
        }
        if let Some(focal_length) = metadata.focal_length_mm {
            outputs.insert("focal_length".to_owned(), Value::Float(focal_length));
        }
        if let Some(capture_time) = &metadata.capture_time {
            outputs.insert(
                "capture_time".to_owned(),
                Value::String(capture_time.clone()),
            );
        }
        if let Some(orientation) = &metadata.orientation {
            outputs.insert("orientation".to_owned(), Value::String(orientation.clone()));
        }
        if let Some(rating) = metadata.rating {
            outputs.insert("rating".to_owned(), Value::Integer(i64::from(rating)));
        }
        Ok(NodeResult::new(outputs))
    }
}

fn metadata_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.metadata", "Metadata");
    descriptor.inputs.push(PortDescriptor::input(
        "camera",
        "Camera Metadata",
        "raw.CameraMetadata",
        false,
    ));
    descriptor.inputs.push(PortDescriptor::input(
        "exif",
        "EXIF Metadata",
        "raw.ExifMetadata",
        false,
    ));
    descriptor.outputs.push(PortDescriptor::output(
        "metadata",
        "Metadata",
        "core.Metadata",
    ));
    for (id, name, data_type) in [
        ("make", "Make", "value.String"),
        ("model", "Model", "value.String"),
        ("camera_model", "Camera Model", "value.String"),
        ("lens_model", "Lens Model", "value.String"),
        ("iso", "ISO", "value.Integer"),
        ("aperture", "Aperture", "value.Float"),
        ("shutter_seconds", "Shutter Speed", "value.Float"),
        ("focal_length", "Focal Length", "value.Float"),
        ("capture_time", "Capture Time", "value.String"),
        ("orientation", "Orientation", "value.String"),
        ("rating", "Rating", "value.Integer"),
    ] {
        descriptor
            .outputs
            .push(PortDescriptor::output(id, name, data_type));
    }
    cpu_capability(&mut descriptor);
    descriptor
}

// ---------------------------------------------------------------------------
// Comparisons
// ---------------------------------------------------------------------------

fn compare(operation: &str, a: f32, b: f32, epsilon: f32) -> Result<bool, NodeError> {
    Ok(match operation {
        "==" => (a - b).abs() <= epsilon,
        "!=" => (a - b).abs() > epsilon,
        ">" => a > b,
        ">=" => a >= b,
        "<" => a < b,
        "<=" => a <= b,
        _ => return Err(NodeError::InvalidParameter("operation".to_owned())),
    })
}

struct FixedComparison {
    operation: &'static str,
}

impl NodeInstance for FixedComparison {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let a = float_input(inputs, "a")?;
        let b = float_input(inputs, "b")?;
        let epsilon = parameters
            .get("epsilon")
            .and_then(ParameterValue::as_float)
            .unwrap_or(1.0e-6);
        let result = compare(self.operation, a, b, epsilon)?;
        Ok(NodeResult::single("result", Value::Condition(result)))
    }
}

struct CompareNode;

impl NodeInstance for CompareNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let a = float_input(inputs, "a")?;
        let b = float_input(inputs, "b")?;
        let operation = string_parameter(parameters, "operation")?;
        let epsilon = parameters
            .get("epsilon")
            .and_then(ParameterValue::as_float)
            .unwrap_or(1.0e-6);
        let result = compare(&operation, a, b, epsilon)?;
        Ok(NodeResult::single("result", Value::Condition(result)))
    }
}

fn comparison_descriptor(type_id: &str, name: &str) -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(type_id, name);
    descriptor
        .inputs
        .push(PortDescriptor::input("a", "A", "value.Float", true));
    descriptor
        .inputs
        .push(PortDescriptor::input("b", "B", "value.Float", true));
    descriptor.outputs.push(PortDescriptor::output(
        "result",
        "Result",
        "value.Condition",
    ));
    descriptor.parameters.push(ParameterDescriptor::float(
        "epsilon",
        "Epsilon",
        1.0e-6,
        Some(0.0),
        None,
    ));
    cpu_capability(&mut descriptor);
    descriptor
}

fn compare_descriptor() -> NodeDescriptor {
    let mut descriptor = comparison_descriptor("core.compare", "Compare");
    descriptor
        .parameters
        .push(ParameterDescriptor::string("operation", "Operation", "=="));
    descriptor
}

// ---------------------------------------------------------------------------
// Boolean logic
// ---------------------------------------------------------------------------

struct AndNode;

impl NodeInstance for AndNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let result = condition_input(inputs, "a")? && condition_input(inputs, "b")?;
        Ok(NodeResult::single("result", Value::Condition(result)))
    }
}

struct OrNode;

impl NodeInstance for OrNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let result = condition_input(inputs, "a")? || condition_input(inputs, "b")?;
        Ok(NodeResult::single("result", Value::Condition(result)))
    }
}

struct NotNode;

impl NodeInstance for NotNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let result = !condition_input(inputs, "value")?;
        Ok(NodeResult::single("result", Value::Condition(result)))
    }
}

fn binary_logic_descriptor(type_id: &str, name: &str) -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new(type_id, name);
    for (id, name) in [("a", "A"), ("b", "B")] {
        descriptor
            .inputs
            .push(PortDescriptor::input(id, name, "value.Condition", true));
    }
    descriptor.outputs.push(PortDescriptor::output(
        "result",
        "Result",
        "value.Condition",
    ));
    cpu_capability(&mut descriptor);
    descriptor
}

fn not_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.not", "Not");
    descriptor.inputs.push(PortDescriptor::input(
        "value",
        "Value",
        "value.Condition",
        true,
    ));
    descriptor.outputs.push(PortDescriptor::output(
        "result",
        "Result",
        "value.Condition",
    ));
    cpu_capability(&mut descriptor);
    descriptor
}

// ---------------------------------------------------------------------------
// Routing
// ---------------------------------------------------------------------------

struct SwitchNode;

impl NodeInstance for SwitchNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let condition = condition_input(inputs, "condition")?;
        let port = if condition { "true" } else { "false" };
        let value = inputs
            .get(port)
            .cloned()
            .ok_or_else(|| NodeError::MissingInput(port.to_owned()))?;
        Ok(NodeResult::single("value", value))
    }
}

fn switch_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.switch", "Switch");
    descriptor.inputs.push(PortDescriptor::input(
        "condition",
        "Condition",
        "value.Condition",
        true,
    ));
    for (id, name) in [("true", "True"), ("false", "False")] {
        descriptor
            .inputs
            .push(PortDescriptor::input(id, name, "core.Any", false));
    }
    scalar_output(&mut descriptor, "core.Any");
    descriptor.lazy_inputs.push(LazyInputGate {
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
    });
    cpu_capability(&mut descriptor);
    descriptor
}

struct SelectNode;

impl NodeInstance for SelectNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let index = integer_input(inputs, "index")?;
        let port = match index {
            0 => "a",
            1 => "b",
            2 => "c",
            3 => "d",
            _ => {
                return Err(NodeError::Message(format!(
                    "select index {index} is outside the available range 0..=3"
                )));
            }
        };
        let value = inputs.get(port).cloned().ok_or_else(|| {
            NodeError::Message(format!("no branch is connected to select input '{port}'"))
        })?;
        Ok(NodeResult::single("value", value))
    }
}

fn select_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.select", "Select");
    descriptor.inputs.push(PortDescriptor::input(
        "index",
        "Index",
        "value.Integer",
        true,
    ));
    for (id, name) in [("a", "A"), ("b", "B"), ("c", "C"), ("d", "D")] {
        descriptor
            .inputs
            .push(PortDescriptor::input(id, name, "core.Any", false));
    }
    descriptor.lazy_inputs.push(LazyInputGate {
        selector: "index".to_owned(),
        required: vec!["index".to_owned()],
        branches: ["a", "b", "c", "d"]
            .into_iter()
            .enumerate()
            .map(|(index, id)| LazyBranch {
                condition: LazyCondition::Index(index as u32),
                inputs: vec![id.to_owned()],
            })
            .collect(),
    });
    scalar_output(&mut descriptor, "core.Any");
    cpu_capability(&mut descriptor);
    descriptor
}

struct EnumSelectNode;

impl NodeInstance for EnumSelectNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let selector = string_input(inputs, "selector")?;
        for port in ["a", "b", "c", "d"] {
            let key = parameters
                .get(&format!("match_{port}"))
                .and_then(ParameterValue::as_string)
                .unwrap_or_default();
            if !key.is_empty() && key == selector {
                let value = inputs.get(port).cloned().ok_or_else(|| {
                    NodeError::Message(format!(
                        "enum-select matched '{selector}' but input '{port}' is not connected"
                    ))
                })?;
                return Ok(NodeResult::single("value", value));
            }
        }
        Err(NodeError::Message(format!(
            "no enum-select branch matches '{selector}'"
        )))
    }
}

fn enum_select_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.enum-select", "Enum Select");
    descriptor.inputs.push(PortDescriptor::input(
        "selector",
        "Selector",
        "value.String",
        true,
    ));
    for (id, name) in [("a", "A"), ("b", "B"), ("c", "C"), ("d", "D")] {
        descriptor
            .inputs
            .push(PortDescriptor::input(id, name, "core.Any", false));
        descriptor.parameters.push(ParameterDescriptor::string(
            format!("match_{id}"),
            format!("{name} Matches"),
            "",
        ));
    }
    scalar_output(&mut descriptor, "core.Any");
    cpu_capability(&mut descriptor);
    descriptor
}

// ---------------------------------------------------------------------------
// Scalar transforms
// ---------------------------------------------------------------------------

struct MapRangeNode;

impl NodeInstance for MapRangeNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let value = float_input(inputs, "value")?;
        let in_min = float_parameter(parameters, "in_min")?;
        let in_max = float_parameter(parameters, "in_max")?;
        let out_min = float_parameter(parameters, "out_min")?;
        let out_max = float_parameter(parameters, "out_max")?;
        if (in_max - in_min).abs() <= f32::EPSILON {
            return Err(NodeError::InvalidParameter("in_max".to_owned()));
        }
        let clamp = parameters
            .get("clamp")
            .and_then(ParameterValue::as_boolean)
            .unwrap_or(false);
        let mut mapped = out_min + (value - in_min) / (in_max - in_min) * (out_max - out_min);
        if clamp {
            mapped = mapped.clamp(out_min.min(out_max), out_min.max(out_max));
        }
        Ok(NodeResult::single("value", Value::Float(mapped)))
    }
}

fn map_range_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.map-range", "Map Range");
    descriptor
        .inputs
        .push(PortDescriptor::input("value", "Value", "value.Float", true));
    for (id, name, default) in [
        ("in_min", "Input Minimum", 0.0),
        ("in_max", "Input Maximum", 1.0),
        ("out_min", "Output Minimum", 0.0),
        ("out_max", "Output Maximum", 1.0),
    ] {
        descriptor
            .parameters
            .push(ParameterDescriptor::float(id, name, default, None, None));
    }
    descriptor
        .parameters
        .push(ParameterDescriptor::boolean("clamp", "Clamp", false));
    scalar_output(&mut descriptor, "value.Float");
    cpu_capability(&mut descriptor);
    descriptor
}

struct ClampNode;

impl NodeInstance for ClampNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let value = float_input(inputs, "value")?;
        let min = float_parameter(parameters, "min")?;
        let max = float_parameter(parameters, "max")?;
        if min > max {
            return Err(NodeError::InvalidParameter("min".to_owned()));
        }
        Ok(NodeResult::single(
            "value",
            Value::Float(value.clamp(min, max)),
        ))
    }
}

fn clamp_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.clamp", "Clamp");
    descriptor
        .inputs
        .push(PortDescriptor::input("value", "Value", "value.Float", true));
    descriptor.parameters.push(ParameterDescriptor::float(
        "min", "Minimum", 0.0, None, None,
    ));
    descriptor.parameters.push(ParameterDescriptor::float(
        "max", "Maximum", 1.0, None, None,
    ));
    scalar_output(&mut descriptor, "value.Float");
    cpu_capability(&mut descriptor);
    descriptor
}

/// Parse `x,y;x,y;...` into ascending control points.
fn parse_curve_points(points: &str) -> Result<Vec<(f32, f32)>, NodeError> {
    let invalid = || NodeError::InvalidParameter("points".to_owned());
    let mut parsed = Vec::new();
    for pair in points.split(';') {
        let (x, y) = pair.split_once(',').ok_or_else(invalid)?;
        let x = x.trim().parse::<f32>().map_err(|_| invalid())?;
        let y = y.trim().parse::<f32>().map_err(|_| invalid())?;
        if !x.is_finite() || !y.is_finite() {
            return Err(invalid());
        }
        parsed.push((x, y));
    }
    if parsed.len() < 2 {
        return Err(invalid());
    }
    parsed.sort_by(|left, right| left.0.total_cmp(&right.0));
    if parsed.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(invalid());
    }
    Ok(parsed)
}

fn evaluate_curve(points: &[(f32, f32)], value: f32) -> f32 {
    let first = points[0];
    let last = points[points.len() - 1];
    if value <= first.0 {
        return first.1;
    }
    if value >= last.0 {
        return last.1;
    }
    for pair in points.windows(2) {
        let (x0, y0) = pair[0];
        let (x1, y1) = pair[1];
        if value >= x0 && value <= x1 {
            let span = x1 - x0;
            if span <= 0.0 {
                return y1;
            }
            return y0 + (value - x0) / span * (y1 - y0);
        }
    }
    last.1
}

struct CurveNode;

impl NodeInstance for CurveNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let value = float_input(inputs, "value")?;
        let points = parse_curve_points(&string_parameter(parameters, "points")?)?;
        Ok(NodeResult::single(
            "value",
            Value::Float(evaluate_curve(&points, value)),
        ))
    }
}

fn curve_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.curve", "Curve");
    descriptor
        .inputs
        .push(PortDescriptor::input("value", "Value", "value.Float", true));
    descriptor.parameters.push(ParameterDescriptor::string(
        "points",
        "Control Points",
        "0,0;1,1",
    ));
    scalar_output(&mut descriptor, "value.Float");
    cpu_capability(&mut descriptor);
    descriptor
}

// ---------------------------------------------------------------------------
// Expression
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(f64),
    Identifier(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LeftParen,
    RightParen,
}

fn tokenize(expression: &str) -> Result<Vec<Token>, NodeError> {
    let invalid = || NodeError::InvalidParameter("expression".to_owned());
    let mut tokens = Vec::new();
    let characters: Vec<char> = expression.chars().collect();
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        match character {
            ' ' | '\t' | '\n' | '\r' => index += 1,
            '+' => {
                tokens.push(Token::Plus);
                index += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                index += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                index += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                index += 1;
            }
            '%' => {
                tokens.push(Token::Percent);
                index += 1;
            }
            '(' => {
                tokens.push(Token::LeftParen);
                index += 1;
            }
            ')' => {
                tokens.push(Token::RightParen);
                index += 1;
            }
            character if character.is_ascii_digit() || character == '.' => {
                let start = index;
                while index < characters.len()
                    && (characters[index].is_ascii_digit() || characters[index] == '.')
                {
                    index += 1;
                }
                let literal: String = characters[start..index].iter().collect();
                let number = literal.parse::<f64>().map_err(|_| invalid())?;
                if !number.is_finite() {
                    return Err(invalid());
                }
                tokens.push(Token::Number(number));
            }
            character if character.is_ascii_alphabetic() || character == '_' => {
                let start = index;
                while index < characters.len()
                    && (characters[index].is_ascii_alphanumeric() || characters[index] == '_')
                {
                    index += 1;
                }
                tokens.push(Token::Identifier(characters[start..index].iter().collect()));
            }
            _ => return Err(invalid()),
        }
    }
    Ok(tokens)
}

struct ExpressionParser<'a> {
    tokens: &'a [Token],
    position: usize,
    variables: &'a BTreeMap<String, f64>,
}

impl<'a> ExpressionParser<'a> {
    fn new(tokens: &'a [Token], variables: &'a BTreeMap<String, f64>) -> Self {
        Self {
            tokens,
            position: 0,
            variables,
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position).cloned();
        if token.is_some() {
            self.position += 1;
        }
        token
    }

    fn invalid(&self) -> NodeError {
        NodeError::InvalidParameter("expression".to_owned())
    }

    fn parse_expression(&mut self) -> Result<f64, NodeError> {
        let mut value = self.parse_term()?;
        while let Some(token) = self.peek() {
            match token {
                Token::Plus => {
                    self.advance();
                    value += self.parse_term()?;
                }
                Token::Minus => {
                    self.advance();
                    value -= self.parse_term()?;
                }
                _ => break,
            }
        }
        Ok(value)
    }

    fn parse_term(&mut self) -> Result<f64, NodeError> {
        let mut value = self.parse_factor()?;
        while let Some(token) = self.peek() {
            match token {
                Token::Star => {
                    self.advance();
                    value *= self.parse_factor()?;
                }
                Token::Slash => {
                    self.advance();
                    let divisor = self.parse_factor()?;
                    if divisor == 0.0 {
                        return Err(self.invalid());
                    }
                    value /= divisor;
                }
                Token::Percent => {
                    self.advance();
                    let divisor = self.parse_factor()?;
                    if divisor == 0.0 {
                        return Err(self.invalid());
                    }
                    value %= divisor;
                }
                _ => break,
            }
        }
        Ok(value)
    }

    fn parse_factor(&mut self) -> Result<f64, NodeError> {
        match self.advance() {
            Some(Token::Minus) => Ok(-self.parse_factor()?),
            Some(Token::Plus) => self.parse_factor(),
            Some(Token::Number(number)) => Ok(number),
            Some(Token::Identifier(name)) => self
                .variables
                .get(&name)
                .copied()
                .ok_or_else(|| self.invalid()),
            Some(Token::LeftParen) => {
                let value = self.parse_expression()?;
                match self.advance() {
                    Some(Token::RightParen) => Ok(value),
                    _ => Err(self.invalid()),
                }
            }
            _ => Err(self.invalid()),
        }
    }
}

fn evaluate_expression(
    expression: &str,
    variables: &BTreeMap<String, f64>,
) -> Result<f64, NodeError> {
    let tokens = tokenize(expression)?;
    if tokens.is_empty() {
        return Err(NodeError::InvalidParameter("expression".to_owned()));
    }
    let mut parser = ExpressionParser::new(&tokens, variables);
    let value = parser.parse_expression()?;
    if parser.position != tokens.len() || !value.is_finite() {
        return Err(NodeError::InvalidParameter("expression".to_owned()));
    }
    Ok(value)
}

struct ExpressionNode;

impl NodeInstance for ExpressionNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let expression = string_parameter(parameters, "expression")?;
        let mut variables = BTreeMap::new();
        for id in ["a", "b", "c", "d"] {
            let value = match inputs.get(id) {
                Some(Value::Float(value)) if value.is_finite() => f64::from(*value),
                Some(Value::Integer(value)) => *value as f64,
                Some(_) => return Err(NodeError::MissingInput(id.to_owned())),
                None => 0.0,
            };
            variables.insert(id.to_owned(), value);
        }
        let value = evaluate_expression(&expression, &variables)?;
        Ok(NodeResult::single("value", Value::Float(value as f32)))
    }
}

fn expression_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.expression", "Expression");
    for (id, name) in [("a", "A"), ("b", "B"), ("c", "C"), ("d", "D")] {
        descriptor
            .inputs
            .push(PortDescriptor::input(id, name, "value.Float", false));
    }
    descriptor
        .parameters
        .push(ParameterDescriptor::string("expression", "Expression", "0"));
    scalar_output(&mut descriptor, "value.Float");
    cpu_capability(&mut descriptor);
    descriptor
}

// ---------------------------------------------------------------------------
// Strings
// ---------------------------------------------------------------------------

struct StringMatchNode;

impl NodeInstance for StringMatchNode {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let value = string_input(inputs, "value")?;
        let pattern = string_parameter(parameters, "pattern")?;
        let case_sensitive = parameters
            .get("case_sensitive")
            .and_then(ParameterValue::as_boolean)
            .unwrap_or(true);
        let matched = if case_sensitive {
            value == pattern
        } else {
            value.to_lowercase() == pattern.to_lowercase()
        };
        Ok(NodeResult::single("result", Value::Condition(matched)))
    }
}

fn string_match_descriptor() -> NodeDescriptor {
    let mut descriptor = NodeDescriptor::new("core.string-match", "String Match");
    descriptor.inputs.push(PortDescriptor::input(
        "value",
        "Value",
        "value.String",
        true,
    ));
    descriptor
        .parameters
        .push(ParameterDescriptor::string("pattern", "Pattern", ""));
    descriptor.parameters.push(ParameterDescriptor::boolean(
        "case_sensitive",
        "Case Sensitive",
        true,
    ));
    descriptor.outputs.push(PortDescriptor::output(
        "result",
        "Result",
        "value.Condition",
    ));
    cpu_capability(&mut descriptor);
    descriptor
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

fn constant_integer() -> Box<dyn NodeInstance> {
    Box::new(ConstantInteger)
}

fn constant_float() -> Box<dyn NodeInstance> {
    Box::new(ConstantFloat)
}

fn constant_boolean() -> Box<dyn NodeInstance> {
    Box::new(ConstantBoolean)
}

fn constant_string() -> Box<dyn NodeInstance> {
    Box::new(ConstantString)
}

pub fn register_nodes(registry: &mut NodeRegistry) -> Result<(), RegistryError> {
    registry.register(constant_float_descriptor(), constant_float)?;
    registry.register(constant_integer_descriptor(), constant_integer)?;
    registry.register(constant_boolean_descriptor(), constant_boolean)?;
    registry.register(constant_string_descriptor(), constant_string)?;
    registry.register(metadata_descriptor(), || Box::new(MetadataNode))?;
    registry.register(comparison_descriptor("core.equal", "Equal"), || {
        Box::new(FixedComparison { operation: "==" })
    })?;
    registry.register(
        comparison_descriptor("core.greater-than", "Greater Than"),
        || Box::new(FixedComparison { operation: ">" }),
    )?;
    registry.register(comparison_descriptor("core.less-than", "Less Than"), || {
        Box::new(FixedComparison { operation: "<" })
    })?;
    registry.register(compare_descriptor(), || Box::new(CompareNode))?;
    registry.register(binary_logic_descriptor("core.and", "And"), || {
        Box::new(AndNode)
    })?;
    registry.register(binary_logic_descriptor("core.or", "Or"), || {
        Box::new(OrNode)
    })?;
    registry.register(not_descriptor(), || Box::new(NotNode))?;
    registry.register(switch_descriptor(), || Box::new(SwitchNode))?;
    registry.register(select_descriptor(), || Box::new(SelectNode))?;
    registry.register(enum_select_descriptor(), || Box::new(EnumSelectNode))?;
    registry.register(map_range_descriptor(), || Box::new(MapRangeNode))?;
    registry.register(clamp_descriptor(), || Box::new(ClampNode))?;
    registry.register(curve_descriptor(), || Box::new(CurveNode))?;
    registry.register(expression_descriptor(), || Box::new(ExpressionNode))?;
    registry.register(string_match_descriptor(), || Box::new(StringMatchNode))
}

pub struct CoreValuesPack;

impl NodePack for CoreValuesPack {
    fn id(&self) -> &'static str {
        "core-values"
    }

    fn register(&self, registry: &mut NodeRegistry) -> Result<(), RegistryError> {
        register_nodes(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn expression_parser_handles_precedence_and_parens() {
        let variables = BTreeMap::new();
        assert_eq!(evaluate_expression("2 + 3 * 4", &variables).unwrap(), 14.0);
        assert_eq!(
            evaluate_expression("(2 + 3) * 4", &variables).unwrap(),
            20.0
        );
        assert_eq!(evaluate_expression("-2 - -3", &variables).unwrap(), 1.0);
    }

    #[test]
    fn curve_parser_sorts_points_and_rejects_duplicates() {
        let points = parse_curve_points("1,1;0,0;0.5,0.7").unwrap();
        assert_eq!(points, vec![(0.0, 0.0), (0.5, 0.7), (1.0, 1.0)]);
        assert!(parse_curve_points("0,0;0,1").is_err());
    }
}
