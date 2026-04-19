mod support;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use support::{TestApp, json_value};

#[tokio::test]
async fn test_register_returns_201_and_token_pair() {
    let app = TestApp::new().await;

    let response = app.register_user("alice@example.com", "password123").await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_value(response).await;
    assert!(body.get("access_token").is_some());
    assert!(body.get("refresh_token").is_some());

    app.cleanup().await;
}

#[tokio::test]
async fn test_register_duplicate_email_returns_409() {
    let app = TestApp::new().await;
    app.register_user_ok("alice@example.com", "password123")
        .await;

    let response = app.register_user("alice@example.com", "password123").await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_value(response).await;
    assert_eq!(body["code"], "EMAIL_ALREADY_EXISTS");

    app.cleanup().await;
}

#[tokio::test]
async fn test_register_invalid_payload_returns_422() {
    let app = TestApp::new().await;

    let response = app
        .request_json(
            Method::POST,
            "/api/v1/auth/register",
            json!({ "email": "not-an-email", "password": "short" }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_value(response).await;
    assert_eq!(body["code"], "VALIDATION_ERROR");

    app.cleanup().await;
}

#[tokio::test]
async fn test_login_with_correct_credentials_returns_200() {
    let app = TestApp::new().await;
    app.register_user_ok("alice@example.com", "password123")
        .await;

    let response = app.login_user("alice@example.com", "password123").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_value(response).await;
    assert!(body.get("access_token").is_some());
    assert!(body.get("refresh_token").is_some());

    app.cleanup().await;
}

#[tokio::test]
async fn test_login_with_wrong_password_returns_401() {
    let app = TestApp::new().await;
    app.register_user_ok("alice@example.com", "password123")
        .await;

    let response = app.login_user("alice@example.com", "wrongpass").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_value(response).await;
    assert_eq!(body["code"], "INVALID_CREDENTIALS");

    app.cleanup().await;
}

#[tokio::test]
async fn test_refresh_with_valid_token_rotates_token_pair() {
    let app = TestApp::new().await;
    let pair = app
        .register_user_ok("alice@example.com", "password123")
        .await;

    let response = app
        .request_json(
            Method::POST,
            "/api/v1/auth/refresh",
            json!({ "refresh_token": pair.refresh_token }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = json_value(response).await;
    assert_ne!(body["refresh_token"], json!(pair.refresh_token));

    app.cleanup().await;
}

#[tokio::test]
async fn test_refresh_with_rotated_token_revokes_all_sessions() {
    let app = TestApp::new().await;
    let pair1 = app
        .register_user_ok("alice@example.com", "password123")
        .await;

    let response = app
        .request_json(
            Method::POST,
            "/api/v1/auth/refresh",
            json!({ "refresh_token": pair1.refresh_token }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let pair2: Value = json_value(response).await;

    let revoked_response = app
        .request_json(
            Method::POST,
            "/api/v1/auth/refresh",
            json!({ "refresh_token": pair1.refresh_token }),
        )
        .await;
    assert_eq!(revoked_response.status(), StatusCode::UNAUTHORIZED);
    let revoked_body = json_value(revoked_response).await;
    assert_eq!(revoked_body["code"], "TOKEN_REVOKED");

    let second_session_response = app
        .request_json(
            Method::POST,
            "/api/v1/auth/refresh",
            json!({ "refresh_token": pair2["refresh_token"] }),
        )
        .await;
    assert_eq!(second_session_response.status(), StatusCode::UNAUTHORIZED);
    let second_session_body = json_value(second_session_response).await;
    assert_eq!(second_session_body["code"], "TOKEN_REVOKED");

    app.cleanup().await;
}

#[tokio::test]
async fn test_change_password_revokes_sessions_and_old_password_stops_working() {
    let app = TestApp::new().await;
    let pair = app
        .register_user_ok("alice@example.com", "password123")
        .await;

    let change_response = app
        .request_json_with_token(
            Method::PUT,
            "/api/v1/auth/password",
            &pair.access_token,
            json!({
                "old_password": "password123",
                "new_password": "newpassword123"
            }),
        )
        .await;
    assert_eq!(change_response.status(), StatusCode::NO_CONTENT);

    let old_login_response = app.login_user("alice@example.com", "password123").await;
    assert_eq!(old_login_response.status(), StatusCode::UNAUTHORIZED);

    let new_login_response = app.login_user("alice@example.com", "newpassword123").await;
    assert_eq!(new_login_response.status(), StatusCode::OK);

    let refresh_response = app
        .request_json(
            Method::POST,
            "/api/v1/auth/refresh",
            json!({ "refresh_token": pair.refresh_token }),
        )
        .await;
    assert_eq!(refresh_response.status(), StatusCode::UNAUTHORIZED);
    let refresh_body = json_value(refresh_response).await;
    assert_eq!(refresh_body["code"], "TOKEN_REVOKED");

    app.cleanup().await;
}
