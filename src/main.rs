/*
 * SPDX-FileCopyrightText: © 2025 Jinwoo Park (pmnxis@gmail.com)
 *
 * SPDX-License-Identifier: MIT
 */

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod expense;
mod fonts;
mod model;
mod parser;
mod table;

#[cfg(target_arch = "wasm32")]
mod ocr;
#[cfg(target_arch = "wasm32")]
mod web_download;

use app::CardReceiptApp;

// Desktop entry point
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    env_logger::init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "카드 영수증 OCR",
        native_options,
        Box::new(|cc| Ok(Box::new(CardReceiptApp::new(cc)))),
    )
}

// WASM entry point (pattern: chama-optics main.rs)
#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast;

    console_error_panic_hook::set_once();
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        // Preload fonts before starting eframe
        crate::fonts::preload_fonts().await;

        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("Failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("the_canvas_id is not a HtmlCanvasElement");

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| {
                    let app = CardReceiptApp::new(cc);
                    // Setup fonts after egui context is available
                    crate::fonts::setup_fonts(&cc.egui_ctx);
                    Ok(Box::new(app))
                }),
            )
            .await;

        // Remove loading text
        if let Some(loading_text) = document.get_element_by_id("loading_text") {
            if let Some(parent) = loading_text.parent_node() {
                parent.remove_child(&loading_text).ok();
            }
        }

        if let Err(e) = start_result {
            log::error!("Failed to start eframe: {:?}", e);
            panic!("Failed to start eframe: {:?}", e);
        }
    });
}
