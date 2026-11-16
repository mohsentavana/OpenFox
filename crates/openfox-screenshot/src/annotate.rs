//! Annotation: crop, highlight, text, arrow, pixelate. All of it is
//! display-list ops — the same primitives the page paints with, which is
//! the entire point.

use crate::capture::CapturedImage;

#[derive(Debug, Clone)]
pub enum Annotation {
    Crop { x: u32, y: u32, w: u32, h: u32 },
    Highlight { x: u32, y: u32, w: u32, h: u32, color: [u8; 3] },
    Text { x: u32, y: u32, size: u16, color: [u8; 3], content: String },
    Arrow { from: (u32, u32), to: (u32, u32), thickness: u16, color: [u8; 3] },
    /// Deterministic block averaging. Pixelate, not blur — blur is
    /// reversible-ish and screenshots deserve better than "ish".
    Pixelate { x: u32, y: u32, w: u32, h: u32, block: u8 },
}

pub struct AnnotatedImage {
    base: CapturedImage,
    layers: Vec<Annotation>,
}

impl AnnotatedImage {
    pub fn new(base: CapturedImage) -> Self {
        AnnotatedImage {
            base,
            layers: Vec::new(),
        }
    }

    pub fn add(&mut self, annotation: Annotation) {
        self.layers.push(annotation);
    }

    pub fn layers(&self) -> &[Annotation] {
        &self.layers
    }

    /// Bake layers into pixels. Crop applies first (it defines the
    /// canvas), then the paint order the user chose.
    pub fn bake(mut self) -> CapturedImage {
        let crop = self
            .layers
            .iter()
            .find_map(|layer| match layer {
                Annotation::Crop { x, y, w, h } => Some((*x, *y, *w, *h)),
                _ => None,
            })
            .unwrap_or((0, 0, self.base.width, self.base.height));

        let (cx, cy, mut cw, mut ch) = crop;
        cw = cw.min(self.base.width.saturating_sub(cx)).max(1);
        ch = ch.min(self.base.height.saturating_sub(cy)).max(1);
        let mut rgba = crop_pixels(&self.base.rgba, self.base.width, cx, cy, cw, ch);

        self.layers.retain(|layer| !matches!(layer, Annotation::Crop { .. }));
        for layer in &self.layers {
            if let Annotation::Pixelate { x, y, w, h, block } = layer {
                pixelate(&mut rgba, cw, ch, x.saturating_sub(cx), y.saturating_sub(cy), *w, *h, *block);
            }
            // Highlight, Text and Arrow are paint ops: they route through
            // the compositor's display list, not pixel math.
        }

        CapturedImage {
            width: cw,
            height: ch,
            rgba,
        }
    }
}

fn crop_pixels(
    source: &[u8],
    source_w: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Vec<u8> {
    let mut out = Vec::with_capacity((w as usize) * (h as usize) * 4);
    let stride = source_w as usize * 4;
    for row in y..y + h {
        let start = row as usize * stride + x as usize * 4;
        let end = start + w as usize * 4;
        if end <= source.len() {
            out.extend_from_slice(&source[start..end]);
        }
    }
    out
}

fn pixelate(
    rgba: &mut [u8],
    width: u32,
    _height: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    block: u8,
) {
    let block = block.max(2) as u32;
    let stride = width as usize * 4;
    let x = x.min(width);
    let y_end = y.saturating_add(h);
    let x_end = x.saturating_add(w).min(width);
    let mut block_y = y;
    while block_y < y_end {
        let mut block_x = x;
        while block_x < x_end {
            // Average one block, then write it back — deterministic, no
            // noise, no crypto pretenses.
            let mut sum = [0u32; 4];
            let mut count = 0u32;
            let by_end = (block_y + block).min(y_end);
            let bx_end = (block_x + block).min(x_end);
            for row in block_y..by_end {
                for col in block_x..bx_end {
                    let offset = row as usize * stride + col as usize * 4;
                    if offset + 3 < rgba.len() {
                        for channel in 0..4 {
                            sum[channel] += rgba[offset + channel] as u32;
                        }
                        count += 1;
                    }
                }
            }
            if count > 0 {
                let average: Vec<u8> =
                    sum.iter().map(|total| (total / count) as u8).collect();
                for row in block_y..by_end {
                    for col in block_x..bx_end {
                        let offset = row as usize * stride + col as usize * 4;
                        if offset + 3 < rgba.len() {
                            rgba[offset..offset + 4].copy_from_slice(&average);
                        }
                    }
                }
            }
            block_x += block;
        }
        block_y += block;
    }
}
