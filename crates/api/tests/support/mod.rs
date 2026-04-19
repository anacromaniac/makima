#![allow(dead_code)]

use std::{collections::HashMap, sync::Arc};

use api::{
    AppState, MIGRATOR, assets::service::AssetTickerLookup, auth::jwt::Claims, build_app,
    build_app_state_with_lookup,
};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, Response, StatusCode, header},
};
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use testcontainers::core::IntoContainerPort;
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ContainerAsync, runners::AsyncRunner},
};
use tower::ServiceExt;
use uuid::Uuid;

const TEST_JWT_SECRET: &str = "test-jwt-secret";

#[derive(Debug, Deserialize)]
pub struct TokenPairResponse {
    pub access_token: String,
    pub refresh_token: String,
}

pub struct TestApp {
    app: Option<Router>,
    state: AppState,
    _postgres: ContainerAsync<Postgres>,
}

impl TestApp {
    pub async fn new() -> Self {
        Self::new_with_asset_lookup(Arc::new(StaticAssetTickerLookup::empty())).await
    }

    pub async fn new_with_asset_lookup(asset_ticker_lookup: Arc<dyn AssetTickerLookup>) -> Self {
        let postgres = Postgres::default()
            .start()
            .await
            .expect("failed to start PostgreSQL test container");

        let host = postgres
            .get_host()
            .await
            .expect("failed to resolve PostgreSQL container host");
        let port = postgres
            .get_host_port_ipv4(5432.tcp())
            .await
            .expect("failed to resolve PostgreSQL container port");

        let database_url = format!("postgresql://postgres:postgres@{host}:{port}/postgres");
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("failed to connect to PostgreSQL test container");

        MIGRATOR
            .run(&pool)
            .await
            .expect("failed to run migrations for test container");

        let state =
            build_app_state_with_lookup(pool, TEST_JWT_SECRET.to_string(), asset_ticker_lookup);
        let app = build_app(state.clone(), &["http://localhost:3000".to_string()]);

        Self {
            app: Some(app),
            state,
            _postgres: postgres,
        }
    }

    pub async fn request(&self, method: Method, uri: &str) -> Response<Body> {
        self.send(method, uri, None, None).await
    }

    pub async fn request_json(&self, method: Method, uri: &str, body: Value) -> Response<Body> {
        self.send(method, uri, None, Some(body)).await
    }

    pub async fn request_json_with_token(
        &self,
        method: Method,
        uri: &str,
        access_token: &str,
        body: Value,
    ) -> Response<Body> {
        self.send(method, uri, Some(access_token), Some(body)).await
    }

    pub async fn request_with_token(
        &self,
        method: Method,
        uri: &str,
        access_token: &str,
    ) -> Response<Body> {
        self.send(method, uri, Some(access_token), None).await
    }

    pub async fn register_user(&self, email: &str, password: &str) -> Response<Body> {
        self.request_json(
            Method::POST,
            "/api/v1/auth/register",
            json!({ "email": email, "password": password }),
        )
        .await
    }

    pub async fn register_user_ok(&self, email: &str, password: &str) -> TokenPairResponse {
        let response = self.register_user(email, password).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        json_body(response).await
    }

    pub async fn login_user(&self, email: &str, password: &str) -> Response<Body> {
        self.request_json(
            Method::POST,
            "/api/v1/auth/login",
            json!({ "email": email, "password": password }),
        )
        .await
    }

    pub async fn login_user_ok(&self, email: &str, password: &str) -> TokenPairResponse {
        let response = self.login_user(email, password).await;
        assert_eq!(response.status(), StatusCode::OK);
        json_body(response).await
    }

    pub async fn create_portfolio(
        &self,
        access_token: &str,
        name: &str,
        description: Option<&str>,
    ) -> Response<Body> {
        self.request_json_with_token(
            Method::POST,
            "/api/v1/portfolios",
            access_token,
            json!({ "name": name, "description": description }),
        )
        .await
    }

    pub async fn create_asset(&self, access_token: &str, body: Value) -> Response<Body> {
        self.request_json_with_token(Method::POST, "/api/v1/assets", access_token, body)
            .await
    }

    pub async fn cleanup(mut self) {
        self.app.take();
        self.state.pool.close().await;
    }

    async fn send(
        &self,
        method: Method,
        uri: &str,
        access_token: Option<&str>,
        body: Option<Value>,
    ) -> Response<Body> {
        let mut builder = Request::builder().method(method).uri(uri);

        if let Some(token) = access_token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }

        let request = if let Some(body) = body {
            builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("failed to build request")
        } else {
            builder
                .body(Body::empty())
                .expect("failed to build request")
        };

        self.app
            .as_ref()
            .expect("test app already cleaned up")
            .clone()
            .oneshot(request)
            .await
            .expect("request execution failed")
    }
}

pub struct StaticAssetTickerLookup {
    by_isin: HashMap<String, Option<String>>,
}

impl StaticAssetTickerLookup {
    pub fn empty() -> Self {
        Self {
            by_isin: HashMap::new(),
        }
    }

    pub fn with_mapping(
        entries: impl IntoIterator<Item = (impl Into<String>, Option<&'static str>)>,
    ) -> Self {
        let by_isin = entries
            .into_iter()
            .map(|(isin, ticker)| (isin.into(), ticker.map(str::to_string)))
            .collect();

        Self { by_isin }
    }
}

#[async_trait::async_trait]
impl AssetTickerLookup for StaticAssetTickerLookup {
    async fn lookup_yahoo_ticker(&self, isin: &str) -> Option<String> {
        self.by_isin.get(isin).cloned().flatten()
    }
}

pub async fn json_body<T: DeserializeOwned>(response: Response<Body>) -> T {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("failed to read response body");
    serde_json::from_slice(&bytes).expect("failed to deserialize JSON response")
}

pub async fn json_value(response: Response<Body>) -> Value {
    json_body(response).await
}

pub fn expired_access_token(user_id: Uuid) -> String {
    let claims = Claims {
        sub: user_id,
        exp: (Utc::now() - Duration::hours(2)).timestamp(),
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(TEST_JWT_SECRET.as_bytes()),
    )
    .expect("failed to encode expired JWT")
}
