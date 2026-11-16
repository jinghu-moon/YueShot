use super::*;
use ::windows::Win32::{
    Foundation::HANDLE,
    Graphics::Dxgi::{DXGI_FRAME_STATISTICS, DXGI_FRAME_STATISTICS_MEDIA, IDXGISwapChainMedia},
    System::Threading::{
        CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, SetWaitableTimer,
        TIMER_ALL_ACCESS,
    },
};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
/// Opt-in real visible composition surface. Owns only its test HWND and never
/// activates it, installs global hooks, or captures the user's desktop.
pub(crate) struct PresentProbe {
    pub(crate) window: ::windows::Win32::Foundation::HWND,
    composition: CompositionTarget,
    renderer: crate::windows::win::d2d::OverlayRenderer,
    bitmap: ID2D1Bitmap1,
    view: crate::windows::win::d2d::RenderView,
    next: std::time::Instant,
    last: Option<std::time::Instant>,
    intervals: Vec<u128>,
    baseline: Vec<u128>,
    draws: Vec<u128>,
    frame_timer: OwnedHandle,
    frame_statistics: Vec<serde_json::Value>,
    baseline_statistics: Vec<serde_json::Value>,
    baseline_display_intervals: Vec<f64>,
    media: Option<IDXGISwapChainMedia>,
    surface: [u32; 2],
    dpi_mode: &'static str,
}

impl PresentProbe {
    /// A known positive input for capture/controller lifecycle tests. It is drawn
    /// by a real independent desktop host, not substituted into captured bytes.
    pub(crate) fn positive_scene(&mut self) -> Rect {
        let [width, height] = self.surface;
        let target = Rect::new(
            width as i32 / 2 - 100,
            height as i32 / 2 - 60,
            width as i32 / 2 + 100,
            height as i32 / 2 + 60,
        );
        let mut pixels = [32, 48, 64, 255].repeat(width as usize * height as usize);
        for y in target.top..target.bottom {
            for x in target.left..target.right {
                let at = (y as usize * width as usize + x as usize) * 4;
                pixels[at..at + 4].copy_from_slice(&[190, 210, 230, 255]);
            }
        }
        self.renderer.update_frame(width, height, &pixels).unwrap();
        self.view.selection = self.view.frame;
        self.view.show_chrome = false;
        self.next = std::time::Instant::now();
        self.pump();
        target
    }

    pub(crate) fn new(device: &GraphicsDevice) -> Self {
        let dpi_mode = crate::windows::monitor::set_per_monitor_v2_awareness().unwrap();
        assert!(dpi_mode.starts_with("per-monitor"));
        let monitor = crate::windows::monitor::captured_monitor_at(Point::new(0, 0)).unwrap();
        let surface = [monitor.width(), monitor.height()];
        let origin = monitor.origin();
        use crate::windows::win::d2d::{OverlayRenderer, RenderView};
        use ::windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, SW_SHOWNOACTIVATE, ShowWindow, WS_EX_NOACTIVATE,
            WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOPMOST, WS_POPUP,
        };
        let window = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST,
                ::windows::core::w!("STATIC"),
                ::windows::core::w!("YueShot visual transfer test"),
                WS_POPUP,
                origin.x,
                origin.y,
                surface[0] as i32,
                surface[1] as i32,
                None,
                None,
                None,
                None,
            )
        }
        .unwrap();
        // Install window teardown before fallible D2D construction.
        struct Guard(::windows::Win32::Foundation::HWND);
        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = unsafe { ::windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.0) };
            }
        }
        let guard = Guard(window);
        let mut renderer = OverlayRenderer::new(Arc::new(device.clone()), 96).unwrap();
        renderer
            .update_frame(
                surface[0],
                surface[1],
                &[48, 64, 80, 255].repeat(surface[0] as usize * surface[1] as usize),
            )
            .unwrap();
        renderer.ensure_back_buffer(surface[0], surface[1]).unwrap();
        let composition = renderer.composition_target(window).unwrap();
        composition.attach(renderer.swap_chain().unwrap()).unwrap();
        let context = device.create_d2d_context().unwrap();
        let bitmap = device
            .create_target_bitmap(&context, renderer.swap_chain().unwrap())
            .unwrap();
        let mut view = RenderView::new(Rect::new(0, 0, surface[0] as i32, surface[1] as i32));
        view.selection = Rect::new(80, 60, surface[0] as i32 - 80, surface[1] as i32 - 60);
        view.show_chrome = true;
        view.cursor_visible = false;
        // ShowWindow reports the previous visibility, not a success BOOL.
        let _ = unsafe { ShowWindow(window, SW_SHOWNOACTIVATE) };
        let timer = unsafe {
            CreateWaitableTimerExW(
                None,
                ::windows::core::PCWSTR::null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS.0,
            )
        }
        .unwrap();
        let media = renderer.swap_chain().unwrap().cast().ok();
        let mut result = Self {
            window,
            composition,
            renderer,
            bitmap,
            view,
            next: std::time::Instant::now(),
            last: None,
            intervals: Vec::new(),
            baseline: Vec::new(),
            draws: Vec::new(),
            frame_timer: unsafe { OwnedHandle::from_raw_handle(timer.0) },
            frame_statistics: Vec::new(),
            baseline_statistics: Vec::new(),
            baseline_display_intervals: Vec::new(),
            media,
            surface,
            dpi_mode,
        };
        std::mem::forget(guard);
        // Paired environment baseline, real Present and message pumping. This
        // is a full-monitor test surface, not the complete product controller.
        // Require an actually presented baseline, not a zero/stale DXGI record
        // or only 100 submission samples before DWM statistics become ready.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
        while result.intervals.len() < 1005 || result.display_intervals().len() < 1000 {
            if std::time::Instant::now() >= deadline {
                result.report();
                panic!("display baseline unavailable");
            }
            result.pump();
            result.wait();
        }
        result.baseline_display_intervals = result.display_intervals();
        result.baseline = std::mem::take(&mut result.intervals)
            .into_iter()
            .skip(5)
            .collect();
        result.last = None;
        result.draws.clear();
        result.baseline_statistics = std::mem::take(&mut result.frame_statistics);
        result
    }

    fn display_intervals(&self) -> Vec<f64> {
        // A statistic is for the last *displayed* present. Reject sparse updates:
        // a jump in PresentCount cannot reconstruct individual frame intervals.
        let mut frequency = 0;
        unsafe { ::windows::Win32::System::Performance::QueryPerformanceFrequency(&mut frequency) }
            .unwrap();
        let mut intervals = Vec::new();
        for pair in self.frame_statistics.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            let count = |v: &serde_json::Value, key: &str| v[key].as_u64().unwrap_or(0);
            if count(a, "present_count") > 0
                && count(b, "present_count") == count(a, "present_count") + 1
                && count(a, "present_refresh_count") == count(a, "sync_refresh_count")
                && count(b, "present_refresh_count") == count(b, "sync_refresh_count")
                && let (Some(a), Some(b)) =
                    (a["sync_qpc_time"].as_i64(), b["sync_qpc_time"].as_i64())
                && b > a
            {
                intervals.push((b - a) as f64 * 1_000_000.0 / frequency as f64);
            }
        }
        intervals
    }

    pub(crate) fn wait(&self) {
        use ::windows::Win32::UI::WindowsAndMessaging::{
            MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, QS_ALLINPUT,
        };
        let handle = HANDLE(self.frame_timer.as_raw_handle());
        let status = unsafe {
            MsgWaitForMultipleObjectsEx(Some(&[handle]), 10_000, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        };
        assert!(status.0 <= 1, "test message/frame wait failed: {status:?}");
    }

    pub(crate) fn pump(&mut self) {
        use ::windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, IsWindow, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
        };
        let mut message = MSG::default();
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        assert!(
            unsafe { IsWindow(Some(self.window)) }.as_bool(),
            "test surface was closed"
        );
        let now = std::time::Instant::now();
        if now < self.next {
            return;
        }
        let started = std::time::Instant::now();
        self.renderer.draw_to(&self.bitmap, &self.view).unwrap();
        let present_status = unsafe {
            self.renderer
                .swap_chain()
                .unwrap()
                .Present(1, DXGI_PRESENT(0))
        };
        assert!(
            present_status.is_ok(),
            "test Present failed: {present_status:?}"
        );
        self.composition.commit().unwrap();
        let finished = std::time::Instant::now();
        let mut statistics = DXGI_FRAME_STATISTICS::default();
        let result = unsafe {
            self.renderer
                .swap_chain()
                .unwrap()
                .GetFrameStatistics(&mut statistics)
        };
        let mut record = match result {
            Ok(()) => {
                serde_json::json!({"present_count":statistics.PresentCount,"present_refresh_count":statistics.PresentRefreshCount,"sync_refresh_count":statistics.SyncRefreshCount,"sync_qpc_time":statistics.SyncQPCTime})
            }
            Err(error) => serde_json::json!({"error":error.code().0}),
        };
        record["present_status"] = serde_json::json!(present_status.0);
        if let Some(media) = &self.media {
            let mut statistics = DXGI_FRAME_STATISTICS_MEDIA::default();
            record["media"] = match unsafe { media.GetFrameStatisticsMedia(&mut statistics) } {
                Ok(()) => {
                    serde_json::json!({"present_count":statistics.PresentCount,"present_refresh_count":statistics.PresentRefreshCount,"sync_refresh_count":statistics.SyncRefreshCount,"sync_qpc_time":statistics.SyncQPCTime,"composition_mode":statistics.CompositionMode.0})
                }
                Err(error) => serde_json::json!({"error":error.code().0}),
            };
        }
        self.frame_statistics.push(record);
        if let Some(last) = self.last {
            self.intervals
                .push(finished.duration_since(last).as_micros());
        }
        self.draws.push(started.elapsed().as_micros());
        self.last = Some(finished);
        self.next = started + std::time::Duration::from_micros(16_667);
        let delay = -(self
            .next
            .saturating_duration_since(std::time::Instant::now())
            .as_nanos()
            .div_ceil(100)
            .max(1) as i64);
        unsafe {
            SetWaitableTimer(
                HANDLE(self.frame_timer.as_raw_handle()),
                &delay,
                0,
                None,
                None,
                false,
            )
        }
        .unwrap();
    }

    fn report(&mut self) {
        println!(
            "VISUAL_PRESENT {}",
            serde_json::json!({"scope":"full-monitor topmost no-activate test surface, real D2D/DComp/Present with native message/frame waits; displayed intervals require consecutive DXGI statistics; not the product controller","surface":self.surface,"dpi_mode":self.dpi_mode,"baseline_intervals_us":self.baseline,"analysis_intervals_us":self.intervals,"analysis_draw_present_us":self.draws,"baseline_display_intervals_us":self.baseline_display_intervals,"analysis_display_intervals_us":self.display_intervals(),"baseline_frame_statistics":self.baseline_statistics,"analysis_frame_statistics":self.frame_statistics})
        );
    }

    pub(crate) fn qualify_product_competition(&mut self) {
        let mut baseline = self.baseline_display_intervals.clone();
        let mut analysis = self.display_intervals();
        self.report();
        assert!(
            baseline.len() >= 1000 && analysis.len() >= 1000,
            "insufficient consecutive display samples"
        );
        baseline.sort_by(f64::total_cmp);
        analysis.sort_by(f64::total_cmp);
        let p95 = |samples: &[f64]| samples[(samples.len() * 95).div_ceil(100) - 1];
        let delta = p95(&analysis) - p95(&baseline);
        println!(
            "PRODUCT_PRESENT {}",
            serde_json::json!({"baseline_samples":baseline.len(),"analysis_samples":analysis.len(),"display_p95_delta_us":delta,"max_delta_us":1000,"scope":"product controller competes on the same device with real full-monitor D2D/DComp/Present"})
        );
        assert!(delta <= 1000.0, "product display P95 regression: {delta}us");
    }

    pub(crate) fn displayed_samples(&self) -> usize {
        self.display_intervals().len()
    }
}

impl Drop for PresentProbe {
    fn drop(&mut self) {
        let _ = unsafe { ::windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.window) };
    }
}

#[test]
#[ignore = "L3 real GPU atlas and offscreen D2D, Demo T22 slicing exactly on T01 edges; no compute shader selected"]
fn visual_replay_demo_t22_partitioned_raw_reads_preserve_pixels_evidence_and_d2d() {
    use crate::visual_detection::{Image, proposals};
    use crate::windows::win::d2d::{OverlayRenderer, RenderView};
    use std::time::{Duration, Instant};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/visual-candidate-v4.1/T01.png");
    let mut decoder = png::Decoder::new(std::io::Cursor::new(std::fs::read(path).unwrap()));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().unwrap();
    let mut source = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut source).unwrap();
    source.truncate(info.buffer_size());
    for pixel in source.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let device = GraphicsDevice::create().unwrap();
    let texture = device
        .create_bgra_texture(info.width, info.height, &source)
        .unwrap();
    let mut renderer = OverlayRenderer::new(Arc::new(device.clone()), 96).unwrap();
    renderer
        .set_frame_texture(
            info.width,
            info.height,
            &texture.texture,
            PixelDescriptor::bgra8(),
        )
        .unwrap();
    renderer
        .ensure_back_buffer(info.width, info.height)
        .unwrap();
    let target = device
        .create_render_target_texture(info.width, info.height, PixelFormat::Bgra8Unorm)
        .unwrap();
    let context = device.create_d2d_context().unwrap();
    let bitmap = create_bitmap_from_texture(
        &context,
        &target.texture,
        D2D1_BITMAP_OPTIONS_TARGET,
        D2D1_ALPHA_MODE_PREMULTIPLIED,
    )
    .unwrap();
    let mut view = RenderView::new(Rect::new(0, 0, info.width as i32, info.height as i32));
    view.selection = Rect::new(41, 31, 141, 101);
    view.show_chrome = false;
    view.cursor_visible = false;
    renderer.draw_to(&bitmap, &view).unwrap();
    let before = device
        .read_back_region_bgra(&target.texture, 0, 0, info.width, info.height)
        .unwrap();
    let mut regions = Vec::new();
    for y in [0, 31, 101, info.height as i32].windows(2) {
        for x in [0, 41, 141, info.width as i32].windows(2) {
            regions.push(Rect::new(x[0], y[0], x[1], y[1]));
        }
    }
    let mut atlas = BatchReadback::new(&device).unwrap();
    atlas.begin(&texture.texture, regions).unwrap();
    while !atlas.submit_slice().unwrap() {}
    let deadline = Instant::now() + Duration::from_secs(2);
    let output = loop {
        if let Some(result) = atlas.poll() {
            break result.unwrap();
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_micros(100));
    };
    let mut assembled = vec![0; source.len()];
    for (r, pixels) in output {
        for y in 0..r.height() as usize {
            let from = y * r.width() as usize * 4;
            let to = ((r.top as usize + y) * info.width as usize + r.left as usize) * 4;
            assembled[to..to + r.width() as usize * 4]
                .copy_from_slice(&pixels[from..from + r.width() as usize * 4]);
        }
    }
    assert_eq!(assembled, source);
    let size = [info.width as usize, info.height as usize];
    let point = Point::new(91, 66);
    let unsliced =
        serde_json::to_value(proposals::seed(Image::new(size, &source).unwrap(), point)).unwrap();
    let sliced = serde_json::to_value(proposals::seed(
        Image::new(size, &assembled).unwrap(),
        point,
    ))
    .unwrap();
    assert_eq!(sliced, unsliced);
    assert_eq!(
        device
            .read_back_region_bgra(&texture.texture, 0, 0, info.width, info.height)
            .unwrap(),
        source
    );
    renderer.draw_to(&bitmap, &view).unwrap();
    assert_eq!(
        device
            .read_back_region_bgra(&target.texture, 0, 0, info.width, info.height)
            .unwrap(),
        before
    );
    println!(
        "DEMO_TRANSFER T22 partitions=9 raw_bytes_exact=true evidence_exact=true source_unchanged=true d2d_output_exact=true compute_shader=false"
    );
}

#[test]
#[ignore = "P3 real GPU/worker development timing; no production/UI qualification"]
fn visual_transfer_progressive_worker_development_costs() {
    use crate::visual_detection::{
        Completion, Image,
        progressive::{Query, Step},
        proposals,
    };
    use serde_json::json;
    use std::time::{Duration, Instant};
    assert!(
        !std::hint::black_box(cfg!(debug_assertions)),
        "timing requires --release"
    );
    enum Work {
        Start(Vec<u8>),
        Regions(Vec<(Rect, Vec<u8>)>),
        Pace,
    }
    struct QueryReply {
        step: Step,
        cpu_us: u128,
        passes: usize,
        extra_bytes: usize,
        buffer_bound: usize,
    }
    enum Reply {
        PollDue,
        Query(QueryReply),
    }
    fn send_reply(
        sender: &std::sync::mpsc::SyncSender<Reply>,
        reply: Reply,
        owner: Option<u32>,
    ) -> Option<()> {
        sender.send(reply).ok()?;
        if let Some(owner) = owner {
            use ::windows::Win32::{
                Foundation::{LPARAM, WPARAM},
                UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP},
            };
            unsafe { PostThreadMessageW(owner, WM_APP + 99, WPARAM(0), LPARAM(0)) }.unwrap();
        }
        Some(())
    }
    fn wait_reply(
        results: &std::sync::mpsc::Receiver<Reply>,
        render: &mut Option<PresentProbe>,
    ) -> Reply {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(render) = render {
                render.pump();
                match results.try_recv() {
                    Ok(reply) => return reply,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => panic!("worker stopped"),
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
                assert!(Instant::now() < deadline, "owner reply timeout");
                render.wait();
                continue;
            }
            match results.recv_timeout(Duration::from_millis(1)) {
                Ok(reply) => return reply,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    assert!(Instant::now() < deadline, "owner reply timeout")
                }
                Err(error) => panic!("worker stopped: {error}"),
            }
        }
    }
    fn next_work(
        receive: &std::sync::mpsc::Receiver<Work>,
        reply: &std::sync::mpsc::SyncSender<Reply>,
        timer: &OwnedHandle,
        owner: Option<u32>,
    ) -> Option<Work> {
        loop {
            match receive.recv().ok()? {
                Work::Pace => {
                    use ::windows::Win32::{
                        Foundation::WAIT_OBJECT_0, System::Threading::WaitForSingleObjectEx,
                    };
                    let handle = HANDLE(timer.as_raw_handle());
                    unsafe { SetWaitableTimer(handle, &-2_500, 0, None, None, false) }.unwrap();
                    let status = unsafe { WaitForSingleObjectEx(handle, 10000, false) };
                    assert_eq!(
                        status, WAIT_OBJECT_0,
                        "bounded polling timer failed or timed out"
                    );
                    send_reply(reply, Reply::PollDue, owner)?;
                }
                work => return Some(work),
            }
        }
    }
    fn transfer(
        batch: &mut BatchReadback,
        source: &ID3D11Texture2D,
        regions: Vec<Rect>,
        events: &mut Vec<u128>,
        jobs: &std::sync::mpsc::SyncSender<Work>,
        results: &std::sync::mpsc::Receiver<Reply>,
        render: &mut Option<PresentProbe>,
    ) -> Vec<(Rect, Vec<u8>)> {
        let started = Instant::now();
        batch.begin(source, regions).unwrap();
        events.push(started.elapsed().as_micros());
        loop {
            let start = Instant::now();
            let done = batch.submit_slice().unwrap();
            events.push(start.elapsed().as_micros());
            if done {
                break;
            }
            if let Some(render) = render {
                render.pump();
            }
            std::thread::yield_now();
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let start = Instant::now();
            let output = batch.poll();
            events.push(start.elapsed().as_micros());
            if let Some(output) = output {
                return output.unwrap();
            }
            assert!(Instant::now() < deadline, "visual transfer timeout");
            jobs.send(Work::Pace).unwrap();
            assert!(matches!(wait_reply(results, render), Reply::PollDue));
        }
    }
    let samples = std::env::var("YUESHOT_VISUAL_P3_SAMPLES")
        .ok()
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(100);
    let sessions = std::env::var("YUESHOT_VISUAL_P3_SESSIONS")
        .ok()
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(1);
    assert!((100..=10000).contains(&samples) && (1..=10).contains(&sessions));
    let device = GraphicsDevice::create().unwrap();
    let mut batch = BatchReadback::new(&device).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut render = (std::env::var("YUESHOT_VISUAL_P3_PRESENT").as_deref() == Ok("1"))
        .then(|| PresentProbe::new(&device));
    let mut rows = Vec::new();
    for session in 0..sessions {
        for (directory, name, point) in [
            ("visual-candidate-v4.1", "T01.png", Point::new(91, 66)),
            ("visual-candidate-v4.1", "T21.png", Point::new(410, 302)),
            ("visual-candidate-v4.1", "T10.png", Point::new(1816, 1066)),
            (
                "visual-candidate-8k-v4",
                "T10-8k.png",
                Point::new(1816, 1066),
            ),
        ] {
            let mut decoder = png::Decoder::new(std::io::Cursor::new(
                std::fs::read(root.join(directory).join(name)).unwrap(),
            ));
            decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
            let mut reader = decoder.read_info().unwrap();
            let mut source = vec![0; reader.output_buffer_size().unwrap()];
            let info = reader.next_frame(&mut source).unwrap();
            assert_eq!(info.color_type, png::ColorType::Rgba);
            source.truncate(info.buffer_size());
            for pixel in source.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            let size = [info.width as usize, info.height as usize];
            let texture = device
                .create_bgra_texture(info.width, info.height, &source)
                .unwrap();
            let expected =
                serde_json::to_value(proposals::seed(Image::new(size, &source).unwrap(), point))
                    .unwrap();
            let roi = Query::initial_region(size, point).unwrap();
            let (jobs, receive) = std::sync::mpsc::sync_channel(1);
            let (reply, results) = std::sync::mpsc::sync_channel(1);
            let owner = render
                .as_ref()
                .map(|_| unsafe { ::windows::Win32::System::Threading::GetCurrentThreadId() });
            let worker = std::thread::spawn(move || {
                let handle = unsafe {
                    CreateWaitableTimerExW(
                        None,
                        ::windows::core::PCWSTR::null(),
                        CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                        TIMER_ALL_ACCESS.0,
                    )
                }
                .unwrap();
                let timer = unsafe { OwnedHandle::from_raw_handle(handle.0) };
                while let Some(Work::Start(initial)) = next_work(&receive, &reply, &timer, owner) {
                    let start = Instant::now();
                    let mut query = Query::new(
                        size,
                        point,
                        Query::initial_region(size, point).unwrap(),
                        initial,
                    )
                    .unwrap();
                    let mut cpu_us = start.elapsed().as_micros();
                    loop {
                        let start = Instant::now();
                        let step = query.step();
                        cpu_us += start.elapsed().as_micros();
                        let reads = matches!(step, Step::Read(_));
                        send_reply(
                            &reply,
                            Reply::Query(QueryReply {
                                step,
                                cpu_us,
                                passes: query.passes,
                                extra_bytes: query.extra_bytes,
                                buffer_bound: query.peak_buffer_bound,
                            }),
                            owner,
                        )
                        .unwrap();
                        if !reads {
                            break;
                        }
                        let Work::Regions(regions) =
                            next_work(&receive, &reply, &timer, owner).unwrap()
                        else {
                            panic!("expected owned regions");
                        };
                        let start = Instant::now();
                        query.apply(regions).unwrap();
                        cpu_us += start.elapsed().as_micros();
                    }
                }
            });
            let mut warm = Vec::new();
            let mut owner = Vec::new();
            let mut cpu = Vec::new();
            let mut first = 0;
            let mut max_passes = 0;
            let mut max_extra = 0;
            let mut max_batches = 0;
            let mut max_buffer_bound = 0;
            for iteration in 0..samples + 6 {
                let mut events = Vec::new();
                let start = Instant::now();
                let mut initial = transfer(
                    &mut batch,
                    &texture.texture,
                    vec![roi],
                    &mut events,
                    &jobs,
                    &results,
                    &mut render,
                );
                jobs.send(Work::Start(initial.remove(0).1)).unwrap();
                let mut batches = 1;
                loop {
                    let Reply::Query(result) = wait_reply(&results, &mut render) else {
                        panic!("unexpected poll wake");
                    };
                    max_passes = max_passes.max(result.passes);
                    max_extra = max_extra.max(result.extra_bytes);
                    max_buffer_bound = max_buffer_bound.max(result.buffer_bound);
                    match result.step {
                        Step::Read(regions) => {
                            let data = transfer(
                                &mut batch,
                                &texture.texture,
                                regions,
                                &mut events,
                                &jobs,
                                &results,
                                &mut render,
                            );
                            batches += 1;
                            jobs.send(Work::Regions(data)).unwrap();
                        }
                        Step::Complete(actual) => {
                            assert_eq!(actual.completion, Completion::Complete);
                            assert_eq!(
                                serde_json::to_value(actual).unwrap(),
                                expected,
                                "progressive GPU/worker differs from full immutable source"
                            );
                            let elapsed = start.elapsed().as_micros();
                            if iteration == 0 {
                                first = elapsed;
                            }
                            if iteration >= 6 {
                                warm.push(elapsed);
                                owner.extend(events);
                                cpu.push(result.cpu_us);
                            }
                            break;
                        }
                        Step::Unavailable(reason) => {
                            panic!("progressive query unavailable: {reason:?}")
                        }
                    }
                }
                max_batches = max_batches.max(batches);
            }
            drop(jobs);
            worker.join().unwrap();
            assert!(!batch.pending());
            assert_eq!(device.read_back_bgra(&texture.texture).unwrap(), source);
            // Include a conservative 2MiB worker stack and 2MiB owner atlas-plan
            // metadata. Incoming payload is already in the Query bound; the
            // staging itself is the separately reported explicit GPU 4MiB.
            let cpu_bound = max_buffer_bound + 4 * 1_048_576;
            assert!(
                cpu_bound <= 16 * 1_048_576,
                "prototype CPU buffers exceed budget"
            );
            rows.push(json!({"session":session,"fixture":name,"size":size,"first_use_us":first,"warm_end_to_end_us":warm,"owner_events_us":owner,"warm_worker_us":cpu,"max_passes":max_passes,"max_batches":max_batches,"max_extra_bytes":max_extra,"cpu_buffer_bound_bytes":cpu_bound,"source_unchanged":true}));
        }
    }
    if let Some(render) = render.as_mut() {
        render.report();
    }
    // Cancellation does not release or overwrite a GPU-in-use staging atlas.
    let source = device
        .create_bgra_texture(64, 64, &vec![255; 64 * 64 * 4])
        .unwrap();
    batch
        .begin(&source.texture, vec![Rect::new(0, 0, 64, 64)])
        .unwrap();
    batch.cancel();
    assert!(
        batch
            .begin(&source.texture, vec![Rect::new(0, 0, 64, 64)])
            .is_err()
    );
    while !batch.submit_slice().unwrap() {}
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(result) = batch.poll() {
            assert_eq!(result.unwrap_err(), "visual transfer cancelled");
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!batch.pending());
    println!(
        "VISUAL_TRANSFER {}",
        json!({"scope":"P3 isolated idle SDR development; not UI/contention/independent qualification","warm_samples_per_scene":samples,"sessions":sessions,"atlas_bytes":ATLAS*ATLAS*4,"copy_slice":COPY_SLICE,"rows":rows,"max_pending_gpu_batches":1,"max_cpu_workers":1,"cancelled_inflight_reuse_rejected":true})
    );
}

#[test]
#[ignore = "P3 real D2D and sampler interleaving, atlas bytes and cancellation; no visible UI"]
fn visual_transfer_atlas_keeps_exact_pixels_while_drawing_and_sampling() {
    use crate::windows::win::d2d::{OverlayRenderer, RenderView};
    use serde_json::json;
    use std::time::{Duration, Instant};
    let device = GraphicsDevice::create().unwrap();
    let size = [1281usize, 721usize];
    let mut source = vec![0; size[0] * size[1] * 4];
    for (i, pixel) in source.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        pixel.copy_from_slice(&[
            (i % 251) as u8,
            (i / size[0] % 239) as u8,
            ((i % size[0] + i / size[0]) % 233) as u8,
            255,
        ]);
    }
    let texture = device
        .create_bgra_texture(size[0] as u32, size[1] as u32, &source)
        .unwrap();
    let mut sampler = AsyncSampleBuffer::new(&device).unwrap();
    let mut renderer = OverlayRenderer::new(Arc::new(device.clone()), 96).unwrap();
    renderer
        .set_frame_texture(
            size[0] as u32,
            size[1] as u32,
            &texture.texture,
            PixelDescriptor::bgra8(),
        )
        .unwrap();
    renderer
        .ensure_back_buffer(size[0] as u32, size[1] as u32)
        .unwrap();
    let target = device
        .create_render_target_texture(size[0] as u32, size[1] as u32, PixelFormat::Bgra8Unorm)
        .unwrap();
    let d2d = device.create_d2d_context().unwrap();
    let bitmap = create_bitmap_from_texture(
        &d2d,
        &target.texture,
        D2D1_BITMAP_OPTIONS_TARGET,
        D2D1_ALPHA_MODE_PREMULTIPLIED,
    )
    .unwrap();
    let mut view = RenderView::new(Rect::new(0, 0, size[0] as i32, size[1] as i32));
    view.selection = Rect::new(40, 40, 500, 400);
    view.show_chrome = false;
    view.cursor_visible = false;
    let regions = vec![
        Rect::new(0, 0, 1281, 7),
        Rect::new(1277, 7, 1281, 721),
        Rect::new(31, 99, 1029, 101),
        Rect::new(20, 20, 532, 280),
    ];
    let mut atlas = BatchReadback::new(&device).unwrap();
    assert!(
        atlas
            .begin(&texture.texture, vec![Rect::new(-1, 0, 1, 1)])
            .is_err()
    );
    assert!(
        atlas
            .begin(&texture.texture, vec![Rect::new(0, 0, 1281, 721)])
            .is_err()
    );
    let mut render_baseline = Vec::new();
    let mut render_interleaved = Vec::new();
    let mut copy_events = Vec::new();
    for iteration in 0..105 {
        let started = Instant::now();
        renderer.draw_to(&bitmap, &view).unwrap();
        let elapsed = started.elapsed().as_micros();
        if iteration >= 5 {
            render_baseline.push(elapsed);
        }
        atlas.begin(&texture.texture, regions.clone()).unwrap();
        let sample = sampler.submit(&texture.texture, 32, 32, 32).unwrap();
        let started = Instant::now();
        renderer.draw_to(&bitmap, &view).unwrap();
        let elapsed = started.elapsed().as_micros();
        if iteration >= 5 {
            render_interleaved.push(elapsed);
        }
        loop {
            let started = Instant::now();
            let done = atlas.submit_slice().unwrap();
            if iteration >= 5 {
                copy_events.push(started.elapsed().as_micros());
            }
            if done {
                break;
            }
            renderer.draw_to(&bitmap, &view).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut output = None;
        let mut sampled = None;
        while output.is_none() || sampled.is_none() {
            if output.is_none() {
                output = atlas.poll();
            }
            if sampled.is_none() {
                sampled = sampler.poll(sample);
            }
            assert!(Instant::now() < deadline);
            if output.is_none() || sampled.is_none() {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        for (rect, data) in output.unwrap().unwrap() {
            for y in 0..rect.height() as usize {
                let start = ((y + rect.top as usize) * size[0] + rect.left as usize) * 4;
                assert_eq!(
                    &data[y * rect.width() as usize * 4..(y + 1) * rect.width() as usize * 4],
                    &source[start..start + rect.width() as usize * 4]
                );
            }
        }
        let data = sampled.unwrap().unwrap();
        for y in 0..32 {
            let start = ((y + 32) * size[0] + 32) * 4;
            assert_eq!(&data[y * 128..(y + 1) * 128], &source[start..start + 128]);
        }
        assert!(!atlas.pending());
    }
    assert_eq!(device.read_back_bgra(&texture.texture).unwrap(), source);
    println!(
        "VISUAL_INTERLEAVE {}",
        json!({"scope":"real offscreen D2D draw submission and sampler interleaving; not presentation interval/UI qualification","size":size,"exact_repetitions":105,"source_unchanged":true,"render_baseline_us":render_baseline,"render_interleaved_us":render_interleaved,"owner_copy_events_us":copy_events})
    );
}

#[test]
#[ignore = "P3 opt-in 100 cold transfer sessions and 60s idle resource reference"]
fn visual_transfer_cold_sessions_and_idle_resource_reference() {
    use crate::visual_detection::progressive::Query;
    use ::windows::Win32::Graphics::Dxgi::{
        DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL,
        DXGI_QUERY_VIDEO_MEMORY_INFO, IDXGIAdapter3,
    };
    use serde_json::json;
    use std::time::{Duration, Instant};
    assert!(
        !std::hint::black_box(cfg!(debug_assertions)),
        "measurement needs release"
    );
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetProcessHandleCount(process: *mut core::ffi::c_void, count: *mut u32) -> i32;
    }
    let memory = || {
        let mut handles = 0;
        let process = (-1isize) as *mut core::ffi::c_void;
        assert_ne!(unsafe { GetProcessHandleCount(process, &mut handles) }, 0);
        json!({"working_set":crate::windows::proc_mem::working_set_bytes().unwrap(),"private_bytes":crate::windows::proc_mem::private_bytes().unwrap(),"handles":handles,"handle_types":crate::windows::proc_mem::handle_types()})
    };
    let device = GraphicsDevice::create().unwrap();
    let dxgi: IDXGIDevice = device.device().cast().unwrap();
    let adapter: IDXGIAdapter3 = unsafe { dxgi.GetAdapter() }.unwrap().cast().unwrap();
    let description = unsafe { adapter.GetDesc1() }.unwrap();
    let gpu = || {
        [
            DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
            DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL,
        ]
        .map(|group| {
            let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
            unsafe { adapter.QueryVideoMemoryInfo(0, group, &mut info) }.unwrap();
            info.CurrentUsage
        })
    };
    let size = [7680usize, 4320usize];
    let point = Point::new(1816, 1066);
    let source = vec![255; size[0] * size[1] * 4];
    let texture = device
        .create_bgra_texture(size[0] as u32, size[1] as u32, &source)
        .unwrap();
    let roi = Query::initial_region(size, point).unwrap();
    // Warm the existing native readback before the no-analysis control. Merely
    // creating the source does not initialise the driver's Map/copy machinery.
    for _ in 0..5 {
        let pixels = device
            .read_back_region_bgra(
                &texture.texture,
                roi.left as u32,
                roi.top as u32,
                roi.width() as u32,
                roi.height() as u32,
            )
            .unwrap();
        assert!(pixels.iter().all(|&v| v == 255));
    }
    // A matched no-analysis control precedes allocations: process/driver lazy
    // initialization cannot be attributed to a visual resource by raw counts.
    // Keep every sample; the subsequent strict session/idle assertions remain.
    let mut control = Vec::new();
    for second in 0..=60 {
        let sample = json!({"second":second,"memory":memory(),"gpu":gpu()});
        println!("VISUAL_RESOURCE_CONTROL {}", sample);
        control.push(sample);
        if second < 60 {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    let mut rows = Vec::new();
    let mut baseline_handles = None;
    assert_eq!(Query::live_count(), 0);
    assert_eq!(LIVE_TRANSFERS.load(std::sync::atomic::Ordering::SeqCst), 0);
    for cycle in 0..105 {
        let started = Instant::now();
        let mut atlas = BatchReadback::new(&device).unwrap();
        let query = Query::new(size, point, roi, vec![255; roi.area() as usize * 4]).unwrap();
        atlas.begin(&texture.texture, vec![roi]).unwrap();
        while !atlas.submit_slice().unwrap() {}
        let deadline = Instant::now() + Duration::from_secs(10);
        let data = loop {
            if let Some(data) = atlas.poll() {
                break data.unwrap();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(data.len(), 1);
        assert!(data[0].1.iter().all(|&v| v == 255));
        assert!(!atlas.pending());
        let active = memory();
        let active_gpu = gpu();
        drop(data);
        drop(query);
        drop(atlas);
        let released = memory();
        assert_eq!(Query::live_count(), 0);
        assert_eq!(LIVE_TRANSFERS.load(std::sync::atomic::Ordering::SeqCst), 0);
        if cycle == 5 {
            baseline_handles = released["handles"].as_u64();
        }
        if cycle > 5 {
            assert_eq!(
                released["handles"].as_u64(),
                baseline_handles,
                "session leaked a native handle"
            );
        }
        rows.push(json!({"cycle":cycle,"warmup":cycle<5,"cold_session_us":started.elapsed().as_micros(),"active":active,"released":released,"active_gpu":active_gpu,"released_gpu":gpu()}));
    }
    println!("VISUAL_RESOURCE_SESSIONS {}", json!({"rows":rows}));
    let mut idle = Vec::new();
    for second in 0..=60 {
        let sample = json!({"second":second,"memory":memory(),"gpu":gpu(),"analysis_submissions":0,"analysis_polls":0,"worker_jobs":0});
        println!("VISUAL_RESOURCE_IDLE {}", sample);
        idle.push(sample);
        if second < 60 {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    // The specification requires an allocation/release balance and a stable
    // platform, not equality of the whole process at two arbitrary instants.
    // Matched no-analysis controls demonstrate lazy Timer/Semaphore creation.
    // All samples (including growth) remain evidence. Require a 20s plateau;
    // native session release counts above remain exact, without a tolerance.
    let plateau = &idle[40..];
    let control_private = control
        .iter()
        .map(|s| s["memory"]["private_bytes"].as_u64().unwrap())
        .collect::<Vec<_>>();
    let noise = control_private.iter().max().unwrap() - control_private.iter().min().unwrap();
    let first = &plateau[0];
    for sample in plateau {
        assert_eq!(
            sample["memory"]["handle_types"], first["memory"]["handle_types"],
            "no stable native-handle platform"
        );
        assert!(
            sample["memory"]["private_bytes"].as_u64().unwrap()
                <= first["memory"]["private_bytes"].as_u64().unwrap() + noise,
            "private bytes grow beyond matched-control measurement noise"
        );
        for segment in 0..2 {
            assert!(
                sample["gpu"][segment].as_u64().unwrap() <= first["gpu"][segment].as_u64().unwrap(),
                "no GPU memory platform"
            );
        }
    }
    assert_eq!(device.read_back_bgra(&texture.texture).unwrap(), source);
    println!(
        "VISUAL_TRANSFER_RESOURCE {}",
        json!({"scope":"isolated cold staging/query lifecycle; not process startup or product UI/session lifecycle","adapter":{"name":String::from_utf16_lossy(&description.Description).trim_end_matches('\0'),"vendor":description.VendorId,"device":description.DeviceId},"control":control,"rows":rows,"idle":idle,"source_unchanged":true,"cpu_index_limit_bytes":8*1_048_576,"gpu_explicit_staging_bytes":ATLAS*ATLAS*4})
    );
}

#[test]
#[ignore = "P3 real textures and injected Map faults; does not remove the physical device"]
fn visual_transfer_formats_capacity_and_map_failures_preserve_lifecycle() {
    use ::windows::Win32::Graphics::Dxgi::DXGI_ERROR_DEVICE_REMOVED;
    use ::windows::core::Error;
    use std::time::{Duration, Instant};
    let device = GraphicsDevice::create().unwrap();
    let source = vec![171; 64 * 64 * 4];
    let texture = device.create_bgra_texture(64, 64, &source).unwrap();
    let fp16 = device
        .create_render_target_texture(64, 64, PixelFormat::Rgba16Float)
        .unwrap();
    let mut batch = BatchReadback::new(&device).unwrap();
    assert!(
        batch
            .begin(&fp16.texture, vec![Rect::new(0, 0, 1, 1)])
            .is_err()
    );
    for regions in [
        vec![],
        vec![Rect::new(0, 0, 0, 1)],
        vec![Rect::new(63, 63, 65, 64)],
        vec![Rect::new(0, 0, 1, 1); 16385],
        vec![Rect::new(0, 0, 64, 64); 65], // 1MiB + 16KiB
    ] {
        assert!(batch.begin(&texture.texture, regions).is_err());
        assert!(!batch.pending());
        assert!(batch.poll().is_none());
    }
    // The region-count limit itself is accepted. A partially submitted atlas
    // cannot be mapped, replaced, or released just because it was cancelled.
    batch
        .begin(&texture.texture, vec![Rect::new(0, 0, 1, 1); 16384])
        .unwrap();
    assert!(!batch.submit_slice().unwrap());
    assert!(batch.poll().is_none());
    batch.cancel();
    assert!(batch.pending());
    assert!(
        batch
            .begin(&texture.texture, vec![Rect::new(0, 0, 1, 1)])
            .is_err()
    );
    while !batch.submit_slice().unwrap() {}
    assert!(
        batch
            .complete_mapping(
                Err(Error::from_hresult(DXGI_ERROR_WAS_STILL_DRAWING)),
                D3D11_MAPPED_SUBRESOURCE::default()
            )
            .is_none()
    );
    assert!(batch.pending());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(result) = batch.poll() {
            assert_eq!(result.unwrap_err(), "visual transfer cancelled");
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!batch.pending());
    // Exact byte limit succeeds and returns every requested region unchanged.
    batch
        .begin(&texture.texture, vec![Rect::new(0, 0, 64, 64); 64])
        .unwrap();
    while !batch.submit_slice().unwrap() {}
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(result) = batch.poll() {
            let result = result.unwrap();
            assert_eq!(result.len(), 64);
            assert!(result.iter().all(|(_, bytes)| bytes == &source));
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    batch
        .begin(&texture.texture, vec![Rect::new(0, 0, 1, 1)])
        .unwrap();
    assert!(batch.submit_slice().unwrap());
    let error = batch
        .complete_mapping(
            Err(Error::from_hresult(DXGI_ERROR_DEVICE_REMOVED)),
            D3D11_MAPPED_SUBRESOURCE::default(),
        )
        .unwrap()
        .unwrap_err();
    assert!(GraphicsDevice::is_device_lost(&error));
    assert!(!batch.pending());
    assert!(batch.poll().is_none());
    assert_eq!(device.read_back_bgra(&texture.texture).unwrap(), source);
    drop(batch);
    assert_eq!(LIVE_TRANSFERS.load(std::sync::atomic::Ordering::SeqCst), 0);
    println!(
        "VISUAL_TRANSFER_FAULTS {}",
        serde_json::json!({"scope":"real format/budget/cancel tests; Map still-drawing and device-removed outcomes injected into the real completion branch, not physical device removal","region_limit":16384,"byte_limit":1048576,"partial_submission_map_rejected":true,"cancel_keeps_pending_until_complete":true,"device_error_no_result":true,"source_unchanged":true,"live_transfers_after_drop":0})
    );
}

#[test]
#[ignore = "L3 Demo Q05: selected product Query and real atlas, enforced source read permissions"]
fn visual_replay_demo_q05_actual_read_coverage_and_bounded_requery() {
    use crate::visual_detection::{
        Completion,
        progressive::{Query, Step},
    };
    use std::time::{Duration, Instant};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/visual-candidate-v4.1");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let group = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == "Q05")
        .unwrap();
    let rect = |value: &serde_json::Value| {
        let a: Vec<i32> = value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap() as i32)
            .collect();
        Rect::new(a[0], a[1], a[0] + a[2], a[1] + a[3])
    };
    let device = GraphicsDevice::create().unwrap();
    let mut previous = None;
    let mut count = 0;
    for member in group["members"].as_array().unwrap() {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(
            std::fs::read(root.join(member["input"]["image"].as_str().unwrap())).unwrap(),
        ));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
        let mut reader = decoder.read_info().unwrap();
        let mut source = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut source).unwrap();
        source.truncate(info.buffer_size());
        for pixel in source.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        if let Some(previous) = &previous {
            assert_eq!(previous, &source, "Q05 sources must be identical");
        }
        previous = Some(source.clone());
        let texture = device
            .create_bgra_texture(info.width, info.height, &source)
            .unwrap();
        let mut transfer = BatchReadback::new(&device).unwrap();
        let access = &member["input"]["region_access"];
        let initial = if access["mode"] == "full_source" {
            Rect::new(0, 0, info.width as i32, info.height as i32)
        } else {
            rect(&access["initial_roi"])
        };
        let additional = access["additional_source_access"].as_bool().unwrap();
        let mut read_count = 0;
        let mut actual_read_bytes = 0;
        let mut read = |regions: Vec<Rect>| {
            // This is the permission boundary, before any GPU copy. The query
            // receives only these owned bytes; the complete test fixture never
            // reaches its CPU reader. Post-test source verification is separate.
            if !additional {
                assert!(regions.iter().all(|region| initial.contains_rect(*region)));
            }
            read_count += regions.len();
            actual_read_bytes += regions
                .iter()
                .map(|region| region.area() as usize * 4)
                .sum::<usize>();
            transfer.begin(&texture.texture, regions).unwrap();
            while !transfer.submit_slice().unwrap() {}
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if let Some(result) = transfer.poll() {
                    break result.unwrap();
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_micros(100));
            }
        };
        let initial_pixels = read(vec![initial]).pop().unwrap().1;
        let mut query = Query::new(
            [info.width as usize, info.height as usize],
            Point::new(128, 96),
            initial,
            initial_pixels,
        )
        .unwrap();
        let mut denied = false;
        let mut result = None;
        for _ in 0..16 {
            match query.step() {
                Step::Read(regions) => {
                    assert!(!regions.is_empty());
                    if !additional && regions.iter().any(|region| !initial.contains_rect(*region)) {
                        denied = true;
                        break; // No copy, fabricated pixels, partial result or candidate escapes.
                    }
                    query.apply(read(regions)).unwrap();
                }
                Step::Complete(detection) => {
                    result = Some(detection);
                    break;
                }
                Step::Unavailable(reason) => panic!("Q05 unexpected unavailable: {reason:?}"),
            }
        }
        let truth = rect(&member["annotations"][0]["rect"]);
        if additional {
            let result = result.expect("Q05 allowed source must complete");
            assert_eq!(result.completion, Completion::Complete);
            assert!(
                result
                    .candidates
                    .iter()
                    .any(|candidate| candidate.rect == truth
                        && serde_json::to_value(candidate).unwrap()["qualified"] == true)
            );
            assert!(
                result
                    .candidates
                    .iter()
                    .all(|candidate| candidate.rect != rect(&access["initial_roi"]))
            );
            if access["mode"] == "roi_with_requery" {
                assert!(query.extra_bytes > 0);
            }
        } else {
            assert!(
                denied && result.is_none(),
                "restricted ROI cannot publish an unverified candidate"
            );
            assert_eq!(read_count, 1);
            assert_eq!(actual_read_bytes, initial.area() as usize * 4);
        }
        assert!(query.extra_bytes <= 1048576);
        assert_eq!(
            device
                .read_back_region_bgra(&texture.texture, 0, 0, info.width, info.height)
                .unwrap(),
            source
        );
        println!(
            "DEMO_READ_COVERAGE id={} read_count={read_count} actual_read_bytes={actual_read_bytes} requested_extra_bytes={} denied={denied} source_unchanged=true",
            member["id"], query.extra_bytes
        );
        count += 1;
    }
    assert_eq!(count, 3);
}
