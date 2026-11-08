//! Optional diagnostics. No queries or clocks are created in the default path.
//! Live reads use DONOTFLUSH and skip busy slots; only the offscreen benchmark
//! explicitly drains the GPU. Timing is diagnostic, never an animation driver.
use super::*;
use std::time::Instant;

pub(super) const STAGES: [&str; 5] = ["copy", "downsample", "horizontal", "vertical", "composite"];

struct Slot {
    stamps: [ID3D11Query; 6],
    frame: Option<usize>,
    valid: bool,
    started: Instant,
    last: Instant,
    sample: Sample,
}

struct Frame {
    disjoint: ID3D11Query,
    pending: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Sample {
    pub(super) serial: usize,
    pub(super) bounds: [f32; 4],
    // CPU prepare, then command submission for the five GPU stages.
    pub(super) cpu_us: [f64; 6],
    pub(super) gpu_us: [f64; 5],
    pub(super) cpu_total_us: f64,
}

pub(super) struct Profiler {
    slots: Vec<Slot>,
    frames: Vec<Frame>,
    frame_open: bool,
    active_frame: Option<usize>,
    current: Option<usize>,
    serial: usize,
    stride: usize,
    log: bool,
    #[cfg(test)]
    pub(super) completed: Vec<Sample>,
}

pub(super) fn query(device: &ID3D11Device, kind: D3D11_QUERY) -> Result<ID3D11Query> {
    let mut result = None;
    unsafe {
        device.CreateQuery(
            &D3D11_QUERY_DESC {
                Query: kind,
                MiscFlags: 0,
            },
            Some(&mut result),
        )?;
    }
    result.context("creating backdrop timing query")
}

/// The generated windows-rs GetData wrapper turns S_FALSE into Ok(()). Keep
/// the raw HRESULT here: reading pending/uninitialized timestamps is invalid.
pub(super) fn ready<T: Default>(
    context: &ID3D11DeviceContext,
    query: &ID3D11Query,
) -> Result<Option<T>> {
    let mut value = T::default();
    let result = unsafe {
        (context.vtable().GetData)(
            context.as_raw(),
            query.as_raw(),
            (&mut value as *mut T).cast(),
            std::mem::size_of::<T>() as u32,
            D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32,
        )
    };
    result.ok()?;
    Ok((result.0 == 0).then_some(value))
}

impl Profiler {
    pub(super) fn from_env(device: &ID3D11Device) -> Option<Self> {
        let stride = std::env::var("GPUI_BACKDROP_PROFILE")
            .ok()?
            .parse::<usize>()
            .ok()?;
        if stride == 0 {
            return None;
        }
        match Self::new(device, stride, 128, true) {
            Ok(profile) => Some(profile),
            Err(error) => {
                log::warn!("Backdrop timing unavailable: {error:#}");
                None
            }
        }
    }

    pub(super) fn new(
        device: &ID3D11Device,
        stride: usize,
        capacity: usize,
        log: bool,
    ) -> Result<Self> {
        let mut slots = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            let stamps = (0..6)
                .map(|_| query(device, D3D11_QUERY_TIMESTAMP))
                .collect::<Result<Vec<_>>>()?;
            slots.push(Slot {
                stamps: stamps.try_into().ok().context("six timestamp queries")?,
                frame: None,
                valid: false,
                started: Instant::now(),
                last: Instant::now(),
                sample: Sample::default(),
            });
        }
        let frames = (0..8)
            .map(|_| {
                Ok(Frame {
                    disjoint: query(device, D3D11_QUERY_TIMESTAMP_DISJOINT)?,
                    pending: false,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            slots,
            frames,
            frame_open: false,
            active_frame: None,
            current: None,
            serial: 0,
            stride: stride.max(1),
            log,
            #[cfg(test)]
            completed: Vec::new(),
        })
    }

    pub(super) fn begin(
        &mut self,
        context: &ID3D11DeviceContext,
        bounds: [f32; 4],
        prepare_us: f64,
    ) {
        if !self.frame_open {
            if let Err(error) = self.poll(context) {
                log::warn!("Backdrop timing read failed: {error:#}");
            }
            self.frame_open = true;
        }
        let serial = self.serial;
        self.serial += 1;
        if !serial.is_multiple_of(self.stride) {
            return;
        }
        let Some(index) = self.slots.iter().position(|slot| slot.frame.is_none()) else {
            return;
        };
        let frame = if let Some(frame) = self.active_frame {
            frame
        } else {
            let Some(frame) = self.frames.iter().position(|frame| !frame.pending) else {
                return;
            };
            // D3D11 specifies one disjoint interval per frame, shared by all
            // sampled regions. Do not begin/end a disjoint query per region.
            unsafe {
                context.Begin(&self.frames[frame].disjoint);
            }
            self.active_frame = Some(frame);
            frame
        };
        let slot = &mut self.slots[index];
        slot.frame = Some(frame);
        slot.valid = false;
        slot.sample = Sample {
            serial,
            bounds,
            ..Default::default()
        };
        slot.sample.cpu_us[0] = prepare_us;
        slot.started = Instant::now();
        slot.last = slot.started;
        unsafe {
            context.End(&slot.stamps[0]);
        }
        self.current = Some(index);
    }

    pub(super) fn mark(&mut self, context: &ID3D11DeviceContext, stage: usize) {
        if let Some(index) = self.current {
            let slot = &mut self.slots[index];
            let now = Instant::now();
            slot.sample.cpu_us[stage] = (now - slot.last).as_secs_f64() * 1e6;
            slot.last = now;
            unsafe {
                context.End(&slot.stamps[stage]);
            }
        }
    }

    pub(super) fn end(&mut self, valid: bool) {
        if let Some(index) = self.current.take() {
            let slot = &mut self.slots[index];
            slot.sample.cpu_total_us =
                slot.started.elapsed().as_secs_f64() * 1e6 + slot.sample.cpu_us[0];
            slot.valid = valid;
        }
    }

    pub(super) fn end_frame(&mut self, context: &ID3D11DeviceContext) {
        if let Some(frame) = self.active_frame.take() {
            unsafe {
                context.End(&self.frames[frame].disjoint);
            }
            self.frames[frame].pending = true;
        }
        self.frame_open = false;
    }

    pub(super) fn poll(&mut self, context: &ID3D11DeviceContext) -> Result<()> {
        for (frame_ix, frame) in self.frames.iter_mut().enumerate() {
            if !frame.pending {
                continue;
            }
            let Some(disjoint) =
                ready::<D3D11_QUERY_DATA_TIMESTAMP_DISJOINT>(context, &frame.disjoint)?
            else {
                continue;
            };
            let invalid = disjoint.Disjoint.as_bool() || disjoint.Frequency == 0;
            let mut frame_complete = true;
            for slot in self
                .slots
                .iter_mut()
                .filter(|slot| slot.frame == Some(frame_ix))
            {
                if invalid || !slot.valid {
                    slot.frame = None;
                    continue;
                }
                let mut stamps = [0u64; 6];
                let mut complete = true;
                for (value, query) in stamps.iter_mut().zip(&slot.stamps) {
                    let Some(stamp) = ready::<u64>(context, query)? else {
                        complete = false;
                        break;
                    };
                    *value = stamp;
                }
                if !complete {
                    frame_complete = false;
                    continue;
                }
                slot.frame = None;
                for i in 0..5 {
                    slot.sample.gpu_us[i] = stamps[i + 1].saturating_sub(stamps[i]) as f64 * 1e6
                        / disjoint.Frequency as f64;
                }
                if self.log {
                    log::info!(
                        "Backdrop profile sample={} bounds={:?} CPU_us=[prepare,copy,downsample,horizontal,vertical,composite]{:?} GPU_stages={:?} GPU_us={:?} CPU_total_us={:.3}",
                        slot.sample.serial,
                        slot.sample.bounds,
                        slot.sample.cpu_us,
                        STAGES,
                        slot.sample.gpu_us,
                        slot.sample.cpu_total_us
                    );
                }
                #[cfg(test)]
                self.completed.push(slot.sample.clone());
            }
            if frame_complete {
                frame.pending = false;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn drain(&mut self, context: &ID3D11DeviceContext) -> Result<()> {
        unsafe {
            context.Flush();
        }
        let start = Instant::now();
        while self.frames.iter().any(|s| s.pending) {
            self.poll(context)?;
            anyhow::ensure!(start.elapsed().as_secs() < 10, "GPU query timeout");
            std::thread::yield_now();
        }
        Ok(())
    }
}
