//! Explicit frontend origins; bearer authentication does not require cookies.
use axum::http::{HeaderValue, Method, Uri, header};
use std::time::Duration;
use tower_http::cors::CorsLayer;

pub fn layer(config: &str) -> anyhow::Result<CorsLayer> {
    let mut origins = Vec::new();
    for origin in config.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let uri: Uri = origin.parse()?;
        anyhow::ensure!(
            matches!(uri.scheme_str(), Some("http" | "https"))
                && uri.authority().is_some()
                && !origin.ends_with('/')
                && !origin.contains('@')
                && uri.path() == "/"
                && uri.query().is_none(),
            "CORS_ALLOWED_ORIGINS must contain HTTP(S) origins without paths or credentials"
        );
        origins.push(origin.parse::<HeaderValue>()?);
    }
    Ok(CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .max_age(Duration::from_secs(600)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::get,
    };
    use tower::ServiceExt;

    fn app() -> Router {
        Router::new()
            .route("/v1/orders", get(|| async { StatusCode::UNAUTHORIZED }))
            .layer(layer("https://exchange.garyrizzo.dev").unwrap())
    }

    #[tokio::test]
    async fn bearer_json_preflight_is_handled_before_the_endpoint() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/v1/orders")
                    .header(header::ORIGIN, "https://exchange.garyrizzo.dev")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .header(
                        header::ACCESS_CONTROL_REQUEST_HEADERS,
                        "authorization,content-type",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_success());
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "https://exchange.garyrizzo.dev"
        );
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_HEADERS],
            "authorization,content-type"
        );
    }

    #[tokio::test]
    async fn authentication_errors_are_readable_by_the_allowed_frontend() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/v1/orders")
                    .header(header::ORIGIN, "https://exchange.garyrizzo.dev")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(
            response
                .headers()
                .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        );
        assert!(
            !response
                .headers()
                .contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
        );
    }

    #[tokio::test]
    async fn unrelated_origins_get_no_cors_permission() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/v1/orders")
                    .header(header::ORIGIN, "https://unrelated.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            !response
                .headers()
                .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        );
    }

    #[test]
    fn rejects_wildcards_paths_and_credentials() {
        for origin in [
            "*",
            "https://example.com/path",
            "https://example.com/",
            "https://user@example.com",
            "https://example.com?q=x",
        ] {
            assert!(layer(origin).is_err(), "{origin}");
        }
        assert!(layer("").is_ok());
    }
}
