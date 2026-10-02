use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use rawweave_image::{Image, Mask, Region};
use rawweave_rendering::{
    CacheKey, CancellationToken, CollectionProgress, CollectionScheduler, GpuContext,
    GraphRevision, MemberCacheKey, MemberWork, MemoryRenderCache, PreviewQuality, RenderResult,
    TileCoord,
};

#[test]
fn collection_scheduler_stops_launching_new_members_after_cancellation() {
    let scheduler = CollectionScheduler::new(2);
    let cancellation = CancellationToken::new();
    let started = Arc::new(AtomicUsize::new(0));
    let works = (0..6)
        .map(|index| MemberWork::new(format!("frame-{index}"), index))
        .collect::<Vec<_>>();
    let cancellation_for_work = cancellation.clone();
    let started_for_work = Arc::clone(&started);

    let error = scheduler
        .run("map", &works, &cancellation, move |member_id, _, _| {
            started_for_work.fetch_add(1, Ordering::SeqCst);
            if member_id == "frame-0" {
                cancellation_for_work.cancel();
            }
            Ok::<_, &'static str>(())
        })
        .unwrap_err();

    assert!(matches!(
        error,
        rawweave_rendering::CollectionError::Cancelled { .. }
    ));
    assert!(started.load(Ordering::SeqCst) < works.len());
}

#[test]
fn collection_scheduler_reports_the_lowest_indexed_member_failure() {
    let scheduler = CollectionScheduler::new(2);
    let barrier = Arc::new(Barrier::new(2));
    let works = [MemberWork::new("first", ()), MemberWork::new("second", ())];
    let barrier_for_work = Arc::clone(&barrier);

    let error = scheduler
        .run(
            "develop",
            &works,
            &CancellationToken::new(),
            move |member_id, _, _| {
                barrier_for_work.wait();
                if member_id == "first" {
                    Err::<(), _>("first failure")
                } else {
                    Err::<(), _>("second failure")
                }
            },
        )
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "image-set member 'first' failed during stage 'develop': first failure"
    );
}

fn image() -> Image {
    Image::from_pixels(1, 1, vec![[0.25, 0.5, 0.75, 1.0]]).unwrap()
}

fn image_with_size(width: u32, height: u32) -> Image {
    Image::from_pixels(
        width,
        height,
        vec![[0.25, 0.5, 0.75, 1.0]; (width * height) as usize],
    )
    .unwrap()
}

fn mask() -> Mask {
    Mask::from_values(rawweave_image::Dimensions::new(1, 1), vec![0.5]).unwrap()
}

#[test]
fn default_collection_scheduler_is_usable() {
    let scheduler = CollectionScheduler::default();
    let works = [MemberWork::new("frame-1", 1), MemberWork::new("frame-2", 2)];

    let result = scheduler
        .run("map", &works, &CancellationToken::new(), |_, value, _| {
            Ok::<_, &'static str>(*value)
        })
        .unwrap();

    assert_eq!(result, [1, 2]);
}

#[test]
fn collection_scheduler_bounds_workers_and_restores_input_order() {
    let scheduler = CollectionScheduler::new(2);
    let cancellation = CancellationToken::new();
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let progress = Arc::new(std::sync::Mutex::new(Vec::<CollectionProgress>::new()));
    let works = ["a", "b", "c", "d"]
        .into_iter()
        .map(|member_id| MemberWork::new(member_id, member_id.to_owned()))
        .collect::<Vec<_>>();
    let progress_observer = Arc::clone(&progress);
    let peak_observer = Arc::clone(&peak);
    let scheduler = scheduler.with_progress(move |event| {
        progress_observer.lock().unwrap().push(event);
    });

    let result = scheduler
        .run("develop", &works, &cancellation, move |member_id, _, _| {
            let current = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak_observer.fetch_max(current, Ordering::SeqCst);
            if member_id == "a" || member_id == "c" {
                thread::sleep(Duration::from_millis(10));
            }
            let output = member_id.to_owned();
            active.fetch_sub(1, Ordering::SeqCst);
            Ok::<_, &'static str>(output)
        })
        .unwrap();

    assert_eq!(result, ["a", "b", "c", "d"]);
    assert!(peak.load(Ordering::SeqCst) <= 2);
    let progress = progress.lock().unwrap();
    assert_eq!(progress.len(), 4);
    assert_eq!(progress.last().unwrap().completed, 4);
    assert_eq!(progress.last().unwrap().total, 4);
}

#[test]
fn collection_scheduler_names_member_and_stage_failures() {
    let scheduler = CollectionScheduler::new(1);
    let works = [MemberWork::new("frame-2", ())];

    let error = scheduler
        .run(
            "raw-develop",
            &works,
            &CancellationToken::new(),
            |_, _, _| Err::<(), _>("decoder unavailable"),
        )
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "image-set member 'frame-2' failed during stage 'raw-develop': decoder unavailable"
    );
}

#[test]
fn collection_scheduler_honors_cancellation_before_start() {
    let scheduler = CollectionScheduler::new(2);
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let works = [MemberWork::new("frame-1", ())];

    let error = scheduler
        .run("map", &works, &cancellation, |_, _, _| {
            Ok::<_, &'static str>(())
        })
        .unwrap_err();

    assert!(error.to_string().contains("cancelled"));
}

#[test]
fn member_cache_key_distinguishes_member_stage_and_upstream_identity() {
    let first = MemberCacheKey::new("imageset-map", 1, "frame-1", 11, 22);
    let same = MemberCacheKey::new("imageset-map", 1, "frame-1", 11, 22);
    let different_member = MemberCacheKey::new("imageset-map", 1, "frame-2", 11, 22);
    let different_stage = MemberCacheKey::new("raw-develop", 1, "frame-1", 11, 22);
    let different_version = MemberCacheKey::new("imageset-map", 2, "frame-1", 11, 22);
    let different_input = MemberCacheKey::new("imageset-map", 1, "frame-1", 12, 22);
    let different_parameters = MemberCacheKey::new("imageset-map", 1, "frame-1", 11, 23);

    assert_eq!(first, same);
    assert_ne!(first, different_member);
    assert_ne!(first, different_stage);
    assert_ne!(first, different_version);
    assert_ne!(first, different_input);
    assert_ne!(first, different_parameters);
}

#[test]
fn memory_cache_evicts_oldest_entry_across_render_mask_and_member_classes() {
    let mut cache = MemoryRenderCache::new(2);
    let old_render_key = key(PreviewQuality::Preview);
    let mask_key = key(PreviewQuality::Final);
    let mut new_render_key = old_render_key.clone();
    new_render_key.parameter_hash += 1;
    let member_key = MemberCacheKey::new("imageset-map", 1, "frame-1", 11, 22);

    cache.insert(
        old_render_key.clone(),
        RenderResult::new(image(), GraphRevision::new(1)),
    );
    cache.insert_mask(
        mask_key.clone(),
        rawweave_rendering::MaskRenderResult::new(mask(), GraphRevision::new(1)),
    );
    cache.insert(
        new_render_key.clone(),
        RenderResult::new(image(), GraphRevision::new(1)),
    );
    assert!(cache.get(&old_render_key).is_none());
    assert!(cache.get_mask(&mask_key).is_some());
    assert!(cache.get(&new_render_key).is_some());

    cache.insert_member(member_key.clone(), Arc::new(image()));

    assert!(cache.get(&new_render_key).is_some());
    assert!(cache.get_mask(&mask_key).is_none());
    assert!(cache.get_member(&member_key).is_some());
    assert_eq!(cache.len(), 2);
}

#[test]
fn memory_cache_bounds_payload_bytes_and_discards_oversized_entries() {
    let mut cache = MemoryRenderCache::with_limits(8, 16);
    let oversized_key = key(PreviewQuality::Preview);
    let first_key = key(PreviewQuality::Draft);
    let second_key = key(PreviewQuality::Final);

    cache.insert(
        oversized_key.clone(),
        RenderResult::new(image_with_size(2, 1), GraphRevision::new(1)),
    );
    assert!(cache.get(&oversized_key).is_none());
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.byte_len(), 0);

    cache.insert(
        first_key.clone(),
        RenderResult::new(image(), GraphRevision::new(1)),
    );
    assert_eq!(cache.byte_len(), 16);
    cache.insert(
        second_key.clone(),
        RenderResult::new(image(), GraphRevision::new(1)),
    );

    assert!(cache.get(&first_key).is_none());
    assert!(cache.get(&second_key).is_some());
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.byte_len(), 16);
}

#[test]
fn memory_cache_accounts_mask_and_member_payload_bytes() {
    let mut cache = MemoryRenderCache::with_limits(8, 20);
    let mask_key = key(PreviewQuality::Preview);
    let member_key = MemberCacheKey::new("imageset-map", 1, "frame-1", 11, 22);

    cache.insert_mask(
        mask_key.clone(),
        rawweave_rendering::MaskRenderResult::new(mask(), GraphRevision::new(1)),
    );
    cache.insert_member(member_key.clone(), Arc::new(image()));

    assert_eq!(cache.byte_len(), 20);
    assert!(cache.get_mask(&mask_key).is_some());
    assert!(cache.get_member(&member_key).is_some());
}

#[test]
fn memory_cache_reuses_only_matching_member_keys() {
    let mut cache = MemoryRenderCache::new(8);
    let key = MemberCacheKey::new("imageset-map", 1, "frame-1", 11, 22);
    let other = MemberCacheKey::new("imageset-map", 1, "frame-2", 11, 22);
    let image = Arc::new(image());

    cache.insert_member(key.clone(), Arc::clone(&image));

    let cached = cache.get_member(&key).unwrap();
    assert!(Arc::ptr_eq(&cached, &image));
    assert!(cache.get_member(&other).is_none());
    assert_eq!(cache.member_len(), 1);
    assert_eq!(cache.invalidate_node("imageset-map"), 1);
    assert!(cache.get_member(&key).is_none());
    assert!(cache.is_empty());
}

fn key(quality: PreviewQuality) -> CacheKey {
    CacheKey::new(
        "resize-node",
        1,
        11,
        33,
        22,
        Region::new(0, 0, 1, 1),
        TileCoord::new(0, 0),
        0,
        quality,
        44,
    )
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
fn opaque_values_are_typed_revision_checked_and_share_arc_storage() {
    assert_eq!(
        MemoryRenderCache::default().max_bytes(),
        2 * 1024 * 1024 * 1024
    );
    let mut cache = MemoryRenderCache::new(8);
    let key = key(PreviewQuality::Preview);
    let revision = GraphRevision::new(1);
    let value = Arc::new(vec![1_u8, 2, 3]);

    assert!(cache.insert_value_if_current(key.clone(), Arc::clone(&value), 3, revision, revision));
    let cached = cache.get_value_current::<Vec<u8>>(&key, revision).unwrap();
    assert!(Arc::ptr_eq(&value, &cached));
    assert_eq!(*cached, vec![1, 2, 3]);
    assert!(cache.get_value_current::<String>(&key, revision).is_none());
    assert!(
        cache
            .get_value_current::<Vec<u8>>(&key, GraphRevision::new(2))
            .is_none()
    );
    assert!(Arc::ptr_eq(
        &value,
        &cache
            .clone()
            .get_value_current::<Vec<u8>>(&key, revision)
            .unwrap()
    ));
    assert!(!cache.insert_value_if_current(
        key.clone(),
        Arc::new(99_u8),
        1,
        revision,
        GraphRevision::new(2)
    ));
    assert!(Arc::ptr_eq(
        &value,
        &cache.get_value_current::<Vec<u8>>(&key, revision).unwrap()
    ));
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.byte_len(), 3);
    assert!(!cache.is_empty());
}

#[test]
fn opaque_values_share_fifo_count_limits_with_images_masks_and_members() {
    let mut cache = MemoryRenderCache::new(3);
    let key = key(PreviewQuality::Preview);
    let revision = GraphRevision::new(1);
    let member = MemberCacheKey::new("map", 1, "frame", 1, 1);
    cache.insert_value_if_current(key.clone(), Arc::new(1_u8), 1, revision, revision);
    cache.insert(key.clone(), RenderResult::new(image(), revision));
    cache.insert_mask(
        key.clone(),
        rawweave_rendering::MaskRenderResult::new(mask(), revision),
    );
    cache.insert_member(member.clone(), Arc::new(image()));
    assert!(cache.get_value_current::<u8>(&key, revision).is_none());
    assert!(cache.get(&key).is_some());
    assert_eq!(cache.len(), 3);
    assert_eq!(cache.byte_len(), 36);

    cache.insert_value_if_current(key.clone(), Arc::new(2_u8), 1, revision, revision);
    assert!(cache.get(&key).is_none());
    assert!(cache.get_mask(&key).is_some());
    assert!(cache.get_member(&member).is_some());
    assert_eq!(cache.byte_len(), 21);
}

#[test]
fn opaque_value_revision_acceptance_does_not_require_retention() {
    let key = key(PreviewQuality::Preview);
    let revision = GraphRevision::new(1);
    for (capacity, max_bytes, payload_bytes) in [(0, 8, 1), (8, 0, 1), (8, 8, 9)] {
        let mut cache = MemoryRenderCache::with_limits(capacity, max_bytes);
        assert!(cache.insert_value_if_current(
            key.clone(),
            Arc::new(1_u8),
            payload_bytes,
            revision,
            revision
        ));
        assert!(cache.get_value_current::<u8>(&key, revision).is_none());
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.byte_len(), 0);
    }
}

#[test]
fn opaque_values_compete_with_images_for_payload_bytes() {
    let mut cache = MemoryRenderCache::with_limits(8, 16);
    let key = key(PreviewQuality::Preview);
    let revision = GraphRevision::new(1);
    cache.insert_value_if_current(key.clone(), Arc::new(1_u8), 16, revision, revision);
    cache.insert(key.clone(), RenderResult::new(image(), revision));
    assert!(cache.get_value_current::<u8>(&key, revision).is_none());
    assert_eq!(cache.byte_len(), 16);
    cache.insert_value_if_current(key.clone(), Arc::new(2_u8), 16, revision, revision);
    assert!(cache.get(&key).is_none());
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.byte_len(), 16);
}

#[test]
fn opaque_value_replacement_updates_bytes_without_refreshing_fifo_order() {
    let mut cache = MemoryRenderCache::with_limits(2, 16);
    let first = key(PreviewQuality::Preview);
    let second = key(PreviewQuality::Final);
    let third = key(PreviewQuality::Draft);
    let revision = GraphRevision::new(1);
    let old = Arc::new(1_u8);
    cache.insert_value_if_current(first.clone(), Arc::clone(&old), 4, revision, revision);
    cache.insert_value_if_current(second.clone(), Arc::new(2_u8), 4, revision, revision);
    cache.insert_value_if_current(
        first.clone(),
        Arc::new("replacement".to_owned()),
        8,
        revision,
        revision,
    );
    assert_eq!(Arc::strong_count(&old), 1);
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.byte_len(), 12);
    assert!(cache.get_value_current::<u8>(&first, revision).is_none());
    assert_eq!(
        cache
            .get_value_current::<String>(&first, revision)
            .unwrap()
            .as_str(),
        "replacement"
    );
    // Oversized replacements preserve the previously retained entry, like image inserts.
    assert!(cache.insert_value_if_current(first.clone(), Arc::new(9_u8), 17, revision, revision));
    assert!(
        cache
            .get_value_current::<String>(&first, revision)
            .is_some()
    );
    assert_eq!(cache.byte_len(), 12);
    cache.insert_value_if_current(first.clone(), Arc::new(3_u8), 2, revision, revision);
    assert_eq!(cache.byte_len(), 6);
    cache.insert_value_if_current(third.clone(), Arc::new(4_u8), 1, revision, revision);
    assert!(cache.get_value_current::<u8>(&first, revision).is_none());
    assert!(cache.get_value_current::<u8>(&second, revision).is_some());
    assert!(cache.get_value_current::<u8>(&third, revision).is_some());
    assert_eq!(cache.byte_len(), 5);
}

#[test]
fn opaque_values_restamp_and_invalidate_by_node_nodes_and_revision() {
    let mut cache = MemoryRenderCache::with_limits(4, 64);
    let first = key(PreviewQuality::Preview);
    let mut second = first.clone();
    second.node_id = "other-node".to_owned();
    let mut third = first.clone();
    third.node_id = "last-node".to_owned();
    let revision = GraphRevision::new(1);
    let next = GraphRevision::new(2);
    for key in [&first, &second, &third] {
        cache.insert_value_if_current(key.clone(), Arc::new(1_u8), 4, revision, revision);
    }
    cache.insert(first.clone(), RenderResult::new(image(), revision));
    cache.restamp_revision(next);
    assert!(cache.get_value_current::<u8>(&first, revision).is_none());
    assert!(cache.get_value_current::<u8>(&first, next).is_some());
    assert!(cache.get_current(&first, next).is_some());
    assert_eq!(cache.len(), 4);
    assert_eq!(cache.byte_len(), 28);
    assert_eq!(cache.invalidate_revision(revision), 0);
    assert_eq!(cache.invalidate_node(&first.node_id), 2);
    assert_eq!(cache.byte_len(), 8);
    assert!(cache.get_value_current::<u8>(&second, next).is_some());
    assert_eq!(
        cache.invalidate_nodes([second.node_id.as_str(), "missing"]),
        1
    );
    assert_eq!(cache.byte_len(), 4);
    assert!(cache.get_value_current::<u8>(&third, next).is_some());
    assert_eq!(cache.invalidate_revision(next), 1);
    assert!(cache.is_empty());
    assert_eq!(cache.byte_len(), 0);

    // Reinsert invalidated identities and verify stale FIFO entries cannot evict live entries.
    cache.insert_value_if_current(second.clone(), Arc::new(2_u8), 4, next, next);
    cache.insert_value_if_current(first.clone(), Arc::new(3_u8), 4, next, next);
    cache.insert(first.clone(), RenderResult::new(image(), next));
    cache.insert_mask(
        first.clone(),
        rawweave_rendering::MaskRenderResult::new(mask(), next),
    );
    cache.insert_value_if_current(third.clone(), Arc::new(4_u8), 4, next, next);
    assert!(cache.get_value_current::<u8>(&second, next).is_none());
    assert!(cache.get_value_current::<u8>(&first, next).is_some());
    assert_eq!(cache.len(), 4);
    assert_eq!(cache.byte_len(), 28);
}

#[test]
fn clearing_cache_releases_opaque_values_and_resets_accounting_and_fifo() {
    let mut cache = MemoryRenderCache::with_limits(2, 32);
    let key = key(PreviewQuality::Preview);
    let revision = GraphRevision::new(1);
    let value = Arc::new(1_u8);
    cache.insert_value_if_current(key.clone(), Arc::clone(&value), 4, revision, revision);
    cache.insert(key.clone(), RenderResult::new(image(), revision));
    cache.clear();
    assert_eq!(Arc::strong_count(&value), 1);
    assert!(cache.get_value_current::<u8>(&key, revision).is_none());
    assert!(cache.is_empty());
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.byte_len(), 0);
    cache.insert(key.clone(), RenderResult::new(image(), revision));
    cache.insert_value_if_current(key.clone(), Arc::clone(&value), 4, revision, revision);
    cache.insert_mask(
        key.clone(),
        rawweave_rendering::MaskRenderResult::new(mask(), revision),
    );
    assert!(cache.get(&key).is_none());
    assert!(cache.get_value_current::<u8>(&key, revision).is_some());
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.byte_len(), 8);
}

#[test]
fn minimal_gpu_compute_is_optional_but_real_when_an_adapter_exists() {
    let Some(gpu) = GpuContext::initialize_or_cpu() else {
        return;
    };
    gpu.run_minimal_compute().unwrap();
}

#[test]
fn gpu_color_matrix_matches_cpu_reference_within_tolerance() {
    let Some(gpu) = GpuContext::initialize_or_cpu() else {
        return;
    };
    let source =
        Image::from_pixels(2, 1, vec![[0.1, 0.2, 0.3, 1.0], [0.8, 0.4, 0.2, 0.5]]).unwrap();
    let matrix = [
        [0.9, 0.1, 0.0, 0.0],
        [0.0, 0.8, 0.2, 0.0],
        [0.1, 0.0, 0.7, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let offset = [0.01, -0.02, 0.03, 0.0];
    let cpu = source.map_pixels(|pixel| {
        let mut output = [0.0; 4];
        for row in 0..4 {
            output[row] = offset[row]
                + matrix[row][0] * pixel[0]
                + matrix[row][1] * pixel[1]
                + matrix[row][2] * pixel[2]
                + matrix[row][3] * pixel[3];
        }
        output
    });
    let actual = gpu.apply_color_matrix(&source, matrix, offset).unwrap();

    for (actual, expected) in actual.pixels().iter().zip(cpu.pixels()) {
        for (actual, expected) in actual.iter().zip(expected) {
            assert!((actual - expected).abs() <= 1e-5, "{actual} != {expected}");
        }
    }
}
