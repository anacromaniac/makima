//! # Importer Crate
//!
//! Broker file parsers for importing transaction data into Makima.

pub mod bgsaxo;
pub mod fineco;

use std::io::Cursor;

use calamine::{Data, Sheets, open_workbook_auto_from_rs};
use chrono::{Duration, NaiveDate};
use domain::{BrokerImportParseError, BrokerImportRowError};
use rust_decimal::Decimal;

const EXCEL_EPOCH_DAYS: i64 = 25_569;

fn open_workbook(file_bytes: &[u8]) -> Result<Sheets<Cursor<Vec<u8>>>, BrokerImportParseError> {
    open_workbook_auto_from_rs(Cursor::new(file_bytes.to_vec())).map_err(|error| {
        BrokerImportParseError {
            row_errors: vec![BrokerImportRowError {
                row: 0,
                message: format!("unable to open workbook: {error}"),
            }],
        }
    })
}

fn normalize_header(value: &str) -> String {
    value
        .trim()
        .replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn data_to_string(cell: Option<&Data>) -> Option<String> {
    match cell? {
        Data::Empty => None,
        Data::String(value) => Some(value.trim().to_string()).filter(|value| !value.is_empty()),
        Data::Float(value) => Some(trim_float(*value)),
        Data::Int(value) => Some(value.to_string()),
        Data::Bool(value) => Some(value.to_string()),
        Data::DateTime(value) => Some(trim_float(value.as_f64())),
        Data::DateTimeIso(value) => {
            Some(value.trim().to_string()).filter(|value| !value.is_empty())
        }
        Data::DurationIso(value) => {
            Some(value.trim().to_string()).filter(|value| !value.is_empty())
        }
        Data::Error(_) => None,
    }
}

fn data_to_decimal(cell: Option<&Data>) -> Option<Decimal> {
    match cell? {
        Data::Empty => None,
        Data::Int(value) => Some(Decimal::from(*value)),
        Data::Float(value) => Decimal::from_str_exact(&trim_float(*value)).ok(),
        Data::String(value) => parse_decimal(value),
        Data::DateTime(value) => Decimal::from_str_exact(&trim_float(value.as_f64())).ok(),
        Data::DateTimeIso(_) | Data::DurationIso(_) | Data::Bool(_) | Data::Error(_) => None,
    }
}

fn data_to_date(cell: Option<&Data>) -> Option<NaiveDate> {
    match cell? {
        Data::DateTime(value) => excel_serial_to_date(value.as_f64()),
        Data::Float(value) => {
            excel_serial_to_date(*value).or_else(|| parse_date(&trim_float(*value)))
        }
        Data::Int(value) => excel_serial_to_date(*value as f64),
        Data::String(value) | Data::DateTimeIso(value) => parse_date(value),
        Data::Empty | Data::Bool(_) | Data::DurationIso(_) | Data::Error(_) => None,
    }
}

fn excel_serial_to_date(value: f64) -> Option<NaiveDate> {
    let days = value.trunc() as i64;
    NaiveDate::from_ymd_opt(1970, 1, 1)?.checked_add_signed(Duration::days(days - EXCEL_EPOCH_DAYS))
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    let trimmed = value.trim();
    ["%d/%m/%Y", "%d/%m/%y", "%Y-%m-%d"]
        .into_iter()
        .find_map(|format| NaiveDate::parse_from_str(trimmed, format).ok())
}

fn parse_decimal(value: &str) -> Option<Decimal> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        return None;
    }

    let normalized = trimmed
        .replace(['\u{a0}', '.'], "")
        .replace(',', ".")
        .replace("EUR", "")
        .replace("CHF", "")
        .replace("USD", "")
        .replace(' ', "");

    Decimal::from_str_exact(&normalized).ok()
}

fn trim_float(value: f64) -> String {
    let mut text = format!("{value:.10}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}
