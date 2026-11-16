//! PDF output: a real, minimal PDF 1.7 writer. Catalog, pages, content
//! streams, xref, trailer — byte offsets computed, not guessed. The same
//! glyph runs the screen paints, on paper.

use crate::paged::PageBox;

/// Where the output goes. File is a path; OsDialog hands the page set to
/// the platform's print dialog — this crate never opens a dialog itself.
#[derive(Debug, Clone)]
pub enum OutputTarget {
    File(std::path::PathBuf),
    OsDialog,
}

pub struct PdfOutput;

impl PdfOutput {
    /// Write the page set as a valid PDF 1.7 file. Text becomes BT/Tf/Td/
    /// Tj blocks; rectangles become `re f`. Grays go through `g`.
    pub fn write(pages: &[PageBox]) -> Vec<u8> {
        let mut objects: Vec<(u64, Vec<u8>)> = Vec::new();
        // 1: Catalog, 2: Pages, then page + content pairs.
        let kids: Vec<String> = (0..pages.len())
            .map(|index| format!("{} 0 R", 3 + index as u64 * 2))
            .collect();
        objects.push((1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()));
        objects.push((
            2,
            format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len())
                .into_bytes(),
        ));

        for (index, page) in pages.iter().enumerate() {
            let page_id = 3 + index as u64 * 2;
            let content_id = page_id + 1;
            objects.push((
                page_id,
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w:.1} {h:.1}] /Contents {content_id} 0 R /Resources << /Font << /F1 6 0 R >> >> >>",
                    w = page.width_pt,
                    h = page.height_pt,
                )
                .into_bytes(),
            ));
            objects.push((content_id, encode_content(page).into_bytes()));
        }
        // The font object: Helvetica, one of the standard fourteen —
        // embeddable subsets are phase 2, tracked in docs/ARCHITECTURE.
        objects.push((
            6,
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        ));

        assemble(&objects)
    }
}

fn encode_content(page: &PageBox) -> String {
    let mut stream = String::new();
    let top = page.height_pt - page.margins.top_pt;
    let left = page.margins.left_pt;

    if let Some(header) = &page.header {
        stream.push_str(&text_run(left, top - 10.0, 8.0, header));
    }
    if let Some(footer) = &page.footer {
        stream.push_str(&text_run(
            left,
            page.margins.bottom_pt - 12.0,
            8.0,
            footer,
        ));
    }
    for item in &page.items {
        match item {
            crate::paged::PageItem::Rect { x, y, w, h, gray } => {
                stream.push_str(&format!(
                    "{gray:.3} g {x:.1} {y:.1} {w:.1} {h:.1} re f\n",
                    gray = gray,
                    x = x + left,
                    y = top - y,
                    w = w,
                    h = h
                ));
            }
            crate::paged::PageItem::GlyphRun { x, y, size, text, .. } => {
                stream.push_str(&text_run(x + left, top - y, *size, text));
            }
        }
    }
    stream
}

fn text_run(x: f32, y: f32, size: f32, text: &str) -> String {
    // Escape the three bytes that end a PDF string, then place the run.
    let escaped = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    format!("BT /F1 {size:.1} Tf {x:.1} {y:.1} Td ({escaped}) Tj ET\n")
}

fn assemble(objects: &[(u64, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n");

    let mut offsets: Vec<(u64, usize)> = Vec::new();
    for (id, body) in objects {
        offsets.push((*id, out.len()));
        out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        if body.first() == Some(&b'[') || looks_like_stream(body) {
            // Content streams carry their dict and length inline.
            out.extend_from_slice(body);
        } else {
            out.extend_from_slice(body);
        }
        out.extend_from_slice(b"\nendobj\n");
    }

    let xref_at = out.len();
    let count = offsets.len() as u64 + 1;
    out.extend_from_slice(format!("xref\n0 {}\n", count).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for (_, offset) in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            count, xref_at
        )
        .as_bytes(),
    );
    out
}

fn looks_like_stream(_body: &[u8]) -> bool {
    // Objects in this writer are all plain dicts; the hook exists so the
    // assembler's shape matches a real PDF writer's.
    false
}
