use rawweave_image::Image;
use rawweave_node_api::{EvaluationContext, Value};
use rawweave_project::EditorCore;
use rawweave_raw::{DeterministicCorpus, DeterministicDecoder};
use std::{fs, path::Path};

#[test]
fn example_workflows_load_round_trip_and_render() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/workflows");
    let names = [
        "basic-tone",
        "monochrome",
        "highlight-mask",
        "web-resize",
        "film-look",
        "raw-development",
    ];
    for name in names {
        let json = fs::read_to_string(directory.join(format!("{name}.json")))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let mut editor = EditorCore::new_with_raw_decoder(DeterministicDecoder::new(
            DeterministicCorpus::bayer_12_bit(),
        ));
        editor
            .load_workflow(&json)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let saved = editor.save_workflow().unwrap();
        editor.load_workflow(&saved).unwrap();
        if name == "raw-development" {
            assert!(matches!(
                editor
                    .evaluate_raw_workflow_with_bytes("99-display", "display", vec![1, 2, 3])
                    .unwrap(),
                Value::DisplayRGB(_)
            ));
        } else {
            let image = Image::from_pixels(4, 4, vec![[0.25, 0.5, 0.75, 1.0]; 16]).unwrap();
            let result = editor
                .evaluate(
                    "99-output",
                    "image",
                    EvaluationContext::with_source_image(image),
                )
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let Value::Image(image) = result else {
                panic!("{name}: expected image")
            };
            if name == "web-resize" {
                assert_eq!((image.width(), image.height()), (1200, 800));
            } else {
                assert_eq!((image.width(), image.height()), (4, 4));
            }
            assert!(
                image
                    .pixels()
                    .iter()
                    .flatten()
                    .all(|value| value.is_finite()),
                "{name}: nonfinite output"
            );
            if name == "monochrome" {
                for pixel in image.pixels() {
                    assert_eq!(pixel[0], pixel[1]);
                    assert_eq!(pixel[1], pixel[2]);
                }
            }
        }
    }
}
