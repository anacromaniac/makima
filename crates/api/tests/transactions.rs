mod support;

use std::sync::Arc;

use application::transactions::ResolvedAssetMetadata;
use axum::http::{Method, StatusCode};
use domain::AssetClass;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use support::{
    StaticAssetReferenceLookup, StaticExchangeRateLookup, StaticPriceAdapters, TestApp,
    expired_access_token, json_value,
};
use uuid::Uuid;

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

#[tokio::test]
async fn test_transactions_require_authentication() {
    let app = TestApp::new().await;

    let response = app
        .request(
            Method::GET,
            &format!("/api/v1/portfolios/{}/transactions", Uuid::now_v7()),
        )
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_value(response).await;
    assert_eq!(body["code"], "UNAUTHORIZED");

    app.cleanup().await;
}

#[tokio::test]
async fn test_transactions_invalid_token_returns_401() {
    let app = TestApp::new().await;
    let token = expired_access_token(Uuid::now_v7());

    let response = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/portfolios/{}/transactions", Uuid::now_v7()),
            &token,
        )
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_value(response).await;
    assert_eq!(body["code"], "UNAUTHORIZED");

    app.cleanup().await;
}

#[tokio::test]
async fn test_create_transaction_auto_creates_asset_and_uses_exchange_rate_lookup() {
    let app = TestApp::new_with_lookups(
        Arc::new(StaticAssetReferenceLookup::with_assets([(
            "IE00BK5BQT80",
            resolved_asset("IE00BK5BQT80", "Vanguard FTSE All-World UCITS ETF", "USD"),
        )])),
        Arc::new(StaticExchangeRateLookup::with_rates([(
            "USD",
            Decimal::new(92, 2),
        )])),
        Arc::new(StaticPriceAdapters::empty()),
    )
    .await;
    let auth = app
        .register_user_ok("alice@example.com", "password123")
        .await;
    let portfolio = app
        .create_portfolio(&auth.access_token, "Core", Some("Main portfolio"))
        .await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    let response = app
        .request_json_with_token(
            Method::POST,
            &format!("/api/v1/portfolios/{portfolio_id}/transactions"),
            &auth.access_token,
            json!({
                "asset_isin": "IE00BK5BQT80",
                "transaction_type": "Buy",
                "date": "2025-01-10",
                "settlement_date": "2025-01-12",
                "quantity": "10",
                "unit_price": "100",
                "commission": "1.50",
                "currency": "USD",
                "gross_amount": null,
                "tax_withheld": null,
                "net_amount": null,
                "notes": "Initial position"
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_value(response).await;
    assert_eq!(body["asset_isin"], "IE00BK5BQT80");
    assert_eq!(body["asset_name"], "Vanguard FTSE All-World UCITS ETF");
    assert_eq!(decimal_text(&body["exchange_rate_to_base"]), "0.92000000");

    let assets = app
        .request_with_token(Method::GET, "/api/v1/assets", &auth.access_token)
        .await;
    let assets_body = json_value(assets).await;
    assert_eq!(assets_body["pagination"]["total_items"], 1);
    assert_eq!(assets_body["data"][0]["isin"], "IE00BK5BQT80");
    assert_eq!(
        app.latest_exchange_rate("USD", "EUR").await,
        Some(Decimal::new(92, 2))
    );

    app.cleanup().await;
}

#[tokio::test]
async fn test_create_transaction_uses_latest_stored_exchange_rate_when_available() {
    let app =
        TestApp::new_with_asset_lookup(Arc::new(StaticAssetReferenceLookup::with_assets([(
            "US0378331005",
            resolved_asset("US0378331005", "Apple Inc.", "USD"),
        )])))
        .await;
    let auth = app
        .register_user_ok("alice@example.com", "password123")
        .await;
    let portfolio = app.create_portfolio(&auth.access_token, "Core", None).await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    app.insert_exchange_rate(
        "USD",
        "EUR",
        chrono::NaiveDate::from_ymd_opt(2025, 1, 9).expect("static date should be valid"),
        Decimal::new(91, 2),
    )
    .await;
    app.insert_exchange_rate(
        "USD",
        "EUR",
        chrono::NaiveDate::from_ymd_opt(2025, 1, 11).expect("static date should be valid"),
        Decimal::new(93, 2),
    )
    .await;

    let response = app
        .request_json_with_token(
            Method::POST,
            &format!("/api/v1/portfolios/{portfolio_id}/transactions"),
            &auth.access_token,
            json!({
                "asset_isin": "US0378331005",
                "transaction_type": "Buy",
                "date": "2025-01-10",
                "quantity": "1",
                "unit_price": "180",
                "currency": "USD"
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_value(response).await;
    assert_eq!(decimal_text(&body["exchange_rate_to_base"]), "0.93000000");

    app.cleanup().await;
}

#[tokio::test]
async fn test_create_transaction_without_required_fields_returns_400() {
    let app = TestApp::new().await;
    let auth = app
        .register_user_ok("alice@example.com", "password123")
        .await;
    let portfolio = app.create_portfolio(&auth.access_token, "Core", None).await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    let response = app
        .request_json_with_token(
            Method::POST,
            &format!("/api/v1/portfolios/{portfolio_id}/transactions"),
            &auth.access_token,
            json!({
                "asset_isin": "IE00BK5BQT80",
                "transaction_type": "Buy",
                "date": "2025-01-10",
                "currency": "EUR"
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_value(response).await;
    assert_eq!(body["code"], "VALIDATION_ERROR");

    app.cleanup().await;
}

#[tokio::test]
async fn test_create_sell_exceeding_quantity_returns_400() {
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

    let response = app
        .request_json_with_token(
            Method::POST,
            &format!("/api/v1/portfolios/{portfolio_id}/transactions"),
            &auth.access_token,
            json!({
                "asset_isin": "IE00BK5BQT80",
                "transaction_type": "Sell",
                "date": "2025-01-10",
                "quantity": "3",
                "unit_price": "100",
                "commission": "0",
                "currency": "EUR"
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_value(response).await;
    assert_eq!(body["code"], "INSUFFICIENT_QUANTITY");

    app.cleanup().await;
}

#[tokio::test]
async fn test_non_eur_transaction_without_lookup_or_rate_returns_400() {
    let app =
        TestApp::new_with_asset_lookup(Arc::new(StaticAssetReferenceLookup::with_assets([(
            "US0378331005",
            resolved_asset("US0378331005", "Apple Inc.", "USD"),
        )])))
        .await;
    let auth = app
        .register_user_ok("alice@example.com", "password123")
        .await;
    let portfolio = app.create_portfolio(&auth.access_token, "Core", None).await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    let response = app
        .request_json_with_token(
            Method::POST,
            &format!("/api/v1/portfolios/{portfolio_id}/transactions"),
            &auth.access_token,
            json!({
                "asset_isin": "US0378331005",
                "transaction_type": "Buy",
                "date": "2025-01-10",
                "quantity": "1",
                "unit_price": "180",
                "currency": "USD"
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_value(response).await;
    assert_eq!(body["code"], "EXCHANGE_RATE_REQUIRED");

    app.cleanup().await;
}

#[tokio::test]
async fn test_list_get_update_and_delete_transactions_work_with_filters() {
    let app = TestApp::new_with_lookups(
        Arc::new(StaticAssetReferenceLookup::with_assets([
            (
                "IE00BK5BQT80",
                resolved_asset("IE00BK5BQT80", "Vanguard FTSE All-World UCITS ETF", "EUR"),
            ),
            (
                "US0378331005",
                resolved_asset("US0378331005", "Apple Inc.", "USD"),
            ),
        ])),
        Arc::new(StaticExchangeRateLookup::with_rates([(
            "USD",
            Decimal::new(91, 2),
        )])),
        Arc::new(StaticPriceAdapters::empty()),
    )
    .await;
    let auth = app
        .register_user_ok("alice@example.com", "password123")
        .await;
    let portfolio = app.create_portfolio(&auth.access_token, "Core", None).await;
    let portfolio_body = json_value(portfolio).await;
    let portfolio_id = portfolio_body["id"].as_str().unwrap();

    let buy_response = app
        .request_json_with_token(
            Method::POST,
            &format!("/api/v1/portfolios/{portfolio_id}/transactions"),
            &auth.access_token,
            json!({
                "asset_isin": "IE00BK5BQT80",
                "transaction_type": "Buy",
                "date": "2025-01-10",
                "quantity": "10",
                "unit_price": "100",
                "currency": "EUR"
            }),
        )
        .await;
    let buy_body = json_value(buy_response).await;
    let buy_id = buy_body["id"].as_str().unwrap().to_string();
    let buy_asset_id = buy_body["asset_id"].as_str().unwrap().to_string();

    app.request_json_with_token(
        Method::POST,
        &format!("/api/v1/portfolios/{portfolio_id}/transactions"),
        &auth.access_token,
        json!({
            "asset_isin": "US0378331005",
            "transaction_type": "Dividend",
            "date": "2025-02-01",
            "gross_amount": "12.50",
            "tax_withheld": "2.50",
            "net_amount": "10.00",
            "currency": "USD"
        }),
    )
    .await;

    let list_response = app
        .request_with_token(
            Method::GET,
            &format!(
                "/api/v1/portfolios/{portfolio_id}/transactions?page=1&limit=1&transaction_type=Buy&asset_id={buy_asset_id}"
            ),
            &auth.access_token,
        )
        .await;
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body = json_value(list_response).await;
    assert_eq!(list_body["pagination"]["total_items"], 1);
    assert_eq!(list_body["data"][0]["id"], buy_id);

    let get_response = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/transactions/{buy_id}"),
            &auth.access_token,
        )
        .await;
    assert_eq!(get_response.status(), StatusCode::OK);

    let update_response = app
        .request_json_with_token(
            Method::PUT,
            &format!("/api/v1/transactions/{buy_id}"),
            &auth.access_token,
            json!({
                "asset_isin": "IE00BK5BQT80",
                "transaction_type": "Buy",
                "date": "2025-01-11",
                "quantity": "12",
                "unit_price": "101",
                "commission": "2.00",
                "currency": "EUR",
                "notes": "Adjusted quantity"
            }),
        )
        .await;
    assert_eq!(update_response.status(), StatusCode::OK);
    let update_body = json_value(update_response).await;
    assert_eq!(update_body["notes"], "Adjusted quantity");

    let delete_response = app
        .request_with_token(
            Method::DELETE,
            &format!("/api/v1/transactions/{buy_id}"),
            &auth.access_token,
        )
        .await;
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let get_after_delete = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/transactions/{buy_id}"),
            &auth.access_token,
        )
        .await;
    assert_eq!(get_after_delete.status(), StatusCode::NOT_FOUND);

    app.cleanup().await;
}

#[tokio::test]
async fn test_transaction_ownership_isolation_returns_404() {
    let app =
        TestApp::new_with_asset_lookup(Arc::new(StaticAssetReferenceLookup::with_assets([(
            "IE00BK5BQT80",
            resolved_asset("IE00BK5BQT80", "Vanguard FTSE All-World UCITS ETF", "EUR"),
        )])))
        .await;
    let alice = app
        .register_user_ok("alice@example.com", "password123")
        .await;
    let bob = app.register_user_ok("bob@example.com", "password123").await;
    let alice_portfolio = app
        .create_portfolio(&alice.access_token, "Alice", None)
        .await;
    let alice_portfolio_body = json_value(alice_portfolio).await;
    let portfolio_id = alice_portfolio_body["id"].as_str().unwrap();

    let transaction = app
        .request_json_with_token(
            Method::POST,
            &format!("/api/v1/portfolios/{portfolio_id}/transactions"),
            &alice.access_token,
            json!({
                "asset_isin": "IE00BK5BQT80",
                "transaction_type": "Buy",
                "date": "2025-01-10",
                "quantity": "10",
                "unit_price": "100",
                "currency": "EUR"
            }),
        )
        .await;
    let transaction_body = json_value(transaction).await;
    let transaction_id = transaction_body["id"].as_str().unwrap();

    let list_response = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/portfolios/{portfolio_id}/transactions"),
            &bob.access_token,
        )
        .await;
    assert_eq!(list_response.status(), StatusCode::NOT_FOUND);

    let get_response = app
        .request_with_token(
            Method::GET,
            &format!("/api/v1/transactions/{transaction_id}"),
            &bob.access_token,
        )
        .await;
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);

    let update_response = app
        .request_json_with_token(
            Method::PUT,
            &format!("/api/v1/transactions/{transaction_id}"),
            &bob.access_token,
            json!({
                "asset_isin": "IE00BK5BQT80",
                "transaction_type": "Buy",
                "date": "2025-01-10",
                "quantity": "1",
                "unit_price": "100",
                "currency": "EUR"
            }),
        )
        .await;
    assert_eq!(update_response.status(), StatusCode::NOT_FOUND);

    let delete_response = app
        .request_with_token(
            Method::DELETE,
            &format!("/api/v1/transactions/{transaction_id}"),
            &bob.access_token,
        )
        .await;
    assert_eq!(delete_response.status(), StatusCode::NOT_FOUND);

    app.cleanup().await;
}

#[tokio::test]
async fn test_delete_transaction_rejects_when_later_sell_would_go_negative() {
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

    let first_buy = app
        .request_json_with_token(
            Method::POST,
            &format!("/api/v1/portfolios/{portfolio_id}/transactions"),
            &auth.access_token,
            json!({
                "asset_isin": "IE00BK5BQT80",
                "transaction_type": "Buy",
                "date": "2025-01-01",
                "quantity": "10",
                "unit_price": "100",
                "currency": "EUR"
            }),
        )
        .await;
    let first_buy_body = json_value(first_buy).await;
    let first_buy_id = first_buy_body["id"].as_str().unwrap();

    app.request_json_with_token(
        Method::POST,
        &format!("/api/v1/portfolios/{portfolio_id}/transactions"),
        &auth.access_token,
        json!({
            "asset_isin": "IE00BK5BQT80",
            "transaction_type": "Sell",
            "date": "2025-01-02",
            "quantity": "6",
            "unit_price": "101",
            "currency": "EUR"
        }),
    )
    .await;

    app.request_json_with_token(
        Method::POST,
        &format!("/api/v1/portfolios/{portfolio_id}/transactions"),
        &auth.access_token,
        json!({
            "asset_isin": "IE00BK5BQT80",
            "transaction_type": "Buy",
            "date": "2025-01-03",
            "quantity": "2",
            "unit_price": "102",
            "currency": "EUR"
        }),
    )
    .await;

    let delete_response = app
        .request_with_token(
            Method::DELETE,
            &format!("/api/v1/transactions/{first_buy_id}"),
            &auth.access_token,
        )
        .await;

    assert_eq!(delete_response.status(), StatusCode::BAD_REQUEST);
    let body = json_value(delete_response).await;
    assert_eq!(body["code"], "INSUFFICIENT_QUANTITY");

    app.cleanup().await;
}
