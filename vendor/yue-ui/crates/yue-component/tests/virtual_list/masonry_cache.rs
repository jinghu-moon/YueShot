use super::*;
use yue_component::{
    VirtualMasonry, VirtualMasonryCache, VirtualMasonryCacheItem, VirtualMasonryState,
    VirtualizerAnchorPosition,
};

struct Harness {
    state: Entity<VirtualMasonryState>,
    builds: Rc<Cell<usize>>,
    mounted: bool,
}
impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if !self.mounted {
            return div().into_any_element();
        }
        let builds = self.builds.clone();
        VirtualMasonry::new("cache-masonry", &self.state, move |_, _, _| {
            builds.set(builds.get() + 1);
            div().h(px(20.))
        })
        .w(px(240.))
        .h(px(100.))
        .into_any_element()
    }
}
fn fresh(cx: &mut App) -> Entity<VirtualMasonryState> {
    cx.new(|_| {
        VirtualMasonryState::new(source(0, 300), VirtualizerSizing::measured(px(2.)), 3).unwrap()
    })
}
#[gpui_kit::test]
fn masonry_cache_recreates_owner_lane_geometry_and_reading_anchor(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| Harness {
        state: fresh(cx),
        builds: Rc::default(),
        mounted: true,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let old = view.read_with(cx, |v, _| v.state.clone());
    old.update(cx, |s, cx| {
        s.scroll_handle()
            .set_precise_offset(gpui_kit::point(0., -60.));
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let cache = old.read_with(cx, |s, _| s.measurement_cache().unwrap());
    assert!(cache.items().iter().filter(|i| i.is_measured()).count() >= 15);
    let anchor = cache.scroll_anchor().unwrap();
    let old_snapshot = old.read_with(cx, |s, _| {
        s.snapshot(60., gpui_kit::size(px(240.), px(100.))).unwrap()
    });
    let weak = old.downgrade();
    view.update(cx, |v, cx| {
        v.mounted = false;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let restored = cx.update(|_, cx| fresh(cx));
    restored.update(cx, |s, _| {
        assert!(s.restore_measurement_cache(&cache).unwrap() >= 15);
        assert!(matches!(
            s.update_sizes_from_snapshot(&old_snapshot, &[]),
            Err(VirtualizerError::StaleSnapshot)
        ));
    });
    drop(old_snapshot);
    drop(old);
    view.update(cx, |v, cx| {
        v.state = restored.clone();
        v.mounted = true;
        v.builds.set(0);
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(
        weak.upgrade().is_none(),
        "portable cache must not retain old controller"
    );
    restored.read_with(cx, |s, _| {
        let snap = s
            .snapshot(
                -s.scroll_handle().precise_offset().y,
                gpui_kit::size(px(240.), px(100.)),
            )
            .unwrap();
        let item = snap.item(s.source().index_of(anchor.0).unwrap()).unwrap();
        assert!(
            (snap.offset()
                - item.item().start()
                - match anchor.1 {
                    VirtualizerAnchorPosition::Within(n) => n,
                    _ => panic!("within"),
                })
            .abs()
                < 0.01
        );
        for ix in 0..300 {
            assert_eq!(snap.item(ix).unwrap().lane(), cache.items()[ix].lane());
        }
    });
    let warm = view.read_with(cx, |v, _| v.builds.get());
    view.update(cx, |v, cx| {
        v.mounted = false;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let cold = cx.update(|_, cx| fresh(cx));
    view.update(cx, |v, cx| {
        v.state = cold;
        v.mounted = true;
        v.builds.set(0);
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let cold = view.read_with(cx, |v, _| v.builds.get());
    println!("CACHE_ROOTS,warm={warm},cold={cold}");
    assert!(
        warm < cold,
        "cached geometry should avoid estimate overbuild: warm={warm},cold={cold}"
    );
}

#[gpui_kit::test]
fn masonry_cache_rejects_resolved_width_and_restores_only_compatible_measurements(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| Harness {
        state: fresh(cx),
        builds: Rc::default(),
        mounted: true,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    let cache = state.read_with(cx, |s, _| s.measurement_cache().unwrap());
    assert_eq!(cache.cross_extent(), px(80.));
    state.update(cx, |s, _| {
        let old = s.snapshot(0., gpui_kit::size(px(240.), px(100.))).unwrap();
        let mut records = cache.items().to_vec();
        let last = records.len() - 1;
        records[last] =
            VirtualMasonryCacheItem::new(last, 0, px(f32::NAN), records[last].lane(), false);
        let malformed = VirtualMasonryCache::new(0, 3, px(80.), px(0.), px(0.), records);
        assert!(s.restore_measurement_cache(&malformed).is_err());
        assert!(
            s.update_sizes_from_snapshot(&old, &[]).is_ok(),
            "fast validation precedes all writes"
        );
        let bad = VirtualMasonryCache::new(0, 3, px(81.), px(0.), px(0.), cache.items().to_vec());
        assert!(s.restore_measurement_cache(&bad).is_err());
        assert!(
            s.update_sizes_from_snapshot(&old, &[]).is_ok(),
            "rejected transaction preserves revision"
        );
        s.restore_measurement_cache(&cache).unwrap();
        assert!(matches!(
            s.update_sizes_from_snapshot(&old, &[]),
            Err(VirtualizerError::StaleSnapshot)
        ));
    });
    let mut fresh =
        VirtualMasonryState::new(source(0, 300), VirtualizerSizing::measured(px(2.)), 3).unwrap();
    fresh.restore_measurement_cache(&cache).unwrap();
    assert!(
        fresh
            .snapshot(0., gpui_kit::size(px(240.), px(100.)))
            .unwrap()
            .item(0)
            .unwrap()
            .item()
            .is_measured()
    );
    state.update(cx, |s, _| {
        s.reflow(5).unwrap();
        assert!(s.restore_measurement_cache(&cache).is_err());
    });
}

#[test]
fn masonry_cache_validates_atomically_and_handles_deleted_appended_and_changed_keys() {
    let item = |ix, lane, extent| VirtualMasonryCacheItem::new(ix, 0, px(extent), lane, true);
    let cache = VirtualMasonryCache::new(
        0,
        3,
        px(80.),
        px(0.),
        px(0.),
        vec![item(0usize, 0, 40.), item(1, 1, 30.), item(2, 2, 20.)],
    )
    .anchor(1usize, VirtualizerAnchorPosition::Within(4.));
    let mut state =
        VirtualMasonryState::new(source(1, 4), VirtualizerSizing::measured(px(10.)), 3).unwrap();
    assert_eq!(state.restore_measurement_cache(&cache).unwrap(), 2);
    let snap = state
        .snapshot(0., gpui_kit::size(px(240.), px(100.)))
        .unwrap();
    assert_eq!(snap.item(0).unwrap().lane(), 1);
    assert_eq!(snap.item(0).unwrap().item().extent(), px(30.));
    assert!(!snap.item(3).unwrap().item().is_measured());
    for bad in [
        VirtualMasonryCache::new(0, 2, px(80.), px(0.), px(0.), vec![]),
        VirtualMasonryCache::new(1, 3, px(80.), px(0.), px(0.), vec![]),
        VirtualMasonryCache::new(0, 3, px(f32::NAN), px(0.), px(0.), vec![]),
        VirtualMasonryCache::new(
            0,
            3,
            px(80.),
            px(0.),
            px(0.),
            vec![item(1, 1, 30.), item(1, 1, 40.)],
        ),
        VirtualMasonryCache::new(0, 3, px(80.), px(0.), px(0.), vec![item(1, 3, 30.)]),
        VirtualMasonryCache::new(0, 3, px(80.), px(0.), px(0.), vec![item(1, 1, f32::NAN)]),
        cache
            .clone()
            .anchor(1usize, VirtualizerAnchorPosition::Before(f64::NAN)),
    ] {
        assert!(state.restore_measurement_cache(&bad).is_err());
        assert_eq!(
            state
                .snapshot(0., gpui_kit::size(px(240.), px(100.)))
                .unwrap()
                .revision(),
            snap.revision()
        );
    }
    struct Changed;
    impl VirtualizerSource for Changed {
        fn len(&self) -> usize {
            3
        }
        fn key_at(&self, ix: usize) -> ElementId {
            ix.into()
        }
        fn index_of(&self, key: &ElementId) -> Option<usize> {
            if let ElementId::Integer(i) = key {
                (*i < 3).then_some(*i as usize)
            } else {
                None
            }
        }
        fn revision_at(&self, ix: usize) -> u64 {
            u64::from(ix == 1)
        }
    }
    let mut changed =
        VirtualMasonryState::new(Rc::new(Changed), VirtualizerSizing::measured(px(10.)), 3)
            .unwrap();
    assert_eq!(changed.restore_measurement_cache(&cache).unwrap(), 2);
    assert_eq!(
        changed
            .snapshot(0., gpui_kit::size(px(240.), px(100.)))
            .unwrap()
            .item(1)
            .unwrap()
            .item()
            .extent(),
        px(10.)
    );
    let historical = VirtualMasonryCache::new(
        0,
        3,
        px(80.),
        px(0.),
        px(0.),
        vec![item(0, 2, 40.), item(1, 0, 30.), item(2, 1, 20.)],
    );
    assert_eq!(changed.restore_measurement_cache(&historical).unwrap(), 2);
    let restored = changed
        .snapshot(0., gpui_kit::size(px(240.), px(100.)))
        .unwrap();
    assert_eq!(
        (0..3)
            .map(|ix| restored.item(ix).unwrap().lane())
            .collect::<Vec<_>>(),
        vec![2, 0, 1],
        "same keys with historical lanes must take the remapping path"
    );
}
