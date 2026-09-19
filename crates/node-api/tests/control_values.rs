use std::collections::BTreeMap;

use rawweave_node_api::{ColorValue, Metadata, ParameterType, ParameterValue, Value};
use rawweave_raw::{CameraMetadata, ExifMetadata, Orientation};

#[test]
fn control_values_have_stable_graph_type_ids() {
    assert_eq!(Value::Integer(3200).data_type(), "value.Integer");
    assert_eq!(Value::Boolean(true).data_type(), "value.Boolean");
    assert_eq!(
        Value::String("hello".to_owned()).data_type(),
        "value.String"
    );
    assert_eq!(Value::Enum("portrait".to_owned()).data_type(), "value.Enum");
    assert_eq!(
        Value::Color(ColorValue::rgb(1.0, 0.5, 0.0)).data_type(),
        "value.Color"
    );
    assert_eq!(Value::Condition(true).data_type(), "value.Condition");
    assert_eq!(
        Value::Metadata(Metadata::default()).data_type(),
        "core.Metadata"
    );
}

#[test]
fn metadata_projects_camera_and_exif_fields_without_inventing_data() {
    let camera = CameraMetadata {
        make: "Fuji".to_owned(),
        model: "X-T5".to_owned(),
        lens: Some("XF 35mm F1.4".to_owned()),
        iso: Some(3200),
        aperture: Some(1.4),
        shutter_seconds: Some(1.0 / 125.0),
        focal_length_mm: Some(35.0),
        capture_time: Some("2024-01-02T03:04:05".to_owned()),
        orientation: Orientation::Rotate90,
        dimensions: None,
    };
    let mut tags = BTreeMap::new();
    tags.insert("Rating".to_owned(), "4".to_owned());
    tags.insert("Artist".to_owned(), "Ansel".to_owned());
    let exif = ExifMetadata { tags };

    let metadata = Metadata::from_sources(&camera, &exif);

    assert_eq!(metadata.make, "Fuji");
    assert_eq!(metadata.model, "X-T5");
    assert_eq!(metadata.lens.as_deref(), Some("XF 35mm F1.4"));
    assert_eq!(metadata.iso, Some(3200));
    assert_eq!(metadata.aperture, Some(1.4));
    assert_eq!(metadata.focal_length_mm, Some(35.0));
    assert_eq!(metadata.orientation.as_deref(), Some("rotate-90"));
    assert_eq!(metadata.rating, Some(4));
    assert_eq!(
        metadata.tags.get("Artist").map(String::as_str),
        Some("Ansel")
    );
}

#[test]
fn metadata_reports_missing_values_as_none_instead_of_guessing() {
    let metadata = Metadata::from_sources(&CameraMetadata::default(), &ExifMetadata::default());
    assert_eq!(metadata.iso, None);
    assert_eq!(metadata.aperture, None);
    assert_eq!(metadata.rating, None);
    assert_eq!(metadata.orientation.as_deref(), Some("normal"));
}

#[test]
fn control_values_round_trip_through_json() {
    for value in [
        Value::Integer(-7),
        Value::Boolean(false),
        Value::String("frame".to_owned()),
        Value::Enum("landscape".to_owned()),
        Value::Color(ColorValue::rgba(0.1, 0.2, 0.3, 1.0)),
        Value::Condition(true),
        Value::Metadata(Metadata {
            make: "Fuji".to_owned(),
            iso: Some(400),
            rating: Some(5),
            ..Metadata::default()
        }),
    ] {
        let json = serde_json::to_string(&value).unwrap();
        let decoded: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, value);
    }
}

#[test]
fn parameter_values_support_integers() {
    assert_eq!(
        ParameterValue::Integer(3).parameter_type(),
        ParameterType::Integer
    );
    let json = serde_json::to_string(&ParameterValue::Integer(3)).unwrap();
    let decoded: ParameterValue = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, ParameterValue::Integer(3));
}

#[test]
fn color_values_reject_non_finite_channels() {
    assert!(ColorValue::new(f32::NAN, 0.0, 0.0, 1.0).is_none());
    assert!(ColorValue::new(0.0, f32::INFINITY, 0.0, 1.0).is_none());
    assert!(ColorValue::new(0.0, 0.0, 0.0, 1.0).is_some());
}
