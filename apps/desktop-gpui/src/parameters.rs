//! Shared presentation metadata and transactional parameter drafts. Rust nodes own processing.
use rawweave_node_api::{ParameterDescriptor, ParameterType, ParameterValue};
use serde_json::Value;
use std::sync::OnceLock;

const UX_JSON: &str = include_str!("../../desktop/frontend/src/editor/parameter-ux.json");
static UX_DATA: OnceLock<Result<Value, serde_json::Error>> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct ParameterUX {
    pub name: String,
    pub description: String,
    pub unit: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: f64,
    pub precision: usize,
    pub factor: f64,
    pub advanced: bool,
    pub recommended_range: bool,
    pub options: Vec<(String, String)>,
    pub whole_number: bool,
    pub point_curve: bool,
    pub scalar_curve: bool,
    pub multiline: bool,
}
impl ParameterUX {
    pub fn range(&self) -> Option<(f64, f64)> {
        let (min, max) = (self.min?, self.max?);
        (min.is_finite() && max.is_finite() && max > min).then_some((min, max))
    }
}
fn readable(id: &str) -> String {
    let mut text = String::new();
    let mut previous_lower = false;
    let mut word_start = true;
    for c in id.chars() {
        if c == '_' || c == '-' {
            text.push(' ');
            word_start = true;
            previous_lower = false;
            continue;
        }
        if previous_lower && c.is_ascii_uppercase() {
            text.push(' ');
            word_start = true;
        }
        text.push(if word_start {
            c.to_ascii_uppercase()
        } else {
            c
        });
        word_start = false;
        previous_lower = c.is_ascii_lowercase();
    }
    text
}
fn literal(value: &ParameterValue) -> String {
    match value {
        ParameterValue::Float(v) => v.to_string(),
        ParameterValue::Integer(v) => v.to_string(),
        ParameterValue::Boolean(v) => v.to_string(),
        ParameterValue::String(v) => v.clone(),
    }
}
pub fn parameter_ux(type_id: &str, parameter: &ParameterDescriptor) -> ParameterUX {
    // The TS compiler and regression tests validate the embedded JSON; backend descriptor defaults remain usable on failure.
    let data = UX_DATA
        .get_or_init(|| serde_json::from_str(UX_JSON))
        .as_ref()
        .ok();
    let exact = data.and_then(|data| data.get("specific")).and_then(|map| {
        map.get(format!("{type_id}:{}", parameter.id)).or_else(|| {
            map.get(format!(
                "{}.{}",
                type_id.split('.').next().unwrap_or_default(),
                parameter.id
            ))
        })
    });
    let text = |key: &str| {
        exact
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let number = |key: &str| {
        exact
            .and_then(|value| value.get(key))
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
    };
    let generic = |section: &str| {
        data.and_then(|data| data.get(section))
            .and_then(|map| map.get(&parameter.id))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let contains = |section: &str, value: &str| {
        data.and_then(|data| data.get(section))
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(|entry| entry.as_str() == Some(value)))
    };
    let numeric = matches!(
        parameter.parameter_type,
        ParameterType::Float | ParameterType::Integer
    );
    let point_curve = parameter.id == "points" && contains("pointCurveNodes", type_id);
    let whole_number = parameter.parameter_type == ParameterType::Integer
        || matches!(type_id, "core.crop" | "core.resize")
            && matches!(parameter.id.as_str(), "x" | "y" | "width" | "height")
        || matches!(
            type_id,
            "core.blur"
                | "core.mask-feather"
                | "core.mask-blur"
                | "core.mask-expand"
                | "core.mask-contract"
        ) && parameter.id == "radius"
        || type_id == "core.mask-painted"
            && matches!(
                parameter.id.as_str(),
                "width" | "height" | "origin_x" | "origin_y"
            );
    let name = text("name")
        .or_else(|| generic("commonNames"))
        .unwrap_or_else(|| parameter.name.clone());
    let description = if point_curve {
        "Linear interpolation between x,y pairs separated by semicolons. Input is horizontal; output is vertical. Enter or leave the field to apply; Escape cancels.".into()
    } else {
        text("description").or_else(||generic("commonDescriptions")).unwrap_or_else(||format!("{name} controls the {} used by this {} node. Increase the value to apply more of this effect; decrease it to apply less. Default: {}.",readable(&parameter.id).to_lowercase(),readable(type_id.split('.').next_back().unwrap_or("operation")).to_lowercase(),literal(&parameter.default)))
    };
    let matrix = parameter.id.len() == 3
        && parameter.id.starts_with('m')
        && parameter
            .id
            .get(1..)
            .is_some_and(|s| s.bytes().all(|c| c.is_ascii_digit()));
    ParameterUX {
        name,
        description,
        unit: text("unit"),
        min: number("min").or_else(|| {
            if numeric {
                parameter.min.map(f64::from)
            } else {
                None
            }
        }),
        max: number("max").or_else(|| {
            if numeric {
                parameter.max.map(f64::from)
            } else {
                None
            }
        }),
        step: if whole_number {
            1.0
        } else {
            number("step").filter(|v| *v > 0.0).unwrap_or(0.01)
        },
        precision: if whole_number {
            0
        } else {
            number("precision")
                .map(|v| v.clamp(0.0, 9.0) as usize)
                .unwrap_or(2)
        },
        factor: number("factor").filter(|v| *v > 0.0).unwrap_or(1.0),
        advanced: !point_curve
            && exact
                .and_then(|v| v.get("advanced"))
                .and_then(Value::as_bool)
                .unwrap_or_else(|| {
                    contains("advancedIds", &parameter.id)
                        || matrix
                        || parameter.id.starts_with("offset_")
                }),
        recommended_range: number("min").is_some() && number("max").is_some(),
        options: exact
            .and_then(|v| v.get("options"))
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|v| {
                        Some((
                            v.get("value")?.as_str()?.to_owned(),
                            v.get("label")?.as_str()?.to_owned(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default(),
        whole_number,
        point_curve,
        scalar_curve: type_id == "core.curve",
        multiline: matches!(
            parameter.id.as_str(),
            "prompt" | "negative_prompt" | "workflow_definition" | "points" | "expression"
        ),
    }
}

pub struct ParameterDraft {
    pub descriptor: ParameterDescriptor,
    pub ux: ParameterUX,
    committed: ParameterValue,
    text: String,
    dirty: bool,
}
impl ParameterDraft {
    pub fn new(
        descriptor: ParameterDescriptor,
        ux: ParameterUX,
        committed: ParameterValue,
    ) -> Self {
        let mut result = Self {
            descriptor,
            ux,
            committed,
            text: String::new(),
            dirty: false,
        };
        result.cancel();
        result
    }
    pub fn numeric(&self) -> bool {
        matches!(
            self.descriptor.parameter_type,
            ParameterType::Float | ParameterType::Integer
        )
    }
    pub fn committed(&self) -> &ParameterValue {
        &self.committed
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn set_text(&mut self, text: String) {
        if text != self.text {
            self.text = text;
            self.dirty = true;
        }
    }
    pub fn shown_number(&self) -> Option<f64> {
        match &self.committed {
            ParameterValue::Float(v) => Some(f64::from(*v) * self.ux.factor),
            ParameterValue::Integer(v) => Some(*v as f64 * self.ux.factor),
            _ => None,
        }
    }
    pub fn cancel(&mut self) {
        self.text = match &self.committed {
            ParameterValue::Float(v) => {
                format!("{:.*}", self.ux.precision, f64::from(*v) * self.ux.factor)
            }
            _ => literal(&self.committed),
        };
        self.dirty = false;
    }
    pub fn set_committed(&mut self, value: ParameterValue) {
        self.committed = value;
        self.cancel();
    }
    pub fn modified(&self) -> bool {
        self.committed != self.descriptor.default
    }
    pub fn outside_recommended(&self) -> bool {
        self.ux.recommended_range
            && self
                .ux
                .range()
                .is_some_and(|(min, max)| match self.committed {
                    ParameterValue::Float(v) => f64::from(v) < min || f64::from(v) > max,
                    ParameterValue::Integer(v) => (v as f64) < min || (v as f64) > max,
                    _ => false,
                })
    }
    pub fn parsed(&self) -> Result<ParameterValue, String> {
        let invalid = || format!("Enter a valid {}", self.ux.name);
        let value = match self.descriptor.parameter_type {
            ParameterType::Float => {
                if self.text.trim().is_empty() {
                    return Err(invalid());
                }
                let value =
                    self.text.trim().parse::<f64>().map_err(|_| invalid())? / self.ux.factor;
                if !value.is_finite()
                    || value.abs() > f64::from(f32::MAX)
                    || self.ux.whole_number && value.fract() != 0.0
                {
                    return Err(invalid());
                }
                ParameterValue::Float(value as f32)
            }
            ParameterType::Integer => {
                ParameterValue::Integer(self.text.trim().parse::<i64>().map_err(|_| invalid())?)
            }
            ParameterType::Boolean => {
                ParameterValue::Boolean(self.text.trim().parse::<bool>().map_err(|_| invalid())?)
            }
            ParameterType::String => {
                if self.ux.point_curve {
                    curve_points(&self.text, self.ux.scalar_curve)?;
                }
                ParameterValue::String(self.text.clone())
            }
        };
        let numeric = match value {
            ParameterValue::Float(v) => Some(f64::from(v)),
            ParameterValue::Integer(v) => Some(v as f64),
            _ => None,
        };
        if let Some(number) = numeric
            && (self
                .descriptor
                .min
                .is_some_and(|min| number < f64::from(min))
                || self
                    .descriptor
                    .max
                    .is_some_and(|max| number > f64::from(max)))
        {
            return Err(format!(
                "{} is outside the node's allowed bounds",
                self.ux.name
            ));
        }
        Ok(value)
    }
    pub fn take_commit(&mut self) -> Result<Option<ParameterValue>, String> {
        if !self.dirty {
            return Ok(None);
        }
        let value = self.parsed()?;
        self.dirty = false;
        if value == self.committed {
            return Ok(None);
        }
        self.committed = value.clone();
        Ok(Some(value))
    }
    pub fn set_slider(&mut self, shown: f32) -> Result<(), String> {
        if !shown.is_finite() {
            return Err("Invalid slider value".into());
        }
        self.set_text(format!("{:.*}", self.ux.precision, f64::from(shown)));
        self.parsed().map(|_| ())
    }
    /// Repeated key steps remain local until key release or blur.
    pub fn step(&mut self, multiplier: f64) -> Result<(), String> {
        if !multiplier.is_finite() {
            return Err("Invalid numeric step".into());
        }
        let next = match self.parsed()? {
            ParameterValue::Integer(value) => {
                if multiplier.fract() != 0.0
                    || multiplier >= -(i64::MIN as f64)
                    || multiplier < i64::MIN as f64
                {
                    return Err("Invalid integer step".into());
                }
                ParameterValue::Integer(
                    value
                        .checked_add(multiplier as i64)
                        .ok_or("Integer step exceeds limits")?,
                )
            }
            ParameterValue::Float(value) => {
                let next = f64::from(value) + self.ux.step * multiplier;
                if !next.is_finite() || next.abs() > f64::from(f32::MAX) {
                    return Err("Numeric step exceeds limits".into());
                }
                let min = self.descriptor.min.map_or(f64::NEG_INFINITY, f64::from);
                let max = self.descriptor.max.map_or(f64::INFINITY, f64::from);
                if min.is_nan() || max.is_nan() || min > max {
                    return Err("Invalid parameter bounds".into());
                }
                ParameterValue::Float(next.clamp(min, max) as f32)
            }
            _ => return Err("Not a numeric parameter".into()),
        };
        let text = match next {
            ParameterValue::Float(value) => {
                round_trip_display(value, self.ux.factor, self.ux.precision)
            }
            _ => literal(&next),
        };
        self.set_text(text);
        Ok(())
    }
}

fn round_trip_display(value: f32, factor: f64, min_precision: usize) -> String {
    let shown = f64::from(value) * factor;
    for precision in min_precision..=9 {
        let text = format!("{shown:.precision$}");
        if text
            .parse::<f64>()
            .is_ok_and(|v| (v / factor) as f32 == value)
        {
            return if text.contains('.') {
                text.trim_end_matches('0').trim_end_matches('.').to_owned()
            } else {
                text
            };
        }
    }
    shown.to_string()
}

/// Authored parameter diagram only, not an evaluated image/value output.
pub fn transfer_points(
    kind: &str,
    values: &std::collections::BTreeMap<String, ParameterValue>,
    scene: bool,
) -> Result<Vec<(f32, f32)>, String> {
    let number = |key: &str, fallback: f32| match values.get(key) {
        Some(ParameterValue::Float(value)) => *value,
        Some(ParameterValue::Integer(value)) => *value as f32,
        None => fallback,
        _ => f32::NAN,
    };
    let mut points = vec![];
    if kind == "core.levels" {
        let (black, white, gamma) = (
            number("black_point", 0.0),
            number("white_point", 1.0),
            number("gamma", 1.0),
        );
        if ![black, white, gamma].iter().all(|value| value.is_finite())
            || white <= black
            || gamma <= 0.0
        {
            return Err(
                "White must exceed black; gamma must be finite and greater than zero.".into(),
            );
        }
        for i in 0..=64 {
            let x = f64::from(black.min(0.0))
                + f64::from(white.max(1.0) - black.min(0.0)) * f64::from(i) / 64.0;
            let normalized = (x - f64::from(black)) / (f64::from(white) - f64::from(black));
            let y = if scene {
                normalized.signum() * normalized.abs().powf(1.0 / f64::from(gamma))
            } else {
                normalized.clamp(0.0, 1.0).powf(1.0 / f64::from(gamma))
            };
            points.push((x as f32, y as f32));
        }
    } else if matches!(kind, "core.map-range" | "core.clamp") {
        let mapping = kind == "core.map-range";
        let (start, end) = (
            number(if mapping { "in_min" } else { "min" }, 0.0),
            number(if mapping { "in_max" } else { "max" }, 1.0),
        );
        let (out_start, out_end) = if mapping {
            (number("out_min", 0.0), number("out_max", 1.0))
        } else {
            (start, end)
        };
        let clamp = if mapping {
            match values.get("clamp") {
                Some(ParameterValue::Boolean(value)) => *value,
                None => false,
                _ => return Err("Clamp must be a boolean.".into()),
            }
        } else {
            true
        };
        if ![start, end, out_start, out_end]
            .iter()
            .all(|value| value.is_finite())
            || (mapping && (end - start).abs() <= f32::EPSILON)
            || (!mapping && start > end)
        {
            return Err("Enter finite range endpoints; mapping inputs must be distinct and clamp minimum must not exceed maximum.".into());
        }
        let (low, high) = (f64::from(start.min(end)), f64::from(start.max(end)));
        let padding = if high == low {
            low.abs().max(1.0) / 4.0
        } else {
            (high - low) / 4.0
        };
        for x in [low - padding, low, high, high + padding] {
            let y = if mapping {
                f64::from(out_start)
                    + (x - f64::from(start)) / (f64::from(end) - f64::from(start))
                        * f64::from(out_end - out_start)
            } else {
                x
            };
            let y = if clamp {
                y.clamp(
                    f64::from(out_start.min(out_end)),
                    f64::from(out_start.max(out_end)),
                )
            } else {
                y
            };
            points.push((x as f32, y as f32));
        }
    } else {
        return Err("No transfer reference for this node.".into());
    }
    if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return Err("This range cannot be drawn with finite coordinates.".into());
    }
    Ok(points)
}

pub fn curve_points(text: &str, scalar: bool) -> Result<Vec<(f32, f32)>, String> {
    let points = if scalar {
        parse_points(text)?
    } else {
        parse_image_points(text)?
    };
    if !scalar && points.iter().any(|(x, _)| !(0.0..=1.0).contains(x)) {
        return Err("Image curve inputs must be between 0 and 1".into());
    }
    Ok(points)
}
/// Move an existing point without crossing neighbours; exact text editing remains available for HDR values.
pub fn move_curve_point(
    text: &str,
    scalar: bool,
    index: usize,
    x: f32,
    y: f32,
) -> Result<String, String> {
    let mut points = curve_points(text, scalar)?;
    if !x.is_finite() || !y.is_finite() || index >= points.len() {
        return Err("Invalid curve point".into());
    }
    let original = points[index];
    let x = if index == 0 || index + 1 == points.len() {
        original.0
    } else {
        let low = points[index - 1].0;
        let high = points[index + 1].0;
        if x <= low || x >= high { original.0 } else { x }
    };
    points[index] = (x, y);
    let text = serialize_points(&points)?;
    curve_points(&text, scalar)?;
    Ok(text)
}

fn serialize_points(points: &[(f32, f32)]) -> Result<String, String> {
    let text = points
        .iter()
        .map(|(x, y)| format!("{x},{y}"))
        .collect::<Vec<_>>()
        .join(";");
    if text.len() > 64 * 1024 {
        return Err("Curve display limit: 64 KiB".into());
    }
    Ok(text)
}
pub fn add_curve_point(text: &str, scalar: bool) -> Result<(String, usize), String> {
    let mut points = curve_points(text, scalar)?;
    if points.len() >= 4096 {
        return Err("Curve display limit: 4096 points".into());
    }
    let (index, pair) = points
        .windows(2)
        .enumerate()
        .max_by(|(a_index, a), (b_index, b)| {
            let gap = |pair: &[(f32, f32)]| {
                pair.first()
                    .zip(pair.last())
                    .map_or(0.0, |(a, b)| f64::from(b.0) - f64::from(a.0))
            };
            gap(a).total_cmp(&gap(b)).then_with(|| b_index.cmp(a_index))
        })
        .ok_or("Enter at least two curve points")?;
    let (a, b) = pair
        .first()
        .zip(pair.last())
        .ok_or("Invalid point interval")?;
    let next = (
        ((f64::from(a.0) + f64::from(b.0)) / 2.0) as f32,
        ((f64::from(a.1) + f64::from(b.1)) / 2.0) as f32,
    );
    if next.0 <= a.0 || next.0 >= b.0 {
        return Err("No distinct f32 midpoint fits this interval".into());
    }
    let selected = index + 1;
    points.insert(selected, next);
    Ok((serialize_points(&points)?, selected))
}
pub fn remove_curve_point(text: &str, scalar: bool, index: usize) -> Result<String, String> {
    let mut points = curve_points(text, scalar)?;
    if index == 0 || index >= points.len().saturating_sub(1) {
        return Err("Curve endpoints cannot be deleted".into());
    }
    points.remove(index);
    serialize_points(&points)
}

pub fn parse_points(text: &str) -> Result<Vec<(f32, f32)>, String> {
    parse_point_pairs(text, false)
}
fn parse_image_points(text: &str) -> Result<Vec<(f32, f32)>, String> {
    parse_point_pairs(text, true)
}
fn parse_point_pairs(text: &str, skip_empty: bool) -> Result<Vec<(f32, f32)>, String> {
    if text.len() > 64 * 1024 {
        return Err("Curve display limit: 64 KiB".into());
    }
    let mut points = Vec::new();
    for pair in text.split(';') {
        if skip_empty && pair.trim().is_empty() {
            continue;
        }
        if points.len() >= 4096 {
            return Err("Curve display limit: 4096 points".into());
        }
        let (x, y) = pair
            .split_once(',')
            .ok_or("Enter finite x,y pairs separated by semicolons")?;
        let (x, y) = (
            x.trim().parse::<f32>().map_err(|_| "Invalid curve input")?,
            y.trim()
                .parse::<f32>()
                .map_err(|_| "Invalid curve output")?,
        );
        if !x.is_finite() || !y.is_finite() {
            return Err("Curve coordinates must be finite f32 values".into());
        }
        points.push((x, y));
    }
    if points.len() < 2 {
        return Err("Enter at least two curve points".into());
    }
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    if points.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err("Curve inputs must be distinct at f32 precision".into());
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawweave_node_api::{ParameterDescriptor, ParameterValue};

    #[test]
    fn parameter_ports_round_trip_override_literals_and_disconnect_when_hidden() {
        use rawweave_node_api::{EvaluationContext, Value};
        let mut session = crate::Session::default();
        session
            .editor
            .add_node("flag", "core.constant-boolean")
            .unwrap();
        session
            .editor
            .add_node("remote", "core.constant-boolean")
            .unwrap();
        session
            .editor
            .set_node_parameter("remote", "value", ParameterValue::Boolean(true))
            .unwrap();
        session.editor.expose_parameter("flag", "value").unwrap();
        session
            .editor
            .connect("remote", "value", "flag", "value")
            .unwrap();
        let document = session.editor.save_workflow().unwrap();
        let mut restored = crate::Session::default();
        restored.load_workflow(&document).unwrap();
        assert!(matches!(
            restored
                .editor
                .evaluate("flag", "value", EvaluationContext::default())
                .unwrap(),
            Value::Boolean(true)
        ));
        restored.editor.unexpose_parameter("flag", "value").unwrap();
        assert!(restored.editor.graph().edges().is_empty());
        assert!(matches!(
            restored
                .editor
                .evaluate("flag", "value", EvaluationContext::default())
                .unwrap(),
            Value::Boolean(false)
        ));
        restored.load_workflow(&document).unwrap();
        assert!(matches!(
            restored
                .editor
                .evaluate("flag", "value", EvaluationContext::default())
                .unwrap(),
            Value::Boolean(true)
        ));
    }

    #[test]
    fn shared_metadata_keeps_units_factors_choices_and_advanced_fields() {
        let exposure = parameter_ux(
            "core.exposure",
            &ParameterDescriptor::float("exposure", "Exposure", 0.0, None, None),
        );
        assert_eq!(exposure.unit.as_deref(), Some("EV"));
        assert_eq!(exposure.range(), Some((-5.0, 5.0)));
        assert!(exposure.recommended_range);
        let strength = parameter_ux(
            "raw.highlight-reconstruction",
            &ParameterDescriptor::float("strength", "Strength", 1.0, Some(0.0), Some(1.0)),
        );
        assert_eq!(strength.factor, 100.0);
        let hue = parameter_ux(
            "pro.color-zones",
            &ParameterDescriptor::float("hue", "Hue", 0.0, None, None),
        );
        assert_eq!(hue.factor, 360.0);
        assert!((hue.step - 1.0 / 360.0).abs() < 1e-10);
        let operator = parameter_ux(
            "pro.tone-map",
            &ParameterDescriptor::string("operator", "Operator", "reinhard"),
        );
        assert_eq!(
            operator
                .options
                .iter()
                .map(|(v, _)| v.as_str())
                .collect::<Vec<_>>(),
            ["reinhard", "filmic", "aces"]
        );
        assert!(
            parameter_ux(
                "ai.generate",
                &ParameterDescriptor::integer("seed", "Seed", 0)
            )
            .advanced
        );
        assert!(
            !parameter_ux(
                "pro.film-simulation",
                &ParameterDescriptor::string("points", "Points", "0,0;1,1")
            )
            .advanced
        );
    }

    #[test]
    fn scene_levels_reference_uses_signed_unclipped_gamma() {
        let values = [
            ("black_point".into(), ParameterValue::Float(0.25)),
            ("white_point".into(), ParameterValue::Float(0.75)),
            ("gamma".into(), ParameterValue::Float(2.0)),
        ]
        .into();
        let scene = transfer_points("core.levels", &values, true).unwrap();
        assert!(scene.first().unwrap().1 < 0.0);
        assert!(scene.last().unwrap().1 > 1.0);
        let image = transfer_points("core.levels", &values, false).unwrap();
        assert_eq!(image.first().unwrap().1, 0.0);
        assert_eq!(image.last().unwrap().1, 1.0);
    }

    #[test]
    fn curve_list_tools_preserve_hdr_and_locked_endpoints() {
        let levels = transfer_points("core.levels", &Default::default(), false).unwrap();
        assert_eq!(levels[32], (0.5, 0.5));
        let values = [
            ("in_min".into(), ParameterValue::Float(1.0)),
            ("in_max".into(), ParameterValue::Float(0.0)),
        ]
        .into();
        let mapped = transfer_points("core.map-range", &values, false).unwrap();
        assert!(
            mapped.first().unwrap().1 > 1.0 && mapped.last().unwrap().1 < 0.0,
            "reversed ranges must extrapolate, not clip"
        );
        let values = [("black_point".into(), ParameterValue::Float(2.0))].into();
        assert!(transfer_points("core.levels", &values, false).is_err());
        assert_eq!(
            move_curve_point("0,0;0.5,0.7;1,1", false, 1, 0.6, 2.0).unwrap(),
            "0,0;0.6,2;1,1"
        );
        assert_eq!(
            move_curve_point("-5,-2;5,12", true, 0, 10.0, -4.0).unwrap(),
            "-5,-4;5,12"
        );
        assert!(move_curve_point("0,0;1,1", false, 1, 0.5, f32::NAN).is_err());
        let (text, index) = add_curve_point("-5,-2;5,12", true).unwrap();
        assert_eq!(text, "-5,-2;0,5;5,12");
        assert_eq!(index, 1);
        assert_eq!(
            add_curve_point("0,0;0.5,0.7;1,1", false).unwrap(),
            ("0,0;0.25,0.35;0.5,0.7;1,1".into(), 1)
        );
        assert_eq!(
            remove_curve_point(&text, true, index).unwrap(),
            "-5,-2;5,12"
        );
        assert!(remove_curve_point(&text, true, 0).is_err());
        assert!(remove_curve_point(&text, true, 2).is_err());
        assert!(remove_curve_point(&text, true, usize::MAX).is_err());
        assert!(add_curve_point("0,0;NaN,1", true).is_err());
        assert!(add_curve_point("-1,0;1,1", false).is_err());
        let near = format!("{},0;{},1", 1.0f32, 1.0f32.next_up());
        assert!(add_curve_point(&near, true).is_err());
        let maximum = (0..4096)
            .map(|i| format!("{i},{i}"))
            .collect::<Vec<_>>()
            .join(";");
        assert!(add_curve_point(&maximum, true).is_err());
        assert!(add_curve_point("0,3.4e38;1,3.4e38", true).is_ok());
    }

    #[test]
    fn multiline_prompts_preserve_lines_and_cancel_uncommitted_edits() {
        let descriptor = ParameterDescriptor::string("prompt", "Prompt", "");
        let ux = parameter_ux("ai.generate", &descriptor);
        assert!(ux.multiline);
        let mut draft = ParameterDraft::new(descriptor, ux, ParameterValue::String(String::new()));
        draft.set_text("First line\n第二行\nLast line".into());
        assert_eq!(
            draft.take_commit().unwrap(),
            Some(ParameterValue::String(
                "First line\n第二行\nLast line".into()
            ))
        );
        draft.set_text("Discard this\nnew line".into());
        draft.cancel();
        assert_eq!(draft.take_commit().unwrap(), None);
        assert_eq!(draft.text(), "First line\n第二行\nLast line");
    }

    #[test]
    fn text_and_slider_drafts_commit_once_without_clamping_recommended_ranges() {
        let descriptor = ParameterDescriptor::float("exposure", "Exposure", 0.0, None, None);
        let ux = parameter_ux("core.exposure", &descriptor);
        let mut draft = ParameterDraft::new(descriptor, ux, ParameterValue::Float(0.0));
        draft.set_text("2".into());
        draft.set_text("2.2".into());
        draft.set_text("2.25".into());
        assert_eq!(draft.committed(), &ParameterValue::Float(0.0));
        assert_eq!(
            draft.take_commit().unwrap(),
            Some(ParameterValue::Float(2.25))
        );
        assert_eq!(draft.take_commit().unwrap(), None);
        draft.set_text("9".into());
        assert_eq!(
            draft.take_commit().unwrap(),
            Some(ParameterValue::Float(9.0))
        );
        assert!(draft.outside_recommended());
        draft.set_slider(3.0).unwrap();
        draft.set_slider(4.0).unwrap();
        assert_eq!(draft.committed(), &ParameterValue::Float(9.0));
        draft.cancel();
        assert_eq!(draft.take_commit().unwrap(), None);
        assert_eq!(draft.committed(), &ParameterValue::Float(9.0));
    }

    #[test]
    fn numeric_validation_percent_units_whole_pixels_and_integer_precision() {
        let descriptor =
            ParameterDescriptor::float("strength", "Strength", 1.0, Some(0.0), Some(1.0));
        let ux = parameter_ux("raw.highlight-reconstruction", &descriptor);
        let mut draft = ParameterDraft::new(descriptor, ux, ParameterValue::Float(1.0));
        draft.set_text("45".into());
        assert_eq!(
            draft.take_commit().unwrap(),
            Some(ParameterValue::Float(0.45))
        );
        for text in ["", "NaN", "inf", "1e300", "-1", "101"] {
            draft.set_text(text.into());
            assert!(draft.take_commit().is_err());
            assert_eq!(draft.committed(), &ParameterValue::Float(0.45));
        }
        let descriptor = ParameterDescriptor::float("width", "Width", 1.0, Some(1.0), None);
        let ux = parameter_ux("core.crop", &descriptor);
        let mut pixels = ParameterDraft::new(descriptor, ux, ParameterValue::Float(1.0));
        pixels.set_text("1.5".into());
        assert!(pixels.take_commit().is_err());
        pixels.set_text("42".into());
        assert_eq!(
            pixels.take_commit().unwrap(),
            Some(ParameterValue::Float(42.0))
        );
        let descriptor = ParameterDescriptor::integer("seed", "Seed", 0);
        let ux = parameter_ux("ai.generate", &descriptor);
        let mut integer = ParameterDraft::new(descriptor, ux, ParameterValue::Integer(0));
        integer.set_text(i64::MAX.to_string());
        assert_eq!(
            integer.take_commit().unwrap(),
            Some(ParameterValue::Integer(i64::MAX))
        );
        assert!(integer.step(1.0).is_err());
    }

    #[test]
    fn rounded_initial_display_does_not_mutate_saved_values_and_held_steps_group() {
        let descriptor = ParameterDescriptor::float("exposure", "Exposure", 0.0, None, None);
        let ux = parameter_ux("core.exposure", &descriptor);
        let mut draft = ParameterDraft::new(descriptor, ux, ParameterValue::Float(1.234567));
        assert_eq!(draft.text(), "1.23");
        assert_eq!(draft.take_commit().unwrap(), None);
        draft.cancel();
        assert_eq!(draft.committed(), &ParameterValue::Float(1.234567));
        draft.set_text("2".into());
        draft.step(1.0).unwrap();
        draft.step(1.0).unwrap();
        assert_eq!(draft.committed(), &ParameterValue::Float(1.234567));
        assert!(
            (match draft.take_commit().unwrap().unwrap() {
                ParameterValue::Float(v) => v,
                _ => panic!(),
            } - 2.02)
                .abs()
                < 0.00001
        );
    }

    #[test]
    fn point_drafts_validate_order_limits_f32_and_keep_hdr_coordinates() {
        let descriptor = ParameterDescriptor::string("points", "Points", "0,0;1,1");
        let ux = parameter_ux("core.curve", &descriptor);
        let mut draft =
            ParameterDraft::new(descriptor, ux, ParameterValue::String("0,0;1,1".into()));
        draft.set_text("0,0;0.5,2.5;1,4".into());
        assert!(draft.take_commit().unwrap().is_some());
        for text in ["", "0,0;0,1", "0,0;1,NaN", "0,0;1,1e50", "1,1;1,0"] {
            draft.set_text(text.into());
            assert!(draft.take_commit().is_err());
        }
        assert!(parse_points(&"0,0;".repeat(4097)).is_err());
        assert!(parse_points(&" ".repeat(65537)).is_err());
    }
}
