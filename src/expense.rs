/*
 * SPDX-FileCopyrightText: © 2025 Jinwoo Park (pmnxis@gmail.com)
 *
 * SPDX-License-Identifier: MIT
 */

//! Expense type detection based on merchant keyword matching.
//! Rules ported from sc-expense Chrome extension (popup.js).

/// Expense recommendation from keyword matching
#[derive(Clone, Debug)]
pub struct ExpenseRecommendation {
    /// Display label (e.g., "Taxi", "Gas", "Office expense")
    pub label: String,
    /// Chinese category for OA system (e.g., "市内交通(Traffic expense in base city)")
    pub category: String,
    /// Whether the fee note uses two-line format (label + merchant)
    pub two_line: bool,
}

struct Rule {
    keywords: &'static [&'static str],
    category: &'static str,
    label: &'static str,
    two_line: bool,
}

const RULES: &[Rule] = &[
    Rule {
        keywords: &["파이낸셜", "네이버파이낸셜"],
        category: "办公费(Office expenses)",
        label: "Office expense",
        two_line: true,
    },
    Rule {
        keywords: &["텔레콤", "통신", "KT", "SKT", "LGU"],
        category: "通讯费(Communication service fee)",
        label: "Telecom",
        two_line: true,
    },
    Rule {
        keywords: &[
            "흥덕",
            "식당",
            "레스토랑",
            "카페",
            "음식",
            "투다리",
            "치킨",
            "피자",
        ],
        category: "业务招待(Entertainment expenses)",
        label: "Business meal",
        two_line: true,
    },
    Rule {
        keywords: &["카카오모빌리티", "택시", "DIDI", "Taxi", "taxi"],
        category: "市内交通(Traffic expense in base city)",
        label: "Taxi",
        two_line: false,
    },
    Rule {
        keywords: &["스타한국물류", "물류", "택배", "배송", "CJ대한통운"],
        category: "快递费(Express fee)",
        label: "Express",
        two_line: false,
    },
    Rule {
        keywords: &["하이패스", "도로공사", "순환도로", "하이웨이", "톨게이트"],
        category: "车辆费(Vehicle expense)",
        label: "Tallgate(ETC)",
        two_line: false,
    },
    Rule {
        keywords: &["주유소", "에너지", "GS칼텍스", "현대오일"],
        category: "车辆费(Vehicle expense)",
        label: "Gas",
        two_line: false,
    },
];

/// Known labels that sc-expense recognizes directly (no keyword matching needed)
const KNOWN_LABELS: &[&str] = &["Gas", "Tallgate", "Highpass", "Taxi", "Express", "Telecom"];

/// Detect expense type from merchant name using sc-expense keyword rules.
/// Returns None if no rule matches.
pub fn detect_expense(merchant: &str) -> Option<ExpenseRecommendation> {
    let trimmed = merchant.trim();

    // If already a known label, no recommendation needed
    if KNOWN_LABELS.contains(&trimmed) {
        return None;
    }

    for rule in RULES {
        for keyword in rule.keywords {
            if trimmed.contains(keyword) {
                return Some(ExpenseRecommendation {
                    label: rule.label.to_string(),
                    category: rule.category.to_string(),
                    two_line: rule.two_line,
                });
            }
        }
    }

    None
}

/// Generate the fee note string for CSV output.
/// This is what the sc-expense Chrome extension expects in the merchant column.
pub fn fee_note_for_csv(expense_label: &str, _merchant: &str) -> String {
    // If it's a known single-word label, use as-is
    if KNOWN_LABELS.contains(&expense_label) {
        return expense_label.to_string();
    }
    // For two-line labels, just use the label (Chrome extension will handle it)
    expense_label.to_string()
}

/// Get all available expense labels for manual selection
pub fn all_expense_labels() -> &'static [&'static str] {
    &[
        "Office expense",
        "Telecom",
        "Business meal",
        "Taxi",
        "Express",
        "Tallgate(ETC)",
        "Gas",
    ]
}
