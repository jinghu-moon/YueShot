use std::path::Path;
use std::sync::Arc;

use yue_shot_capture::artifact::SelectionPixels;
use yue_shot_capture::geometry::MonitorLayout;
use yue_shot_capture::ports::{ArtifactWriter, CaptureEventSink, ClipboardWriter, OverlayPlatform};
use yue_shot_capture::{
    CaptureArtifact, CaptureError, CaptureResult, CaptureRuntime, CaptureState,
};

use yue_shot_storage::capture_history::CaptureHistoryStore;

pub struct UnavailableOverlay;

impl OverlayPlatform for UnavailableOverlay {
    fn state(&self) -> CaptureState {
        CaptureState::Idle
    }
    fn request_start(&self) -> CaptureResult<bool> {
        Err(CaptureError::Unsupported)
    }
    fn request_cancel(&self) -> CaptureResult<()> {
        Err(CaptureError::Unsupported)
    }
    fn request_confirm(&self) -> CaptureResult<()> {
        Err(CaptureError::Unsupported)
    }
    fn set_preferences(&self, _: yue_shot_capture::preferences::OverlayPreferences) {}
    fn shutdown(&self) {}
}

pub(crate) enum SaveRequest {
    Selection(SaveSelection),
    Bands {
        session_id: String,
        spec: yue_shot_capture::ports::SelectionSpec,
        bands: Box<dyn yue_shot_capture::ports::BandSource + Send>,
        dpi: u32,
    },
}

pub(crate) struct SaveSelection {
    pub session_id: String,
    pub selection: SelectionPixels,
}

#[derive(Clone)]
pub(super) struct Events {
    pin: async_channel::Sender<()>,
    refresh: async_channel::Sender<()>,
    save: async_channel::Sender<SaveRequest>,
    failed: async_channel::Sender<String>,
}

impl Events {
    pub(super) fn new(
        refresh: async_channel::Sender<()>,
        save: async_channel::Sender<SaveRequest>,
        pin: async_channel::Sender<()>,
        failed: async_channel::Sender<String>,
    ) -> Self {
        Self {
            refresh,
            save,
            pin,
            failed,
        }
    }
}

impl CaptureEventSink for Events {
    fn on_started(&self, session_id: &str, _: &MonitorLayout) {
        log::info!(target:"yueshot.event","capture.started session={session_id}");
    }
    fn on_state(&self, _: &str, _: CaptureState, _: Option<&MonitorLayout>) {}
    fn on_completed(&self, artifact: &CaptureArtifact) {
        log::debug!(
            "[yue-shot][capture] saved {}x{}",
            artifact.width,
            artifact.height
        );
        let _ = self.refresh.try_send(());
    }
    fn on_pin_requested(&self, _: &str) {
        if let Err(error) = self.pin.try_send(()) {
            log::debug!("[yue-shot][pin] could not queue selection: {error}");
        }
    }
    fn on_save_requested(&self, session_id: &str, pixels: &SelectionPixels) {
        let [width, height] = pixels.pixels.size();
        match self.save.try_send(SaveRequest::Selection(SaveSelection {
            session_id: session_id.into(),
            selection: pixels.clone(),
        })) {
            Ok(()) => log::debug!(
                "[yue-shot][save] selection queued for save panel ({}x{})",
                width,
                height
            ),
            Err(error) => log::debug!("[yue-shot][save] could not queue selection: {error}"),
        }
    }
    fn on_save_bands_requested(
        &self,
        session_id: &str,
        spec: yue_shot_capture::ports::SelectionSpec,
        bands: Box<dyn yue_shot_capture::ports::BandSource + Send>,
        dpi: u32,
    ) {
        if let Err(error) = self.save.try_send(SaveRequest::Bands {
            session_id: session_id.into(),
            spec,
            bands,
            dpi,
        }) {
            log::debug!("[yue-shot][save] could not queue long capture: {error}");
        }
    }
    fn on_cancelled(&self, session_id: &str, reason: &str) {
        log::debug!("[yue-shot][capture] cancelled {session_id}: {reason}");
    }
    fn on_failed(&self, session_id: Option<&str>, error: &CaptureError, provider: &str) {
        log::debug!("[yue-shot][capture] failed {session_id:?} via {provider}: {error}");
        // Bounded, nonblocking handoff: never open a GPUI window on the capture thread.
        let _ = self
            .failed
            .try_send(format!("截图失败（{provider}）：{error}；可重新发起截图"));
    }
}

struct SystemClipboardWriter;

impl ClipboardWriter for SystemClipboardWriter {
    fn copy_text(&self, text: &str) {
        if let Err(error) = yue_clipboard::SystemClipboard.copy_text(text) {
            log::debug!("[yue-shot][clipboard] text write failed: {error}");
        }
    }

    fn copy_image(
        &self,
        width: u32,
        height: u32,
        bgra: &[u8],
        png: Option<&Path>,
    ) -> Result<(), String> {
        let png = png.and_then(|path| std::fs::read(path).ok());
        yue_clipboard::SystemClipboard.copy_image(&yue_clipboard::ClipboardImage {
            width,
            height,
            pixels: bgra,
            channels: yue_clipboard::Channels::Bgra,
            png: png.as_deref(),
        })
    }
}

pub fn start(
    data: &Path,
    history: Option<CaptureHistoryStore>,
    events: Events,
    detection: yue_shot_capture::window_detection::DetectionOptions,
    preferences: yue_shot_capture::preferences::OverlayPreferences,
) -> Result<Arc<CaptureRuntime>, String> {
    yue_shot_capture::windows::monitor::set_per_monitor_v2_awareness()?;
    let service = Arc::new(yue_shot_capture::CaptureService::new());
    let events: Arc<dyn CaptureEventSink> = Arc::new(events);
    let clipboard: Arc<dyn ClipboardWriter> = Arc::new(SystemClipboardWriter);
    let writer: Arc<dyn ArtifactWriter> = Arc::new(match history {
        Some(history) => crate::capture::HistoryArtifactWriter::new(history.artifact_root())
            .with_history(history),
        None => crate::capture::HistoryArtifactWriter::new(data.join("artifacts/capture")),
    });
    let overlay = yue_shot_capture::windows::overlay::WindowsOverlay::spawn_overlay(
        service,
        events,
        clipboard,
        writer,
        detection,
        preferences,
    )
    .map_err(|message| format!("capture overlay: {message}"))?;
    Ok(Arc::new(CaptureRuntime::from_platform(Box::new(overlay))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use yue_media::pixels::{PixelBuffer, PixelDescriptor, PixelSource};
    use yue_shot_capture::geometry::Rect;
    use yue_shot_capture::ports::{BandSource, SelectionSpec};
    use yue_shot_capture::session::CapturedFrame;

    static TEST_ROOT_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn capture_failure_handoff_is_bounded_and_never_blocks_the_overlay() {
        let (sender, receiver) = async_channel::bounded(2);
        let sink = Events::new(
            async_channel::unbounded().0,
            async_channel::unbounded().0,
            async_channel::unbounded().0,
            sender,
        );
        for _ in 0..100 {
            sink.on_failed(None, &CaptureError::Unsupported, "test");
        }
        assert_eq!(receiver.len(), 2);
        assert!(receiver.try_recv().unwrap().contains("截图失败"));
        assert!(receiver.try_recv().unwrap().contains("可重新发起截图"));
    }

    #[test]
    fn pin_request_is_queued_once_for_the_product_thread() {
        let (sender, receiver) = async_channel::unbounded();
        let events = Events {
            failed: async_channel::bounded(8).0,
            pin: sender,
            refresh: async_channel::unbounded().0,
            save: async_channel::unbounded().0,
        };
        events.on_pin_requested("capture-pin");
        assert_eq!(receiver.try_recv(), Ok(()));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn save_request_forwards_the_original_pixel_buffer_to_the_ui_channel() {
        let (save_sender, save_receiver) = async_channel::unbounded();
        let events = Events {
            failed: async_channel::bounded(8).0,
            pin: async_channel::unbounded().0,
            refresh: async_channel::unbounded().0,
            save: save_sender,
        };
        let descriptor = PixelDescriptor::bgra8();
        let pixels =
            Arc::new(PixelBuffer::from_tight(1, 1, descriptor, vec![1, 2, 3, 255]).unwrap());
        let selection = SelectionPixels {
            frame: CapturedFrame {
                width: 1,
                height: 1,
                source: PixelSource::new(descriptor, 1),
                captured_at_unix_ms: 1,
                provider: "test",
                origin: None,
            },
            region: Rect::new(0, 0, 1, 1),
            pixels: Arc::clone(&pixels),
            origin: None,
            corner_radius: 0,
        };

        events.on_save_requested("test", &selection);

        let SaveRequest::Selection(request) = save_receiver.try_recv().unwrap() else {
            panic!("expected ordinary selection");
        };
        assert_eq!(request.selection.pixels.as_bytes(), pixels.as_bytes());
    }

    #[test]
    fn long_save_request_transfers_owned_bands_without_reading_or_exporting() {
        struct Deferred;
        impl BandSource for Deferred {
            fn source(&self) -> PixelSource {
                PixelSource::new(PixelDescriptor::bgra8(), 7)
            }
            fn rows(&self) -> u32 {
                80_000
            }
            fn read_rows(&mut self, _: u32, _: u32, _: &mut Vec<u8>) -> bool {
                panic!("event handoff must not read pixels");
            }
        }
        let (sender, receiver) = async_channel::unbounded();
        let events = Events {
            failed: async_channel::bounded(8).0,
            save: sender,
            pin: async_channel::unbounded().0,
            refresh: async_channel::unbounded().0,
        };
        let spec = SelectionSpec {
            region: Rect::new(0, 0, 1000, 80_000),
            source: Deferred.source(),
            captured_at_unix_ms: 17,
            origin: None,
            corner_radius: 0,
        };
        events.on_save_bands_requested("long", spec, Box::new(Deferred), 144);
        let SaveRequest::Bands {
            session_id,
            spec: received,
            bands,
            dpi,
        } = receiver.try_recv().unwrap()
        else {
            panic!("expected long source");
        };
        assert_eq!(session_id, "long");
        assert_eq!(received.region, spec.region);
        assert_eq!(bands.rows(), 80_000);
        assert_eq!(dpi, 144);
        assert!(receiver.try_recv().is_err());
    }

    struct TestRows {
        pixels: Vec<u8>,
    }

    impl BandSource for TestRows {
        fn source(&self) -> PixelSource {
            PixelSource::new(PixelDescriptor::bgra8(), 7)
        }
        fn rows(&self) -> u32 {
            1
        }
        fn read_rows(&mut self, first: u32, rows: u32, out: &mut Vec<u8>) -> bool {
            if first != 0 || rows != 1 {
                return false;
            }
            out.clear();
            out.extend_from_slice(&self.pixels);
            true
        }
    }

    #[test]
    fn sdr_artifact_is_a_complete_png_with_bgra_converted_to_rgba() {
        let source = PixelSource::new(PixelDescriptor::bgra8(), 7);
        let mut rows = TestRows {
            pixels: vec![1, 2, 3, 255],
        };
        let root = std::env::temp_dir().join(format!(
            "yue-shot-sdr-{}-{}",
            std::process::id(),
            TEST_ROOT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let history = CaptureHistoryStore::open(&root).unwrap();
        let writer = crate::capture::HistoryArtifactWriter::new(history.artifact_root())
            .with_history(history.clone());
        let artifact = writer
            .write_bands(
                "test",
                &SelectionSpec {
                    region: Rect::new(0, 0, 1, 1),
                    source,
                    captured_at_unix_ms: 0,
                    origin: None,
                    corner_radius: 0,
                },
                &mut rows,
                96,
                None,
            )
            .unwrap();
        let bytes = std::fs::read(artifact.artifact_path()).unwrap();
        let page = history.history_page(None, None).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(
            page.items[0].source.artifact.absolute_path,
            artifact.artifact_path()
        );
        std::fs::remove_dir_all(root).unwrap();

        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        assert_eq!((reader.info().width, reader.info().height), (1, 1));
        let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut decoded).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(&decoded[..info.buffer_size()], &[3, 2, 1, 255]);
    }

    #[test]
    fn writer_persists_the_artifact_and_registers_product_history() {
        let root = std::env::temp_dir().join(format!(
            "yue-shot-app-test-{}-{}",
            std::process::id(),
            TEST_ROOT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let history = CaptureHistoryStore::open(&root).unwrap();
        let writer = crate::capture::HistoryArtifactWriter::new(history.artifact_root())
            .with_history(history.clone());
        let source = PixelSource::new(PixelDescriptor::bgra8(), 11);
        let selection = SelectionPixels {
            frame: yue_shot_capture::session::CapturedFrame {
                width: 1,
                height: 1,
                source,
                captured_at_unix_ms: 7,
                provider: "test",
                origin: None,
            },
            region: yue_shot_capture::geometry::Rect::new(0, 0, 1, 1),
            pixels: Arc::new(
                PixelBuffer::from_tight(1, 1, PixelDescriptor::bgra8(), vec![1, 2, 3, 255])
                    .unwrap(),
            ),
            origin: None,
            corner_radius: 0,
        };

        let artifact = <crate::capture::HistoryArtifactWriter as ArtifactWriter>::write(
            &writer,
            "capture-test",
            &selection,
            96,
            None,
        )
        .unwrap();
        assert!(artifact.artifact_path().is_file());
        assert_eq!(history.history_page(None, Some(10)).unwrap().items.len(), 1);
        assert_eq!(
            history.history_page(None, Some(10)).unwrap().items[0]
                .source
                .artifact
                .absolute_path,
            artifact.artifact_path()
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn writer_preserves_high_precision_pixels_in_the_source_master() {
        use yue_media::pixels::{AlphaMode, PixelEncoding};

        let root = std::env::temp_dir().join(format!(
            "yue-shot-hdr-test-{}-{}",
            std::process::id(),
            TEST_ROOT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let history = CaptureHistoryStore::open(&root).unwrap();
        let writer = crate::capture::HistoryArtifactWriter::new(history.artifact_root())
            .with_history(history.clone());
        let descriptor = PixelDescriptor::new(
            yue_media::PixelFormat::Rgba16Float,
            PixelEncoding::ScRgb,
            AlphaMode::Straight,
        )
        .unwrap();
        let source = PixelSource::new(descriptor, 13);
        let pixel_bytes: Vec<u8> = [0.25_f32, 1.5, 3.0, 1.0]
            .into_iter()
            .flat_map(|value| half::f16::from_f32(value).to_le_bytes())
            .collect();
        let selection = SelectionPixels {
            frame: yue_shot_capture::session::CapturedFrame {
                width: 1,
                height: 1,
                source,
                captured_at_unix_ms: 9,
                provider: "test-hdr",
                origin: None,
            },
            region: yue_shot_capture::geometry::Rect::new(0, 0, 1, 1),
            pixels: Arc::new(
                PixelBuffer::from_tight(1, 1, descriptor, pixel_bytes.clone()).unwrap(),
            ),
            origin: None,
            corner_radius: 0,
        };

        let artifact = <crate::capture::HistoryArtifactWriter as ArtifactWriter>::write(
            &writer,
            "capture-hdr",
            &selection,
            96,
            None,
        )
        .unwrap();
        let yue_shot_model::CapturePayload::PixelMaster { reference } = artifact.payload else {
            panic!("high precision source must not be quantized to PNG8");
        };
        let mut master = yue_shot_storage::pixel_master::PixelMaster::open(&reference).unwrap();
        assert_eq!(master.read_rows(0, 1).unwrap().as_bytes(), pixel_bytes);
        assert_eq!(history.history_page(None, Some(10)).unwrap().items.len(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod native_tests {
    #[test]
    #[ignore = "takes the F5 hotkey and creates a real overlay window"]
    fn the_shell_can_host_the_capture_overlay() {
        let root =
            std::env::temp_dir().join(format!("yueshot-capture-host-{}", std::process::id()));
        let runtime = super::start(
            &root,
            None,
            super::Events::new(
                async_channel::unbounded().0,
                async_channel::unbounded().0,
                async_channel::unbounded().0,
                async_channel::bounded(8).0,
            ),
            Default::default(),
            Default::default(),
        )
        .expect("overlay starts on a desktop");
        assert_eq!(runtime.state(), yue_shot_capture::CaptureState::Idle);
        let source = std::sync::Arc::new(crate::pin_source::PinSource::default());
        source.set_home(runtime.clone());
        assert!(source.claim().is_none());
        runtime.shutdown();
        runtime.shutdown();
        drop(runtime);
        let _ = std::fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod runtime_acknowledgement_tests {
    use super::*;
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    /// A platform that hands out one selection and counts the cancel requests it sees.
    struct FakeOverlay {
        selection: Mutex<Option<Arc<yue_shot_capture::artifact::SelectionPixels>>>,
        cancels: Arc<AtomicUsize>,
    }

    impl yue_shot_capture::ports::OverlayPlatform for FakeOverlay {
        fn state(&self) -> CaptureState {
            CaptureState::Selected
        }

        fn request_start(&self) -> yue_shot_capture::CaptureResult<bool> {
            Ok(false)
        }

        fn request_cancel(&self) -> yue_shot_capture::CaptureResult<()> {
            self.cancels.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn request_confirm(&self) -> yue_shot_capture::CaptureResult<()> {
            Ok(())
        }

        fn set_preferences(&self, _preferences: yue_shot_capture::preferences::OverlayPreferences) {
        }

        fn selection(&self) -> Option<Arc<yue_shot_capture::artifact::SelectionPixels>> {
            self.selection
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }

        fn consume_selection(
            &self,
            expected: &Arc<yue_shot_capture::artifact::SelectionPixels>,
        ) -> yue_shot_capture::CaptureResult<bool> {
            let mut current = self.selection.lock().unwrap();
            if current
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, expected))
            {
                *current = None;
                self.cancels.fetch_add(1, Ordering::SeqCst);
                Ok(true)
            } else {
                Ok(false)
            }
        }

        fn shutdown(&self) {}
    }

    fn selection_pixels() -> Arc<yue_shot_capture::artifact::SelectionPixels> {
        let frame = yue_shot_capture::session::CapturedFrame {
            width: 4,
            height: 4,
            source: yue_media::pixels::PixelSource::new(
                yue_media::pixels::PixelDescriptor::bgra8(),
                0,
            ),
            captured_at_unix_ms: 0,
            provider: "test",
            origin: Some(yue_shot_capture::geometry::Point::new(0, 0)),
        };
        Arc::new(yue_shot_capture::artifact::SelectionPixels {
            corner_radius: 0,
            frame,
            region: yue_shot_capture::geometry::Rect::new(0, 0, 2, 2),
            pixels: yue_media::pixels::PixelBuffer::from_tight(
                2,
                2,
                yue_media::pixels::PixelDescriptor::bgra8(),
                vec![0; 16],
            )
            .unwrap()
            .into(),
            origin: Some(yue_shot_capture::geometry::Point::new(0, 0)),
        })
    }

    /// How many cancel requests the platform behind `runtime` has seen.
    ///
    /// `CaptureRuntime` hands the platform out by value and does not give it back, so the
    /// counter is kept next to the runtime and shared through this cell instead of being
    /// read off the platform afterwards.
    fn fake_overlay() -> (Arc<yue_shot_capture::CaptureRuntime>, Arc<AtomicUsize>) {
        let cancels = Arc::new(AtomicUsize::new(0));
        let runtime = Arc::new(yue_shot_capture::CaptureRuntime::from_platform(Box::new(
            FakeOverlay {
                selection: Mutex::new(Some(selection_pixels())),
                cancels: Arc::clone(&cancels),
            },
        )));
        (runtime, cancels)
    }

    #[test]
    fn only_successful_creation_consumes_and_ends_the_capture_session() {
        let (runtime, cancels) = fake_overlay();
        let home = runtime;
        let first = home.selection().unwrap();
        assert!(Arc::ptr_eq(&first, &home.selection().unwrap()));
        assert_eq!(
            cancels.load(Ordering::SeqCst),
            0,
            "reservation cannot end capture"
        );
        assert!(home.consume_selection(&first).unwrap());
        assert!(home.selection().is_none());
        assert_eq!(cancels.load(Ordering::SeqCst), 1);
        assert!(!home.consume_selection(&first).unwrap());
        assert_eq!(
            cancels.load(Ordering::SeqCst),
            1,
            "one successful destination, one cancellation"
        );
    }
}
