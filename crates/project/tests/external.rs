use std::path::PathBuf;
use std::time::Duration;

use rawweave_external_host::{HostConfig, ResourceLimits};
use rawweave_image::Image;
use rawweave_node_api::{EvaluationContext, Value};
use rawweave_project::{EditorCore, ExternalHost, ExternalNodePack};

fn fixture_host() -> ExternalHost {
    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/fixture_external_host.py");
    let config = HostConfig::new("python3")
        .with_args([script.to_string_lossy().as_ref()])
        .with_limits(ResourceLimits {
            request_timeout: Duration::from_secs(2),
            ..ResourceLimits::default()
        });
    ExternalHost::connect("fixture", config).expect("fixture host")
}

#[test]
fn discovered_external_image_and_mask_nodes_round_trip_and_evaluate() {
    let host = fixture_host();
    let pack = ExternalNodePack::discover(host).expect("discover fixture nodes");
    let mut editor = EditorCore::default();
    editor
        .register_external_node_pack(pack)
        .expect("register external pack");

    let image_type = "external.fixture.fixture.image-pass";
    let mask_type = "external.fixture.fixture.mask-pass";
    assert!(
        editor
            .node_descriptors()
            .iter()
            .any(|descriptor| descriptor.type_id == image_type)
    );
    assert!(
        editor
            .node_descriptors()
            .iter()
            .any(|descriptor| descriptor.type_id == mask_type)
    );

    editor.add_node("input", "core.image-input").unwrap();
    editor.add_node("image-pass", image_type).unwrap();
    editor
        .add_node("mask-source", "core.mask-linear-gradient")
        .unwrap();
    editor.add_node("mask-pass", mask_type).unwrap();
    editor
        .connect("input", "image", "image-pass", "image")
        .unwrap();
    editor
        .connect("input", "image", "mask-source", "image")
        .unwrap();
    editor
        .connect("mask-source", "mask", "mask-pass", "mask")
        .unwrap();

    let image = Image::from_pixels(2, 1, vec![[0.1, 0.2, 0.3, 1.0], [0.4, 0.5, 0.6, 1.0]]).unwrap();
    let context = EvaluationContext::with_source_image(image.clone());
    assert_eq!(
        editor
            .evaluate("image-pass", "image", context.clone())
            .unwrap(),
        Value::Image(image)
    );
    assert!(matches!(
        editor.evaluate("mask-pass", "mask", context),
        Ok(Value::Mask(_))
    ));

    let serialized = editor.save_workflow().unwrap();
    let mut loaded = editor.clone();
    loaded.load_workflow(&serialized).unwrap();
    assert_eq!(loaded.save_workflow().unwrap(), serialized);
}

#[test]
fn missing_external_host_and_incompatible_capabilities_are_diagnostics() {
    let missing = ExternalHost::connect("missing", HostConfig::new("rawweave-does-not-exist"))
        .expect("configuration is accepted before launch");
    let diagnostics = missing.diagnostics();
    assert!(diagnostics.to_string().contains("rawweave-does-not-exist"));
}
