//! The glyph atlas: ab_glyph rasterization into one fixed-budget R8 image,
//! shelf-allocated and reset whole when full. The compositor uploads it
//! verbatim; cells carry texture coordinates and metrics only.

use ab_glyph::{point, Font, FontRef, Glyph, OutlinedGlyph, PxScale, ScaleFont};
use std::collections::HashMap;

pub const ATLAS_SIZE: u32 = 1024;
const ROW_PAD: u32 = 2;

/// One rasterized glyph cell in the atlas: texture coordinates plus the
/// metrics the compositor needs to place it.
#[derive(Debug, Clone, Copy)]
pub struct GlyphCell {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub width: f32,
    pub height: f32,
    pub advance: f32,
    pub ascent: f32,
}

/// One shaped character: the cell if it has ink, and the advance either way.
/// Spaces and control characters advance the pen without costing a texture.
#[derive(Debug, Clone, Copy)]
pub struct Shaped {
    pub cell: Option<GlyphCell>,
    pub advance: f32,
}

/// A fixed-budget R8 atlas with on-demand rasterization. Full reset when
/// exhausted instead of an LRU: one clear beats one eviction policy that is
/// wrong half the time. Dirty-rect uploads are a phase 2 item.
pub struct GlyphAtlas {
    font: FontRef<'static>,
    image: Vec<u8>,
    cells: HashMap<(char, u16), Shaped>,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
}

impl GlyphAtlas {
    pub fn new(font_bytes: Vec<u8>) -> Result<Self, crate::RenderError> {
        let font = FontRef::try_from_slice(font_bytes.leak())
            .map_err(|e| crate::RenderError::Font(format!("{e}")))?;
        Ok(GlyphAtlas {
            font,
            image: vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize],
            cells: HashMap::new(),
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
        })
    }

    /// Shapes a character at a pixel size, rasterizing into the atlas on
    /// first use. Sizes round to whole pixels: subpixel positions belong to
    /// the shaper phase.
    pub fn shape(&mut self, ch: char, px: f32) -> Shaped {
        let px = px.round().max(8.0).min(96.0) as u16;
        if let Some(shaped) = self.cells.get(&(ch, px)) {
            return *shaped;
        }
        let shaped = self.rasterize(ch, px as f32);
        self.cells.insert((ch, px), shaped);
        shaped
    }

    fn rasterize(&mut self, ch: char, px: f32) -> Shaped {
        let scaled = self.font.as_scaled(px);
        let gid = self.font.glyph_id(ch);
        let glyph = Glyph { id: gid, scale: PxScale::from(px), position: point(0.0, 0.0) };
        let advance = scaled.h_advance(gid);
        let Some(outlined): Option<OutlinedGlyph> = self.font.outline_glyph(glyph) else {
            return Shaped { cell: None, advance };
        };
        let bounds = outlined.px_bounds();
        let width = bounds.width().ceil().max(1.0) as u32;
        let height = bounds.height().ceil().max(1.0) as u32;

        if width + ROW_PAD >= ATLAS_SIZE || height >= ATLAS_SIZE {
            return Shaped { cell: None, advance };
        }
        if self.cursor_x + width + ROW_PAD >= ATLAS_SIZE {
            self.cursor_x = 0;
            self.cursor_y += self.row_height + ROW_PAD;
            self.row_height = 0;
        }
        if self.cursor_y + height >= ATLAS_SIZE {
            // Atlas exhausted: reset rather than evict. See the type docs.
            self.image.fill(0);
            self.cells.clear();
            self.cursor_x = 0;
            self.cursor_y = 0;
            self.row_height = 0;
            return self.rasterize(ch, px);
        }

        let cell_x = self.cursor_x;
        let cell_y = self.cursor_y;
        let origin_x = bounds.min.x.floor() as i32;
        let origin_y = bounds.min.y.floor() as i32;
        let image = &mut self.image;
        outlined.draw(move |x, y, coverage| {
            let px_x = cell_x as i32 + (x as i32 - origin_x);
            let px_y = cell_y as i32 + (y as i32 - origin_y);
            if px_x >= 0 && px_y >= 0 && (px_x as u32) < ATLAS_SIZE && (px_y as u32) < ATLAS_SIZE {
                image[(px_y * ATLAS_SIZE as i32 + px_x) as usize] =
                    (coverage * 255.0).clamp(0.0, 255.0) as u8;
            }
        });

        self.cursor_x += width + ROW_PAD;
        self.row_height = self.row_height.max(height);

        let inv = 1.0 / ATLAS_SIZE as f32;
        Shaped {
            cell: Some(GlyphCell {
                u0: cell_x as f32 * inv,
                v0: cell_y as f32 * inv,
                u1: (cell_x + width) as f32 * inv,
                v1: (cell_y + height) as f32 * inv,
                width: bounds.width(),
                height: bounds.height(),
                advance,
                ascent: scaled.ascent(),
            }),
            advance,
        }
    }

    /// The alpha image and its size for the texture upload.
    pub fn atlas_bytes(&self) -> (&[u8], u32, u32) {
        (&self.image, ATLAS_SIZE, ATLAS_SIZE)
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }
}
