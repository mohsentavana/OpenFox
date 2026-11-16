//! Capture: full-page, viewport, or region. Full-page is a tile plan the
//! engine replays — the display list is deterministic, so scroll-and-
//! stitch is exact, not a video trick.

use crate::ShotError;

#[derive(Debug, Clone)]
pub enum CaptureRequest {
    Viewport,
    FullPage,
    Region { x: u32, y: u32, w: u32, h: u32 },
}

/// The raw frame: RGBA, top-down, no alpha premultiplication surprises —
/// pixels exactly as the compositor presented them.
pub struct CapturedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl CapturedImage {
    /// Encode a real, minimal PNG: IHDR + IDAT (stored/uncompressed
    /// deflate blocks with zlib framing) + IEND. ~70 lines that remove a
    /// dependency and keep the codec honest.
    pub fn encode_png(&self) -> Result<Vec<u8>, ShotError> {
        let mut png = Vec::new();
        png.extend_from_slice(b"\x89PNG\r\n\x1a\n");

        // IHDR: width, height, 8-bit RGBA, color type 6.
        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&self.width.to_be_bytes());
        ihdr.extend_from_slice(&self.height.to_be_bytes());
        ihdr.push(8); // bit depth
        ihdr.push(6); // RGBA
        ihdr.push(0); // compression: deflate
        ihdr.push(0); // filter: adaptive
        ihdr.push(0); // interlace: none
        push_chunk(&mut png, b"IHDR", &ihdr);

        // Raw scanlines: each row is a filter byte (0 = none) then RGBA.
        let stride = self.width as usize * 4;
        let mut raw = Vec::with_capacity((stride + 1) * self.height as usize);
        for row in 0..self.height as usize {
            raw.push(0u8);
            let start = row * stride;
            raw.extend_from_slice(&self.rgba[start..start + stride]);
        }

        // Stored deflate blocks: 65535-byte ceilings, then zlib adler32.
        let compressed = stored_zlib(&raw);
        push_chunk(&mut png, b"IDAT", &compressed);
        push_chunk(&mut png, b"IEND", &[]);
        Ok(png)
    }
}

/// The tile plan for a full-page capture: which page rects to render and
/// paste where. Overlap is zero — the display list replay is exact.
pub fn full_page_tiles(page_w: u32, page_h: u32, viewport_h: u32, dpr: u32) -> Vec<Tile> {
    let dpr = dpr.max(1);
    let effective = (viewport_h / dpr).max(1);
    let mut tiles = Vec::new();
    let mut y = 0;
    while y < page_h {
        let h = effective.min(page_h - y);
        tiles.push(Tile { x: 0, y, w: page_w, h });
        y += h;
    }
    tiles
}

#[derive(Debug, Clone, Copy)]
pub struct Tile {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Validate a region request against the page size. DPI math lives in the
/// caller; this is the geometry police.
pub fn validate_region(
    request: &CaptureRequest,
    page_w: u32,
    page_h: u32,
) -> Result<(), ShotError> {
    if let CaptureRequest::Region { w, h, .. } = request {
        if *w > page_w || *h > page_h {
            return Err(ShotError::RegionOutside {
                w: *w,
                h: *h,
                width: page_w,
                height: page_h,
            });
        }
    }
    Ok(())
}

/// Device-pixel-ratio scale math in one place: CSS px in, device px out.
pub fn scale_by_dpr(value: u32, dpr: u32) -> u32 {
    value.saturating_mul(dpr.max(1))
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(kind, data);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn stored_zlib(data: &[u8]) -> Vec<u8> {
    // zlib header: no compression, default window.
    let mut out = vec![0x78, 0x01];
    let mut offset = 0;
    while offset < data.len() {
        let take = (data.len() - offset).min(65535);
        let last = offset + take == data.len();
        out.push(if last { 1 } else { 0 });
        out.extend_from_slice(&(take as u16).to_le_bytes());
        out.extend_from_slice(&(!(take as u16)).to_le_bytes());
        out.extend_from_slice(&data[offset..offset + take]);
        offset += take;
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for byte in data {
        a = (a + *byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn crc32(kind: &[u8], data: &[u8]) -> u32 {
    // The standard PNG CRC-32 over kind + data.
    let mut table = [0u32; 256];
    for (index, entry) in table.iter_mut().enumerate() {
        let mut c = index as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        *entry = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for byte in kind.iter().chain(data.iter()) {
        crc = table[((crc ^ *byte as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}
