mod support;

use std::collections::BTreeMap;

use axum::http::{Method, StatusCode};
use calamine::{Data, Reader, open_workbook_auto};
use csv::ReaderBuilder;
use rust_decimal::Decimal;
use serde_json::Value;
use support::{TestApp, expired_access_token, json_value};

fn multipart_body(file_name: &str, bytes: &[u8]) -> (String, Vec<u8>) {
    let boundary = "makima-import-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    (format!("multipart/form-data; boundary={boundary}"), body)
}

fn decimal_text(value: &Value) -> Decimal {
    value
        .as_str()
        .expect("decimal field must be a string")
        .parse()
        .expect("decimal field must parse")
}

fn expected_bgsaxo_positions() -> BTreeMap<String, Decimal> {
    let bytes = include_bytes!("../../../test_files/posizioni_bgsaxo.csv");
    let mut reader = ReaderBuilder::new()
        .delimiter(b',')
        .flexible(true)
        .from_reader(&bytes[..]);
    let mut by_isin = BTreeMap::new();

    for row in reader.records() {
        let row = row.expect("CSV row should parse");
        let isin = row.get(36).unwrap_or("").trim();
        let side = row.get(1).unwrap_or("").trim();
        let quantity = row.get(3).unwrap_or("").trim();
        if isin.is_empty() || side != "Long" || quantity.is_empty() {
            continue;
        }
        by_isin.insert(
            isin.to_string(),
            quantity
                .replace(',', ".")
                .parse()
                .expect("quantity should parse"),
        );
    }

    by_isin
}

fn expected_fineco_positions() -> BTreeMap<String, Decimal> {
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test_files/posizioni_fineco.xls");
    let mut workbook =
        open_workbook_auto(&fixture_path).expect("Fineco positions workbook should open");
    let sheet_name = workbook.sheet_names().first().cloned().unwrap();
    let range = workbook.worksheet_range(&sheet_name).unwrap();
    let rows = range.rows().collect::<Vec<_>>();

    let (header_index, isin_index, quantity_index) = rows
        .iter()
        .enumerate()
        .find_map(|(index, row)| {
            let headers = row
                .iter()
                .enumerate()
                .filter_map(|(column, cell)| match cell {
                    Data::String(value) => Some((value.trim().to_ascii_lowercase(), column)),
                    _ => None,
                })
                .collect::<BTreeMap<_, _>>();

            Some((
                index,
                *headers.get("isin")?,
                *headers
                    .iter()
                    .find_map(|(header, column)| header.starts_with("quantit").then_some(column))?,
            ))
        })
        .expect("Fineco position header row should exist");

    let mut by_isin = BTreeMap::new();
    for row in rows.into_iter().skip(header_index + 1) {
        let isin = match row.get(isin_index) {
            Some(Data::String(value)) => value.trim(),
            _ => "",
        };
        if isin.is_empty() {
            continue;
        }
        let quantity = match row.get(quantity_index) {
            Some(Data::Float(value)) => Decimal::from_str_exact(&format!("{value}")).unwrap(),
            Some(Data::Int(value)) => Decimal::from(*value),
            Some(Data::String(value)) => value.parse().unwrap(),
            _ => continue,
        };
        if quantity > Decimal::ZERO {
            by_isin.insert(isin.to_string(), quantity);
        }
    }

    by_isin
}

async fn import_file(
    app: &TestApp,
    access_token: &str,
    broker: &str,
    portfolio_id: &str,
    file_name: &str,
    file_bytes: &[u8],
) -> axum::http::Response<axum::body::Body> {
    let (content_type, body) = multipart_body(file_name, file_bytes);
    app.request_bytes_with_token(
        Method::POST,
        &format!("/api/v1/import/{broker}?portfolio_id={portfolio_id}"),
        access_token,
        &content_type,
        body,
    )
    .await
}

#[tokio::test]
async fn test_import_requires_authentication() {
    let app = TestApp::new().await;
    let (content_type, body) = multipart_body(
        "fineco.xls",
        include_bytes!("../../../test_files/fineco.xls"),
    );

    let response = app
        .request_bytes(
            Method::POST,
            &format!(
                "/api/v1/import/fineco?portfolio_id={}",
                uuid::Uuid::now_v7()
            ),
            &content_type,
            body,
        )
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}

#[tokio::test]
async fn test_import_invalid_token_returns_401() {
    let app = TestApp::new().await;
    let token = expired_access_token(uuid::Uuid::now_v7());
    let (content_type, body) = multipart_body(
        "fineco.xls",
        include_bytes!("../../../test_files/fineco.xls"),
    );

    let response = app
        .request_bytes_with_token(
            Method::POST,
            &format!(
                "/api/v1/import/fineco?portfolio_id={}",
                uuid::Uuid::now_v7()
            ),
            &token,
            &content_type,
            body,
        )
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}

#[tokio::test]
async fn test_import_invalid_broker_returns_400() {
    let app = TestApp::new().await;
    let auth = app
        .register_user_ok("import-invalid@example.com", "password123")
        .await;
    let portfolio = app.create_portfolio(&auth.access_token, "Core", None).await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    let response = import_file(
        &app,
        &auth.access_token,
        "unknown",
        portfolio_id,
        "fineco.xls",
        include_bytes!("../../../test_files/fineco.xls"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_value(response).await;
    assert_eq!(body["code"], "INVALID_BROKER");

    app.cleanup().await;
}

#[tokio::test]
async fn test_import_invalid_file_returns_400_with_row_errors() {
    let app = TestApp::new().await;
    let auth = app
        .register_user_ok("import-invalid-file@example.com", "password123")
        .await;
    let portfolio = app.create_portfolio(&auth.access_token, "Core", None).await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    let response = import_file(
        &app,
        &auth.access_token,
        "fineco",
        portfolio_id,
        "broken.xls",
        b"not-an-excel-file",
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_value(response).await;
    assert_eq!(body["code"], "IMPORT_PARSE_ERROR");
    assert!(body["row_errors"].as_array().unwrap().len() >= 1);

    app.cleanup().await;
}

#[tokio::test]
async fn test_import_returns_404_for_non_owned_portfolio() {
    let app = TestApp::new().await;
    let alice = app
        .register_user_ok("import-alice@example.com", "password123")
        .await;
    let bob = app
        .register_user_ok("import-bob@example.com", "password123")
        .await;
    let portfolio = app
        .create_portfolio(&alice.access_token, "Alice", None)
        .await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    let response = import_file(
        &app,
        &bob.access_token,
        "fineco",
        portfolio_id,
        "fineco.xls",
        include_bytes!("../../../test_files/fineco.xls"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    app.cleanup().await;
}

#[tokio::test]
async fn test_import_fineco_matches_expected_open_positions_and_duplicate_reimport_warns() {
    let app = TestApp::new().await;
    let auth = app
        .register_user_ok("import-fineco@example.com", "password123")
        .await;
    let portfolio = app
        .create_portfolio(&auth.access_token, "Fineco", None)
        .await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    let response = import_file(
        &app,
        &auth.access_token,
        "fineco",
        portfolio_id,
        "fineco.xls",
        include_bytes!("../../../test_files/fineco.xls"),
    )
    .await;

    let status = response.status();
    let body = json_value(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["transactions_imported"].as_u64().unwrap() > 0);
    assert!(body["assets_created"].as_array().unwrap().len() > 0);

    let positions = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/portfolios/{portfolio_id}/positions"),
            &auth.access_token,
        )
        .await;
    let positions_status = positions.status();
    let positions_body = json_value(positions).await;
    assert_eq!(positions_status, StatusCode::OK);
    let actual = positions_body
        .as_array()
        .unwrap()
        .iter()
        .map(|position| {
            (
                position["asset"]["isin"].as_str().unwrap().to_string(),
                decimal_text(&position["quantity"]),
            )
        })
        .collect::<BTreeMap<_, _>>();

    assert_eq!(actual, expected_fineco_positions());

    let duplicate_response = import_file(
        &app,
        &auth.access_token,
        "fineco",
        portfolio_id,
        "fineco.xls",
        include_bytes!("../../../test_files/fineco.xls"),
    )
    .await;
    assert_eq!(duplicate_response.status(), StatusCode::OK);
    let duplicate_body = json_value(duplicate_response).await;
    assert_eq!(duplicate_body["transactions_imported"], 0);
    assert!(!duplicate_body["warnings"].as_array().unwrap().is_empty());

    app.cleanup().await;
}

#[tokio::test]
async fn test_import_bgsaxo_matches_expected_open_positions_and_returns_warnings() {
    let app = TestApp::new().await;
    let auth = app
        .register_user_ok("import-bgsaxo@example.com", "password123")
        .await;
    let portfolio = app
        .create_portfolio(&auth.access_token, "BG Saxo", None)
        .await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    let response = import_file(
        &app,
        &auth.access_token,
        "bgsaxo",
        portfolio_id,
        "bgsaxo.xlsx",
        include_bytes!("../../../test_files/bgsaxo.xlsx"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_value(response).await;
    assert!(body["transactions_imported"].as_u64().unwrap() > 0);
    assert!(body["assets_created"].as_array().unwrap().len() > 0);
    assert!(!body["warnings"].as_array().unwrap().is_empty());

    let positions = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/portfolios/{portfolio_id}/positions"),
            &auth.access_token,
        )
        .await;
    assert_eq!(positions.status(), StatusCode::OK);
    let positions_body = json_value(positions).await;
    let actual = positions_body
        .as_array()
        .unwrap()
        .iter()
        .map(|position| {
            (
                position["asset"]["isin"].as_str().unwrap().to_string(),
                decimal_text(&position["quantity"]),
            )
        })
        .collect::<BTreeMap<_, _>>();

    assert_eq!(actual, expected_bgsaxo_positions());

    app.cleanup().await;
}
