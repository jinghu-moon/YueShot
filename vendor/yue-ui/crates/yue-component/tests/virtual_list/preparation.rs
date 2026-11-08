use super::*;

struct PreparedRow {
    label: String,
    payload: Vec<String>,
}
impl Render for PreparedRow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(20.))
            .child(self.label.clone())
            .child(format!("{}", self.payload.len()))
    }
}
struct ViewPreparation {
    state: Entity<VirtualizerState>,
    factories: Rc<Cell<usize>>,
    optimized: bool,
    revision: u64,
    mounted: bool,
    rows: Option<Rc<std::cell::RefCell<Vec<gpui_kit::WeakEntity<PreparedRow>>>>>,
}
impl Render for ViewPreparation {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if !self.mounted {
            return div().into_any_element();
        }
        let factories = self.factories.clone();
        let rows = self.rows.clone();
        let prepare =
            move |item: yue_component::VirtualizerItemIdentity, _: &mut Window, cx: &mut App| {
                factories.set(factories.get() + 1);
                let ix = item.index();
                let payload = (0..256)
                    .map(|n| format!("preparing item {ix} field {n}: immutable application data"))
                    .collect();
                let view = cx.new(|_| PreparedRow {
                    label: format!("record {ix}"),
                    payload,
                });
                if let Some(rows) = &rows {
                    rows.borrow_mut().push(view.downgrade());
                }
                view
            };
        if self.optimized {
            VirtualList::new_with_item_views("prepared-list", &self.state, prepare)
                .content_revision(self.revision)
                .w(px(240.))
                .h(px(100.))
                .into_any_element()
        } else {
            VirtualList::new("prepared-list", &self.state, move |item, w, cx| {
                prepare(item.identity(), w, cx)
            })
            .w(px(240.))
            .h(px(100.))
            .into_any_element()
        }
    }
}
#[gpui_kit::test]
fn retained_item_preparation_is_bounded_and_invalidates_on_content_revision(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| ViewPreparation {
        state: cx.new(|_| fixed(100_000)),
        factories: Rc::new(Cell::new(0)),
        optimized: true,
        revision: 0,
        mounted: true,
        rows: Some(Rc::default()),
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (state, factories) = view.read_with(cx, |v, _| (v.state.clone(), v.factories.clone()));
    let initial = factories.get();
    for sample in 1..10 {
        state.update(cx, |s, cx| {
            s.native_scroll_handle()
                .unwrap()
                .set_offset(gpui_kit::point(px(0.), px(-(sample as f32))));
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
    }
    assert!(
        factories.get() <= initial + 1,
        "factories={}, initial={initial}",
        factories.get()
    );
    let before = factories.get();
    view.update(cx, |v, cx| {
        v.revision += 1;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(factories.get() >= before + 5);
    state.update(cx, |s, cx| {
        s.scroll_to_key(
            50_000usize.into(),
            VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
        )
        .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(factories.get() < 40);
    cx.run_until_parked();
    let rows = view.read_with(cx, |v, _| v.rows.clone().unwrap());
    assert!(
        rows.borrow()[0].upgrade().is_none(),
        "prepared views leaving the workset must be released"
    );
    view.update(cx, |v, cx| {
        v.mounted = false;
        cx.notify();
    });
    for _ in 0..3 {
        cx.update(|w, cx| w.draw(cx).clear(cx));
    }
    cx.run_until_parked();
    assert!(
        rows.borrow().iter().all(|row| row.upgrade().is_none()),
        "unmount must release the entire preparation cache"
    );
}

#[gpui_kit::test]
#[ignore = "manual A/B preparation CPU probe; run release with --ignored --nocapture --test-threads=1"]
fn upgrades_same_range_preparation_evidence(cx: &mut TestAppContext) {
    println!("VIRTUAL_PREPARATION,optimized,sample,batch_ns,factories");
    for optimized in [false, true] {
        let (view, cx) = cx.add_window_view(|_, cx| ViewPreparation {
            state: cx.new(|_| fixed(1_000_000)),
            factories: Rc::new(Cell::new(0)),
            optimized,
            revision: 0,
            mounted: true,
            rows: None,
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let (state, factories) = view.read_with(cx, |v, _| (v.state.clone(), v.factories.clone()));
        for sample in 0..100 {
            factories.set(0);
            let start = Instant::now();
            state.update(cx, |s, cx| {
                s.native_scroll_handle()
                    .unwrap()
                    .set_offset(gpui_kit::point(px(0.), px(-((sample % 18 + 1) as f32))));
                cx.notify();
            });
            cx.update(|w, cx| w.draw(cx).clear(cx));
            println!(
                "VIRTUAL_PREPARATION,{optimized},{sample},{},{}",
                start.elapsed().as_nanos(),
                factories.get()
            );
            if optimized && sample > 0 {
                assert_eq!(factories.get(), 0);
            }
        }
    }
}
