/*
 * SPDX-FileCopyrightText: © 2025 Jinwoo Park (pmnxis@gmail.com)
 *
 * SPDX-License-Identifier: MIT
 */

//! Tesseract.js interop via wasm-bindgen
//! Pattern: chama-optics js/heif_helper.js + image/heic_web.rs

use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/js/ocr_bridge.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn ocr_recognize(image_bytes: &[u8]) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch)]
    async fn open_file_picker(queue_callback: JsValue) -> Result<JsValue, JsValue>;

    fn download_file(data: &[u8], filename: &str, mime_type: &str);
}

/// Perform OCR on image bytes, returns recognized text
pub async fn recognize_text(image_bytes: &[u8]) -> Result<String, String> {
    let result = ocr_recognize(image_bytes)
        .await
        .map_err(|e| format!("OCR error: {:?}", e))?;
    result
        .as_string()
        .ok_or_else(|| "OCR returned non-string result".into())
}

/// Open file picker and return vec of (filename, bytes)
pub async fn pick_files() -> Result<Vec<(String, Vec<u8>)>, String> {
    let result = open_file_picker(JsValue::NULL)
        .await
        .map_err(|e| format!("File picker error: {:?}", e))?;

    let array: js_sys::Array = result
        .dyn_into()
        .map_err(|_| "Expected array from file picker".to_string())?;

    let mut files = Vec::new();
    for i in 0..array.length() {
        let obj = array.get(i);
        let name = js_sys::Reflect::get(&obj, &"name".into())
            .map_err(|_| "Missing name field".to_string())?
            .as_string()
            .unwrap_or_default();
        let bytes_js = js_sys::Reflect::get(&obj, &"bytes".into())
            .map_err(|_| "Missing bytes field".to_string())?;
        let uint8: js_sys::Uint8Array = bytes_js
            .dyn_into()
            .map_err(|_| "Expected Uint8Array for bytes".to_string())?;
        files.push((name, uint8.to_vec()));
    }
    Ok(files)
}

