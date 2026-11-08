//! Manual, reproducible headless CPU probes. They measure Element lifecycle
//! phases and submitted draw work, not a Windows GPU/presentation frame.
use super::*;
use std::cell::RefCell;

#[derive(Clone, Copy, Default)]
struct Allocations {
    live: isize,
    peak: isize,
    allocations: usize,
}
thread_local! { static ALLOCATIONS: Cell<Option<Allocations>> = const { Cell::new(None) }; }
struct CountingAllocator;
// Test-only thread-local accounting. No pointer map, locks, logging or allocation
// runs inside the allocator. Production crates retain their normal allocator.
fn record_allocation(delta: isize, allocation: bool) {
    let _ = ALLOCATIONS.try_with(|stats| {
        if let Some(mut value) = stats.get() {
            value.live += delta;
            value.peak = value.peak.max(value.live);
            value.allocations += usize::from(allocation);
            stats.set(Some(value));
        }
    });
}
unsafe impl std::alloc::GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let pointer = unsafe { std::alloc::System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size() as isize, true);
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: std::alloc::Layout) {
        record_allocation(-(layout.size() as isize), false);
        unsafe {
            std::alloc::System.dealloc(pointer, layout);
        }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
        let next = unsafe { std::alloc::System.realloc(pointer, layout, size) };
        if !next.is_null() {
            record_allocation(size as isize - layout.size() as isize, true);
        }
        next
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
#[ignore = "manual Grid metadata A/B; run release with --ignored --nocapture --test-threads=1"]
fn followup_grid_metadata_evidence() {
    use yue_component::VirtualGridSnapshot;
    for known in [false, true] {
        for side in [20, 50] {
            let binding = VirtualizerBinding::new();
            let a = source(0, 100_000);
            let b = source(0, 100_000);
            let sizing = || {
                if known {
                    VirtualizerSizing::known(vec![px(20.); 100_000])
                } else {
                    VirtualizerSizing::fixed(px(20.))
                }
            };
            let rows =
                VirtualizerState::new_attached(Axis::Vertical, a.clone(), sizing(), &binding)
                    .unwrap();
            let cols =
                VirtualizerState::new_attached(Axis::Horizontal, b.clone(), sizing(), &binding)
                    .unwrap();
            let v = VirtualizerViewport::new(
                &binding,
                1,
                0.,
                px(side as f32 * 20.),
                px(side as f32 * 20.),
            );
            let grid =
                VirtualGridSnapshot::new(rows.snapshot(&v).unwrap(), cols.snapshot(&v).unwrap())
                    .unwrap();
            for sample in 0..5 {
                // Alternate order to avoid assigning all warm runs to one path.
                for legacy in if sample % 2 == 0 {
                    [true, false]
                } else {
                    [false, true]
                } {
                    a.calls.set(0);
                    b.calls.set(0);
                    ALLOCATIONS.with(|stats| stats.set(Some(Allocations::default())));
                    let started = Instant::now();
                    let mut checksum = 0.;
                    for _ in 0..1000 {
                        if legacy {
                            // Exact pre-fix enumeration strategy, retained only
                            // in this probe; both paths produce the same pairs.
                            let columns = grid.columns().working_indices();
                            let plan = &grid;
                            let legacy_cells =
                                plan.rows().working_indices().into_iter().flat_map(|row| {
                                    let row = plan.rows().item(row).unwrap();
                                    columns.clone().into_iter().map(move |column| {
                                        let column = plan.columns().item(column).unwrap();
                                        (row.clone(), column)
                                    })
                                });
                            for pair in legacy_cells {
                                let pair = std::hint::black_box(pair);
                                checksum += std::hint::black_box(pair.0.start() + pair.1.start());
                            }
                        } else {
                            for cell in grid.cells() {
                                let cell = std::hint::black_box(cell);
                                checksum += std::hint::black_box(
                                    cell.row().start() + cell.column().start(),
                                );
                            }
                        }
                    }
                    let elapsed = started.elapsed().as_nanos();
                    let stats = ALLOCATIONS.with(|stats| stats.replace(None).unwrap());
                    assert_eq!(stats.live, 0);
                    std::hint::black_box(checksum);
                    println!(
                        "FOLLOWUP_GRID_PERF,known={known},side={side},legacy={legacy},sample={sample},iterations=1000,ns={elapsed},allocations={},peak_bytes={},row_calls={},column_calls={}",
                        stats.allocations,
                        stats.peak,
                        a.calls.get(),
                        b.calls.get()
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "manual observed-size aggregation A/B; run release with --ignored --nocapture --test-threads=1"]
fn grid_observed_size_aggregation_evidence() {
    use std::collections::HashMap;

    const CELLS: usize = 4096;
    const SIDE: usize = 64;
    const SAMPLES: usize = 7;
    const ITERATIONS: usize = 100;
    for sample in 0..SAMPLES {
        for collector_path in if sample % 2 == 0 {
            [true, false]
        } else {
            [false, true]
        } {
            ALLOCATIONS.with(|stats| stats.set(Some(Allocations::default())));
            let started = Instant::now();
            let mut checksum = 0.0f32;
            for _epoch in 0..ITERATIONS {
                if collector_path {
                    let mut cells = HashMap::<(usize, usize), (f32, f32)>::new();
                    for index in 0..CELLS {
                        let width = 40. + (index % 7) as f32;
                        let height = 20. + (index % 11) as f32;
                        cells
                            .entry((index / SIDE, index % SIDE))
                            .and_modify(|current| {
                                current.0 = current.0.max(width);
                                current.1 = current.1.max(height);
                            })
                            .or_insert((width, height));
                    }
                    let mut rows = HashMap::<usize, f32>::new();
                    let mut columns = HashMap::<usize, f32>::new();
                    for ((row, column), (width, height)) in cells {
                        rows.entry(row)
                            .and_modify(|current| *current = current.max(height))
                            .or_insert(height);
                        columns
                            .entry(column)
                            .and_modify(|current| *current = current.max(width))
                            .or_insert(width);
                    }
                    let row_updates = rows.into_iter().collect::<Vec<_>>();
                    let column_updates = columns.into_iter().collect::<Vec<_>>();
                    checksum += row_updates.len() as f32 + column_updates.len() as f32;
                } else {
                    let mut rows = HashMap::<usize, f32>::new();
                    let mut columns = HashMap::<usize, f32>::new();
                    for index in 0..CELLS {
                        let width = 40. + (index % 7) as f32;
                        let height = 20. + (index % 11) as f32;
                        rows.entry(index / SIDE)
                            .and_modify(|current| *current = current.max(height))
                            .or_insert(height);
                        columns
                            .entry(index % SIDE)
                            .and_modify(|current| *current = current.max(width))
                            .or_insert(width);
                    }
                    checksum += rows.len() as f32 + columns.len() as f32;
                }
            }
            let elapsed = started.elapsed().as_nanos();
            let stats = ALLOCATIONS.with(|stats| stats.replace(None).unwrap());
            assert_eq!(stats.live, 0);
            std::hint::black_box(checksum);
            println!(
                "GRID_OBSERVED_SIZES,collector={collector_path},sample={sample},cells={CELLS},iterations={ITERATIONS},ns={elapsed},allocations={},peak_bytes={}",
                stats.allocations, stats.peak
            );
        }
    }
}

#[test]
#[ignore = "equivalent consumer ABBA; run release with --ignored --nocapture --test-threads=1"]
fn grid_observed_equivalent_evidence() {
    use super::grid_observed::{Consumer, Path, workload};
    let frames = workload(64);
    // Equality is checked outside the timed region; consume every output value
    // inside it. The exhaustive independent oracle is a normal regression test.
    let mut pair = [Path::Cells, Path::Axes].map(|path| Consumer::new(path, 64, 4096));
    for epoch in 1..=2 {
        if epoch > 1 {
            for c in &mut pair {
                c.reset(epoch).unwrap();
            }
        }
        for batch in &frames {
            for c in &mut pair {
                c.observe(epoch, batch).unwrap();
            }
            assert_eq!(pair[0].output(), pair[1].output());
        }
    }
    for scenario in ["cold", "warm", "reset"] {
        for (sample, path) in [
            Path::Cells,
            Path::Axes,
            Path::Axes,
            Path::Cells,
            Path::Cells,
            Path::Axes,
            Path::Axes,
            Path::Cells,
        ]
        .into_iter()
        .enumerate()
        {
            ALLOCATIONS.with(|stats| stats.set(Some(Allocations::default())));
            let started = Instant::now();
            let mut checksum = 0f64;
            let mut c = Consumer::new(path, 64, 4096);
            for iteration in 0..40 {
                if iteration > 0 {
                    match scenario {
                        "cold" => c = Consumer::new(path, 64, 4096),
                        "reset" => c.reset(iteration + 1).unwrap(),
                        _ => {}
                    }
                }
                let epoch = if scenario == "reset" {
                    iteration + 1
                } else {
                    1
                };
                let batches = if scenario == "cold" {
                    &frames[..1]
                } else {
                    &frames[..]
                };
                for batch in batches {
                    c.observe(epoch, batch).unwrap();
                    let (rows, columns) = c.output();
                    checksum += rows
                        .into_iter()
                        .chain(columns)
                        .map(|v| f64::from(std::hint::black_box(v)))
                        .sum::<f64>();
                }
            }
            drop(c);
            let ns = started.elapsed().as_nanos();
            let stats = ALLOCATIONS.with(|stats| stats.replace(None).unwrap());
            assert_eq!(stats.live, 0);
            std::hint::black_box(checksum);
            println!(
                "GRID_OBSERVED_EQ,scenario={scenario},path={path:?},sample={sample},iterations=40,ns={ns},allocations={},peak_bytes={},checksum={checksum}",
                stats.allocations, stats.peak
            );
        }
    }
}

#[test]
#[ignore = "manual retained/peak heap payload evidence; run with --ignored --nocapture"]
fn geometry_allocation_evidence() {
    for mode in 0..3 {
        for n in [1_000, 10_000, 100_000, 1_000_000] {
            let source = source(0, n);
            ALLOCATIONS.with(|stats| stats.set(Some(Allocations::default())));
            let start = Instant::now();
            let sizing = match mode {
                0 => VirtualizerSizing::fixed(px(20.)),
                1 => VirtualizerSizing::known(vec![px(20.); n]),
                _ => VirtualizerSizing::measured(px(20.)),
            };
            let state = VirtualizerState::new(Axis::Vertical, source.clone(), sizing).unwrap();
            let elapsed = start.elapsed().as_nanos();
            let before = ALLOCATIONS.with(|stats| stats.get().unwrap());
            drop(state);
            let after = ALLOCATIONS.with(|stats| stats.replace(None).unwrap());
            assert_eq!(
                after.live, 0,
                "dropping state releases its measured heap payload"
            );
            println!(
                "VIRTUAL_ALLOC,{mode},{n},{elapsed},{},{},{}",
                before.live, before.peak, before.allocations
            );
        }
    }
}

#[derive(Default)]
struct Phases {
    build: Cell<usize>,
    request: Cell<usize>,
    prepaint: Cell<usize>,
    paint: Cell<usize>,
    build_ns: Cell<u128>,
    request_ns: Cell<u128>,
    prepaint_ns: Cell<u128>,
    paint_ns: Cell<u128>,
}
impl Phases {
    fn reset(&self) {
        self.build.set(0);
        self.request.set(0);
        self.prepaint.set(0);
        self.paint.set(0);
        self.build_ns.set(0);
        self.request_ns.set(0);
        self.prepaint_ns.set(0);
        self.paint_ns.set(0);
    }
}
struct Probe<E> {
    inner: E,
    phases: Rc<Phases>,
}
impl<E: Element> IntoElement for Probe<E> {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl<E: Element> Element for Probe<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;
    fn id(&self) -> Option<ElementId> {
        self.inner.id()
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        w: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.phases.request.set(self.phases.request.get() + 1);
        let t = Instant::now();
        let result = self.inner.request_layout(id, inspector, w, cx);
        self.phases
            .request_ns
            .set(self.phases.request_ns.get() + t.elapsed().as_nanos());
        result
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        w: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.phases.prepaint.set(self.phases.prepaint.get() + 1);
        let t = Instant::now();
        let result = self.inner.prepaint(id, inspector, bounds, layout, w, cx);
        self.phases
            .prepaint_ns
            .set(self.phases.prepaint_ns.get() + t.elapsed().as_nanos());
        result
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        w: &mut Window,
        cx: &mut App,
    ) {
        self.phases.paint.set(self.phases.paint.get() + 1);
        let t = Instant::now();
        self.inner
            .paint(id, inspector, bounds, layout, prepaint, w, cx);
        self.phases
            .paint_ns
            .set(self.phases.paint_ns.get() + t.elapsed().as_nanos());
    }
}

fn row(index: usize, children: usize, tall: bool, phases: &Rc<Phases>) -> Probe<Div> {
    let t = Instant::now();
    let mut inner = div().flex();
    if tall {
        inner = inner.flex_col();
    }
    inner = inner
        .h(px(if tall { children as f32 * 20. } else { 20. }))
        .children((0..children).map(|n| {
            div()
                .h(px(20.))
                .child(format!("Record {}", index * children + n))
        }));
    phases.build.set(phases.build.get() + 1);
    phases
        .build_ns
        .set(phases.build_ns.get() + t.elapsed().as_nanos());
    Probe {
        inner,
        phases: phases.clone(),
    }
}

struct Workload {
    count: usize,
    children: usize,
    tall: bool,
    mode: usize,
    phases: Rc<Phases>,
    state: Entity<VirtualizerState>,
    native: ListState,
    uniform: gpui_kit::UniformListScrollHandle,
}
impl Render for Workload {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let phases = self.phases.clone();
        let children = self.children;
        let tall = self.tall;
        match self.mode {
            3 => list(self.native.clone(), move |i, _, _| {
                div()
                    .id(i)
                    .w_full()
                    .child(row(i, children, tall, &phases))
                    .test_support()
                    .into_any_element()
            })
            .w(px(240.))
            .h(px(100.))
            .into_any_element(),
            4 => gpui_kit::uniform_list("native-uniform", self.count, move |range, _, _| {
                range
                    .map(|i| {
                        div()
                            .id(i)
                            .w_full()
                            .child(row(i, children, tall, &phases))
                            .test_support()
                    })
                    .collect()
            })
            .track_scroll(&self.uniform)
            .w(px(240.))
            .h(px(100.))
            .into_any_element(),
            _ => VirtualList::new("bench-list", &self.state, move |item, _, _| {
                row(item.index(), children, tall, &phases)
            })
            .w(px(240.))
            .h(px(100.))
            .into_any_element(),
        }
    }
}

#[gpui_kit::test]
#[ignore = "manual CPU probe; run with --ignored --nocapture --test-threads=1"]
fn virtualizer_cpu_evidence(cx: &mut TestAppContext) {
    println!(
        "VIRTUAL_PERF,mode,n,children,tall,sample,batch_ns,build,request,prepaint,paint,build_ns,request_ns,prepaint_ns,paint_ns"
    );
    let samples = Rc::new(RefCell::new(Vec::new()));
    let mut cases = vec![
        (0, 1_000, 1, false),
        (0, 10_000, 1, false),
        (0, 100_000, 1, false),
        (0, 1_000_000, 1, false),
    ];
    for mode in 0..5 {
        for children in [1, 20] {
            cases.push((mode, 100_000, children, false));
        }
    }
    // Same 100,000 records: 100 giant cards vs 100,000 independent roots.
    cases.extend([
        (2, 100, 1_000, true),
        (2, 100_000, 1, true),
        (3, 100, 1_000, true),
        (3, 100_000, 1, true),
    ]);
    for (mode, count, children, tall) in cases {
        let phases = Rc::new(Phases::default());
        let (view, cx) = cx.add_window_view({
            let phases = phases.clone();
            move |_, cx| {
                let extent = px(if tall { children as f32 * 20. } else { 20. });
                let sizing = match mode {
                    1 => VirtualizerSizing::known(vec![extent; count]),
                    2 => VirtualizerSizing::measured(extent),
                    _ => VirtualizerSizing::fixed(extent),
                };
                Workload {
                    count,
                    children,
                    tall,
                    mode,
                    phases,
                    state: cx.new(|_| {
                        VirtualizerState::new(Axis::Vertical, source(0, count), sizing).unwrap()
                    }),
                    native: ListState::new_with_uniform_height(
                        count,
                        ListAlignment::Top,
                        px(0.),
                        extent,
                    ),
                    uniform: gpui_kit::UniformListScrollHandle::new(),
                }
            }
        });
        for sample in 0..51 {
            phases.reset();
            let t = Instant::now();
            view.update(cx, |v, cx| {
                let index = if tall && children > 1 {
                    (sample / 2) % count
                } else {
                    sample * 7
                };
                match mode {
                    3 => v.native.scroll_to(gpui_kit::ListOffset {
                        item_ix: index,
                        offset_in_item: px(0.),
                    }),
                    4 => v
                        .uniform
                        .scroll_to_item_strict(index, gpui_kit::ScrollStrategy::Top),
                    _ => {
                        v.state.update(cx, |s, _| {
                            s.scroll_to_key(
                                index.into(),
                                VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
                            )
                            .unwrap()
                        });
                    }
                }
                cx.notify();
            });
            cx.update(|w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            });
            let elapsed = t.elapsed().as_nanos();
            println!(
                "VIRTUAL_PERF,{mode},{count},{children},{tall},{sample},{elapsed},{},{},{},{},{},{},{},{}",
                phases.build.get(),
                phases.request.get(),
                phases.prepaint.get(),
                phases.paint.get(),
                phases.build_ns.get(),
                phases.request_ns.get(),
                phases.prepaint_ns.get(),
                phases.paint_ns.get()
            );
            if sample > 0 {
                samples.borrow_mut().push(elapsed);
            }
            if !tall && mode < 4 {
                assert!(phases.build.get() <= 12);
            }
        }
    }
    assert_eq!(samples.borrow().len(), 900);
}

#[test]
#[ignore = "manual pure geometry timing; run with --ignored --nocapture"]
fn fixed_query_cpu_evidence() {
    for n in [1_000, 10_000, 100_000, 1_000_000, 1_000_000_000] {
        let state = fixed(n);
        let t = Instant::now();
        let mut max_rows = 0;
        for i in 0..20_000 {
            let snapshot = state
                .snapshot(&viewport(
                    &state,
                    i as u64,
                    ((i * 13) % (n - 10)) as f64 * 20.,
                ))
                .unwrap();
            max_rows = max_rows.max(snapshot.rendered_range().len());
            std::hint::black_box(snapshot);
        }
        println!(
            "VIRTUAL_QUERY,{n},20000,{},{max_rows}",
            t.elapsed().as_nanos()
        );
        assert_eq!(max_rows, 5);
    }
}

#[test]
#[ignore = "manual A/B Known mutation time/allocation probe; run release with --ignored --nocapture"]
fn upgrades_known_update_evidence() {
    use yue_component::VirtualizerSizeUpdate;
    println!("VIRTUAL_KNOWN_UPDATE,incremental,n,sample,ns,allocations,live,peak");
    for n in [1_000, 100_000, 1_000_000] {
        for incremental in [false, true] {
            let mut state = VirtualizerState::new(
                Axis::Vertical,
                source(0, n),
                VirtualizerSizing::known(vec![px(20.); n]),
            )
            .unwrap();
            for sample in 0..10 {
                // Force the same frozen-snapshot semantics on both paths.
                let frozen = state.snapshot(&viewport(&state, sample + 1, 0.)).unwrap();
                ALLOCATIONS.with(|stats| stats.set(Some(Allocations::default())));
                let start = Instant::now();
                if incremental {
                    state
                        .update_sizes(&[VirtualizerSizeUpdate::new(n / 2, px(21. + sample as f32))])
                        .unwrap();
                } else {
                    let mut sizes = vec![px(20.); n];
                    sizes[n / 2] = px(21. + sample as f32);
                    state
                        .replace_source(
                            source(0, n),
                            VirtualizerSizing::known(sizes),
                            VirtualizerChange::Reorder,
                        )
                        .unwrap();
                }
                let ns = start.elapsed().as_nanos();
                let stats = ALLOCATIONS.with(|stats| stats.replace(None).unwrap());
                println!(
                    "VIRTUAL_KNOWN_UPDATE,{incremental},{n},{sample},{ns},{},{},{}",
                    stats.allocations, stats.live, stats.peak
                );
                assert_eq!(frozen.item(0).unwrap().extent(), px(20.));
                if incremental {
                    assert!(
                        stats.peak < 100_000,
                        "point update must not copy N extents: {}",
                        stats.peak
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "manual Known read CPU probe; run release with --ignored --nocapture --test-threads=1"]
fn refinements_known_read_evidence() {
    for n in [1_000, 100_000, 1_000_000] {
        let state = VirtualizerState::new(
            Axis::Vertical,
            source(0, n),
            VirtualizerSizing::known(vec![px(20.); n]),
        )
        .unwrap();
        for sequential in [false, true] {
            for sample in 0..3 {
                let start = Instant::now();
                for query in 0..10_000 {
                    let offset = ((query * 7919) % (n - 10)) as f64 * 20.;
                    let snapshot = state.snapshot(&viewport(&state, 1, offset)).unwrap();
                    if sequential {
                        for item in snapshot.working_items() {
                            std::hint::black_box(item.unwrap());
                        }
                    } else {
                        for ix in snapshot.rendered_range() {
                            std::hint::black_box(snapshot.item(ix).unwrap());
                        }
                    }
                }
                println!(
                    "VIRTUAL_KNOWN_READ,{n},{sequential},{sample},10000,{}",
                    start.elapsed().as_nanos()
                );
            }
        }
    }
}

#[test]
#[ignore = "manual Masonry append CPU probe; run release with --ignored --nocapture --test-threads=1"]
fn refinements_masonry_append_evidence() {
    use yue_component::VirtualMasonryState;
    for n in [1_000, 10_000, 100_000] {
        for frozen in [false, true] {
            for sample in 0..3 {
                let mut state =
                    VirtualMasonryState::new(source(0, n), VirtualizerSizing::measured(px(40.)), 3)
                        .unwrap();
                let snapshot = frozen.then(|| {
                    state
                        .snapshot(0., gpui_kit::size(px(300.), px(100.)))
                        .unwrap()
                });
                let start = Instant::now();
                state
                    .append_source(source(0, n + 1), VirtualizerSizing::measured(px(40.)))
                    .unwrap();
                println!(
                    "VIRTUAL_MASONRY_APPEND,{n},{frozen},{sample},{}",
                    start.elapsed().as_nanos()
                );
                if let Some(snapshot) = snapshot {
                    assert!(snapshot.item(n).is_err());
                }
            }
        }
    }
}

#[test]
#[ignore = "manual Grid snapshot allocation attribution; run release with --ignored --nocapture --test-threads=1"]
fn grid_snapshot_allocation_evidence() {
    use yue_component::VirtualGridSnapshot;

    const ITERATIONS: usize = 500;
    let binding = VirtualizerBinding::new();
    let rows = VirtualizerState::new_attached(
        Axis::Vertical,
        source(0, 100_000),
        VirtualizerSizing::fixed(px(20.)),
        &binding,
    )
    .unwrap();
    let columns = VirtualizerState::new_attached(
        Axis::Horizontal,
        source(0, 100_000),
        VirtualizerSizing::fixed(px(50.)),
        &binding,
    )
    .unwrap();
    let row_viewport = VirtualizerViewport::new(&binding, 1, 0., px(400.), px(1_000.));
    let column_viewport = VirtualizerViewport::new(&binding, 1, 0., px(1_000.), px(400.));
    let grid = VirtualGridSnapshot::new(
        rows.snapshot(&row_viewport).unwrap(),
        columns.snapshot(&column_viewport).unwrap(),
    )
    .unwrap();
    for path in [
        "row_indices",
        "column_indices",
        "row_items",
        "column_items",
        "cell_iter_setup",
        "cells",
    ] {
        ALLOCATIONS.with(|stats| stats.set(Some(Allocations::default())));
        let started = Instant::now();
        for _ in 0..ITERATIONS {
            match path {
                "row_indices" => {
                    std::hint::black_box(grid.rows().working_indices());
                }
                "column_indices" => {
                    std::hint::black_box(grid.columns().working_indices());
                }
                "row_items" => {
                    for item in grid.rows().working_items() {
                        std::hint::black_box(item.unwrap());
                    }
                }
                "column_items" => {
                    for item in grid.columns().working_items() {
                        std::hint::black_box(item.unwrap());
                    }
                }
                "cell_iter_setup" => {
                    drop(std::hint::black_box(grid.cells()));
                }
                "cells" => {
                    for cell in grid.cells() {
                        std::hint::black_box(cell);
                    }
                }
                _ => unreachable!(),
            }
        }
        let elapsed = started.elapsed().as_nanos();
        let stats = ALLOCATIONS.with(|stats| stats.replace(None).unwrap());
        println!(
            "GRID_SNAPSHOT_ALLOC,path={path},iterations={ITERATIONS},ns={elapsed},allocations={},peak_bytes={}",
            stats.allocations, stats.peak
        );
    }
}
