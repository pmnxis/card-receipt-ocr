/*
 * SPDX-FileCopyrightText: © 2025 Jinwoo Park (pmnxis@gmail.com)
 *
 * SPDX-License-Identifier: MIT
 */

//! Browser download helper for WASM.
//! Pattern: chama-optics src/util/web_download.rs

use wasm_bindgen::JsCast;

/// Trigger a browser file download from raw bytes.
pub fn download_file(filename: &str, data: &[u8], mime_type: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or("No window object available")?;
    let document = window.document().ok_or("No document available")?;
    let body = document.body().ok_or("No body element available")?;

    let array = js_sys::Uint8Array::from(data);
    let blob_parts = js_sys::Array::new();
    blob_parts.push(&array.buffer());

    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime_type);

    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&blob_parts, &options)
        .map_err(|e| format!("Failed to create Blob: {:?}", e))?;

    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|e| format!("Failed to create object URL: {:?}", e))?;

    let anchor: web_sys::HtmlAnchorElement = document
        .create_element("a")
        .map_err(|e| format!("Failed to create anchor element: {:?}", e))?
        .dyn_into()
        .map_err(|_| "Created element is not an anchor".to_string())?;

    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.style().set_property("display", "none").ok();

    body.append_child(&anchor).ok();
    anchor.click();
    body.remove_child(&anchor).ok();

    // Revoke after a short delay to ensure download starts
    let url_clone = url.clone();
    let closure = wasm_bindgen::closure::Closure::once(move || {
        web_sys::Url::revoke_object_url(&url_clone).ok();
    });
    window
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            5000,
        )
        .ok();
    closure.forget();

    Ok(())
}

/// Download CSV content with UTF-8 BOM for Excel compatibility
pub fn download_csv(filename: &str, csv_content: &str) -> Result<(), String> {
    download_file(filename, csv_content.as_bytes(), "text/csv;charset=utf-8;")
}

/// Bundle images (numbered), CSV, and PDF into a single ZIP archive and trigger download.
///
/// - Images are renamed to their 1-based index with the original extension (`1.jpg`, `2.png`, …)
/// - CSV is stored as `카드사용내역.csv`
/// - PDF is stored as `영수증모음.pdf`
pub fn download_receipt_bundle(
    images: &[(&str, &[u8])], // (original_filename, bytes)
    csv_bytes: &[u8],
    pdf_bytes: &[u8],
    zip_filename: &str,
) -> Result<(), String> {
    use std::io::Write;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    // Images are already compressed (JPEG/PNG) — store without re-compression.
    let store = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    // CSV and PDF benefit from deflate compression.
    let deflate = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let mut buf: Vec<u8> = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut zip = ZipWriter::new(cursor);

        // Numbered receipt images
        for (i, (original_name, bytes)) in images.iter().enumerate() {
            if bytes.is_empty() {
                continue;
            }
            let ext = std::path::Path::new(original_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("jpg")
                .to_ascii_lowercase();
            let entry_name = format!("{}.{}", i + 1, ext);
            zip.start_file(&entry_name, store)
                .map_err(|e| format!("ZIP: start_file error: {e}"))?;
            zip.write_all(bytes)
                .map_err(|e| format!("ZIP: write error: {e}"))?;
        }

        // CSV
        if !csv_bytes.is_empty() {
            zip.start_file("카드사용내역.csv", deflate)
                .map_err(|e| format!("ZIP: CSV start_file error: {e}"))?;
            zip.write_all(csv_bytes)
                .map_err(|e| format!("ZIP: CSV write error: {e}"))?;
        }

        // PDF
        if !pdf_bytes.is_empty() {
            zip.start_file("영수증모음.pdf", deflate)
                .map_err(|e| format!("ZIP: PDF start_file error: {e}"))?;
            zip.write_all(pdf_bytes)
                .map_err(|e| format!("ZIP: PDF write error: {e}"))?;
        }

        zip.finish()
            .map_err(|e| format!("ZIP: finish error: {e}"))?;
    }

    download_file(zip_filename, &buf, "application/zip")
}
