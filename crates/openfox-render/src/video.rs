//! Video frame import: platform surfaces in, compositor quads out, paced
//! at media fps with a drop-late policy.

/// Texture formats the import path understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TexFormat {
    Nv12,
    Bgra8,
}

/// A decoded frame's backing, whatever the platform decoder produced.
/// GPU handles stay on the thread that imported them — these are draw-time
/// resources, not messages.
pub enum PlatformSurface {
    D3D11Texture { ptr: *mut std::ffi::c_void, fmt: TexFormat },
    MediaCodecSurface { ptr: *mut std::ffi::c_void },
    CpuPlane { w: u32, h: u32, y: Vec<u8>, uv: Vec<u8> },
}

/// BT.601 limited-range YUV -> RGB, the web-video default. Rows are the
/// R, G and B output coefficients over (Y', Cb-128, Cr-128); the Y' row
/// scalar includes the limited-range expansion from the 16..235 window.
pub const BT601_LIMITED: [[f32; 3]; 3] = [
    [1.164383, 0.000000, 1.596027],   // R
    [1.164383, -0.391762, -0.812968], // G
    [1.164383, 2.017232, 0.000000],   // B
];

/// The limited-range luma floor subtracted before the matrix applies.
pub const YUV_HEADROOM: f32 = 16.0;

/// One YCbCr pixel to RGB via BT.601 limited range.
pub fn yuv_to_rgb(y: u8, u: u8, v: u8) -> [u8; 3] {
    let y = y as f32 - YUV_HEADROOM;
    let cb = u as f32 - 128.0;
    let cr = v as f32 - 128.0;
    let mut out = [0u8; 3];
    for (channel, row) in BT601_LIMITED.iter().enumerate() {
        out[channel] = (row[0] * y + row[1] * cb + row[2] * cr).round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// Presents at media fps and drops frames that missed their slot — audio
/// cadence outranks video smoothness, and a queued late frame would only
/// delay every frame behind it.
pub struct FramePacer {
    frame_ms: u64,
    next_present_ms: u64,
}

impl FramePacer {
    pub fn new(fps: u32) -> FramePacer {
        FramePacer { frame_ms: 1000 / fps.max(1) as u64, next_present_ms: 0 }
    }

    /// Decides for a frame arriving at `now_ms`: true presents, false holds
    /// (early) or drops (late by more than a full interval, after which the
    /// slot resynchronizes to the stream).
    pub fn on_frame(&mut self, now_ms: u64) -> bool {
        if now_ms < self.next_present_ms {
            return false; // early: wait for the slot
        }
        if now_ms > self.next_present_ms + self.frame_ms {
            self.next_present_ms = now_ms + self.frame_ms;
            return false; // late beyond recovery: drop and resync
        }
        self.next_present_ms += self.frame_ms;
        true
    }
}

/// No DRM. EME demands proprietary CDM black boxes and a license server
/// that decides who may watch what; OpenFox plays clear media and leaves
/// the catalog wars to browsers that want to fight them. Freedom over
/// catalog is the whole trade, stated once, here.
pub const DRM_SUPPORTED: bool = false;

/// The frame path for one media element: imports surfaces, paces them, and
/// converts CPU planes when the compositor needs RGBA.
pub struct VideoFrames {
    pacer: FramePacer,
    pending: Option<PlatformSurface>,
}

impl VideoFrames {
    pub fn new(fps: u32) -> VideoFrames {
        VideoFrames { pacer: FramePacer::new(fps), pending: None }
    }

    /// Offers a decoded frame; true when it should draw this tick. Frames
    /// that lose the pacing decision are dropped on the floor — a held GPU
    /// frame is a leak waiting to happen.
    pub fn import(&mut self, surface: PlatformSurface, now_ms: u64) -> bool {
        if self.pacer.on_frame(now_ms) {
            self.pending = Some(surface);
            true
        } else {
            false
        }
    }

    /// Takes the frame due for presentation, GPU handle or CPU planes.
    pub fn take(&mut self) -> Option<PlatformSurface> {
        self.pending.take()
    }

    /// CPU planes to RGBA8 for the software path. NV12 packs Cb and Cr
    /// interleaved in `uv`, half resolution in each dimension.
    pub fn to_rgba(surface: &PlatformSurface) -> Option<Vec<u8>> {
        let PlatformSurface::CpuPlane { w, h, y, uv } = surface else {
            return None; // GPU-backed frames never cross the CPU
        };
        let w = *w as usize;
        let h = *h as usize;
        let mut out = Vec::with_capacity(w * h * 4);
        for row in 0..h {
            for col in 0..w {
                let y_px = y[row * w + col];
                let uv_index = (row / 2) * w + (col & !1);
                let (cb, cr) = if uv_index + 1 < uv.len() { (uv[uv_index], uv[uv_index + 1]) } else { (128, 128) };
                let [r, g, b] = yuv_to_rgb(y_px, cb, cr);
                out.extend_from_slice(&[r, g, b, 255]);
            }
        }
        Some(out)
    }
}
