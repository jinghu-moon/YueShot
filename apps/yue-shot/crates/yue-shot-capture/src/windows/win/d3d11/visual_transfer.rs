//! Bounded atlas readback. Only its graphics owner may call context methods.
use super::*;
use crate::geometry::{Point, Rect};
use crate::visual_detection::allocation;

const ATLAS: u32 = 1024;
const COPY_SLICE: usize = 64;
#[cfg(test)]
pub(crate) static LIVE_TRANSFERS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
pub(crate) type OwnedRegions = Vec<(Rect, Vec<u8>)>;

struct Piece {
    source: Rect,
    destination: Point,
    output: usize,
}

struct Batch {
    source: ID3D11Texture2D,
    regions: Vec<Rect>,
    pieces: Vec<Piece>,
    submitted: usize,
    cancelled: bool,
}

pub(crate) struct BatchReadback {
    device: GraphicsDevice,
    staging: ID3D11Texture2D,
    batch: Option<Batch>,
    #[cfg(test)]
    pub(crate) injected_map: Option<i32>,
    #[cfg(test)]
    pub(crate) poll_count: u64,
}

impl BatchReadback {
    pub fn new(device: &GraphicsDevice) -> Result<Self, String> {
        if !allocation::permit("staging_atlas") {
            return Err("visual atlas allocation failed".into());
        }
        let staging = device.create_staging_texture_for_format(
            ATLAS,
            ATLAS,
            DXGI_FORMAT_B8G8R8A8_UNORM,
            "visual atlas",
        )?;
        #[cfg(test)]
        LIVE_TRANSFERS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Self {
            device: device.clone(),
            staging,
            batch: None,
            #[cfg(test)]
            injected_map: None,
            #[cfg(test)]
            poll_count: 0,
        })
    }

    pub fn begin(&mut self, source: &ID3D11Texture2D, regions: Vec<Rect>) -> Result<(), String> {
        if self.batch.is_some() {
            return Err("visual transfer is already pending".into());
        }
        let mut description = D3D11_TEXTURE2D_DESC::default();
        unsafe { source.GetDesc(&mut description) };
        let bounds = Rect::new(0, 0, description.Width as i32, description.Height as i32);
        if description.Width > 16384
            || description.Height > 16384
            || description.Format != DXGI_FORMAT_B8G8R8A8_UNORM
            || description.SampleDesc.Count != 1
            || regions.is_empty()
            || regions.len() > 16384
            || regions
                .iter()
                .any(|r| r.is_empty() || !bounds.contains_rect(*r))
        {
            return Err("invalid visual batch source or regions".into());
        }
        let bytes = regions
            .iter()
            .try_fold(0usize, |total, r| total.checked_add(r.area() as usize * 4))
            .ok_or("visual batch byte overflow")?;
        if bytes > 1_048_576 {
            return Err("visual batch exceeds 1 MiB".into());
        }
        let mut pieces = Vec::new();
        for (output, region) in regions.iter().enumerate() {
            for y in (region.top..region.bottom).step_by(256) {
                for x in (region.left..region.right).step_by(ATLAS as usize) {
                    if !allocation::reserve(&mut pieces, 1, "atlas_pieces") {
                        return Err("visual piece allocation failed".into());
                    }
                    pieces.push(Piece {
                        source: Rect::new(
                            x,
                            y,
                            (x + ATLAS as i32).min(region.right),
                            (y + 256).min(region.bottom),
                        ),
                        destination: Point::new(0, 0),
                        output,
                    });
                }
            }
        }
        // Tall strips first. Shelf padding is GPU allocation, not transferred
        // evidence: only copied subrectangles may be read from the mapping.
        pieces.sort_unstable_by_key(|p| {
            (
                std::cmp::Reverse(p.source.height()),
                std::cmp::Reverse(p.source.width()),
                p.output,
                p.source.top,
                p.source.left,
            )
        });
        let (mut x, mut y, mut shelf) = (0i32, 0i32, 0i32);
        for piece in &mut pieces {
            if x + piece.source.width() > ATLAS as i32 {
                y += shelf;
                x = 0;
                shelf = 0;
            }
            if y + piece.source.height() > ATLAS as i32 {
                return Err("visual atlas packing capacity exceeded".into());
            }
            piece.destination = Point::new(x, y);
            x += piece.source.width();
            shelf = shelf.max(piece.source.height());
        }
        self.batch = Some(Batch {
            source: source.clone(),
            regions,
            pieces,
            submitted: 0,
            cancelled: false,
        });
        Ok(())
    }

    /// At most 64 bounded copy commands per owner event. Callers yield to render
    /// between slices. The batch is flushed once, after the final slice.
    pub fn submit_slice(&mut self) -> Result<bool, String> {
        let batch = self.batch.as_mut().ok_or("no visual batch")?;
        if batch.submitted == batch.pieces.len() {
            return Ok(true);
        }
        let end = (batch.submitted + COPY_SLICE).min(batch.pieces.len());
        unsafe {
            let context = self.device.owned_context("visual batch submit");
            for piece in &batch.pieces[batch.submitted..end] {
                let r = piece.source;
                let source_box = D3D11_BOX {
                    left: r.left as u32,
                    top: r.top as u32,
                    front: 0,
                    right: r.right as u32,
                    bottom: r.bottom as u32,
                    back: 1,
                };
                context.CopySubresourceRegion(
                    &self.staging,
                    0,
                    piece.destination.x as u32,
                    piece.destination.y as u32,
                    0,
                    &batch.source,
                    0,
                    Some(&source_box),
                );
            }
            batch.submitted = end;
            if end == batch.pieces.len() {
                context.Flush();
            }
        }
        Ok(end == batch.pieces.len())
    }

    pub fn cancel(&mut self) {
        if let Some(batch) = self.batch.as_mut() {
            batch.cancelled = true;
            // Only already-issued commands need draining. No cancelled slice is submitted.
            if batch.submitted == 0 {
                self.batch = None;
            } else if batch.submitted < batch.pieces.len() {
                batch.pieces.truncate(batch.submitted);
                unsafe { self.device.owned_context("cancel visual batch").Flush() };
            }
        }
    }

    pub fn pending(&self) -> bool {
        self.batch.is_some()
    }

    pub fn poll(&mut self) -> Option<Result<OwnedRegions, String>> {
        #[cfg(test)]
        {
            self.poll_count += 1;
        }
        let batch = self.batch.as_ref()?;
        if batch.submitted != batch.pieces.len() {
            return None;
        }
        #[cfg(test)]
        if let Some(code) = self.injected_map {
            return self.complete_mapping(
                Err(::windows::core::Error::from_hresult(
                    ::windows::core::HRESULT(code),
                )),
                D3D11_MAPPED_SUBRESOURCE::default(),
            );
        }
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        let result = unsafe {
            self.device.owned_context("visual batch poll").Map(
                &self.staging,
                0,
                D3D11_MAP_READ,
                D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32,
                Some(&mut mapped),
            )
        };
        self.complete_mapping(result, mapped)
    }

    // Keep the actual Map outcome handling in one place so fault injection
    // exercises the branch used by the real driver, without removing a device.
    fn complete_mapping(
        &mut self,
        result: ::windows::core::Result<()>,
        mapped: D3D11_MAPPED_SUBRESOURCE,
    ) -> Option<Result<OwnedRegions, String>> {
        self.batch.as_ref()?;
        if let Err(error) = result {
            if error.code().0 == 1 || error.code().0 == DXGI_ERROR_WAS_STILL_DRAWING.0 {
                return None;
            }
            self.batch = None;
            return Some(Err(super::super::hresult("Map(visual atlas)", &error)));
        }
        let batch = self.batch.take().unwrap();
        let output = (|| {
            if batch.cancelled {
                return Err("visual transfer cancelled".into());
            }
            if mapped.pData.is_null() || mapped.RowPitch < ATLAS * 4 {
                return Err("invalid visual atlas mapping".into());
            }
            let mut output = Vec::new();
            if !allocation::reserve(&mut output, batch.regions.len(), "readback_regions") {
                return Err("visual region allocation failed".into());
            }
            for &r in &batch.regions {
                let mut bytes = Vec::new();
                if !allocation::reserve(&mut bytes, r.area() as usize * 4, "readback_pixels") {
                    return Err("visual result allocation failed".into());
                }
                bytes.resize(r.area() as usize * 4, 0);
                output.push((r, bytes));
            }
            for piece in &batch.pieces {
                let (region, data) = &mut output[piece.output];
                for row in 0..piece.source.height() as usize {
                    let from = (piece.destination.y as usize + row) * mapped.RowPitch as usize
                        + piece.destination.x as usize * 4;
                    let to = ((piece.source.top - region.top) as usize + row)
                        * region.width() as usize
                        * 4
                        + (piece.source.left - region.left) as usize * 4;
                    let length = piece.source.width() as usize * 4;
                    unsafe {
                        data[to..to + length].copy_from_slice(std::slice::from_raw_parts(
                            (mapped.pData as *const u8).add(from),
                            length,
                        ))
                    };
                }
            }
            Ok(output)
        })();
        unsafe {
            self.device
                .owned_context("visual batch unmap")
                .Unmap(&self.staging, 0)
        };
        Some(output)
    }
}

#[cfg(test)]
impl Drop for BatchReadback {
    fn drop(&mut self) {
        LIVE_TRANSFERS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
pub(crate) mod tests;
