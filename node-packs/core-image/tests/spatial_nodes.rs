use std::collections::BTreeMap;

use rawweave_core_image::register_nodes;
use rawweave_image::{Dimensions, LabelMap};
use rawweave_node_api::{EvaluationContext, Inputs, NodeRegistry, Parameters, Value};

#[test]
fn select_label_converts_one_committed_label_map_into_a_normal_mask() {
    let mut registry = NodeRegistry::default();
    register_nodes(&mut registry).unwrap();
    let node = registry.instantiate("core.select-label").unwrap();
    let labels = BTreeMap::from([(String::from("sky"), 3_u16)]);
    let label_map =
        LabelMap::from_values_with_labels(Dimensions::new(2, 1), (10, 20), vec![3, 0], labels)
            .unwrap();
    let inputs = [(String::from("label_map"), Value::LabelMap(label_map))]
        .into_iter()
        .collect::<Inputs>();
    let parameters = [(String::from("label"), "sky".into())]
        .into_iter()
        .collect::<Parameters>();

    let result = node
        .evaluate(&inputs, &parameters, &EvaluationContext::default())
        .unwrap();
    let Value::Mask(mask) = &result.outputs["mask"] else {
        panic!("select label must emit a core.Mask");
    };
    assert_eq!(mask.origin(), (10, 20));
    assert_eq!(mask.values(), vec![1.0, 0.0]);
}
