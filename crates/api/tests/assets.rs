mod support;

use std::sync::Arc;

use application::transactions::ResolvedAssetMetadata;
use axum::http::{Method, StatusCode};
use domain::AssetClass;
use serde_json::json;
use support::{StaticAssetReferenceLookup, TestApp, expired_access_token, json_value};
use uuid::Uuid;

#[tokio::test]
async fn test_assets_require_authentication() {
    let app = TestApp::new().await;

    let response = app.request(Method::GET, "/api/v1/assets").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_value(response).await;
    assert_eq!(body["code"], "UNAUTHORIZED");

    app.cleanup().await;
}

#[tokio::test]
async fn test_assets_invalid_token_returns_401() {
    let app = TestApp::new().await;
    let token = expired_access_token(Uuid::now_v7());

    let response = app
        .request_with_token(Method::GET, "/api/v1/assets", &token)
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_value(response).await;
    assert_eq!(body["code"], "UNAUTHORIZED");

    app.cleanup().await;
}

#[tokio::test]
async fn test_create_asset_without_ticker_uses_openfigi_mapping() {
    let app =
        TestApp::new_with_asset_lookup(Arc::new(StaticAssetReferenceLookup::with_assets([(
            "IE00BK5BQT80",
            ResolvedAssetMetadata {
                isin: "IE00BK5BQT80".to_string(),
                yahoo_ticker: Some("VWCE.MI".to_string()),
                name: "Vanguard FTSE All-World UCITS ETF".to_string(),
                asset_class: AssetClass::Stock,
                currency: "EUR".to_string(),
                exchange: Some("Milan".to_string()),
            },
        )])))
        .await;
    let auth = app
        .register_user_ok("alice@example.com", "password123")
        .await;

    let response = app
        .create_asset(
            &auth.access_token,
            json!({
                "isin": "IE00BK5BQT80",
                "name": "Vanguard FTSE All-World UCITS ETF",
                "asset_class": "Stock",
                "currency": "EUR",
                "exchange": "Milan"
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_value(response).await;
    assert_eq!(body["isin"], "IE00BK5BQT80");
    assert_eq!(body["yahoo_ticker"], "VWCE.MI");

    app.cleanup().await;
}

#[tokio::test]
async fn test_create_asset_without_ticker_succeeds_when_lookup_returns_none() {
    let app = TestApp::new().await;
    let auth = app
        .register_user_ok("alice@example.com", "password123")
        .await;

    let response = app
        .create_asset(
            &auth.access_token,
            json!({
                "isin": "US0378331005",
                "name": "Apple Inc.",
                "asset_class": "Stock",
                "currency": "USD",
                "exchange": "NASDAQ"
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_value(response).await;
    assert!(body["yahoo_ticker"].is_null());

    app.cleanup().await;
}

#[tokio::test]
async fn test_create_asset_with_duplicate_isin_returns_409() {
    let app = TestApp::new().await;
    let auth = app
        .register_user_ok("alice@example.com", "password123")
        .await;

    let payload = json!({
        "isin": "US5949181045",
        "name": "Microsoft Corporation",
        "asset_class": "Stock",
        "currency": "USD",
        "exchange": "NASDAQ",
        "yahoo_ticker": "MSFT"
    });

    let first = app.create_asset(&auth.access_token, payload.clone()).await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let second = app.create_asset(&auth.access_token, payload).await;
    assert_eq!(second.status(), StatusCode::CONFLICT);
    let body = json_value(second).await;
    assert_eq!(body["code"], "ASSET_ALREADY_EXISTS");

    app.cleanup().await;
}

#[tokio::test]
async fn test_create_asset_with_invalid_isin_returns_400() {
    let app = TestApp::new().await;
    let auth = app
        .register_user_ok("alice@example.com", "password123")
        .await;

    let response = app
        .create_asset(
            &auth.access_token,
            json!({
                "isin": "BAD-ISIN",
                "name": "Broken Asset",
                "asset_class": "Stock",
                "currency": "USD"
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_value(response).await;
    assert_eq!(body["code"], "VALIDATION_ERROR");

    app.cleanup().await;
}

#[tokio::test]
async fn test_list_get_and_update_assets_support_filters_and_not_found() {
    let app = TestApp::new().await;
    let auth = app
        .register_user_ok("alice@example.com", "password123")
        .await;

    app.create_asset(
        &auth.access_token,
        json!({
            "isin": "US02079K3059",
            "name": "Alphabet Inc.",
            "asset_class": "Stock",
            "currency": "USD",
            "exchange": "NASDAQ",
            "yahoo_ticker": "GOOGL"
        }),
    )
    .await;
    app.create_asset(
        &auth.access_token,
        json!({
            "isin": "IE00B4L5Y983",
            "name": "iShares Core MSCI World UCITS ETF",
            "asset_class": "Stock",
            "currency": "USD",
            "exchange": "LSE",
            "yahoo_ticker": "SWDA.L"
        }),
    )
    .await;
    app.create_asset(
        &auth.access_token,
        json!({
            "isin": "US912810TM09",
            "name": "US Treasury Bond",
            "asset_class": "Bond",
            "currency": "USD",
            "exchange": "OTC",
            "yahoo_ticker": "^TNX"
        }),
    )
    .await;

    let list_response = app
        .request_with_token(
            Method::GET,
            "/api/v1/assets?page=1&limit=10&asset_class=Stock&search=world",
            &auth.access_token,
        )
        .await;
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body = json_value(list_response).await;
    assert_eq!(list_body["pagination"]["total_items"], 1);
    assert_eq!(list_body["data"][0]["isin"], "IE00B4L5Y983");

    let get_response = app
        .request_with_token(
            Method::GET,
            "/api/v1/assets/US02079K3059",
            &auth.access_token,
        )
        .await;
    assert_eq!(get_response.status(), StatusCode::OK);
    let get_body = json_value(get_response).await;
    assert_eq!(get_body["name"], "Alphabet Inc.");

    let update_response = app
        .request_json_with_token(
            Method::PUT,
            "/api/v1/assets/US02079K3059",
            &auth.access_token,
            json!({
                "name": "Alphabet Class A",
                "asset_class": "Stock",
                "currency": "USD",
                "exchange": "NASDAQ",
                "yahoo_ticker": "GOOGL"
            }),
        )
        .await;
    assert_eq!(update_response.status(), StatusCode::OK);
    let updated = json_value(update_response).await;
    assert_eq!(updated["name"], "Alphabet Class A");

    let missing = app
        .request_with_token(
            Method::GET,
            "/api/v1/assets/US0000000000",
            &auth.access_token,
        )
        .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    app.cleanup().await;
}
