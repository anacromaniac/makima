//! BG Saxo broker parser.

use std::collections::HashMap;

use calamine::{Data, Reader};
use domain::{
    AssetClass, BrokerImportParseError, BrokerImportRowError, BrokerImporter,
    ParsedBrokerTransaction, TransactionType,
};
use rust_decimal::Decimal;

use crate::{data_to_date, data_to_decimal, data_to_string, normalize_header, open_workbook};

/// Parser for BG Saxo Excel exports.
#[derive(Debug, Default, Clone, Copy)]
pub struct BgSaxoImporter;

impl BrokerImporter for BgSaxoImporter {
    fn parse(
        &self,
        file_bytes: &[u8],
    ) -> Result<Vec<ParsedBrokerTransaction>, BrokerImportParseError> {
        let mut workbook = open_workbook(file_bytes)?;
        let trades = workbook
            .worksheet_range("Contrattazioni")
            .map_err(|error| BrokerImportParseError {
                row_errors: vec![BrokerImportRowError {
                    row: 0,
                    message: format!("unable to read Contrattazioni sheet: {error}"),
                }],
            })?;
        let bookings =
            workbook
                .worksheet_range("Bookings")
                .map_err(|error| BrokerImportParseError {
                    row_errors: vec![BrokerImportRowError {
                        row: 0,
                        message: format!("unable to read Bookings sheet: {error}"),
                    }],
                })?;

        let trade_headers = header_index(trades.rows().next());
        let booking_headers = header_index(bookings.rows().next());
        let commissions = booking_commissions(bookings.rows().skip(1), &booking_headers);

        let mut parsed = Vec::new();
        let mut errors = Vec::new();

        for (offset, row) in trades.rows().skip(1).enumerate() {
            let row_number = (offset + 2) as u32;
            if row.iter().all(|cell| matches!(cell, Data::Empty)) {
                continue;
            }

            match parse_trade_row(row, row_number, &trade_headers, &commissions) {
                Ok(Some(transaction)) => parsed.push(transaction),
                Ok(None) => {}
                Err(message) => errors.push(BrokerImportRowError {
                    row: row_number,
                    message,
                }),
            }
        }

        if errors.is_empty() {
            Ok(parsed)
        } else {
            Err(BrokerImportParseError { row_errors: errors })
        }
    }
}

fn header_index(row: Option<&[Data]>) -> HashMap<String, usize> {
    row.into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, cell)| {
            data_to_string(Some(cell)).map(|value| (normalize_header(&value), index))
        })
        .collect()
}

fn booking_commissions<'a>(
    rows: impl Iterator<Item = &'a [Data]>,
    headers: &HashMap<String, usize>,
) -> HashMap<String, Decimal> {
    let mut by_trade_id = HashMap::new();

    for row in rows {
        let kind = header_value(row, headers, "tipo").unwrap_or_default();
        if kind != "Commissione" {
            continue;
        }

        let Some(trade_id) = header_value(row, headers, "id contrattazione") else {
            continue;
        };
        let amount = data_to_decimal(header_cell(row, headers, "importo contabilizzato"))
            .map(|value| value.abs())
            .unwrap_or(Decimal::ZERO);
        let rate = data_to_decimal(header_cell(row, headers, "tasso di conversione"))
            .filter(|rate| *rate > Decimal::ZERO)
            .unwrap_or(Decimal::ONE);
        let commission = amount / rate;

        by_trade_id
            .entry(trade_id)
            .and_modify(|value| *value += commission)
            .or_insert(commission);
    }

    by_trade_id
}

fn parse_trade_row(
    row: &[Data],
    row_number: u32,
    headers: &HashMap<String, usize>,
    commissions: &HashMap<String, Decimal>,
) -> Result<Option<ParsedBrokerTransaction>, String> {
    let operation_label =
        header_value(row, headers, "tipo").ok_or_else(|| "missing operation type".to_string())?;
    if operation_label.trim().is_empty() {
        return Ok(None);
    }

    let transaction_type = if operation_label.starts_with("Acquista") {
        TransactionType::Buy
    } else if operation_label.starts_with("Vendi") {
        TransactionType::Sell
    } else {
        return Ok(None);
    };

    let date = data_to_date(header_cell(row, headers, "data della negoziazione"))
        .ok_or_else(|| "missing trade date".to_string())?;
    let settlement_date = data_to_date(header_cell(row, headers, "data valuta"));
    let quantity = data_to_decimal(header_cell(row, headers, "traded quantity"))
        .map(|value| value.abs())
        .ok_or_else(|| "missing traded quantity".to_string())?;
    let unit_price = data_to_decimal(header_cell(row, headers, "prezzo"))
        .ok_or_else(|| "missing unit price".to_string())?;
    let isin =
        header_value(row, headers, "instrument isin").ok_or_else(|| "missing ISIN".to_string())?;
    let asset_name = header_value(row, headers, "strumento")
        .ok_or_else(|| "missing instrument name".to_string())?;
    let asset_currency = header_value(row, headers, "valuta strumento")
        .ok_or_else(|| "missing instrument currency".to_string())?
        .to_ascii_uppercase();
    let exchange = header_value(row, headers, "mercato");
    let currency = asset_currency.clone();
    let asset_class = header_value(row, headers, "_tipo").map(|value| map_asset_class(&value));
    let trade_id = header_value(row, headers, "id contrattazione")
        .unwrap_or_else(|| format!("row-{row_number}"));
    let commission = commissions.get(&trade_id).copied().unwrap_or(Decimal::ZERO);

    Ok(Some(ParsedBrokerTransaction {
        date,
        settlement_date,
        isin,
        asset_name,
        asset_class,
        asset_currency,
        exchange,
        transaction_type,
        quantity: Some(quantity),
        unit_price: Some(unit_price),
        commission,
        currency,
        gross_amount: None,
        tax_withheld: None,
        net_amount: None,
        notes: Some(operation_label),
    }))
}

fn map_asset_class(value: &str) -> AssetClass {
    match value.trim().to_ascii_lowercase().as_str() {
        "stock" => AssetClass::Stock,
        "etf" | "etc" | "etn" => AssetClass::Alternative,
        "bond" => AssetClass::Bond,
        "commodity" => AssetClass::Commodity,
        "crypto" => AssetClass::Crypto,
        _ => AssetClass::Alternative,
    }
}

fn header_cell<'a>(
    row: &'a [Data],
    headers: &HashMap<String, usize>,
    header: &str,
) -> Option<&'a Data> {
    headers.get(header).and_then(|index| row.get(*index))
}

fn header_value(row: &[Data], headers: &HashMap<String, usize>, header: &str) -> Option<String> {
    data_to_string(header_cell(row, headers, header))
}

#[cfg(test)]
mod tests {
    use super::BgSaxoImporter;
    use domain::BrokerImporter;

    #[test]
    fn test_parse_bgsaxo_fixture() {
        let bytes = include_bytes!("../fixtures/bgsaxo_sample.xlsx");
        let transactions = BgSaxoImporter
            .parse(bytes)
            .expect("BG Saxo fixture should parse");
        assert!(!transactions.is_empty());
    }
}
