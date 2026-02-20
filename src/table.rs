/*
 * SPDX-FileCopyrightText: © 2025 Jinwoo Park (pmnxis@gmail.com)
 *
 * SPDX-License-Identifier: MIT
 */

//! Sortable transaction table UI using egui_extras::TableBuilder

use egui::{RichText, Ui};
use egui_extras::{Column, TableBuilder};

use crate::model::{AppState, SortColumn, SortDirection};

pub fn render_transaction_table(ui: &mut Ui, state: &mut AppState) {
    let table = TableBuilder::new(ui)
        .striped(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(35.0)) // #
        .column(Column::exact(100.0)) // 날짜/시간
        .column(Column::remainder()) // 가맹점 (유연하게 늘어남/줄어듦)
        .column(Column::exact(100.0)) // 비용종류
        .column(Column::exact(100.0)); // 금액 (항상 표시)

    table
        .header(22.0, |mut header| {
            header.col(|ui| {
                sort_header_label(ui, state, "#", SortColumn::Index);
            });
            header.col(|ui| {
                sort_header_label(ui, state, "날짜/시간", SortColumn::DateTime);
            });
            header.col(|ui| {
                sort_header_label(ui, state, "가맹점", SortColumn::Merchant);
            });
            header.col(|ui| {
                ui.strong("비용종류");
            });
            header.col(|ui| {
                sort_header_label(ui, state, "금액 (원)", SortColumn::Amount);
            });
        })
        .body(|body| {
            body.rows(20.0, state.transactions.len(), |mut row| {
                let idx = row.index();
                let is_selected = state.selected_index == Some(idx);
                row.set_selected(is_selected);

                // Extract data into locals to avoid borrow conflicts
                let datetime_str = state.transactions[idx]
                    .datetime
                    .format("%m.%d %H:%M")
                    .to_string();
                let merchant = state.transactions[idx].merchant.clone();
                let expense_type = state.transactions[idx].expense_type.clone();
                let amount = state.transactions[idx].amount;

                row.col(|ui| {
                    ui.label(format!("{}", idx + 1));
                });
                row.col(|ui| {
                    ui.label(&datetime_str);
                });
                row.col(|ui| {
                    ui.label(&merchant);
                });
                row.col(|ui| {
                    if let Some(et) = &expense_type {
                        ui.label(RichText::new(et).color(egui::Color32::from_rgb(100, 200, 100)));
                    } else {
                        ui.colored_label(egui::Color32::from_rgb(150, 150, 150), "-");
                    }
                });
                row.col(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format_amount(amount)).strong());
                    });
                });

                if row.response().clicked() {
                    state.selected_index = if is_selected { None } else { Some(idx) };
                }
            });
        });

    // Footer
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(format!("총 {}건", state.transactions.len()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(format!("합계: {}원", format_amount(state.total_amount())))
                    .strong()
                    .size(15.0),
            );
        });
    });
}

fn sort_header_label(ui: &mut Ui, state: &mut AppState, label: &str, column: SortColumn) {
    let arrow = if state.sort_column == column {
        match state.sort_direction {
            SortDirection::Ascending => " ▲",
            SortDirection::Descending => " ▼",
        }
    } else {
        ""
    };

    if ui
        .button(RichText::new(format!("{}{}", label, arrow)).strong())
        .clicked()
    {
        if state.sort_column == column {
            state.sort_direction = match state.sort_direction {
                SortDirection::Ascending => SortDirection::Descending,
                SortDirection::Descending => SortDirection::Ascending,
            };
        } else {
            state.sort_column = column;
            state.sort_direction = SortDirection::Ascending;
        }
        state.sort_transactions();
    }
}

pub fn format_amount(amount: u64) -> String {
    let s = amount.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
