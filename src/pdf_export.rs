/*
 * SPDX-FileCopyrightText: © 2025 Jinwoo Park (pmnxis@gmail.com)
 *
 * SPDX-License-Identifier: MIT
 */

//! Minimal PDF generator for receipts.
//! One image per A4 page with an ASCII footer line.
//! No external PDF library — pure PDF syntax written as raw bytes.

use std::io::Write;

use crate::model::CardTransaction;

/// A4 page size in PDF points (1 pt = 1/72 inch)
const A4_W: f64 = 595.276;
const A4_H: f64 = 841.890;
/// Page margin in points (~10 mm)
const MARGIN: f64 = 28.35;
/// Footer area height in points (~15 mm)
const FOOTER_H: f64 = 42.52;

/// Generate a PDF byte stream with one receipt image per A4 page.
///
/// Each page contains:
/// - The receipt image scaled to fill the available area (aspect-ratio preserved, centred)
/// - An ASCII footer: `{index}. {datetime}  {amount}  {expense_type}`
///
/// Uses the PDF built-in Helvetica font; only ASCII characters appear in the footer.
pub fn generate_receipts_pdf(transactions: &[CardTransaction]) -> Result<Vec<u8>, String> {
    if transactions.is_empty() {
        return Err("No transactions to include in PDF".into());
    }

    let n = transactions.len();

    // PDF object layout (1-indexed):
    //   1        – Catalog
    //   2        – Pages tree
    //   3        – Helvetica font resource
    //   for page i (0-based):
    //     4+3*i  – Page dictionary
    //     5+3*i  – Page content stream
    //     6+3*i  – Image XObject
    let total_objs = 3 + 3 * n;

    let mut buf: Vec<u8> = Vec::with_capacity(512 * 1024);
    let mut offsets = vec![0usize; total_objs + 1]; // 1-indexed; index 0 unused

    macro_rules! w {
        ($($arg:tt)*) => { write!(buf, $($arg)*).unwrap() }
    }

    // ── PDF header ──────────────────────────────────────────────────────────
    w!("%PDF-1.4\n");
    buf.extend_from_slice(b"%\xe2\xe3\xcf\xd3\n"); // binary marker (signals binary content)

    // ── Object 1: Catalog ───────────────────────────────────────────────────
    offsets[1] = buf.len();
    w!("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    // ── Object 2: Pages tree ────────────────────────────────────────────────
    let kids: String = (0..n)
        .map(|i| format!("{} 0 R", 4 + 3 * i))
        .collect::<Vec<_>>()
        .join(" ");
    offsets[2] = buf.len();
    w!(
        "2 0 obj\n<< /Type /Pages /Kids [{}] /Count {} >>\nendobj\n",
        kids,
        n
    );

    // ── Object 3: Helvetica font ────────────────────────────────────────────
    offsets[3] = buf.len();
    w!("3 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>\nendobj\n");

    // ── Per-page objects ────────────────────────────────────────────────────
    for (i, txn) in transactions.iter().enumerate() {
        let page_id = 4 + 3 * i;
        let content_id = 5 + 3 * i;
        let image_id = 6 + 3 * i;

        // Load image and convert to RGB JPEG for PDF embedding
        let img = image::load_from_memory(&txn.image_bytes)
            .map_err(|e| format!("Receipt #{}: failed to load image — {e}", i + 1))?;
        let rgb = img.into_rgb8();
        let (img_w, img_h) = (rgb.width(), rgb.height());

        let mut jpeg_buf: Vec<u8> = Vec::new();
        image::DynamicImage::from(rgb)
            .write_to(
                &mut std::io::Cursor::new(&mut jpeg_buf),
                image::ImageFormat::Jpeg,
            )
            .map_err(|e| format!("Receipt #{}: JPEG encode failed — {e}", i + 1))?;

        // ── Image placement: centred, aspect-ratio preserved ────────────────
        let avail_w = A4_W - 2.0 * MARGIN;
        let avail_h = A4_H - FOOTER_H - 2.0 * MARGIN;
        let aspect = img_w as f64 / img_h as f64;
        let (draw_w, draw_h) = if aspect > avail_w / avail_h {
            (avail_w, avail_w / aspect)
        } else {
            (avail_h * aspect, avail_h)
        };
        let img_x = MARGIN + (avail_w - draw_w) / 2.0;
        let img_y = FOOTER_H + MARGIN + (avail_h - draw_h) / 2.0;

        // ── Footer text (ASCII only — Helvetica has no CJK glyphs) ──────────
        let expense = txn.expense_type.as_deref().unwrap_or("-");
        let expense_ascii: String = expense
            .chars()
            .map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '?' })
            .collect();
        let footer = format!(
            "{}. {}  {}  {}",
            i + 1,
            txn.datetime.format("%Y-%m-%d %H:%M"),
            fmt_amount(txn.amount),
            expense_ascii,
        );

        // ── PDF content stream ───────────────────────────────────────────────
        // Draw image: q ... cm /ImN Do Q
        // Draw footer text: BT /F1 10 Tf x y Td (text) Tj ET
        let content = format!(
            "q\n{:.2} 0 0 {:.2} {:.2} {:.2} cm\n/Im{} Do\nQ\nBT\n/F1 10 Tf\n{:.2} {:.2} Td\n({}) Tj\nET\n",
            draw_w,
            draw_h,
            img_x,
            img_y,
            image_id,
            MARGIN,
            FOOTER_H / 2.0 - 5.0,
            pdf_str(&footer),
        );
        let content_bytes = content.as_bytes();

        // ── Page dictionary ──────────────────────────────────────────────────
        offsets[page_id] = buf.len();
        w!(
            "{} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> /XObject << /Im{} {} 0 R >> >> >>\nendobj\n",
            page_id,
            A4_W,
            A4_H,
            content_id,
            image_id,
            image_id
        );

        // ── Content stream ───────────────────────────────────────────────────
        offsets[content_id] = buf.len();
        w!(
            "{} 0 obj\n<< /Length {} >>\nstream\n",
            content_id,
            content_bytes.len()
        );
        buf.extend_from_slice(content_bytes);
        w!("\nendstream\nendobj\n");

        // ── Image XObject (DCTDecode = JPEG) ─────────────────────────────────
        offsets[image_id] = buf.len();
        w!(
            "{} 0 obj\n<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            image_id,
            img_w,
            img_h,
            jpeg_buf.len()
        );
        buf.extend_from_slice(&jpeg_buf);
        w!("\nendstream\nendobj\n");
    }

    // ── Cross-reference table ────────────────────────────────────────────────
    // Each entry is exactly 20 bytes: 10-digit offset SP 5-digit gen SP [f|n] SP LF
    let xref_pos = buf.len();
    w!("xref\n0 {}\n", total_objs + 1);
    w!("0000000000 65535 f \n"); // free object 0
    for &offset in offsets[1..=total_objs].iter() {
        w!("{:010} 00000 n \n", offset);
    }

    // ── Trailer ──────────────────────────────────────────────────────────────
    w!("trailer\n<< /Size {} /Root 1 0 R >>\n", total_objs + 1);
    w!("startxref\n{}\n%%EOF\n", xref_pos);

    Ok(buf)
}

/// Format an amount with thousands separators: 45000 → "45,000"
fn fmt_amount(amount: u64) -> String {
    let s = amount.to_string();
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut result = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && (n - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    result
}

/// Escape special characters for a PDF literal string `(...)`.
fn pdf_str(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            c if c.is_ascii() => out.push(c),
            _ => {} // skip non-ASCII (Helvetica has no CJK glyphs)
        }
    }
    out
}
