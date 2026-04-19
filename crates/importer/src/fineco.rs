//! Fineco broker parser.

use std::collections::HashMap;

use calamine::{Data, Reader};
use domain::{
    AssetClass, BrokerImportParseError, BrokerImportRowError, BrokerImporter,
    ParsedBrokerTransaction, TransactionType,
};
use rust_decimal::Decimal;

use crate::{data_to_date, data_to_decimal, data_to_string, normalize_header, open_workbook};

/// Parser for Fineco Excel exports.
#[derive(Debug, Default, Clone, Copy)]
pub struct FinecoImporter;

impl BrokerImporter for FinecoImporter {
    fn parse(
        &self,
        file_bytes: &[u8],
    ) -> Result<Vec<ParsedBrokerTransaction>, BrokerImportParseError> {
        let mut workbook = open_workbook(file_bytes)?;
        let sheet_name =
            workbook
                .sheet_names()
                .first()
                .cloned()
                .ok_or_else(|| BrokerImportParseError {
                    row_errors: vec![BrokerImportRowError {
                        row: 0,
                        message: "workbook does not contain any sheets".to_string(),
                    }],
                })?;
        let range =
            workbook
                .worksheet_range(&sheet_name)
                .map_err(|error| BrokerImportParseError {
                    row_errors: vec![BrokerImportRowError {
                        row: 0,
                        message: format!("unable to read {sheet_name} sheet: {error}"),
                    }],
                })?;

        let rows = range.rows().collect::<Vec<_>>();
        let Some((header_row_index, headers)) = detect_header_row(&rows) else {
            return Err(BrokerImportParseError {
                row_errors: vec![BrokerImportRowError {
                    row: 0,
                    message: "unable to locate Fineco header row".to_string(),
                }],
            });
        };

        let mut parsed = Vec::new();
        let mut errors = Vec::new();
        for (offset, row) in rows.into_iter().skip(header_row_index + 1).enumerate() {
            let row_number = (header_row_index + offset + 2) as u32;
            if row.iter().all(|cell| matches!(cell, Data::Empty)) {
                continue;
            }

            match parse_row(row, &headers) {
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

fn detect_header_row(rows: &[&[Data]]) -> Option<(usize, HashMap<String, usize>)> {
    rows.iter().enumerate().find_map(|(index, row)| {
        let headers = row
            .iter()
            .enumerate()
            .filter_map(|(column, cell)| {
                data_to_string(Some(cell)).map(|value| (normalize_header(&value), column))
            })
            .collect::<HashMap<_, _>>();

        if headers.contains_key("operazione")
            && headers.contains_key("titolo")
            && headers.contains_key("isin")
            && headers.contains_key("quantita")
        {
            Some((index, headers))
        } else {
            None
        }
    })
}

fn parse_row(
    row: &[Data],
    headers: &HashMap<String, usize>,
) -> Result<Option<ParsedBrokerTransaction>, String> {
    let trade_date_cell = header_cell(row, headers, "operazione");
    let date = data_to_date(trade_date_cell).ok_or_else(|| "missing trade date".to_string())?;
    let description = header_value(row, headers, "descrizione").unwrap_or_default();
    if description.trim().is_empty() {
        return Ok(None);
    }

    let transaction_type = match normalize_header(&description).as_str() {
        value if value.contains("compravendita") => {
            let sign = header_value(row, headers, "segno").unwrap_or_default();
            if sign.eq_ignore_ascii_case("A") || sign == "+" {
                TransactionType::Buy
            } else if sign.eq_ignore_ascii_case("V") || sign == "-" {
                TransactionType::Sell
            } else {
                infer_type_from_amounts(row, headers)?
            }
        }
        value if value.contains("aumento capitale") => TransactionType::Buy,
        _ => return Ok(None),
    };

    let settlement_date = data_to_date(header_cell(row, headers, "data valuta"));
    let quantity = data_to_decimal(header_cell(row, headers, "quantita"))
        .map(|value| value.abs())
        .ok_or_else(|| "missing quantity".to_string())?;
    let unit_price = data_to_decimal(header_cell(row, headers, "prezzo"))
        .ok_or_else(|| "missing unit price".to_string())?;
    let currency = header_value(row, headers, "divisa")
        .ok_or_else(|| "missing currency".to_string())?
        .to_ascii_uppercase();
    let isin = header_value(row, headers, "isin").ok_or_else(|| "missing ISIN".to_string())?;
    let asset_name = header_value(row, headers, "titolo")
        .ok_or_else(|| "missing instrument name".to_string())?;
    let commission = extract_commission(row, headers);

    Ok(Some(ParsedBrokerTransaction {
        date,
        settlement_date,
        isin,
        asset_name,
        asset_class: Some(AssetClass::Alternative),
        asset_currency: currency.clone(),
        exchange: None,
        transaction_type,
        quantity: Some(quantity),
        unit_price: Some(unit_price),
        commission,
        currency,
        gross_amount: None,
        tax_withheld: None,
        net_amount: None,
        notes: Some(description.replace('\n', " ")),
    }))
}

fn infer_type_from_amounts(
    row: &[Data],
    headers: &HashMap<String, usize>,
) -> Result<TransactionType, String> {
    let countervalue = data_to_decimal(header_cell(row, headers, "controvalore"))
        .or_else(|| data_to_decimal(header_cell(row, headers, "controvalore ")))
        .ok_or_else(|| "unable to infer buy/sell without sign or countervalue".to_string())?;

    if countervalue < Decimal::ZERO {
        Ok(TransactionType::Buy)
    } else {
        Ok(TransactionType::Sell)
    }
}

fn extract_commission(row: &[Data], headers: &HashMap<String, usize>) -> Decimal {
    let candidate_headers = [
        "commissioni amministrato",
        "commissioni fondi sgr",
        "commissioni fondi banca corrispondente",
        "commissioni fondi sw/ingr/uscita",
    ];

    candidate_headers
        .into_iter()
        .filter_map(|header| data_to_decimal(header_cell(row, headers, header)))
        .fold(Decimal::ZERO, |acc, value| acc + value.abs())
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
    use super::FinecoImporter;
    use domain::BrokerImporter;

    #[test]
    fn test_parse_fineco_fixture() {
        let bytes = include_bytes!("../fixtures/fineco_sample.xls");
        let transactions = FinecoImporter
            .parse(bytes)
            .expect("Fineco fixture should parse");
        assert!(!transactions.is_empty());
    }
}
