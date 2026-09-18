use rawweave_image::{Image, Region};
use rawweave_rendering::{
    CacheKey, GpuContext, GraphRevision, MemoryRenderCache, PreviewQuality, RenderResult, TileCoord,
};

fn image() -> Image {
    Image::from_pixels(1, 1, vec![[0.25, 0.5, 0.75, 1.0]]).unwrap()
}

fn key(quality: PreviewQuality) -> CacheKey {
    CacheKey {
        node_id: "resize-node".to_owned(),
        implementation_version: 1,
        parameter_hash: 11,
        upstream_hash: 22,
        region: Region::new(0, 0, 1, 1),
        tile: TileCoord::new(0, 0),
        mip_level: 0,
        quality,
    }
}

#[test]
fn memory_cache_distinguishes_all_render_key_dimensions() {
    let mut cache = MemoryRenderCache::new(8);
    let render = RenderResult::new(image(), GraphRevision::new(3));

    assert!(cache.get(&key(PreviewQuality::Preview)).is_none());
    cache.insert(key(PreviewQuality::Preview), render.clone());
    assert_eq!(cache.get(&key(PreviewQuality::Preview)), Some(render));
    assert!(cache.get(&key(PreviewQuality::Final)).is_none());

    let mut changed = key(PreviewQuality::Preview);
    changed.parameter_hash += 1;
    assert!(cache.get(&changed).is_none());
}

#[test]
fn stale_render_results_are_rejected_and_revision_invalidation_is_targeted() {
    let mut cache = MemoryRenderCache::new(8);
    let first = key(PreviewQuality::Preview);
    let mut downstream = first.clone();
    downstream.node_id = "output-node".to_owned();
    let result = RenderResult::new(image(), GraphRevision::new(4));
    cache.insert(first.clone(), result.clone());
    cache.insert(downstream.clone(), result);

    assert!(!cache.accepts_revision(&first, GraphRevision::new(5)));
    assert_eq!(cache.invalidate_node("resize-node"), 1);
    assert!(cache.get(&first).is_none());
    assert!(cache.get(&downstream).is_some());
    assert_eq!(cache.invalidate_revision(GraphRevision::new(4)), 1);
    assert!(cache.get(&downstream).is_none());
}

#[test]
fn minimal_gpu_compute_is_optional_but_real_when_an_adapter_exists() {
    let Some(gpu) = GpuContext::initialize_or_cpu() else {
        return;
    };
    gpu.run_minimal_compute().unwrap();
}
