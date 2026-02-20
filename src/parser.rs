/*
 * SPDX-FileCopyrightText: © 2025 Jinwoo Park (pmnxis@gmail.com)
 *
 * SPDX-License-Identifier: MIT
 */

//! Parse OCR text into structured CardTransaction data.
//! Supports 3 Korean card receipt formats:
//! - 하나카드 (web receipt)
//! - 네이버 현대카드 (app screenshot, dark bg)
//! - 카드앱 스크린샷 (매출전표 modal)

use chrono::NaiveDateTime;
use regex::Regex;

use crate::model::{CardFormat, CardTransaction};

/// Detect format and parse OCR text into a CardTransaction
pub fn parse_receipt(filename: &str, raw_text: &str) -> Result<CardTransaction, String> {
    let format = detect_format(raw_text);
    let (datetime, merchant, amount) = match format {
        CardFormat::HanaCard => parse_hana_card(raw_text)?,
        CardFormat::NaverHyundaiCard => parse_naver_hyundai(raw_text)?,
        CardFormat::CardAppScreenshot => parse_card_app_screenshot(raw_text)?,
        CardFormat::Unknown => parse_fallback(raw_text)?,
    };

    Ok(CardTransaction {
        filename: filename.to_string(),
        datetime,
        merchant,
        amount,
        raw_ocr_text: raw_text.to_string(),
        card_format: format,
        expense_type: None,
        image_bytes: Vec::new(),
    })
}

fn detect_format(text: &str) -> CardFormat {
    if text.contains("하나카드") || text.contains("거래일시") {
        CardFormat::HanaCard
    } else if text.contains("결제 정보") || text.contains("현대카드") || text.contains("거래 일자") {
        CardFormat::NaverHyundaiCard
    } else if text.contains("카드이용내역")
        || text.contains("매출전표")
        || text.contains("상세 이용내역")
    {
        CardFormat::CardAppScreenshot
    } else {
        CardFormat::Unknown
    }
}

/// 하나카드 format:
/// 거래일시 2026.01.22 16:35:39
/// 승인금액 27,600 원
/// 가맹점명 네이버파이낸셜(주)
fn parse_hana_card(text: &str) -> Result<(NaiveDateTime, String, u64), String> {
    let date_re =
        Regex::new(r"거래일시\s+(\d{4})[.\s](\d{2})[.\s](\d{2})\s*(\d{2}):(\d{2}):?(\d{2})?")
            .unwrap();
    let datetime = if let Some(caps) = date_re.captures(text) {
        let s = format!(
            "{}-{}-{} {}:{}:{}",
            &caps[1],
            &caps[2],
            &caps[3],
            &caps[4],
            &caps[5],
            caps.get(6).map_or("00", |m| m.as_str())
        );
        NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
            .map_err(|e| format!("날짜 파싱 오류: {}", e))?
    } else {
        return Err("거래일시를 찾을 수 없습니다".into());
    };

    let amount = extract_amount_after_label(text, "승인금액")
        .or_else(|_| extract_first_amount(text))?;

    let merchant = extract_text_after_label(text, "가맹점명")
        .unwrap_or_else(|| extract_merchant_before_amount(text));

    Ok((datetime, merchant, amount))
}

/// 네이버 현대카드 format:
/// 해진구도일주유소일산지점
/// 43,489원
/// 거래 일자 26. 1. 31 · 14:59:27
fn parse_naver_hyundai(text: &str) -> Result<(NaiveDateTime, String, u64), String> {
    let date_re = Regex::new(
        r"거래\s*일자\s+(\d{2})[.\s]+(\d{1,2})[.\s]+(\d{1,2})\s*[·\-:]\s*(\d{2}):(\d{2}):?(\d{2})?"
    ).unwrap();

    // Also try "거래 일자" with the dot-separated format
    let date_re2 = Regex::new(
        r"거래\s*일\s*자\s+(\d{2})\.\s*(\d{1,2})\.\s*(\d{1,2})\s*[·\-]\s*(\d{2}):(\d{2}):?(\d{2})?"
    ).unwrap();

    let datetime = if let Some(caps) = date_re.captures(text).or_else(|| date_re2.captures(text)) {
        let year = 2000 + caps[1].parse::<i32>().unwrap_or(26);
        let s = format!(
            "{}-{:02}-{:02} {}:{}:{}",
            year,
            caps[2].parse::<u32>().unwrap_or(1),
            caps[3].parse::<u32>().unwrap_or(1),
            &caps[4],
            &caps[5],
            caps.get(6).map_or("00", |m| m.as_str())
        );
        NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
            .map_err(|e| format!("날짜 파싱 오류: {}", e))?
    } else {
        return Err("거래 일자를 찾을 수 없습니다".into());
    };

    // Try labeled "금액" first, then first non-zero amount, then first amount
    let amount = extract_amount_after_label(text, "금액")
        .or_else(|_| extract_first_nonzero_amount(text))
        .or_else(|_| extract_first_amount(text))?;
    let merchant = extract_merchant_before_amount(text);

    Ok((datetime, merchant, amount))
}

/// 카드앱 스크린샷 format:
/// 상세 이용내역
/// 스타한국물류
/// 16,500원
/// 거래일 2026.01.23 11:59
fn parse_card_app_screenshot(text: &str) -> Result<(NaiveDateTime, String, u64), String> {
    // Try "거래일" (without 시)
    let date_re =
        Regex::new(r"거래일\s+(\d{4})[.\s](\d{2})[.\s](\d{2})\s+(\d{2}):(\d{2})").unwrap();
    // Also try "거래일" with full datetime
    let date_re2 = Regex::new(
        r"거래일\s+(\d{4})[.\s](\d{2})[.\s](\d{2})\s*(\d{2}):(\d{2}):?(\d{2})?",
    )
    .unwrap();

    let datetime =
        if let Some(caps) = date_re.captures(text).or_else(|| date_re2.captures(text)) {
            let s = format!(
                "{}-{}-{} {}:{}:{}",
                &caps[1],
                &caps[2],
                &caps[3],
                &caps[4],
                &caps[5],
                caps.get(6).map_or("00", |m| m.as_str())
            );
            NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                .map_err(|e| format!("날짜 파싱 오류: {}", e))?
        } else {
            return Err("거래일을 찾을 수 없습니다".into());
        };

    // For card app screenshots, try labeled amounts first (공급가액),
    // then first non-zero amount (avoid 부가세 0원 / 봉사료 0원)
    let amount = extract_amount_after_label(text, "공급가액")
        .or_else(|_| extract_first_nonzero_amount(text))
        .or_else(|_| extract_first_amount(text))?;

    let merchant = extract_merchant_from_card_detail(text)
        .or_else(|| extract_text_after_label(text, "상세 이용내역"))
        .unwrap_or_else(|| extract_merchant_before_amount(text));

    Ok((datetime, merchant, amount))
}

fn parse_fallback(text: &str) -> Result<(NaiveDateTime, String, u64), String> {
    parse_hana_card(text)
        .or_else(|_| parse_naver_hyundai(text))
        .or_else(|_| parse_card_app_screenshot(text))
        .map_err(|_| "알 수 없는 영수증 형식입니다".into())
}

// --- Helper functions ---

fn extract_amount_after_label(text: &str, label: &str) -> Result<u64, String> {
    let pattern = format!(r"{}\s+([\d,]+)\s*원", regex::escape(label));
    let re = Regex::new(&pattern).unwrap();
    if let Some(caps) = re.captures(text) {
        parse_krw_amount(&caps[1])
    } else {
        Err(format!("'{}' 뒤에서 금액을 찾을 수 없습니다", label))
    }
}

fn extract_first_amount(text: &str) -> Result<u64, String> {
    let re = Regex::new(r"([\d,]+)\s*원").unwrap();
    if let Some(caps) = re.captures(text) {
        parse_krw_amount(&caps[1])
    } else {
        Err("금액을 찾을 수 없습니다".into())
    }
}

fn extract_first_nonzero_amount(text: &str) -> Result<u64, String> {
    let re = Regex::new(r"([\d,]+)\s*원").unwrap();
    for caps in re.captures_iter(text) {
        if let Ok(amount) = parse_krw_amount(&caps[1]) {
            if amount > 0 {
                return Ok(amount);
            }
        }
    }
    Err("0이 아닌 금액을 찾을 수 없습니다".into())
}

fn parse_krw_amount(s: &str) -> Result<u64, String> {
    let cleaned: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    cleaned
        .parse::<u64>()
        .map_err(|e| format!("금액 파싱 오류: {}", e))
}

fn extract_text_after_label(text: &str, label: &str) -> Option<String> {
    for (i, line) in text.lines().enumerate() {
        if line.contains(label) {
            // Value on same line after label
            if let Some(after) = line.split(label).nth(1) {
                let trimmed = after.trim();
                if !trimmed.is_empty() && trimmed != "X" && trimmed != "x" {
                    return Some(trimmed.to_string());
                }
            }
            // Or on the next line
            if let Some(next) = text.lines().nth(i + 1) {
                let trimmed = next.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

/// Extract merchant name from "상세 이용내역" popup in card app screenshots.
/// Handles two-column OCR where right-panel fields (거래구분, 승인번호 etc.)
/// may appear interspersed between the header and merchant name.
fn extract_merchant_from_card_detail(text: &str) -> Option<String> {
    const NON_MERCHANT: &[&str] = &[
        "거래구분",
        "승인번호",
        "거래상태",
        "이용카드",
        "가맹점",
        "공급가액",
        "부가세",
        "봉사료",
        "자원순환",
        "거래일",
        "결제확정",
        "일시불",
        "본인",
        "신용",
        "체크",
        "카드이용내역",
        "매출전표",
        "구글페이",
    ];
    let amount_re = Regex::new(r"[\d,]+\s*원").unwrap();

    let mut found_header = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.contains("상세 이용내역") {
            found_header = true;
            continue;
        }
        if !found_header {
            continue;
        }

        // Skip empty lines, close button, OCR noise
        if trimmed.is_empty()
            || trimmed == "X"
            || trimmed == "x"
            || trimmed.starts_with('~')
            || trimmed.chars().count() <= 1
        {
            continue;
        }

        // Skip known non-merchant field labels
        if NON_MERCHANT.iter().any(|p| trimmed.contains(p)) {
            continue;
        }

        // Skip amount lines
        if amount_re.is_match(trimmed) {
            continue;
        }

        // Skip lines that are just numbers
        if trimmed.chars().all(|c| c.is_ascii_digit() || c == ',' || c == ' ') {
            continue;
        }

        return Some(trimmed.to_string());
    }
    None
}

fn extract_merchant_before_amount(text: &str) -> String {
    let amount_re = Regex::new(r"[\d,]+\s*원").unwrap();
    let skip_patterns = [
        "카드이용내역",
        "매출전표",
        "상세 이용내역",
        "하나카드",
        "현대카드",
        "결제 정보",
        "결제 구분",
        "결제 카드",
        "금액 상세",
        "카드번호",
        "카드 소지자",
        "가상카드번호",
        "거래유형",
        "거래구분",
        "일시불",
        "승인번호",
        "승인상태",
        "거래상태",
        "이용카드",
        "결제확정",
        "현지승인금액",
        "CNY",
        "USD",
        "JPY",
        "EUR",
        "VISA",
        "MasterCard",
        "UnionPay",
        "실제 결제금액",
        "해외이용수수료",
        "가맹점 번호",
        "가맹점 상세",
        "대표자명",
        "사업자 등록번호",
        "업종",
    ];
    let mut candidate = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if amount_re.is_match(trimmed) {
            break;
        }
        if skip_patterns.iter().any(|p| trimmed.contains(p)) {
            continue;
        }
        if trimmed.len() > 1 {
            candidate = trimmed.to_string();
        }
    }
    candidate
}
