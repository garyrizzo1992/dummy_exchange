//! Limits for the publicly accessible API and password hashing.
use super::{AppState, Credentials};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex, Semaphore};
#[derive(Clone)]
pub struct Controls {
    buckets: Arc<Mutex<HashMap<String, (Instant, u32)>>>,
    hashing: Arc<Semaphore>,
    trust_cloudflare: bool,
}
impl Controls {
    pub fn new() -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
            hashing: Arc::new(Semaphore::new(4)),
            trust_cloudflare: std::env::var("TRUST_CLOUDFLARE").as_deref() == Ok("true"),
        }
    }
    async fn admit(&self, key: String, limit: u32) -> bool {
        let mut buckets = self.buckets.lock().await;
        let now = Instant::now();
        buckets.retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(60));
        if buckets.len() >= 4096 && !buckets.contains_key(&key) {
            return false;
        }
        let bucket = buckets.entry(key).or_insert((now, 0));
        if bucket.1 >= limit {
            return false;
        }
        bucket.1 += 1;
        true
    }
}
pub fn jwt_secret() -> anyhow::Result<String> {
    let secret =
        std::env::var("JWT_SECRET").map_err(|_| anyhow::anyhow!("JWT_SECRET is required"))?;
    anyhow::ensure!(
        secret.len() >= 32 && !secret.starts_with("development-secret"),
        "JWT_SECRET must contain at least 32 bytes and must not be the development secret"
    );
    Ok(secret)
}
pub fn validate_credentials(
    credentials: &Credentials,
    registering: bool,
) -> Result<(), StatusCode> {
    if credentials.email.len() > 254
        || !credentials.email.contains('@')
        || credentials.email.contains(char::is_whitespace)
        || credentials.password.len() > 128
        || credentials.password.is_empty()
        || (registering && credentials.password.len() < 12)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}
pub async fn password_hash(controls: &Controls, password: String) -> Result<String, StatusCode> {
    let permit = controls
        .hashing
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        Argon2::default()
            .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
            .map(|hash| hash.to_string())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}
pub async fn verify_password(
    controls: &Controls,
    password: String,
    stored: Option<String>,
) -> Result<bool, StatusCode> {
    let permit = controls
        .hashing
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if let Some(hash) = stored {
            if hash.starts_with("$argon2id$") {
                return PasswordHash::new(&hash)
                    .map(|parsed| {
                        Argon2::default()
                            .verify_password(password.as_bytes(), &parsed)
                            .is_ok()
                    })
                    .unwrap_or(false);
            }
            if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                let candidate = format!("{:x}", Sha256::digest(password.as_bytes()));
                return bool::from(candidate.as_bytes().ct_eq(hash.as_bytes()));
            }
        }
        // Missing and disabled accounts still pay the password hashing cost.
        let _ =
            Argon2::default().hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng));
        false
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
pub async fn protect(State(app): State<AppState>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    if path.starts_with("/v1/") {
        let peer = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|p| p.0.ip().to_string())
            .unwrap_or_else(|| "unknown".into());
        let ip = if app.security.trust_cloudflare {
            request
                .headers()
                .get("cf-connecting-ip")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<std::net::IpAddr>().ok())
                .map(|v| v.to_string())
                .unwrap_or(peer)
        } else {
            peer
        };
        let (group, limit) = if path == "/v1/auth/register" {
            ("register", 5)
        } else if path == "/v1/auth/login" {
            ("login", 10)
        } else {
            ("api", 600)
        };
        if !app.security.admit(format!("{group}:{ip}"), limit).await {
            let mut response = StatusCode::TOO_MANY_REQUESTS.into_response();
            response
                .headers_mut()
                .insert("retry-after", HeaderValue::from_static("60"));
            return response;
        }
    }
    let mut response = match tokio::time::timeout(Duration::from_secs(10), next.run(request)).await
    {
        Ok(response) => response,
        Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
    };
    for (name, value) in [
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "no-referrer"),
        ("cache-control", "no-store"),
        (
            "content-security-policy",
            "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
        ),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn argon_passwords_are_salted_and_verified() {
        let controls = Controls::new();
        let first = password_hash(&controls, "long-test-password".into())
            .await
            .unwrap();
        let second = password_hash(&controls, "long-test-password".into())
            .await
            .unwrap();
        assert_ne!(first, second);
        assert!(
            verify_password(&controls, "long-test-password".into(), Some(first.clone()))
                .await
                .unwrap()
        );
        assert!(
            !verify_password(&controls, "wrong".into(), Some(first))
                .await
                .unwrap()
        );
        let old = format!("{:x}", Sha256::digest(b"old-password"));
        assert!(
            verify_password(&controls, "old-password".into(), Some(old))
                .await
                .unwrap()
        );
        assert!(
            !verify_password(&controls, "anything".into(), Some("disabled".into()))
                .await
                .unwrap()
        );
    }
    #[tokio::test]
    async fn auth_limits_reject_and_isolate_clients() {
        let controls = Controls::new();
        for _ in 0..5 {
            assert!(controls.admit("register:one".into(), 5).await);
        }
        assert!(!controls.admit("register:one".into(), 5).await);
        assert!(controls.admit("register:two".into(), 5).await);
    }
}
