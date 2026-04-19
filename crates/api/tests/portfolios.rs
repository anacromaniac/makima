mod support;

use std::sync::Arc;

use application::transactions::ResolvedAssetMetadata;
use axum::http::{Method, StatusCode};
use domain::AssetClass;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use support::{
    StaticAssetReferenceLookup, StaticExchangeRateLookup, TestApp, expired_access_token, json_value,
};

fn resolved_asset(
    isin: &str,
    name: &str,
    asset_class: AssetClass,
    currency: &str,
) -> ResolvedAssetMetadata {
    ResolvedAssetMetadata {
        isin: isin.to_string(),
        yahoo_ticker: Some(format!("{}.MI", name.replace(' ', ""))),
        name: name.to_string(),
        asset_class,
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
    quantity: &str,
    unit_price: &str,
    currency: &str,
) {
    let response = app
        .request_json_with_token(
            Method::POST,
            &format!("/api/v1/portfolios/{portfolio_id}/transactions"),
            access_token,
            json!({
                "asset_isin": asset_isin,
                "transaction_type": "Buy",
                "date": "2025-01-10",
                "quantity": quantity,
                "unit_price": unit_price,
                "commission": "0",
                "currency": currency
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn asset_id_by_isin(app: &TestApp, access_token: &str, isin: &str) -> String {
    let response = app
        .request_with_token(Method::GET, "/api/v1/assets", access_token)
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    json_value(response).await["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|asset| asset["isin"] == isin)
        .and_then(|asset| asset["id"].as_str())
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn test_portfolios_require_authentication() {
    let app = TestApp::new().await;

    let response = app.request(Method::GET, "/api/v1/portfolios").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_value(response).await;
    assert_eq!(body["code"], "UNAUTHORIZED");

    app.cleanup().await;
}

#[tokio::test]
async fn test_create_portfolio_returns_201() {
    let app = TestApp::new().await;
    let pair = app
        .register_user_ok("alice@example.com", "password123")
        .await;

    let response = app
        .create_portfolio(
            &pair.access_token,
            "Core Portfolio",
            Some("Long-term holdings"),
        )
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_value(response).await;
    assert_eq!(body["name"], "Core Portfolio");
    assert_eq!(body["base_currency"], "EUR");

    app.cleanup().await;
}

#[tokio::test]
async fn test_list_portfolios_returns_only_callers_items_and_pagination() {
    let app = TestApp::new().await;
    let alice = app
        .register_user_ok("alice@example.com", "password123")
        .await;
    let bob = app.register_user_ok("bob@example.com", "password123").await;

    app.create_portfolio(&alice.access_token, "Alpha", None)
        .await;
    app.create_portfolio(&alice.access_token, "Beta", None)
        .await;
    app.create_portfolio(&bob.access_token, "Gamma", None).await;

    let response = app
        .request_with_token(
            Method::GET,
            "/api/v1/portfolios?page=1&limit=1",
            &alice.access_token,
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_value(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["pagination"]["page"], 1);
    assert_eq!(body["pagination"]["limit"], 1);
    assert_eq!(body["pagination"]["total_items"], 2);
    assert_eq!(body["pagination"]["total_pages"], 2);
    assert_eq!(body["data"][0]["name"], "Alpha");

    app.cleanup().await;
}

#[tokio::test]
async fn test_get_update_and_delete_owned_portfolio_work() {
    let app = TestApp::new().await;
    let pair = app
        .register_user_ok("alice@example.com", "password123")
        .await;

    let create_response = app
        .create_portfolio(&pair.access_token, "Original", Some("Before update"))
        .await;
    let created: Value = json_value(create_response).await;
    let portfolio_id = created["id"].as_str().unwrap();

    let get_response = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/portfolios/{portfolio_id}"),
            &pair.access_token,
        )
        .await;
    assert_eq!(get_response.status(), StatusCode::OK);

    let update_response = app
        .request_json_with_token(
            Method::PUT,
            &format!("/api/v1/portfolios/{portfolio_id}"),
            &pair.access_token,
            json!({ "name": "Updated", "description": "After update" }),
        )
        .await;
    assert_eq!(update_response.status(), StatusCode::OK);
    let updated = json_value(update_response).await;
    assert_eq!(updated["name"], "Updated");

    let delete_response = app
        .request_with_token(
            Method::DELETE,
            &format!("/api/v1/portfolios/{portfolio_id}"),
            &pair.access_token,
        )
        .await;
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let get_after_delete = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/portfolios/{portfolio_id}"),
            &pair.access_token,
        )
        .await;
    assert_eq!(get_after_delete.status(), StatusCode::NOT_FOUND);

    app.cleanup().await;
}

#[tokio::test]
async fn test_portfolio_ownership_isolation_returns_404() {
    let app = TestApp::new().await;
    let alice = app
        .register_user_ok("alice@example.com", "password123")
        .await;
    let bob = app.register_user_ok("bob@example.com", "password123").await;

    let create_response = app
        .create_portfolio(&alice.access_token, "Private", None)
        .await;
    let created: Value = json_value(create_response).await;
    let portfolio_id = created["id"].as_str().unwrap();

    let get_response = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/portfolios/{portfolio_id}"),
            &bob.access_token,
        )
        .await;
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);

    let update_response = app
        .request_json_with_token(
            Method::PUT,
            &format!("/api/v1/portfolios/{portfolio_id}"),
            &bob.access_token,
            json!({ "name": "Stolen", "description": null }),
        )
        .await;
    assert_eq!(update_response.status(), StatusCode::NOT_FOUND);

    let delete_response = app
        .request_with_token(
            Method::DELETE,
            &format!("/api/v1/portfolios/{portfolio_id}"),
            &bob.access_token,
        )
        .await;
    assert_eq!(delete_response.status(), StatusCode::NOT_FOUND);

    app.cleanup().await;
}

#[tokio::test]
async fn test_create_portfolio_with_invalid_payload_returns_400() {
    let app = TestApp::new().await;
    let pair = app
        .register_user_ok("alice@example.com", "password123")
        .await;

    let response = app
        .request_json_with_token(
            Method::POST,
            "/api/v1/portfolios",
            &pair.access_token,
            json!({ "name": "", "description": "Invalid" }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_value(response).await;
    assert_eq!(body["code"], "VALIDATION_ERROR");

    app.cleanup().await;
}

#[tokio::test]
async fn test_portfolio_summary_requires_authentication() {
    let app = TestApp::new().await;

    let response = app
        .request(
            Method::GET,
            "/api/v1/portfolios/00000000-0000-0000-0000-000000000000/summary",
        )
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_value(response).await;
    assert_eq!(body["code"], "UNAUTHORIZED");

    app.cleanup().await;
}

#[tokio::test]
async fn test_portfolio_summary_invalid_token_returns_401() {
    let app = TestApp::new().await;
    let token = expired_access_token(uuid::Uuid::now_v7());

    let response = app
        .request_with_token(
            Method::GET,
            "/api/v1/portfolios/00000000-0000-0000-0000-000000000000/summary",
            &token,
        )
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_value(response).await;
    assert_eq!(body["code"], "UNAUTHORIZED");

    app.cleanup().await;
}

#[tokio::test]
async fn test_portfolio_summary_returns_zero_values_for_empty_portfolio() {
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
            &format!("/api/v1/portfolios/{portfolio_id}/summary"),
            &auth.access_token,
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_value(response).await;
    assert_decimal_eq(&body["total_value"], Decimal::ZERO);
    assert_decimal_eq(&body["total_gain_loss_absolute"], Decimal::ZERO);
    assert_decimal_eq(&body["total_gain_loss_percentage"], Decimal::ZERO);
    assert_eq!(body["asset_allocation"].as_array().unwrap().len(), 0);
    assert_eq!(body["warnings"].as_array().unwrap().len(), 0);

    app.cleanup().await;
}

#[tokio::test]
async fn test_portfolio_summary_returns_gain_loss_allocation_and_warnings() {
    let app = TestApp::new_with_lookups(
        Arc::new(StaticAssetReferenceLookup::with_assets([
            (
                "IE00STOCK001",
                resolved_asset("IE00STOCK001", "Euro Equity ETF", AssetClass::Stock, "EUR"),
            ),
            (
                "US00BOND0002",
                resolved_asset("US00BOND0002", "US Treasury ETF", AssetClass::Bond, "USD"),
            ),
            (
                "GB00COMM0003",
                resolved_asset("GB00COMM0003", "Gold ETC", AssetClass::Commodity, "EUR"),
            ),
        ])),
        Arc::new(StaticExchangeRateLookup::with_rates([(
            "USD",
            Decimal::new(50, 2),
        )])),
        Arc::new(support::StaticPriceAdapters::empty()),
    )
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
        "IE00STOCK001",
        "10",
        "100",
        "EUR",
    )
    .await;
    create_buy_transaction(
        &app,
        &auth.access_token,
        portfolio_id,
        "US00BOND0002",
        "2",
        "250",
        "USD",
    )
    .await;
    create_buy_transaction(
        &app,
        &auth.access_token,
        portfolio_id,
        "GB00COMM0003",
        "1",
        "50",
        "EUR",
    )
    .await;

    app.create_price_history_table().await;
    app.insert_price_history(
        uuid::Uuid::parse_str(&asset_id_by_isin(&app, &auth.access_token, "IE00STOCK001").await)
            .unwrap(),
        chrono::NaiveDate::from_ymd_opt(2025, 1, 11).unwrap(),
        Decimal::new(120, 0),
        "EUR",
        "manual",
    )
    .await;
    app.insert_price_history(
        uuid::Uuid::parse_str(&asset_id_by_isin(&app, &auth.access_token, "US00BOND0002").await)
            .unwrap(),
        chrono::NaiveDate::from_ymd_opt(2025, 1, 11).unwrap(),
        Decimal::new(300, 0),
        "USD",
        "manual",
    )
    .await;

    let response = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/portfolios/{portfolio_id}/summary"),
            &auth.access_token,
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_value(response).await;
    assert_decimal_eq(&body["total_value"], Decimal::new(1500, 0));
    assert_decimal_eq(&body["total_gain_loss_absolute"], Decimal::new(250, 0));
    assert_decimal_eq(&body["total_gain_loss_percentage"], Decimal::new(20, 0));

    let allocation = body["asset_allocation"].as_array().unwrap();
    assert_eq!(allocation.len(), 2);
    let stock = allocation
        .iter()
        .find(|entry| entry["asset_class"] == "Stock")
        .unwrap();
    assert_decimal_eq(&stock["value"], Decimal::new(1200, 0));
    assert_decimal_eq(&stock["percentage"], Decimal::new(80, 0));

    let bond = allocation
        .iter()
        .find(|entry| entry["asset_class"] == "Bond")
        .unwrap();
    assert_decimal_eq(&bond["value"], Decimal::new(300, 0));
    assert_decimal_eq(&bond["percentage"], Decimal::new(20, 0));

    let warnings = body["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0],
        "Missing current price for asset GB00COMM0003 (Gold ETC)"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn test_portfolio_summary_returns_404_for_missing_or_foreign_portfolio() {
    let app = TestApp::new().await;
    let alice = app
        .register_user_ok("alice@example.com", "password123")
        .await;
    let bob = app.register_user_ok("bob@example.com", "password123").await;
    let portfolio = app
        .create_portfolio(&alice.access_token, "Private", None)
        .await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    let foreign_response = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/portfolios/{portfolio_id}/summary"),
            &bob.access_token,
        )
        .await;
    assert_eq!(foreign_response.status(), StatusCode::NOT_FOUND);

    let missing_response = app
        .request_with_token(
            Method::GET,
            "/api/v1/portfolios/00000000-0000-0000-0000-000000000000/summary",
            &alice.access_token,
        )
        .await;
    assert_eq!(missing_response.status(), StatusCode::NOT_FOUND);

    app.cleanup().await;
}
