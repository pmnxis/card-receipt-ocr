/*
 * SPDX-FileCopyrightText: © 2025 Jinwoo Park (pmnxis@gmail.com)
 *
 * SPDX-License-Identifier: MIT
 */

//! Font loading for Korean text support
//! Pattern: chama-optics src/fonts/mod.rs + src/resources.rs

use std::sync::Arc;

/// Load Source Han Sans and configure egui fonts for Korean rendering
pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    #[cfg(target_arch = "wasm32")]
    {
        if let Some(shsans_data) = wasm_font_cache::get("SourceHanSansVF-remapped.otf") {
            log::info!(
                "Loaded Source Han Sans ({} bytes)",
                shsans_data.len()
            );
            fonts.font_data.insert(
                "Source Han Sans".to_owned(),
                Arc::new(egui::FontData::from_owned(shsans_data).weight(400)),
            );

            // Insert at the front of Proportional (default body text)
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "Source Han Sans".to_owned());

            // Also add as fallback for Monospace
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("Source Han Sans".to_owned());
        } else {
            log::warn!("Source Han Sans font not found in cache");
        }
    }

    ctx.set_fonts(fonts);
    log::info!("Fonts configured");
}

// ===== WASM Font Cache =====

#[cfg(target_arch = "wasm32")]
mod wasm_font_cache {
    use std::collections::HashMap;
    use std::sync::OnceLock;

    static FONT_CACHE: OnceLock<HashMap<String, Vec<u8>>> = OnceLock::new();

    pub fn init(fonts: HashMap<String, Vec<u8>>) {
        FONT_CACHE.set(fonts).ok();
    }

    pub fn get(name: &str) -> Option<Vec<u8>> {
        FONT_CACHE.get()?.get(name).cloned()
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(module = "/js/font_loader.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn fetch_font_bytes(url: &str) -> Result<JsValue, JsValue>;
}

/// Preload font files from server into memory cache.
/// Must be called before eframe starts.
#[cfg(target_arch = "wasm32")]
pub async fn preload_fonts() {
    let font_names = ["SourceHanSansVF-remapped.otf"];

    let mut cache = std::collections::HashMap::new();

    for name in &font_names {
        let url = format!("./Fonts/{}", name);
        match fetch_bytes(&url).await {
            Ok(bytes) => {
                log::info!("Preloaded font {} ({} bytes)", name, bytes.len());
                cache.insert(name.to_string(), bytes);
            }
            Err(e) => {
                log::warn!("Failed to preload font {}: {}", name, e);
            }
        }
    }

    log::info!(
        "Font preload complete: {}/{} fonts loaded",
        cache.len(),
        font_names.len()
    );
    wasm_font_cache::init(cache);
}

#[cfg(target_arch = "wasm32")]
async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let result = fetch_font_bytes(url)
        .await
        .map_err(|e| format!("Font fetch '{}': {:?}", url, e))?;

    let uint8_array = js_sys::Uint8Array::new(&result);
    Ok(uint8_array.to_vec())
}
