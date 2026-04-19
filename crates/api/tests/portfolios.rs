mod support;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use support::{TestApp, json_value};

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
