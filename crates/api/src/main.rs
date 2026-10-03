//! HTTP endpoints for users, orders, balances and market data.
//! Each handler reads a request, uses PostgreSQL, and returns a response.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use exchange_domain::{NewOrder, OrderType, Side};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::{env, sync::Arc, time::Instant};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;
#[derive(Clone)]
struct AppState {
    db: PgPool,
    // Arc lets request handlers share the same secret without copying it.
    jwt_secret: Arc<String>,
    metrics: PrometheusHandle,
}

#[derive(Deserialize)]
struct Credentials {
    email: String,
    password: String,
}

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

#[derive(Serialize)]
struct Token {
    access_token: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let db = PgPool::connect_with(exchange_config::database::connection_options()?).await?;
    if env::args().nth(1).as_deref() == Some("migrate") {
        sqlx::migrate!("../../migrations").run(&db).await?;
        return Ok(());
    }
    let metrics = PrometheusBuilder::new().install_recorder()?;
    let app = AppState {
        db,
        jwt_secret: Arc::new(
            env::var("JWT_SECRET")
                .unwrap_or_else(|_| "development-secret-change-me-32bytes".into()),
        ),
        metrics,
    };
    let router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readiness))
        .route("/metrics", get(render_metrics))
        .route("/v1/auth/register", post(register))
        .route("/v1/auth/login", post(login))
        .route("/v1/orders", post(place_order))
        .route("/v1/orders/{id}/cancel", post(cancel_order))
        .route("/v1/orders", get(open_orders))
        .route("/v1/accounts/balances", get(balances))
        .route("/v1/fills", get(fills))
        .route("/v1/instruments", get(instruments))
        .route("/v1/markets/{symbol}/ticker", get(ticker))
        .route("/v1/markets/{symbol}/book", get(order_book))
        .with_state(app)
        .layer(middleware::from_fn(track_request_metrics))
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(TraceLayer::new_for_http());
    let bind = env::var("API_BIND").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, router).await?;
    Ok(())
}

async fn readiness(State(app): State<AppState>) -> impl IntoResponse {
    if sqlx::query("SELECT 1").execute(&app.db).await.is_ok() {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "database unavailable")
    }
}

async fn render_metrics(State(app): State<AppState>) -> String {
    app.metrics.render()
}

async fn track_request_metrics(
    request: axum::extract::Request,
    next: middleware::Next,
) -> Response {
    let started = Instant::now();
    let method = request.method().to_string();
    let response = next.run(request).await;
    let status = response.status().as_u16().to_string();
    metrics::counter!("http_requests_total", "method" => method.clone(), "status" => status)
        .increment(1);
    metrics::histogram!("http_request_duration_seconds", "method" => method)
        .record(started.elapsed().as_secs_f64());
    response
}

fn hash_password(password: &str) -> String {
    format!("{:x}", Sha256::digest(password.as_bytes()))
}

fn authenticated_user(headers: &HeaderMap, app: &AppState) -> Result<Uuid, StatusCode> {
    let authorization = headers
        .get("authorization")
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let authorization = authorization
        .to_str()
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    let token = authorization
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let decoded = decode::<Claims>(
        token,
        &DecodingKey::from_secret(app.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;
    Uuid::parse_str(&decoded.claims.sub).map_err(|_| StatusCode::UNAUTHORIZED)
}

async fn register(
    State(app): State<AppState>,
    Json(credentials): Json<Credentials>,
) -> Result<Json<Token>, StatusCode> {
    let mut tx = app
        .db
        .begin()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email,password_hash) VALUES($1,$2,$3)")
        .bind(id)
        .bind(&credentials.email)
        .bind(hash_password(&credentials.password))
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    // New demo users start with dollars but no cryptocurrency.
    for currency in ["USD", "BTC", "ETH", "SOL"] {
        let starting_balance = if currency == "USD" {
            Decimal::new(100000, 0)
        } else {
            Decimal::ZERO
        };
        sqlx::query("INSERT INTO accounts(user_id,currency,available) VALUES($1,$2,$3)")
            .bind(id)
            .bind(currency)
            .bind(starting_balance)
            .execute(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(create_token(id, &app.jwt_secret)))
}

async fn login(
    State(app): State<AppState>,
    Json(credentials): Json<Credentials>,
) -> Result<Json<Token>, StatusCode> {
    let row = sqlx::query("SELECT id FROM users WHERE email=$1 AND password_hash=$2")
        .bind(credentials.email)
        .bind(hash_password(&credentials.password))
        .fetch_optional(&app.db)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    Ok(Json(create_token(row.get("id"), &app.jwt_secret)))
}

fn create_token(id: Uuid, secret: &str) -> Token {
    Token {
        access_token: encode(
            &Header::default(),
            &Claims {
                sub: id.to_string(),
                exp: (chrono::Utc::now().timestamp() + 86400) as usize,
            },
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap(),
    }
}

async fn place_order(
    State(app): State<AppState>,
    headers: HeaderMap,
    Json(order): Json<NewOrder>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let user_id = authenticated_user(&headers, &app)?;
    order.validate().map_err(|_| StatusCode::BAD_REQUEST)?;
    let id = Uuid::new_v4();
    let mut tx = app
        .db
        .begin()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    // Reusing a client order ID returns the existing order, without charging twice.
    let found = sqlx::query("SELECT id,status FROM orders WHERE user_id=$1 AND client_order_id=$2")
        .bind(user_id)
        .bind(&order.client_order_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(row) = found {
        return Ok((
            StatusCode::OK,
            Json(serde_json::json!({
                "id": row.get::<Uuid, _>("id"),
                "status": row.get::<String, _>("status"),
                "idempotent": true
            })),
        ));
    }
    let state = sqlx::query("SELECT reference_price FROM market_state WHERE instrument=$1")
        .bind(&order.instrument)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let reference: Decimal = state.get("reference_price");
    // Market buys can pay up to 5% above the reference price. Market sells have no floor.
    let price = if let Some(limit_price) = order.limit_price {
        limit_price
    } else if order.side == Side::Buy {
        reference * Decimal::new(105, 2)
    } else {
        Decimal::ZERO
    };
    let reserve = if order.side == Side::Buy {
        order.quantity * price
    } else {
        order.quantity
    };
    let base = order
        .instrument
        .split('-')
        .next()
        .ok_or(StatusCode::BAD_REQUEST)?;
    let quote = order
        .instrument
        .split('-')
        .nth(1)
        .ok_or(StatusCode::BAD_REQUEST)?;
    let currency = if order.side == Side::Buy { quote } else { base };
    // Move funds into the reserved balance until the order fills or is cancelled.
    let result = sqlx::query(
        "UPDATE accounts
         SET available=available-$1,reserved=reserved+$1
         WHERE user_id=$2 AND currency=$3 AND available >= $1",
    )
    .bind(reserve)
    .bind(user_id)
    .bind(currency)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() != 1 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let side = match order.side {
        Side::Buy => "buy",
        Side::Sell => "sell",
    };
    let order_type = match order.order_type {
        OrderType::Market => "market",
        OrderType::Limit => "limit",
    };
    sqlx::query(
        "INSERT INTO orders(
            id,user_id,client_order_id,instrument,side,order_type,
            quantity,remaining,limit_price,status
         )
         VALUES($1,$2,$3,$4,$5,$6,$7,$7,$8,'open')",
    )
    .bind(id)
    .bind(user_id)
    .bind(&order.client_order_id)
    .bind(&order.instrument)
    .bind(side)
    .bind(order_type)
    .bind(order.quantity)
    .bind(price)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    sqlx::query(
        "INSERT INTO outbox_events(kind,aggregate_id,payload) VALUES('order.accepted',$1,$2)",
    )
    .bind(id)
    .bind(serde_json::json!({
        "instrument": order.instrument
    }))
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    metrics::counter!(
        "orders_accepted_total",
        "instrument" => order.instrument.clone(),
        "side" => side,
        "order_type" => order_type
    )
    .increment(1);
    metrics::histogram!(
        "order_notional_usd",
        "instrument" => order.instrument.clone(),
        "side" => side,
        "order_type" => order_type
    )
    .record((order.quantity * price).to_f64().unwrap_or_default());
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": id,
            "status": "open"
        })),
    ))
}

async fn cancel_order(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user_id = authenticated_user(&headers, &app)?;
    let mut tx = app
        .db
        .begin()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let order = sqlx::query(
        "SELECT instrument,side,remaining,limit_price
         FROM orders
         WHERE id=$1 AND user_id=$2 AND status IN ('open','partially_filled')
         FOR UPDATE",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let symbol: String = order.get("instrument");
    let side: String = order.get("side");
    let remaining: Decimal = order.get("remaining");
    let price: Decimal = order.get("limit_price");
    let currency = if side == "buy" {
        symbol.split('-').nth(1).unwrap()
    } else {
        symbol.split('-').next().unwrap()
    };
    let release = if side == "buy" {
        remaining * price
    } else {
        remaining
    };
    sqlx::query("UPDATE orders SET status='cancelled' WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE accounts SET available=available+$1,reserved=reserved-$1 WHERE user_id=$2 AND currency=$3")
        .bind(release)
        .bind(user_id)
        .bind(currency)
        .execute(&mut *tx)
        .await
        .map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    metrics::counter!(
        "orders_cancelled_total",
        "instrument" => symbol,
        "side" => side
    )
    .increment(1);
    Ok(Json(serde_json::json!({
        "id": id,
        "status": "cancelled"
    })))
}

async fn open_orders(
    State(app): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let user_id = authenticated_user(&headers, &app)?;
    let rows = sqlx::query(
        "SELECT id,instrument,side,quantity,remaining,limit_price,status,created_at
         FROM orders
         WHERE user_id=$1 AND status IN ('open','partially_filled')
         ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(&app.db)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut orders = Vec::new();
    for row in rows {
        // get::<Uuid, _> reads a UUID; the _ lets Rust infer the column-name type.
        orders.push(serde_json::json!({
            "id": row.get::<Uuid, _>("id"),
            "instrument": row.get::<String, _>("instrument"),
            "side": row.get::<String, _>("side"),
            "quantity": row.get::<Decimal, _>("quantity"),
            "remaining": row.get::<Decimal, _>("remaining"),
            "limit_price": row.get::<Option<Decimal>, _>("limit_price"),
            "status": row.get::<String, _>("status"),
        }));
    }
    Ok(Json(orders))
}

async fn balances(
    State(app): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let user_id = authenticated_user(&headers, &app)?;
    let rows = sqlx::query(
        "SELECT currency,available,reserved FROM accounts WHERE user_id=$1 ORDER BY currency",
    )
    .bind(user_id)
    .fetch_all(&app.db)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut balances = Vec::new();
    for row in rows {
        balances.push(serde_json::json!({
            "currency": row.get::<String, _>("currency"),
            "available": row.get::<Decimal, _>("available"),
            "reserved": row.get::<Decimal, _>("reserved"),
        }));
    }
    Ok(Json(balances))
}

async fn fills(
    State(app): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let user_id = authenticated_user(&headers, &app)?;
    let rows = sqlx::query(
        "SELECT f.id,f.instrument,f.price,f.quantity,f.created_at
         FROM fills f
         JOIN orders o ON o.id IN (f.maker_order_id,f.taker_order_id)
         WHERE o.user_id=$1
         ORDER BY f.created_at DESC",
    )
    .bind(user_id)
    .fetch_all(&app.db)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut fills = Vec::new();
    for row in rows {
        fills.push(serde_json::json!({
            "id": row.get::<Uuid, _>("id"),
            "instrument": row.get::<String, _>("instrument"),
            "price": row.get::<Decimal, _>("price"),
            "quantity": row.get::<Decimal, _>("quantity"),
        }));
    }
    Ok(Json(fills))
}

async fn instruments(
    State(app): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let rows = sqlx::query(
        "SELECT i.symbol,i.base_currency,i.quote_currency,i.tick_size,s.reference_price,s.updated_at
         FROM instruments i
         JOIN market_state s ON s.instrument=i.symbol
         WHERE i.enabled
         ORDER BY i.symbol",
    )
        .fetch_all(&app.db)
        .await
        .map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;
    let mut instruments = Vec::new();
    for row in rows {
        instruments.push(serde_json::json!({
            "symbol": row.get::<String, _>("symbol"),
            "base": row.get::<String, _>("base_currency"),
            "quote": row.get::<String, _>("quote_currency"),
            "tick_size": row.get::<Decimal, _>("tick_size"),
            "reference_price": row.get::<Decimal, _>("reference_price"),
        }));
    }
    Ok(Json(instruments))
}

async fn ticker(
    State(app): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let row = sqlx::query("SELECT instrument,reference_price,change_24h,updated_at FROM market_state WHERE instrument=$1")
        .bind(symbol)
        .fetch_optional(&app.db)
        .await
        .map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({
        "instrument": row.get::<String, _>("instrument"),
        "price": row.get::<Decimal, _>("reference_price"),
        "change_24h":row.get::<Decimal, _>("change_24h"),
        "updated_at": row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")
    })))
}

async fn order_book(
    State(app): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let rows = sqlx::query(
        "SELECT side,limit_price,SUM(remaining) AS quantity
         FROM orders
         WHERE instrument=$1 AND status IN ('open','partially_filled')
         GROUP BY side,limit_price
         ORDER BY side,limit_price",
    )
    .bind(&symbol)
    .fetch_all(&app.db)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut bids = Vec::new();
    let mut asks = Vec::new();
    // Bids are buy orders; asks are sell orders.
    for row in rows {
        let level = serde_json::json!({
            "price": row.get::<Decimal, _>("limit_price"),
            "quantity": row.get::<Decimal, _>("quantity")
        });
        if row.get::<String, _>("side") == "buy" {
            bids.push(level)
        } else {
            asks.push(level)
        }
    }
    Ok(Json(serde_json::json!({
        "instrument": symbol,
        "bids": bids,
        "asks": asks
    })))
}
