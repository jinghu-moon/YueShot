//! Manual, reproducible headless CPU probes. They measure Element lifecycle
//! phases and submitted draw work, not a Windows GPU/presentation frame.
use super::*;
use std::time::Instant;

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

struct PerfProbe {
    state: Entity<ListGridState>,
    count: usize,
    virtualized: bool,
    builds: Rc<Cell<usize>>,
}
impl Render for PerfProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let grid = ListGrid::columns(3, px(12.)).unwrap();
        let builds = self.builds.clone();
        let content = if self.virtualized {
            YueVirtualListGrid::new("perf-virtual", &self.state, move |ix, key, _, _| {
                builds.set(builds.get() + 1);
                card(key.clone(), ix, ix % 3 == 1)
            })
            .scrollbar(false)
            .into_any_element()
        } else {
            YueList::new("perf-ordinary")
                .grid(grid)
                .children((0..self.count).map(|ix| {
                    builds.set(builds.get() + 1);
                    card(ix.into(), ix, ix % 3 == 1)
                }))
                .into_any_element()
        };
        div().w(px(600.)).h(px(320.)).child(content)
    }
}

#[gpui_kit::test]
#[ignore = "manual headless CPU/allocation evidence; run release --ignored --nocapture --test-threads=1"]
fn ordinary_and_virtual_card_cpu_evidence(cx: &mut TestAppContext) {
    cx.update(|cx| {
        yue_component::init(cx, YueTheme::new(ThemeMode::Light, Accent::Azure)).unwrap()
    });
    println!(
        "LIST_GRID_CPU,path,count,sample,frames,ns_per_frame,allocations_per_frame,peak_bytes,cards_per_frame"
    );
    for (virtualized, count) in [(false, 1_000), (true, 1_000), (true, 10_000)] {
        let mut probe = None;
        let (_, cx) = cx.add_window_view(|w, cx| {
            let view = cx.new(|cx| PerfProbe {
                state: cx.new(|cx| {
                    ListGridState::new(
                        Records::from((0..count).collect(), 0),
                        ListGrid::columns(3, px(12.)).unwrap(),
                        ListGridSizing::measured(px(180.)),
                        cx,
                    )
                    .unwrap()
                }),
                count,
                virtualized,
                builds: Rc::new(Cell::new(0)),
            });
            probe = Some(view.clone());
            gpui_kit::base::Root::new(view, w, cx)
        });
        let probe = probe.unwrap();
        draw(cx);
        draw(cx);
        for sample in 0..3 {
            let builds = probe.read_with(cx, |p, _| p.builds.clone());
            let mut elapsed = 0;
            let mut allocations = 0;
            let mut peak = 0;
            let mut cards = 0;
            for _ in 0..6 {
                probe.update(cx, |_, cx| cx.notify());
                let (ns, stats, count) = cx.update(|w, cx| {
                    w.refresh();
                    builds.set(0);
                    ALLOCATIONS.with(|s| s.set(Some(Allocations::default())));
                    let start = Instant::now();
                    w.draw(cx).clear(cx);
                    let elapsed = start.elapsed().as_nanos();
                    let stats = ALLOCATIONS.with(|s| s.replace(None)).unwrap();
                    (elapsed, stats, builds.get())
                });
                elapsed += ns;
                allocations += stats.allocations;
                peak = peak.max(stats.peak);
                cards += count;
            }
            println!(
                "LIST_GRID_CPU,{},{count},{sample},6,{},{},{},{}",
                if virtualized { "virtual" } else { "ordinary" },
                elapsed / 6,
                allocations / 6,
                peak,
                cards / 6
            );
        }
    }
}
