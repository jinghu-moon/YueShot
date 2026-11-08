//! Opt-in real-desktop qualification through the F5 worker, native renderer,
//! panel encoders, durable history and F6 handoff. No clipboard/settings writes.
use super::*;
use crate::pin_source::{PinSource, SelectionProvider};
use std::{path::PathBuf, sync::Mutex, time::Instant};
use yue_media::{PixelFormat, pixels::PixelEncoding};
use yue_shot_capture::{
    CaptureService, SelectionPixels,
    artifact::round_pixel_corners,
    geometry::Rect,
    ports::ArtifactWriter,
    windows::{
        capture_worker::{CaptureWorker, StartRequest},
        color_output::ColorSnapshot,
        monitor,
        providers::{FrozenFrame, FrozenFramePixels, ProviderKind},
        renderer::{OverlayFrameState, Win32Renderer},
    },
};

struct Worker(CaptureWorker);
impl Drop for Worker {
    fn drop(&mut self) {
        self.0.shutdown();
    }
}

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct HiddenHost(windows_sys::Win32::Foundation::HWND);
impl HiddenHost {
    fn new() -> Self {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let window = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                0,
                0,
                64,
                64,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(
            !window.is_null(),
            "the hidden DirectComposition host must exist"
        );
        Self(window)
    }
}
impl Drop for HiddenHost {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(self.0);
        }
    }
}

fn capture_live_hdr() -> FrozenFrame {
    monitor::set_per_monitor_v2_awareness().unwrap();
    let cursor = monitor::cursor_position().unwrap();
    let monitor = monitor::captured_monitor_at(cursor).unwrap();
    let color = ColorSnapshot::query(monitor.handle).unwrap();
    eprintln!("[hdr-chain] acquisition output={color:?}");
    assert_eq!(
        color.hdr_active(),
        Some(true),
        "requires a confirmed active HDR output"
    );
    assert!(color.sdr_white_nits().is_some());
    // Posting is only a wake-up; this test polls the real capacity-1 mailbox.
    let worker = Worker(CaptureWorker::new());
    let generation = worker.0.next_generation();
    worker
        .0
        .start(StartRequest {
            generation,
            monitor,
            cursor_screen: cursor,
            requested_at: Instant::now(),
            notify_thread: unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() },
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let ready = loop {
        if let Some(result) = worker.0.take_ready() {
            break result.unwrap();
        }
        assert!(
            Instant::now() < deadline,
            "real F5 capture worker timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(ready.generation, generation);
    assert!(
        worker.0.take_ready().is_none(),
        "a frame must be consumed once"
    );
    let frozen = ready.frozen;
    assert_eq!(frozen.provider, ProviderKind::Wgc);
    assert_eq!(
        frozen.frame.source.descriptor().format(),
        PixelFormat::Rgba16Float,
        "hdr-capture builds must route the actual F5 worker to FP16 acquisition"
    );
    assert_eq!(
        frozen.frame.source.descriptor().encoding(),
        PixelEncoding::ScRgb
    );
    assert_eq!(
        frozen.color_snapshot(),
        &color,
        "output must stay stable during capture"
    );
    assert!(!frozen.pixels_read(), "arming F5 must keep readback lazy");
    frozen
}

#[derive(Default)]
struct Slot(Mutex<Option<Arc<SelectionPixels>>>);
impl SelectionProvider for Slot {
    fn selection(&self) -> Option<Arc<SelectionPixels>> {
        self.0.lock().unwrap().clone()
    }
    fn consume_selection(&self, expected: &Arc<SelectionPixels>) -> Result<bool, String> {
        let mut slot = self.0.lock().unwrap();
        if slot
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, expected))
        {
            *slot = None;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[::core::prelude::v1::test]
#[ignore = "live HDR desktop: captures screen, renders annotations on a hidden GPU target, saves isolated temporary artifacts; no clipboard or display-setting changes"]
fn native_hdr_worker_selection_panel_save_history_and_f6_keep_the_source() {
    let started = Instant::now();
    let frozen = capture_live_hdr();
    let full = frozen.read_region(frozen.frame.rect()).unwrap();
    let mut minimum = f64::INFINITY;
    let mut maximum = f64::NEG_INFINITY;
    let mut above_one = 0usize;
    let mut negative = 0usize;
    for pixel in yue_media::pixel_color::linear_rgba(&full) {
        assert!(pixel.into_iter().all(f64::is_finite));
        assert!((0.0..=1.0).contains(&pixel[3]));
        for channel in &pixel[..3] {
            minimum = minimum.min(*channel);
            maximum = maximum.max(*channel);
            above_one += usize::from(*channel > 1.0);
            negative += usize::from(*channel < 0.0);
        }
    }
    eprintln!(
        "[hdr-chain] frame={}x{} bytes={} min={minimum} max={maximum} above_one={above_one} negative={negative} hash={}",
        frozen.frame.width,
        frozen.frame.height,
        full.as_bytes().len(),
        blake3::hash(full.as_bytes())
    );
    assert!(
        above_one > 0,
        "keep a bright test pattern visible; an all-dark frame cannot prove >1 capture"
    );
    // Ordinary desktop content cannot contain negative scRGB values. The
    // negative-channel contract remains covered by the synthetic GPU tests;
    // this real-screen check records the observed count rather than inventing it.
    assert!(frozen.frame.width >= 640 && frozen.frame.height >= 360);
    let region = Rect::new(64, 64, 576, 320);
    let mut selected = CaptureService::new()
        .prepare_selection(&frozen.frame, region, &FrozenFramePixels::new(&frozen))
        .unwrap();
    for (row_ix, row) in selected
        .pixels
        .as_bytes()
        .as_chunks::<4096>()
        .0
        .iter()
        .enumerate()
    {
        let at = (64 + row_ix) * full.row_stride() + 64 * 8;
        assert_eq!(
            row,
            &full.as_bytes()[at..at + row.len()],
            "ROI must retain full-frame FP16 bits"
        );
    }
    drop(full);
    assert!(
        !frozen.pixels_read(),
        "ROI/full diagnostic reads must not fill the frozen-frame cache"
    );

    let host = HiddenHost::new();
    let mut renderer = Win32Renderer::new(
        windows::Win32::Foundation::HWND(host.0),
        &frozen.monitor.layout,
        frozen.device().unwrap().clone(),
    )
    .unwrap();
    renderer.set_frame(&frozen).unwrap();
    let mut view = OverlayFrameState::new();
    view.selection = region;
    view.annotation_items
        .push(yue_shot_capture::annotation::AnnotationItem {
            id: 1,
            kind: yue_shot_capture::annotation::AnnotationKind::Rectangle,
            geometry: yue_shot_capture::annotation::AnnotationGeometry::Rect {
                bounds: Rect::new(84, 84, 144, 124),
            },
            style: yue_shot_capture::annotation::AnnotationStyle {
                stroke_color: [0.5, 0.5, 0.5, 1.0],
                fill_color: Some([0.5, 0.5, 0.5, 1.0]),
                ..Default::default()
            },
        });
    let annotated = renderer.render_export(&view, None).unwrap();
    assert_eq!(annotated.descriptor(), selected.pixels.descriptor());
    assert_eq!(annotated.size(), selected.pixels.size());
    let expected_gray =
        0.21404114048223255 * frozen.color_snapshot().sdr_white_nits().unwrap() as f64 / 80.;
    let gray = yue_media::pixel_color::linear_rgba(&annotated)
        .nth(40 * 512 + 40)
        .unwrap();
    for value in &gray[..3] {
        assert!(
            (value - expected_gray).abs() < 0.003,
            "annotation UI white scale: {gray:?}"
        );
    }
    // A point well outside the annotation must retain the frozen scene.
    let before = yue_media::pixel_color::linear_rgba(&selected.pixels)
        .nth(200 * 512 + 400)
        .unwrap();
    let after = yue_media::pixel_color::linear_rgba(&annotated)
        .nth(200 * 512 + 400)
        .unwrap();
    for (a, b) in after.into_iter().zip(before) {
        assert!((a - b).abs() <= 0.003 + b.abs() * 0.002);
    }
    let before_rounding = annotated.as_bytes().to_vec();
    selected.pixels = Arc::new(round_pixel_corners(annotated, 16).unwrap());
    selected.corner_radius = 16;
    selected.validate().unwrap();
    let (before_pixels, _) = before_rounding.as_chunks::<8>();
    let (after_pixels, _) = selected.pixels.as_bytes().as_chunks::<8>();
    for (before, after) in before_pixels.iter().zip(after_pixels) {
        assert_eq!(
            &before[..6],
            &after[..6],
            "rounding may only change straight alpha"
        );
    }
    assert!(
        yue_media::pixel_color::linear_rgba(&selected.pixels)
            .next()
            .unwrap()[3]
            < 1.0
    );
    let source_hash = blake3::hash(selected.pixels.as_bytes());

    let root = Directory(std::env::temp_dir().join(format!(
        "snapclip-native-hdr-chain-{}-{}",
        std::process::id(),
        yue_shot_model::unix_time_ms()
    )));
    let store = yue_shot_storage::capture_history::CaptureHistoryStore::open(&root.0).unwrap();
    let writer = crate::capture::HistoryArtifactWriter::new(root.0.join("artifacts/capture"))
        .with_history(store.clone());
    let artifact = writer
        .write(
            "native-hdr-chain",
            &selected,
            frozen.monitor.layout.dpi,
            None,
        )
        .unwrap();
    assert!(matches!(
        artifact.payload,
        yue_shot_model::CapturePayload::PixelMaster { .. }
    ));
    let page = store.history_page(None, None).unwrap();
    assert_eq!(page.items.len(), 1);
    let source = &page.items[0].source;
    let replay = crate::history_source::read_source(source).unwrap();
    assert_eq!(replay.pixel_source(), selected.source());
    assert_eq!(replay.pixels().as_bytes(), selected.pixels.as_bytes());
    let picture = Arc::new(Picture::from_shared_pixels(selected.pixels.clone()));
    let original = preview::original(&picture).unwrap();
    assert_eq!(
        original.image().as_bytes(0).unwrap(),
        selected.pixels.as_bytes()
    );
    let mut export = Export {
        color_export: ColorExport::PreserveHdr,
        picture: picture.clone(),
        output: Size::new(512, 256),
        directory: root.0.to_string_lossy().into_owned(),
        name: "native-hdr".into(),
        format: SaveFormat::Png,
        tier: 1,
        fill: [255; 3],
        date: "2026-11-04".into(),
        time: "1200".into(),
        sequence: 1,
        conflict: ConflictPolicy::Overwrite,
        limit: None,
    };
    for format in [
        Some(SaveFormat::Png),
        cfg!(feature = "avif").then_some(SaveFormat::Avif),
    ]
    .into_iter()
    .flatten()
    {
        export.format = format;
        let saved = export.save().unwrap();
        let bytes = std::fs::read(&saved.path).unwrap();
        let decoded = yue_pin::decode_image(&bytes).unwrap();
        assert!(decoded.is_hdr());
        assert_eq!(decoded.pixels().size(), [512, 256]);
        let alpha = yue_media::pixel_color::linear_rgba(decoded.pixels())
            .next()
            .unwrap()[3];
        let expected = yue_media::pixel_color::linear_rgba(&selected.pixels)
            .next()
            .unwrap()[3];
        assert!((alpha - expected).abs() < 0.002);
        if format == SaveFormat::Png {
            let mut reader = png::Decoder::new(std::io::Cursor::new(&bytes))
                .read_info()
                .unwrap();
            assert_eq!(reader.info().bit_depth, png::BitDepth::Sixteen);
            assert_eq!(
                reader
                    .info()
                    .coding_independent_code_points
                    .unwrap()
                    .transfer_function,
                16
            );
            let mut raw = vec![0; reader.output_buffer_size().unwrap()];
            let info = reader.next_frame(&mut raw).unwrap();
            assert_eq!(
                &raw[..info.buffer_size()],
                yue_media::pixel_color::to_rec2020_pq16(&selected.pixels).unwrap(),
                "PNG16 must encode the explicit PQ derivative without another quantization"
            );
        }
        eprintln!(
            "[hdr-chain] panel save format={format:?} bytes={} hash={}",
            bytes.len(),
            blake3::hash(&bytes)
        );
    }
    export.format = SaveFormat::Png;
    export.name = "native-sdr".into();
    export.color_export = ColorExport::Sdr {
        source_peak_nits: picture.peak_nits(),
    };
    let sdr = export.save().unwrap();
    assert!(
        !yue_pin::decode_image(&std::fs::read(sdr.path).unwrap())
            .unwrap()
            .is_hdr()
    );
    let history_png = root.0.join("history-hdr.png");
    crate::history_source::export_source(source, &history_png).unwrap();
    assert!(
        yue_pin::decode_image(&std::fs::read(history_png).unwrap())
            .unwrap()
            .is_hdr()
    );

    let slot = Arc::new(Slot(Mutex::new(Some(Arc::new(selected.clone())))));
    let pins = Arc::new(PinSource::new());
    pins.set_home(slot.clone());
    let claim = pins.claim().unwrap();
    assert_eq!(
        claim.selection().region(),
        selected.screen_region().unwrap()
    );
    assert_eq!(claim.selection().image().pixel_source(), selected.source());
    assert!(Arc::ptr_eq(
        claim.selection().image().pixels(),
        &selected.pixels
    ));
    assert!(
        pins.claim().is_none(),
        "an in-flight F6 claim cannot duplicate the source"
    );
    drop(claim);
    assert!(
        slot.selection().is_some(),
        "an unacknowledged F6 creation keeps its retry source"
    );
    assert!(pins.claim().unwrap().commit().unwrap());
    assert!(slot.selection().is_none());
    assert_eq!(
        blake3::hash(picture.original_pixels().unwrap().as_bytes()),
        source_hash
    );
    assert_eq!(blake3::hash(replay.pixels().as_bytes()), source_hash);
    eprintln!(
        "[hdr-chain] complete source_hash={source_hash} elapsed_ms={} (no actual clipboard writes or F6 window input)",
        started.elapsed().as_millis()
    );
}
