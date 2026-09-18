use rawweave_color::SceneLinearRGB;
use rawweave_node_api::Value;
use rawweave_raw::{CameraProfile, DeterministicCorpus, EmbeddedPreview, LensProfile};

#[test]
fn raw_and_color_values_have_stable_graph_type_ids() {
    let frame = DeterministicCorpus::bayer_12_bit();
    assert_eq!(Value::RawFrame(frame).data_type(), "raw.Frame");
    assert_eq!(
        Value::Mosaic(DeterministicCorpus::bayer_12_bit().mosaic().clone()).data_type(),
        "raw.Mosaic"
    );
    assert_eq!(
        Value::SceneLinearRGB(SceneLinearRGB::from_pixels(1, 1, vec![[0.0; 3]]).unwrap())
            .data_type(),
        "color.SceneLinearRGB"
    );
    assert_eq!(Value::Bytes(vec![1, 2]).data_type(), "core.Bytes");
    assert_eq!(
        Value::CameraProfile(CameraProfile::identity("Make", "Model")).data_type(),
        "raw.CameraProfile"
    );
    assert_eq!(
        Value::LensProfile(LensProfile::identity("Lens")).data_type(),
        "raw.LensProfile"
    );
    assert_eq!(
        Value::EmbeddedPreview(EmbeddedPreview::unavailable()).data_type(),
        "raw.EmbeddedPreview"
    );
}
