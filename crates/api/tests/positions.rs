mod support;

use std::sync::Arc;

use application::transactions::ResolvedAssetMetadata;
use axum::http::{Method, StatusCode};
use chrono::NaiveDate;
use domain::AssetClass;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use support::{StaticAssetReferenceLookup, TestApp, expired_access_token, json_value};

fn resolved_asset(isin: &str, name: &str, currency: &str) -> ResolvedAssetMetadata {
    ResolvedAssetMetadata {
        isin: isin.to_string(),
        yahoo_ticker: Some(format!("{name}.MI").replace(' ', "")),
        name: name.to_string(),
        asset_class: AssetClass::Stock,
        currency: currency.to_string(),
        exchange: Some("Milan".to_string()),
    }
}

fn decimal_text(value: &Value) -> String {
    value.to_string().trim_matches('"').to_string()
}

fn assert_decimal_eq(value: &Value, expected: Decimal) {
    let actual = decimal_text(value)
        .parse::<Decimal>()
        .expect("response field must be a decimal string");
    assert_eq!(actual, expected);
}

async fn create_buy_transaction(
    app: &TestApp,
    access_token: &str,
    portfolio_id: &str,
    asset_isin: &str,
    date: &str,
    quantity: &str,
    unit_price: &str,
) {
    let response = app
        .request_json_with_token(
            Method::POST,
            &format!("/api/v1/portfolios/{portfolio_id}/transactions"),
            access_token,
            json!({
                "asset_isin": asset_isin,
                "transaction_type": "Buy",
                "date": date,
                "quantity": quantity,
                "unit_price": unit_price,
                "commission": "0",
                "currency": "EUR"
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn create_sell_transaction(
    app: &TestApp,
    access_token: &str,
    portfolio_id: &str,
    asset_isin: &str,
    date: &str,
    quantity: &str,
    unit_price: &str,
) {
    let response = app
        .request_json_with_token(
            Method::POST,
            &format!("/api/v1/portfolios/{portfolio_id}/transactions"),
            access_token,
            json!({
                "asset_isin": asset_isin,
                "transaction_type": "Sell",
                "date": date,
                "quantity": quantity,
                "unit_price": unit_price,
                "commission": "0",
                "currency": "EUR"
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_positions_require_authentication() {
    let app = TestApp::new().await;

    let response = app
        .request(
            Method::GET,
            "/api/v1/portfolios/00000000-0000-0000-0000-000000000000/positions",
        )
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_value(response).await;
    assert_eq!(body["code"], "UNAUTHORIZED");

    app.cleanup().await;
}

#[tokio::test]
async fn test_positions_invalid_token_returns_401() {
    let app = TestApp::new().await;
    let token = expired_access_token(uuid::Uuid::now_v7());

    let response = app
        .request_with_token(
            Method::GET,
            "/api/v1/portfolios/00000000-0000-0000-0000-000000000000/positions",
            &token,
        )
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_value(response).await;
    assert_eq!(body["code"], "UNAUTHORIZED");

    app.cleanup().await;
}

#[tokio::test]
async fn test_list_positions_returns_open_positions_by_default_with_gain_loss() {
    let app = TestApp::new_with_asset_lookup(Arc::new(StaticAssetReferenceLookup::with_assets([
        (
            "IE00BK5BQT80",
            resolved_asset("IE00BK5BQT80", "Vanguard FTSE All-World UCITS ETF", "EUR"),
        ),
        (
            "LU1681045370",
            resolved_asset("LU1681045370", "Amundi MSCI World", "EUR"),
        ),
    ])))
    .await;
    let auth = app
        .register_user_ok("alice@example.com", "password123")
        .await;
    let portfolio = app.create_portfolio(&auth.access_token, "Core", None).await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    create_buy_transaction(
        &app,
        &auth.access_token,
        portfolio_id,
        "IE00BK5BQT80",
        "2025-01-10",
        "10",
        "100",
    )
    .await;
    create_buy_transaction(
        &app,
        &auth.access_token,
        portfolio_id,
        "IE00BK5BQT80",
        "2025-01-11",
        "10",
        "120",
    )
    .await;
    create_buy_transaction(
        &app,
        &auth.access_token,
        portfolio_id,
        "LU1681045370",
        "2025-01-12",
        "5",
        "50",
    )
    .await;
    create_sell_transaction(
        &app,
        &auth.access_token,
        portfolio_id,
        "LU1681045370",
        "2025-01-13",
        "5",
        "55",
    )
    .await;

    app.create_price_history_table().await;

    let assets = app
        .request_with_token(Method::GET, "/api/v1/assets", &auth.access_token)
        .await;
    let assets_body = json_value(assets).await;
    let open_asset_id = assets_body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|asset| asset["isin"] == "IE00BK5BQT80")
        .and_then(|asset| asset["id"].as_str())
        .unwrap();

    app.insert_price_history(
        uuid::Uuid::parse_str(open_asset_id).unwrap(),
        NaiveDate::from_ymd_opt(2025, 1, 14).unwrap(),
        Decimal::new(130, 0),
        "EUR",
        "manual",
    )
    .await;

    let response = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/portfolios/{portfolio_id}/positions"),
            &auth.access_token,
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_value(response).await;
    let positions = body.as_array().unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0]["asset"]["isin"], "IE00BK5BQT80");
    assert_decimal_eq(&positions[0]["quantity"], Decimal::new(20, 0));
    assert_decimal_eq(&positions[0]["average_cost"], Decimal::new(110, 0));
    assert_decimal_eq(&positions[0]["current_price"], Decimal::new(130, 0));
    assert_decimal_eq(&positions[0]["current_value"], Decimal::new(2600, 0));
    assert_decimal_eq(&positions[0]["gain_loss_absolute"], Decimal::new(400, 0));
    assert_eq!(
        decimal_text(&positions[0]["gain_loss_percentage"]),
        "18.181818181818181818181818180"
    );
    assert_eq!(positions[0]["closed"], false);

    app.cleanup().await;
}

#[tokio::test]
async fn test_list_positions_show_closed_includes_closed_positions() {
    let app =
        TestApp::new_with_asset_lookup(Arc::new(StaticAssetReferenceLookup::with_assets([(
            "LU1681045370",
            resolved_asset("LU1681045370", "Amundi MSCI World", "EUR"),
        )])))
        .await;
    let auth = app
        .register_user_ok("alice@example.com", "password123")
        .await;
    let portfolio = app.create_portfolio(&auth.access_token, "Core", None).await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    create_buy_transaction(
        &app,
        &auth.access_token,
        portfolio_id,
        "LU1681045370",
        "2025-01-10",
        "5",
        "50",
    )
    .await;
    create_sell_transaction(
        &app,
        &auth.access_token,
        portfolio_id,
        "LU1681045370",
        "2025-01-11",
        "5",
        "55",
    )
    .await;

    let response = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/portfolios/{portfolio_id}/positions?show_closed=true"),
            &auth.access_token,
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_value(response).await;
    let positions = body.as_array().unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0]["closed"], true);
    assert_decimal_eq(&positions[0]["quantity"], Decimal::ZERO);
    assert_decimal_eq(&positions[0]["average_cost"], Decimal::ZERO);
    assert!(positions[0]["current_price"].is_null());
    assert!(positions[0]["current_value"].is_null());
    assert!(positions[0]["gain_loss_absolute"].is_null());
    assert!(positions[0]["gain_loss_percentage"].is_null());

    app.cleanup().await;
}

#[tokio::test]
async fn test_list_positions_without_price_history_returns_null_value_fields() {
    let app =
        TestApp::new_with_asset_lookup(Arc::new(StaticAssetReferenceLookup::with_assets([(
            "IE00BK5BQT80",
            resolved_asset("IE00BK5BQT80", "Vanguard FTSE All-World UCITS ETF", "EUR"),
        )])))
        .await;
    let auth = app
        .register_user_ok("alice@example.com", "password123")
        .await;
    let portfolio = app.create_portfolio(&auth.access_token, "Core", None).await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    create_buy_transaction(
        &app,
        &auth.access_token,
        portfolio_id,
        "IE00BK5BQT80",
        "2025-01-10",
        "10",
        "100",
    )
    .await;

    let response = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/portfolios/{portfolio_id}/positions"),
            &auth.access_token,
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_value(response).await;
    let positions = body.as_array().unwrap();
    assert_eq!(positions.len(), 1);
    assert!(positions[0]["current_price"].is_null());
    assert!(positions[0]["current_value"].is_null());
    assert!(positions[0]["gain_loss_absolute"].is_null());
    assert!(positions[0]["gain_loss_percentage"].is_null());

    app.cleanup().await;
}

#[tokio::test]
async fn test_list_positions_invalid_show_closed_returns_400() {
    let app = TestApp::new().await;
    let auth = app
        .register_user_ok("alice@example.com", "password123")
        .await;
    let portfolio = app.create_portfolio(&auth.access_token, "Core", None).await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    let response = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/portfolios/{portfolio_id}/positions?show_closed=maybe"),
            &auth.access_token,
        )
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    app.cleanup().await;
}

#[tokio::test]
async fn test_list_positions_returns_404_for_non_owned_portfolio() {
    let app = TestApp::new().await;
    let alice = app
        .register_user_ok("alice@example.com", "password123")
        .await;
    let bob = app.register_user_ok("bob@example.com", "password123").await;
    let portfolio = app
        .create_portfolio(&alice.access_token, "Core", None)
        .await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    let response = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/portfolios/{portfolio_id}/positions"),
            &bob.access_token,
        )
        .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_value(response).await;
    assert_eq!(body["code"], "NOT_FOUND");

    app.cleanup().await;
}
