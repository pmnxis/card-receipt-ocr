/*
 * SPDX-FileCopyrightText: © 2025 Jinwoo Park (pmnxis@gmail.com)
 *
 * SPDX-License-Identifier: MIT
 */

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CardTransaction {
    pub filename: String,
    pub datetime: NaiveDateTime,
    pub merchant: String,
    pub amount: u64,
    pub raw_ocr_text: String,
    pub card_format: CardFormat,
    /// User-confirmed expense type label (e.g., "Taxi", "Gas")
    pub expense_type: Option<String>,
    #[serde(skip)]
    pub image_bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum CardFormat {
    HanaCard,
    NaverHyundaiCard,
    CardAppScreenshot,
    Unknown,
}

impl std::fmt::Display for CardFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CardFormat::HanaCard => write!(f, "하나카드"),
            CardFormat::NaverHyundaiCard => write!(f, "네이버현대카드"),
            CardFormat::CardAppScreenshot => write!(f, "카드앱"),
            CardFormat::Unknown => write!(f, "기타"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PendingImage {
    pub filename: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SortColumn {
    Index,
    DateTime,
    Merchant,
    Amount,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

pub struct AppState {
    pub transactions: Vec<CardTransaction>,
    pub pending_images: Vec<PendingImage>,
    pub sort_column: SortColumn,
    pub sort_direction: SortDirection,
    pub ocr_in_progress: bool,
    pub status_message: String,
    pub error_messages: Vec<String>,
    pub selected_index: Option<usize>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            transactions: Vec::new(),
            pending_images: Vec::new(),
            sort_column: SortColumn::DateTime,
            sort_direction: SortDirection::Ascending,
            ocr_in_progress: false,
            status_message: "이미지를 업로드하세요".into(),
            error_messages: Vec::new(),
            selected_index: None,
        }
    }

    pub fn sort_transactions(&mut self) {
        let dir = &self.sort_direction;
        match self.sort_column {
            SortColumn::Index => {} // natural order
            SortColumn::DateTime => self.transactions.sort_by(|a, b| {
                let cmp = a.datetime.cmp(&b.datetime);
                if *dir == SortDirection::Descending {
                    cmp.reverse()
                } else {
                    cmp
                }
            }),
            SortColumn::Merchant => self.transactions.sort_by(|a, b| {
                let cmp = a.merchant.cmp(&b.merchant);
                if *dir == SortDirection::Descending {
                    cmp.reverse()
                } else {
                    cmp
                }
            }),
            SortColumn::Amount => self.transactions.sort_by(|a, b| {
                let cmp = a.amount.cmp(&b.amount);
                if *dir == SortDirection::Descending {
                    cmp.reverse()
                } else {
                    cmp
                }
            }),
        }
    }

    pub fn total_amount(&self) -> u64 {
        self.transactions.iter().map(|t| t.amount).sum()
    }

    pub fn to_csv(&self) -> String {
        // UTF-8 BOM for Excel compatibility
        let mut csv = String::from("\u{FEFF}");
        csv.push_str("파일명,날짜,가맹점,금액\n");
        for t in &self.transactions {
            // Use expense_type instead of merchant when set
            // (sc-expense Chrome extension reads this column)
            let merchant_col = t.expense_type.as_deref().unwrap_or(&t.merchant);
            csv.push_str(&format!(
                "{},{},{},{}\n",
                t.filename,
                t.datetime.format("%m.%d %H:%M"),
                merchant_col,
                t.amount,
            ));
        }
        csv
    }
}
