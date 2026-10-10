//! Presentation-only node taxonomy and validated drop coordinates.
use rawweave_node_api::NodeDescriptor;
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};
static TAXONOMY: OnceLock<Result<Value, serde_json::Error>> = OnceLock::new();
fn taxonomy() -> Option<&'static Value> {
    TAXONOMY
        .get_or_init(|| serde_json::from_str(include_str!("../assets/editor/node-categories.json")))
        .as_ref()
        .ok()
}
pub fn node_category(type_id: &str) -> String {
    if let Some(categories) = taxonomy()
        .and_then(|data| data.get("categories"))
        .and_then(Value::as_array)
    {
        for category in categories {
            if category
                .get(1)
                .and_then(Value::as_array)
                .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(type_id)))
                && let Some(label) = category.get(0).and_then(Value::as_str)
            {
                return label.into();
            }
        }
    }
    let family = type_id
        .split('.')
        .next()
        .filter(|family| !family.is_empty())
        .unwrap_or(type_id);
    if let Some(label) = taxonomy()
        .and_then(|data| data.get("labels"))
        .and_then(|labels| labels.get(family))
        .and_then(Value::as_str)
    {
        return label.into();
    }
    family
        .split(['-', '_'])
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn library_groups(
    nodes: Vec<NodeDescriptor>,
    query: &str,
    compatible: &[String],
) -> Vec<(String, Vec<NodeDescriptor>)> {
    let query = query.trim().to_lowercase();
    let matches_name = |node: &NodeDescriptor| {
        format!("{} {}", node.name, node.type_id)
            .to_lowercase()
            .contains(&query)
    };
    let has_name_matches = !query.is_empty() && nodes.iter().any(matches_name);
    let mut groups = BTreeMap::<String, Vec<NodeDescriptor>>::new();
    for node in nodes {
        let category = node_category(&node.type_id);
        if !query.is_empty()
            && !(matches_name(&node)
                || !has_name_matches && category.to_lowercase().contains(&query))
        {
            continue;
        }
        if !compatible.is_empty()
            && !node.inputs.iter().any(|input| {
                compatible.iter().any(|output| {
                    rawweave_project::types_compatible(input.data_type.as_str(), output)
                })
            })
        {
            continue;
        }
        groups.entry(category).or_default().push(node);
    }
    let order = |label: &str| {
        taxonomy()
            .and_then(|data| data.get("categories"))
            .and_then(Value::as_array)
            .and_then(|categories| {
                categories
                    .iter()
                    .position(|category| category.get(0).and_then(Value::as_str) == Some(label))
            })
            .unwrap_or(usize::MAX)
    };
    let mut groups = groups.into_iter().collect::<Vec<_>>();
    groups.sort_by(|(a, _), (b, _)| order(a).cmp(&order(b)).then_with(|| a.cmp(b)));
    groups
}
pub fn drop_position(
    pointer: (f32, f32),
    origin: (f32, f32),
    pan: (f32, f32),
    zoom: f32,
) -> Result<(f32, f32), String> {
    if !zoom.is_finite()
        || zoom <= 0.0
        || [pointer.0, pointer.1, origin.0, origin.1, pan.0, pan.1]
            .iter()
            .any(|v| !v.is_finite())
    {
        return Err("Invalid canvas transform".into());
    }
    let position = (
        (pointer.0 - origin.0 - pan.0) / zoom,
        (pointer.1 - origin.1 - pan.1) / zoom,
    );
    if !position.0.is_finite()
        || !position.1.is_finite()
        || position.0.abs() > 1e6
        || position.1.abs() > 1e6
    {
        return Err("Drop is outside the supported canvas bounds".into());
    }
    Ok(position)
}
#[cfg(test)]
mod tests {
    use super::*;
    use rawweave_node_api::{NodeDescriptor, PortDescriptor};
    #[test]
    fn task_order_search_priority_and_unknown_families_match_tauri() {
        let nodes = vec![
            NodeDescriptor::new("core.constant-float", "Constant Float"),
            NodeDescriptor::new("raw.decode", "RAW Decode"),
            NodeDescriptor::new("core.image-input", "Image Input"),
            NodeDescriptor::new("core.exposure", "Exposure"),
            NodeDescriptor::new("core.local-exposure", "Local Exposure"),
            NodeDescriptor::new("plugin_tools.test", "Custom Node"),
        ];
        let groups = library_groups(nodes.clone(), "", &[]);
        assert_eq!(
            groups
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            [
                "Input & output",
                "RAW development",
                "Tone & exposure",
                "Values",
                "Plugin Tools"
            ]
        );
        assert_eq!(library_groups(nodes.clone(), " tone ", &[])[0].1.len(), 2);
        assert_eq!(library_groups(nodes.clone(), "exposure", &[])[0].1.len(), 2);
        assert_eq!(
            library_groups(nodes.clone(), "core.image-input", &[])[0]
                .1
                .len(),
            1
        );
        let mut with_name_match = nodes.clone();
        with_name_match.push(NodeDescriptor::new("pro.tone-map", "Tone Mapping"));
        let exact = library_groups(with_name_match, "tone", &[]);
        assert_eq!(exact[0].1.len(), 1);
        assert_eq!(exact[0].1[0].type_id, "pro.tone-map");
        assert!(library_groups(nodes, "no such node", &[]).is_empty());
        assert_eq!(node_category("ai.custom"), "AI");
        assert_eq!(node_category("core.mask-multiply"), "Mask combine");
    }
    #[test]
    fn compatible_filter_uses_engine_numeric_boolean_any_rules() {
        let make = |id: &str, kind: &str| {
            let mut node = NodeDescriptor::new(id, id);
            node.inputs
                .push(PortDescriptor::input("in", "In", kind, true));
            node
        };
        let nodes = vec![
            make("core.float", "value.Float"),
            make("core.integer", "value.Integer"),
            make("core.boolean", "value.Boolean"),
            make("core.condition", "value.Condition"),
            make("core.any", "core.Any"),
            make("core.image", "image.Image"),
        ];
        let count = |k: &str| {
            library_groups(nodes.clone(), "", &[k.into()])
                .iter()
                .map(|(_, nodes)| nodes.len())
                .sum::<usize>()
        };
        assert_eq!(count("value.Float"), 3);
        assert_eq!(count("value.Boolean"), 3);
        assert_eq!(count("core.Any"), 6);
        assert_eq!(count("image.Image"), 2);
        assert!(rawweave_project::types_compatible(
            "value.Float",
            "value.Integer"
        ));
    }
    #[test]
    fn drops_account_for_embedded_origin_pan_zoom_and_reject_invalid_positions() {
        assert_eq!(
            drop_position((520.0, 280.0), (200.0, 80.0), (30.0, -20.0), 2.0).unwrap(),
            (145.0, 110.0)
        );
        assert_eq!(
            drop_position((200.0, 80.0), (200.0, 80.0), (20.0, 20.0), 0.5).unwrap(),
            (-40.0, -40.0)
        );
        for zoom in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(drop_position((1.0, 1.0), (0.0, 0.0), (0.0, 0.0), zoom).is_err());
        }
        assert!(drop_position((f32::NAN, 0.0), (0.0, 0.0), (0.0, 0.0), 1.0).is_err());
        assert!(drop_position((f32::MAX, 0.0), (0.0, 0.0), (0.0, 0.0), 0.1).is_err());
    }
}
