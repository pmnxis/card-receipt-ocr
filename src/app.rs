/*
 * SPDX-FileCopyrightText: © 2025 Jinwoo Park (pmnxis@gmail.com)
 *
 * SPDX-License-Identifier: MIT
 */

//! Main eframe::App implementation
//! Pattern: chama-optics src/app.rs (Arc<Mutex<Vec>> queue + spawn_local + polling)

use std::sync::{Arc, Mutex};

use chrono::NaiveDateTime;
use eframe::egui;

use crate::expense;
use crate::model::{AppState, CardTransaction, PendingImage};
use crate::parser;
use crate::table;

#[cfg(target_arch = "wasm32")]
use crate::ocr;
#[cfg(target_arch = "wasm32")]
use crate::web_download;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

/// Completed OCR result: Ok(transaction) or Err(filename, error)
type OcrResult = Result<CardTransaction, (String, String)>;

pub struct CardReceiptApp {
    state: AppState,
    /// Async OCR tasks push completed results here
    completed_queue: Arc<Mutex<Vec<OcrResult>>>,
    /// File picker pushes new files here
    #[allow(clippy::type_complexity)]
    file_queue: Arc<Mutex<Vec<(String, Vec<u8>)>>>,
    /// Number of OCR tasks currently in flight
    ocr_remaining: Arc<Mutex<usize>>,
    // Preview / edit state
    preview_texture: Option<egui::TextureHandle>,
    preview_loaded_for: Option<usize>,
    edit_merchant: String,
    edit_amount_str: String,
    edit_datetime_str: String,
    edit_expense_type: String,
}

impl CardReceiptApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            state: AppState::new(),
            completed_queue: Arc::new(Mutex::new(Vec::new())),
            file_queue: Arc::new(Mutex::new(Vec::new())),
            ocr_remaining: Arc::new(Mutex::new(0)),
            preview_texture: None,
            preview_loaded_for: None,
            edit_merchant: String::new(),
            edit_amount_str: String::new(),
            edit_datetime_str: String::new(),
            edit_expense_type: String::new(),
        }
    }

    /// Start OCR processing for all pending images
    #[cfg(target_arch = "wasm32")]
    fn process_pending_images(&mut self, ctx: &egui::Context) {
        let pending: Vec<PendingImage> = self.state.pending_images.drain(..).collect();
        if pending.is_empty() {
            return;
        }

        {
            let mut remaining = self.ocr_remaining.lock().unwrap();
            *remaining += pending.len();
        }
        self.state.ocr_in_progress = true;

        for image in pending {
            let completed_queue = Arc::clone(&self.completed_queue);
            let remaining = Arc::clone(&self.ocr_remaining);
            let filename = image.filename.clone();
            let bytes = image.bytes;
            let ctx = ctx.clone();

            spawn_local(async move {
                let result = match ocr::recognize_text(&bytes).await {
                    Ok(text) => match parser::parse_receipt(&filename, &text) {
                        Ok(mut txn) => {
                            txn.image_bytes = bytes;
                            Ok(txn)
                        }
                        Err(e) => Err((filename.clone(), format!("파싱 실패: {}", e))),
                    },
                    Err(e) => Err((filename.clone(), format!("OCR 실패: {}", e))),
                };

                completed_queue.lock().unwrap().push(result);
                let mut rem = remaining.lock().unwrap();
                *rem = rem.saturating_sub(1);
                ctx.request_repaint();
            });
        }
    }

    /// Poll for completed OCR results (called each frame)
    fn poll_results(&mut self) {
        // Check completed transactions
        let mut completed = self.completed_queue.lock().unwrap();
        for result in completed.drain(..) {
            match result {
                Ok(txn) => {
                    self.state.transactions.push(txn);
                }
                Err((filename, error)) => {
                    self.state
                        .error_messages
                        .push(format!("{}: {}", filename, error));
                }
            }
        }
        drop(completed);

        // Check for newly picked files
        let mut files = self.file_queue.lock().unwrap();
        for (name, bytes) in files.drain(..) {
            self.state.pending_images.push(PendingImage {
                filename: name,
                bytes,
            });
        }
        drop(files);

        // Update progress status
        let remaining = *self.ocr_remaining.lock().unwrap();
        if remaining > 0 {
            self.state.status_message = format!("OCR 처리 중... ({}개 남음)", remaining);
            self.state.ocr_in_progress = true;
        } else if self.state.ocr_in_progress {
            // OCR just completed: force datetime ascending sort
            self.state.ocr_in_progress = false;
            self.state.sort_column = crate::model::SortColumn::DateTime;
            self.state.sort_direction = crate::model::SortDirection::Ascending;
            self.state.sort_transactions();
            if self.state.error_messages.is_empty() {
                self.state.status_message =
                    format!("완료! {}개 거래 인식됨", self.state.transactions.len());
            } else {
                self.state.status_message = format!(
                    "완료! {}개 인식, {}개 실패",
                    self.state.transactions.len(),
                    self.state.error_messages.len()
                );
            }
        }
    }

    /// Update preview texture and edit fields when selection changes
    fn update_preview(&mut self, ctx: &egui::Context) {
        // Validate selected_index
        if let Some(idx) = self.state.selected_index
            && idx >= self.state.transactions.len()
        {
            self.state.selected_index = None;
        }

        if self.state.selected_index != self.preview_loaded_for {
            if let Some(idx) = self.state.selected_index {
                let txn = &self.state.transactions[idx];
                self.edit_merchant = txn.merchant.clone();
                self.edit_amount_str = table::format_amount(txn.amount);
                self.edit_datetime_str = txn.datetime.format("%Y.%m.%d %H:%M").to_string();
                self.edit_expense_type = txn.expense_type.clone().unwrap_or_default();
                self.preview_texture =
                    decode_image_to_texture(ctx, &txn.filename, &txn.image_bytes);
                self.preview_loaded_for = Some(idx);
            } else {
                self.preview_loaded_for = None;
                self.preview_texture = None;
            }
        }
    }

    /// Apply edited fields back to the transaction
    fn apply_edits(&mut self, idx: usize) {
        if idx >= self.state.transactions.len() {
            return;
        }

        self.state.transactions[idx].merchant = self.edit_merchant.clone();

        let amount_str = self.edit_amount_str.replace(",", "").replace(" ", "");
        if let Ok(amount) = amount_str.parse::<u64>() {
            self.state.transactions[idx].amount = amount;
        }

        if let Ok(dt) = NaiveDateTime::parse_from_str(&self.edit_datetime_str, "%Y.%m.%d %H:%M") {
            self.state.transactions[idx].datetime = dt;
        }

        // Save expense type (empty string → None)
        self.state.transactions[idx].expense_type = if self.edit_expense_type.is_empty() {
            None
        } else {
            Some(self.edit_expense_type.clone())
        };
    }
}

impl eframe::App for CardReceiptApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_results();

        // Keep repainting while OCR is in progress
        if self.state.ocr_in_progress {
            ctx.request_repaint();
        }

        // Handle drag-and-drop
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                for file in &i.raw.dropped_files {
                    if let Some(bytes) = &file.bytes {
                        let name = file.name.clone();
                        if is_image_file(&name) {
                            self.state.pending_images.push(PendingImage {
                                filename: name,
                                bytes: bytes.to_vec(),
                            });
                        }
                    }
                }
            }
        });

        // Update preview when selection changes
        self.update_preview(ctx);

        // Top panel: title + controls
        egui::Panel::top("top_panel").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("카드 영수증 OCR");
            });
            ui.add_space(2.0);

            ui.horizontal(|ui| {
                // File upload button
                if ui.button("이미지 업로드").clicked() {
                    #[cfg(target_arch = "wasm32")]
                    {
                        let file_queue = Arc::clone(&self.file_queue);
                        spawn_local(async move {
                            match ocr::pick_files().await {
                                Ok(files) => {
                                    let mut q = file_queue.lock().unwrap();
                                    for (name, bytes) in files {
                                        if is_image_file(&name) {
                                            q.push((name, bytes));
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!("File picker error: {}", e);
                                }
                            }
                        });
                    }
                }

                // Process button
                let has_pending = !self.state.pending_images.is_empty();
                if ui
                    .add_enabled(
                        has_pending && !self.state.ocr_in_progress,
                        egui::Button::new(format!(
                            "OCR 인식 시작 ({}개)",
                            self.state.pending_images.len()
                        )),
                    )
                    .clicked()
                {
                    #[cfg(target_arch = "wasm32")]
                    self.process_pending_images(ctx);
                }

                // CSV export button
                if ui
                    .add_enabled(
                        !self.state.transactions.is_empty(),
                        egui::Button::new("CSV 내보내기"),
                    )
                    .clicked()
                {
                    #[cfg(target_arch = "wasm32")]
                    {
                        let csv = self.state.to_csv();
                        if let Err(e) = web_download::download_csv("카드사용내역.csv", &csv) {
                            self.state.status_message = format!("CSV 다운로드 실패: {}", e);
                        }
                    }
                }

                // ZIP bundle export: numbered images + CSV + PDF
                if ui
                    .add_enabled(
                        !self.state.transactions.is_empty(),
                        egui::Button::new("ZIP 내보내기"),
                    )
                    .clicked()
                {
                    #[cfg(target_arch = "wasm32")]
                    {
                        let csv = self.state.to_csv();
                        let images: Vec<(&str, &[u8])> = self
                            .state
                            .transactions
                            .iter()
                            .map(|t| (t.filename.as_str(), t.image_bytes.as_slice()))
                            .collect();
                        match crate::pdf_export::generate_receipts_pdf(&self.state.transactions) {
                            Ok(pdf_bytes) => {
                                if let Err(e) = web_download::download_receipt_bundle(
                                    &images,
                                    csv.as_bytes(),
                                    &pdf_bytes,
                                    "영수증모음.zip",
                                ) {
                                    self.state.status_message = format!("ZIP 다운로드 실패: {}", e);
                                }
                            }
                            Err(e) => {
                                self.state.status_message = format!("PDF 생성 실패: {}", e);
                            }
                        }
                    }
                }

                // Clear button
                if ui.button("초기화").clicked() {
                    self.state = AppState::new();
                    self.preview_texture = None;
                    self.preview_loaded_for = None;
                }
            });

            // Status bar
            ui.horizontal(|ui| {
                if self.state.ocr_in_progress {
                    ui.spinner();
                }
                ui.label(&self.state.status_message);

                if !self.state.pending_images.is_empty() && !self.state.ocr_in_progress {
                    ui.label(format!("| 대기 중: {}개", self.state.pending_images.len()));
                }
            });
            ui.add_space(2.0);
        });

        // [테이블] [수정 칸] [미리보기] 3칼럼 레이아웃
        // Side panels must be added before CentralPanel
        if let Some(idx) = self.state.selected_index {
            let mut close_panel = false;
            let mut save_edits = false;

            // Rightmost: image preview (scrollable for tall phone screenshots)
            egui::Panel::right("image_preview")
                .resizable(true)
                .default_size(300.0)
                .min_size(180.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("미리보기");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("✕").clicked() {
                                close_panel = true;
                            }
                        });
                    });
                    ui.separator();

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if let Some(texture) = &self.preview_texture {
                            let available_width = ui.available_width();
                            let [tw, th] = texture.size();
                            let scale = available_width / tw as f32;
                            let display_height = th as f32 * scale;
                            ui.image(egui::load::SizedTexture::new(
                                texture.id(),
                                egui::vec2(available_width, display_height),
                            ));
                        } else {
                            ui.colored_label(egui::Color32::GRAY, "이미지를 불러올 수 없습니다");
                        }
                    });
                });

            // Middle: edit fields (chama-optics Grid pattern)
            egui::Panel::right("edit_panel")
                .resizable(true)
                .default_size(220.0)
                .min_size(180.0)
                .show(ctx, |ui| {
                    ui.strong("항목 수정");
                    ui.separator();
                    ui.add_space(4.0);

                    egui::Grid::new("edit_grid")
                        .num_columns(2)
                        .spacing([10.0, 0.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("가맹점");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.edit_merchant)
                                    .desired_width(f32::INFINITY),
                            );
                            ui.end_row();

                            ui.label("금액");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.edit_amount_str)
                                    .desired_width(f32::INFINITY),
                            );
                            ui.end_row();

                            ui.label("날짜");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.edit_datetime_str)
                                    .desired_width(f32::INFINITY),
                            );
                            ui.end_row();

                            // Expense type field
                            ui.label("비용종류");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.edit_expense_type)
                                    .desired_width(f32::INFINITY),
                            );
                            ui.end_row();
                        });

                    ui.add_space(4.0);

                    // Expense recommendation from keyword matching
                    let recommendation = expense::detect_expense(&self.edit_merchant);
                    if let Some(rec) = &recommendation {
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                egui::Color32::from_rgb(100, 180, 255),
                                format!("추천: {}", rec.label),
                            );
                            if ui.button("적용").clicked() {
                                self.edit_expense_type = rec.label.clone();
                            }
                        });
                    }

                    // Quick-select buttons for common expense types
                    ui.add_space(4.0);
                    ui.label("빠른 선택:");
                    ui.horizontal_wrapped(|ui| {
                        for label in expense::all_expense_labels() {
                            if ui.small_button(*label).clicked() {
                                self.edit_expense_type = label.to_string();
                            }
                        }
                    });

                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        if ui.button("저장").clicked() {
                            save_edits = true;
                        }
                        if ui.button("닫기").clicked() {
                            close_panel = true;
                        }
                    });
                });

            if save_edits {
                self.apply_edits(idx);
                self.preview_loaded_for = None;
            }
            if close_panel {
                self.state.selected_index = None;
                self.preview_loaded_for = None;
                self.preview_texture = None;
            }
        }

        // Central panel: transaction table or empty state
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.state.transactions.is_empty() && !self.state.ocr_in_progress {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new(
                            "이미지를 여기에 드래그하거나\n위의 '이미지 업로드' 버튼을 클릭하세요",
                        )
                        .size(18.0)
                        .color(egui::Color32::GRAY),
                    );
                });
            } else {
                table::render_transaction_table(ui, &mut self.state);
            }

            // Error messages at the bottom
            if !self.state.error_messages.is_empty() {
                ui.separator();
                ui.collapsing("오류 내역", |ui| {
                    for msg in &self.state.error_messages {
                        ui.colored_label(egui::Color32::from_rgb(255, 100, 100), msg);
                    }
                });
            }
        });
    }
}

fn is_image_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".png")
}

fn decode_image_to_texture(
    ctx: &egui::Context,
    name: &str,
    bytes: &[u8],
) -> Option<egui::TextureHandle> {
    if bytes.is_empty() {
        return None;
    }
    let img = image::load_from_memory(bytes).ok()?;
    // Resize if too large for preview (max 1024px on longest side)
    let img = if img.width() > 1024 || img.height() > 1024 {
        img.resize(1024, 1024, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let pixels = rgba.into_raw();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    Some(ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR))
}
