mod support;

use axum::http::{Method, StatusCode};
use serde_json::json;
use support::{TestApp, expired_access_token, json_value};
use uuid::Uuid;

#[tokio::test]
async fn test_get_me_with_valid_jwt_returns_200() {
    let app = TestApp::new().await;
    let pair = app
        .register_user_ok("alice@example.com", "password123")
        .await;

    let response = app
        .request_with_token(Method::GET, "/api/v1/users/me", &pair.access_token)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_value(response).await;
    assert_eq!(body["email"], "alice@example.com");
    assert!(body.get("created_at").is_some());
    assert!(body.get("password_hash").is_none());

    app.cleanup().await;
}

#[tokio::test]
async fn test_get_me_without_token_returns_401() {
    let app = TestApp::new().await;

    let response = app.request(Method::GET, "/api/v1/users/me").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_value(response).await;
    assert_eq!(body["code"], "UNAUTHORIZED");

    app.cleanup().await;
}

#[tokio::test]
async fn test_get_me_with_expired_token_returns_401() {
    let app = TestApp::new().await;
    let expired_token = expired_access_token(Uuid::now_v7());

    let response = app
        .request_with_token(Method::GET, "/api/v1/users/me", &expired_token)
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_value(response).await;
    assert_eq!(body["code"], "UNAUTHORIZED");

    app.cleanup().await;
}

#[tokio::test]
async fn test_get_me_with_invalid_token_returns_401() {
    let app = TestApp::new().await;

    let response = app
        .request_with_token(Method::GET, "/api/v1/users/me", "not-a-jwt")
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_value(response).await;
    assert_eq!(body["code"], "UNAUTHORIZED");

    app.cleanup().await;
}

#[tokio::test]
async fn test_public_routes_remain_accessible_without_token() {
    let app = TestApp::new().await;

    let health_response = app.request(Method::GET, "/health").await;
    assert_eq!(health_response.status(), StatusCode::OK);

    let ready_response = app.request(Method::GET, "/ready").await;
    assert_eq!(ready_response.status(), StatusCode::OK);

    let auth_response = app
        .request_json(
            Method::POST,
            "/api/v1/auth/login",
            json!({ "email": "invalid", "password": "short" }),
        )
        .await;
    assert_eq!(auth_response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    app.cleanup().await;
}
