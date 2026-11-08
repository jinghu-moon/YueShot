//! Image-space camera math. Width is the full image's physical display width,
//! so changing a comparison reference never moves the visible rectangle.
use super::ImagePreviewFit;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Camera {
    pub center: [f32; 2],
    pub width: f32,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            center: [0.5, 0.5],
            width: 1.,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Viewport {
    pub origin: [f32; 2],
    pub size: [f32; 2],
    pub scale: f32,
    pub inset: f32,
}
impl Viewport {
    pub fn is_valid(self) -> bool {
        self.size.iter().all(|v| v.is_finite() && *v > 0.)
            && self.scale.is_finite()
            && self.scale > 0.
    }
    pub fn safe_size(self) -> [f32; 2] {
        [
            (self.size[0] - self.inset * 2.).max(1.),
            (self.size[1] - self.inset * 2.).max(1.),
        ]
    }
    pub fn midpoint(self) -> [f32; 2] {
        let s = self.safe_size();
        [
            self.origin[0] + self.inset + s[0] / 2.,
            self.origin[1] + self.inset + s[1] / 2.,
        ]
    }
    pub fn fit_zoom(self, image: [f32; 2], fit: ImagePreviewFit) -> f32 {
        let s = self.safe_size();
        let x = s[0] * self.scale / image[0];
        let y = s[1] * self.scale / image[1];
        if fit == ImagePreviewFit::FitWidth {
            x.min(1.)
        } else {
            x.min(y).min(1.)
        }
    }
    pub fn zoom_limits(self, image: [f32; 2]) -> [f32; 2] {
        [
            self.fit_zoom(image, ImagePreviewFit::FitAll)
                .clamp(f32::MIN_POSITIVE, 0.05),
            8.,
        ]
    }
    pub fn fit(self, image: [f32; 2], fit: ImagePreviewFit) -> Camera {
        let z = self.fit_zoom(image, fit);
        let mut c = Camera {
            center: [0.5, 0.5],
            width: z * image[0],
        };
        if fit == ImagePreviewFit::FitWidth {
            let h = c.display_size(image, self.scale)[1];
            if h > self.safe_size()[1] {
                c.center[1] = self.safe_size()[1] / (2. * h);
            }
        }
        c
    }
}
impl Camera {
    /// Quantize only a settled, integer physical zoom. Never round fractional
    /// zoom/active gestures; doing so makes smooth panning and zooming jitter.
    pub fn pixel_aligned(self, image: [f32; 2], vp: Viewport) -> Self {
        let zoom = self.zoom(image);
        if !vp.is_valid() || zoom < 1. || (zoom - zoom.round()).abs() > 0.00001 {
            return self;
        }
        let origin = self.top_left(image, vp);
        let snapped = origin.map(|v| (v * vp.scale).round() / vp.scale);
        let size = self.display_size(image, vp.scale);
        let midpoint = vp.midpoint();
        Self {
            center: [
                (midpoint[0] - snapped[0]) / size[0],
                (midpoint[1] - snapped[1]) / size[1],
            ],
            ..self
        }
    }
    pub fn zoom(self, image: [f32; 2]) -> f32 {
        self.width / image[0]
    }
    pub fn display_size(self, image: [f32; 2], scale: f32) -> [f32; 2] {
        [self.width / scale, self.width / scale * image[1] / image[0]]
    }
    pub fn top_left(self, image: [f32; 2], vp: Viewport) -> [f32; 2] {
        let s = self.display_size(image, vp.scale);
        let m = vp.midpoint();
        [m[0] - self.center[0] * s[0], m[1] - self.center[1] * s[1]]
    }
    pub fn image_point(self, position: [f32; 2], image: [f32; 2], vp: Viewport) -> [f32; 2] {
        let o = self.top_left(image, vp);
        let s = self.display_size(image, vp.scale);
        [(position[0] - o[0]) / s[0], (position[1] - o[1]) / s[1]]
    }
    pub fn anchored(self, width: f32, anchor: [f32; 2], image: [f32; 2], vp: Viewport) -> Self {
        let q = self.image_point(anchor, image, vp);
        let mut next = Self { width, ..self };
        let s = next.display_size(image, vp.scale);
        let m = vp.midpoint();
        next.center = [
            q[0] - (anchor[0] - m[0]) / s[0],
            q[1] - (anchor[1] - m[1]) / s[1],
        ];
        next
    }
    pub fn constrained(mut self, image: [f32; 2], vp: Viewport) -> Self {
        let limits = vp.zoom_limits(image);
        self.width = self.width.clamp(limits[0] * image[0], limits[1] * image[0]);
        let s = self.display_size(image, vp.scale);
        let safe = vp.safe_size();
        for axis in 0..2 {
            // Fit is an initial view, not a pan lock. Keep a strip of the image
            // inside the safe area even after a large pointer/keyboard move.
            // A tiny image stays entirely visible; its normalized center may
            // lie outside [0, 1] when it is moved through the surrounding space.
            let visible = (vp.inset * 2.).max(1.).min(s[axis]).min(safe[axis]);
            let low = (visible - safe[axis] / 2.) / s[axis];
            self.center[axis] = self.center[axis].clamp(low, 1. - low);
        }
        self
    }
    pub fn pan(self, delta: [f32; 2], image: [f32; 2], vp: Viewport) -> Self {
        let s = self.display_size(image, vp.scale);
        Self {
            center: [
                self.center[0] - delta[0] / s[0],
                self.center[1] - delta[1] / s[1],
            ],
            ..self
        }
        .constrained(image, vp)
    }
    pub fn interpolate(self, target: Self, t: f32) -> Self {
        if t <= 0. {
            return self;
        }
        if t >= 1. {
            return target;
        }
        Self {
            center: [
                self.center[0] + (target.center[0] - self.center[0]) * t,
                self.center[1] + (target.center[1] - self.center[1]) * t,
            ],
            width: (self.width.ln() + (target.width.ln() - self.width.ln()) * t).exp(),
        }
    }
}

/// A centered, complete square grid, inset from the viewport's rounded corners.
/// The preferred size is a density target; both axes always use the same side.
#[derive(Clone, Copy, Debug)]
pub(super) struct CheckerGrid {
    pub origin: [f32; 2],
    pub side: f32,
    pub columns: usize,
    pub rows: usize,
}
impl CheckerGrid {
    pub fn new(size: [f32; 2], radius: f32, preferred: f32, scale: f32) -> Option<Self> {
        if !size.iter().all(|v| v.is_finite() && *v > 0.)
            || !scale.is_finite()
            || scale <= 0.
            || !preferred.is_finite()
            || preferred <= 0.
        {
            return None;
        }
        let inset = radius.max(0.).min(size[0].min(size[1]) / 2.);
        let available = size.map(|v| v - inset * 2.);
        if available.iter().any(|v| *v <= 0.) {
            return None;
        }
        let counts = available.map(|v| ((v / preferred).floor() as usize).clamp(1, 256));
        let side = (available[0] / counts[0] as f32).min(available[1] / counts[1] as f32);
        // Whole physical pixels keep neighboring tiles sharp at fractional DPI.
        let side = ((side * scale).floor() / scale).max((1. / scale).min(side));
        Some(Self {
            origin: [
                (size[0] - side * counts[0] as f32) / 2.,
                (size[1] - side * counts[1] as f32) / 2.,
            ],
            side,
            columns: counts[0],
            rows: counts[1],
        })
    }
}

pub(super) fn aligned(a: [f32; 2], b: [f32; 2]) -> bool {
    // At most one output pixel of aspect-ratio rounding, in either axis.
    (a[0] / a[1] * b[1] - b[0]).abs() <= 1.01 || (a[1] / a[0] * b[0] - b[1]).abs() <= 1.01
}

/// Visible source-pixel boundaries, independent of bitmap area. Each axis is
/// bounded even for extreme offscreen images; no source-sized allocation.
pub(super) struct PixelGrid {
    pub origin: [f32; 2],
    pub step: [f32; 2],
    pub clip: [f32; 4],
    pub indices: [std::ops::RangeInclusive<u32>; 2],
}
impl PixelGrid {
    pub fn new(camera: Camera, image: [f32; 2], vp: Viewport) -> Option<Self> {
        if !vp.is_valid() || camera.zoom(image) < 4. {
            return None;
        }
        let origin = camera.top_left(image, vp);
        let size = camera.display_size(image, vp.scale);
        let step = [size[0] / image[0], size[1] / image[1]];
        let clip = [
            origin[0].max(vp.origin[0]),
            origin[1].max(vp.origin[1]),
            (origin[0] + size[0]).min(vp.origin[0] + vp.size[0]),
            (origin[1] + size[1]).min(vp.origin[1] + vp.size[1]),
        ];
        if clip[2] <= clip[0] || clip[3] <= clip[1] {
            return None;
        }
        let indices = std::array::from_fn(|axis| {
            let first = ((clip[axis] - origin[axis]) / step[axis]).ceil().max(1.) as u32;
            let last = ((clip[axis + 2] - origin[axis]) / step[axis])
                .floor()
                .min(image[axis] - 1.)
                .max(0.) as u32;
            first..=last
        });
        if indices.iter().any(|range: &std::ops::RangeInclusive<u32>| {
            !range.is_empty() && range.end() - range.start() >= 4096
        }) {
            return None;
        }
        Some(Self {
            origin,
            step,
            clip,
            indices,
        })
    }
}
pub(super) fn snap_split(v: f32) -> f32 {
    if v < 0.012 {
        0.
    } else if v > 0.988 {
        1.
    } else {
        v
    }
}
