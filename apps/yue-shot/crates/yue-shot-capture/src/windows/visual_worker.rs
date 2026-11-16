//! Single CPU image worker. Only owned pixels cross this boundary; no GPU/COM handles.
use crate::geometry::{Point, Rect};
use crate::visual_detection::{
    Completion,
    progressive::{Query, Step},
};
use crate::window_detection::{SelectionSource, WindowCandidate};
use ::windows::Win32::{
    Foundation::{HANDLE, LPARAM, WPARAM},
    System::Threading::{
        CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, SetWaitableTimer,
        TIMER_ALL_ACCESS, WaitForSingleObject,
    },
    UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP},
};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::thread::JoinHandle;

pub(crate) const VISUAL_READY_MESSAGE: u32 = WM_APP + 46;
#[cfg(test)]
pub(crate) static LIVE_WORKERS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Key {
    pub request: u64,
    pub source: SelectionSource,
    pub owner: WindowCandidate,
    pub point: Point,
    pub submitted: std::time::Instant,
}

pub(crate) enum Work {
    Start {
        key: Key,
        size: [usize; 2],
        region: Rect,
        pixels: Vec<u8>,
    },
    Restart(Key),
    Apply {
        key: Key,
        regions: Vec<(Rect, Vec<u8>)>,
    },
    Pace {
        micros: u32,
    },
    Reset,
}

// A single preallocated mailbox slot: boxing adds a fallible allocation per reply
// solely to save fewer than 256 bytes in that one slot.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Reply {
    Step {
        key: Key,
        step: Step,
        coverage: Rect,
        buffer_bound: usize,
    },
    Poll,
    Idle,
}

pub(crate) struct VisualWorker {
    #[cfg(test)]
    submitted: AtomicU64,
    sender: Option<mpsc::SyncSender<Work>>,
    result: Arc<Mutex<Option<Reply>>>,
    current: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl VisualWorker {
    pub fn new(owner_thread: u32) -> Result<Self, String> {
        if !crate::visual_detection::allocation::permit("worker_mailbox") {
            return Err("visual mailbox allocation failed".into());
        }
        let (sender, receive) = mpsc::sync_channel(1);
        let result = Arc::new(Mutex::new(None));
        let current = Arc::new(AtomicU64::new(0));
        let output = result.clone();
        let generation = current.clone();
        // Create before spawning: timer failure cannot leave a silent dead worker.
        let handle = unsafe {
            CreateWaitableTimerExW(
                None,
                None,
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS.0,
            )
        }
        .map_err(|e| e.to_string())?;
        let timer = unsafe { OwnedHandle::from_raw_handle(handle.0) };
        let thread = std::thread::Builder::new()
            .name("visual-candidates".into())
            .stack_size(2 * 1_048_576)
            .spawn(move || {
                #[cfg(test)]
                struct Lifetime;
                #[cfg(test)]
                impl Drop for Lifetime {
                    fn drop(&mut self) {
                        LIVE_WORKERS.fetch_sub(1, Ordering::SeqCst);
                    }
                }
                #[cfg(test)]
                LIVE_WORKERS.fetch_add(1, Ordering::SeqCst);
                #[cfg(test)]
                let _lifetime = Lifetime;
                let mut query: Option<Query> = None;
                let mut cache_source = None;
                while let Ok(work) = receive.recv() {
                    let mut message = None;
                    let key = match work {
                        Work::Reset => {
                            query = None;
                            cache_source = None;
                            message = Some(Reply::Idle);
                            None
                        }
                        Work::Pace { micros } => {
                            if generation.load(Ordering::Acquire) == 0 {
                                query = None;
                                cache_source = None;
                            }
                            let handle = HANDLE(timer.as_raw_handle());
                            let due = -i64::from(micros) * 10;
                            let armed =
                                unsafe { SetWaitableTimer(handle, &due, 0, None, None, false) }
                                    .is_ok();
                            if armed {
                                unsafe { WaitForSingleObject(handle, 100) };
                            }
                            message = Some(Reply::Poll);
                            None
                        }
                        Work::Start {
                            key,
                            size,
                            region,
                            pixels,
                        } => {
                            query = None;
                            cache_source = Some(key.source);
                            match Query::new(size, key.point, region, pixels) {
                                Ok(q) => query = Some(q),
                                Err(reason) => {
                                    message = Some(Reply::Step {
                                        key,
                                        step: Step::Unavailable(reason),
                                        coverage: Rect::default(),
                                        buffer_bound: 0,
                                    })
                                }
                            }
                            Some(key)
                        }
                        Work::Restart(key) => {
                            if cache_source != Some(key.source)
                                || query.as_mut().is_none_or(|q| !q.restart(key.point))
                            {
                                message = Some(Reply::Step {
                                    key,
                                    step: Step::Unavailable(Completion::Failed),
                                    coverage: Rect::default(),
                                    buffer_bound: 0,
                                });
                            }
                            Some(key)
                        }
                        Work::Apply { key, regions } => {
                            if let Some(q) = &mut query {
                                if let Err(reason) = q.apply(regions) {
                                    message = Some(Reply::Step {
                                        key,
                                        step: Step::Unavailable(reason),
                                        coverage: q.coverage(),
                                        buffer_bound: q.peak_buffer_bound,
                                    });
                                }
                            } else {
                                message = Some(Reply::Step {
                                    key,
                                    step: Step::Unavailable(Completion::Failed),
                                    coverage: Rect::default(),
                                    buffer_bound: 0,
                                });
                            }
                            Some(key)
                        }
                    };
                    if let Some(key) = key {
                        if generation.load(Ordering::Acquire) != key.request {
                            query = None;
                            cache_source = None;
                            message = Some(Reply::Idle);
                        }
                        if message.is_none()
                            && let Some(q) = &mut query
                        {
                            let step = q.step();
                            message = Some(Reply::Step {
                                key,
                                step,
                                coverage: q.coverage(),
                                buffer_bound: q.peak_buffer_bound,
                            });
                        }
                        if generation.load(Ordering::Acquire) != key.request {
                            query = None;
                            cache_source = None;
                            message = Some(Reply::Idle);
                        }
                    }
                    if let Some(message) = message {
                        if let Ok(mut slot) = output.lock() {
                            *slot = Some(message);
                        }
                        if owner_thread != 0 {
                            let _ = unsafe {
                                PostThreadMessageW(
                                    owner_thread,
                                    VISUAL_READY_MESSAGE,
                                    WPARAM(0),
                                    LPARAM(0),
                                )
                            };
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            #[cfg(test)]
            submitted: AtomicU64::new(0),
            sender: Some(sender),
            result,
            current,
            thread: Some(thread),
        })
    }

    pub fn select(&self, request: u64) {
        self.current.store(request, Ordering::Release);
    }
    pub fn submit(&self, work: Work) -> bool {
        let submitted = self
            .sender
            .as_ref()
            .is_some_and(|s| s.try_send(work).is_ok());
        #[cfg(test)]
        if submitted {
            self.submitted.fetch_add(1, Ordering::Relaxed);
        }
        submitted
    }
    #[cfg(test)]
    pub(super) fn submitted_jobs(&self) -> u64 {
        self.submitted.load(Ordering::Relaxed)
    }
    pub fn take(&self) -> Option<Reply> {
        self.result.lock().ok()?.take()
    }

    #[cfg(test)]
    pub(super) fn publish_for_replay(&self, reply: Reply) {
        *self.result.lock().unwrap() = Some(reply);
    }
    pub fn cancel(&self) {
        self.select(0);
        let _ = self.submit(Work::Reset);
        let _ = self.take();
    }
}

impl Drop for VisualWorker {
    fn drop(&mut self) {
        self.cancel();
        self.sender.take();
        // No GPU wait; CPU loops and the timer are bounded. Cancel itself never joins.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn receive(worker: &VisualWorker) -> Reply {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(reply) = worker.take() {
                return reply;
            }
            assert!(Instant::now() < deadline, "CPU worker stopped responding");
            std::thread::sleep(Duration::from_micros(100));
        }
    }

    #[test]
    fn visual_replay_cancel_during_poll_releases_cpu_pixels_before_idle() {
        use crate::visual_detection::progressive::LIVE_QUERIES;
        let initial = LIVE_QUERIES.load(Ordering::SeqCst);
        let worker = VisualWorker::new(0).unwrap();
        let source = SelectionSource {
            capture_generation: 1,
            pixels: yue_media::pixels::PixelSource::new(
                yue_media::pixels::PixelDescriptor::bgra8(),
                1,
            ),
        };
        let key = Key {
            request: 1,
            source,
            owner: WindowCandidate::new(
                crate::window_detection::WindowIdentity::new(1, 2, 3, 1),
                Rect::new(0, 0, 512, 512),
                0,
                1,
            ),
            point: Point::new(256, 256),
            submitted: Instant::now(),
        };
        worker.select(1);
        assert!(worker.submit(Work::Start {
            key,
            size: [512, 512],
            region: Rect::new(0, 0, 512, 512),
            pixels: [32, 64, 96, 255].repeat(512 * 512)
        }));
        assert!(matches!(receive(&worker), Reply::Step { .. }));
        assert_eq!(LIVE_QUERIES.load(Ordering::SeqCst), initial + 1);
        // Same cancellation edge as an owner invalidation while Pace is in flight.
        worker.select(0);
        assert!(worker.submit(Work::Pace { micros: 250 }));
        assert!(matches!(receive(&worker), Reply::Poll));
        assert_eq!(LIVE_QUERIES.load(Ordering::SeqCst), initial);
        assert!(worker.take().is_none());
        let shutdown = Instant::now();
        drop(worker);
        println!(
            "VISUAL_SHUTDOWN cpu_only_us={}",
            shutdown.elapsed().as_micros()
        );
        assert!(shutdown.elapsed() < Duration::from_millis(100));
    }
}
