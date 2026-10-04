mod security;

// HTTP endpoints for users, orders, balances and market data.
// Each handler reads a request, uses PostgreSQL, and returns a response.

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use exchange_domain::NewOrder;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::{env, sync::Arc, time::Instant};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use tracing::Instrument;
use uuid::Uuid;
#[derive(Clone)]
struct AppState {
    db: PgPool,
    // Arc lets request handlers share the same secret without copying it.
    jwt_secret: Arc<String>,
    metrics: PrometheusHandle,
    security: security::Controls,
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
    let _telemetry = exchange_config::telemetry::init("exchange-api")?;
    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(3))
        .connect_with(exchange_config::database::connection_options()?)
        .await?;
    if env::args().nth(1).as_deref() == Some("migrate") {
        sqlx::migrate!("../../migrations").run(&db).await?;
        return Ok(());
    }
    let metrics = PrometheusBuilder::new().install_recorder()?;
    let app = AppState {
        db,
        jwt_secret: Arc::new(security::jwt_secret()?),
        security: security::Controls::new(),
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
        .route("/v1/accounts/leaderboard", get(leaderboard))
        .route("/v1/fills", get(fills))
        .route("/v1/ui", get(frontend))
        .route("/v1/ui/", get(frontend))
        .route("/v1/ui/style.css", get(frontend_css))
        .nest_service(
            "/v1/ui/pkg",
            tower_http::services::ServeDir::new(format!(
                "{}/pkg",
                env::var("FRONTEND_DIR").unwrap_or_else(|_| "crates/frontend/dist".into())
            )),
        )
        .route("/v1/simulation", get(simulation))
        .route("/v1/markets/{symbol}/trades", get(recent_trades))
        .route("/v1/instruments", get(instruments))
        .route("/v1/markets/{symbol}/ticker", get(ticker))
        .route("/v1/markets/{symbol}/book", get(order_book))
        .layer(middleware::from_fn_with_state(
            app.clone(),
            security::protect,
        ))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .with_state(app)
        .layer(middleware::from_fn(track_request_metrics))
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(TraceLayer::new_for_http());
    let bind = env::var("API_BIND").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
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
    let headers = request
        .headers()
        .iter()
        .filter_map(|(key, value)| value.to_str().ok().map(|v| (key.to_string(), v.to_owned())))
        .collect();
    let span = tracing::info_span!("http.request", method=%method, path=%request.uri().path(), trace_id=tracing::field::Empty);
    exchange_config::telemetry::set_parent(&span, &headers);
    let response = next.run(request).instrument(span).await;
    let status = response.status().as_u16().to_string();
    metrics::counter!("http_requests_total", "method" => method.clone(), "status" => status)
        .increment(1);
    metrics::histogram!("http_request_duration_seconds", "method" => method)
        .record(started.elapsed().as_secs_f64());
    response
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
    security::validate_credentials(&credentials, true)?;
    let password_hash =
        security::password_hash(&app.security, credentials.password.clone()).await?;
    let mut tx = app
        .db
        .begin()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email,password_hash) VALUES($1,$2,$3)")
        .bind(id)
        .bind(&credentials.email)
        .bind(password_hash)
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
    security::validate_credentials(&credentials, false)?;
    let row = sqlx::query("SELECT id,password_hash FROM users WHERE email=$1")
        .bind(&credentials.email)
        .fetch_optional(&app.db)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let stored = row.as_ref().map(|r| r.get::<String, _>("password_hash"));
    let valid =
        security::verify_password(&app.security, credentials.password.clone(), stored.clone())
            .await?;
    if !valid {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let row = row.ok_or(StatusCode::UNAUTHORIZED)?;
    let id: Uuid = row.get("id");
    if let Some(old) = stored.filter(|h| !h.starts_with("$argon2id$")) {
        let upgraded = security::password_hash(&app.security, credentials.password).await?;
        sqlx::query("UPDATE users SET password_hash=$1 WHERE id=$2 AND password_hash=$3")
            .bind(upgraded)
            .bind(id)
            .bind(old)
            .execute(&app.db)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    }
    Ok(Json(create_token(id, &app.jwt_secret)))
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

fn trading_status(error: exchange_trading::TradingError) -> StatusCode {
    use exchange_trading::TradingError::*;
    match error {
        Invalid => StatusCode::BAD_REQUEST,
        NotFound => StatusCode::NOT_FOUND,
        InsufficientFunds => StatusCode::UNPROCESSABLE_ENTITY,
        Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}
async fn place_order(
    State(app): State<AppState>,
    headers: HeaderMap,
    Json(order): Json<NewOrder>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let user = authenticated_user(&headers, &app)?;
    let mut connection = app
        .db
        .acquire()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let (created, value) = exchange_trading::place_order(&mut connection, user, order)
        .await
        .map_err(trading_status)?;
    Ok((
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(value),
    ))
}
async fn cancel_order(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user = authenticated_user(&headers, &app)?;
    let mut connection = app
        .db
        .acquire()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    exchange_trading::cancel_order(&mut connection, user, id)
        .await
        .map(Json)
        .map_err(trading_status)
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
         ORDER BY created_at DESC LIMIT 100",
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

#[derive(Deserialize, Default)]
struct LeaderboardPage {
    #[serde(default)]
    offset: u32,
}

async fn leaderboard(
    State(app): State<AppState>,
    Query(page): Query<LeaderboardPage>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // One statement keeps the ranking, count and prices in a consistent snapshot.
    let result = sqlx::query_scalar::<_, serde_json::Value>(
        "WITH prices AS (
            SELECT i.base_currency AS currency, s.reference_price AS price
            FROM instruments i JOIN market_state s ON s.instrument=i.symbol
            WHERE i.quote_currency='USD'
        ), totals AS (
            SELECT u.id, t.trader_key,
                   COALESCE(SUM((a.available+a.reserved) *
                     CASE WHEN a.currency='USD' THEN 1 ELSE COALESCE(p.price,0) END),0) AS equity
            FROM users u LEFT JOIN accounts a ON a.user_id=u.id
            LEFT JOIN prices p ON p.currency=a.currency
            LEFT JOIN simulated_traders t ON t.user_id=u.id
            GROUP BY u.id,t.trader_key
        ), ranked AS (
            SELECT *, ROW_NUMBER() OVER (ORDER BY equity DESC,id) AS rank
            FROM totals WHERE id <> '00000000-0000-0000-0000-000000000001'::uuid
        ), page AS (
            SELECT * FROM ranked ORDER BY rank LIMIT 101 OFFSET $1
        ) SELECT jsonb_build_object(
            'accounts', COALESCE((SELECT jsonb_agg(jsonb_build_object(
                'rank',rank,'account_id',id,'trader',trader_key,'equity_usd',equity::text
            ) ORDER BY rank) FROM (SELECT * FROM page ORDER BY rank LIMIT 100) visible),'[]'::jsonb),
            'total',(SELECT COUNT(*) FROM ranked),
            'has_more',(SELECT COUNT(*) > 100 FROM page),
            'system_liquidity',(SELECT jsonb_build_object(
                'account_id',id,'label','System liquidity','equity_usd',equity::text
            ) FROM totals WHERE id='00000000-0000-0000-0000-000000000001'::uuid)
        )",
    )
    .bind(i64::from(page.offset))
    .fetch_one(&app.db)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(result))
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
         ORDER BY f.created_at DESC LIMIT 100",
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
        "(SELECT side,limit_price,SUM(remaining) AS quantity FROM orders
         WHERE instrument=$1 AND side='buy' AND order_type='limit' AND status IN ('open','partially_filled')
         GROUP BY side,limit_price ORDER BY limit_price DESC LIMIT 20)
         UNION ALL
         (SELECT side,limit_price,SUM(remaining) AS quantity FROM orders
         WHERE instrument=$1 AND side='sell' AND order_type='limit' AND status IN ('open','partially_filled')
         GROUP BY side,limit_price ORDER BY limit_price ASC LIMIT 20)",
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

async fn frontend() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../../frontend/index.html"))
}
async fn frontend_css() -> impl IntoResponse {
    (
        [("content-type", "text/css")],
        include_str!("../../frontend/style.css"),
    )
}
async fn recent_trades(
    State(app): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let rows = sqlx::query("SELECT price,quantity,created_at FROM fills WHERE instrument=$1 ORDER BY created_at DESC LIMIT 60")
        .bind(symbol).fetch_all(&app.db).await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(rows.into_iter().map(|r| serde_json::json!({"price":r.get::<Decimal,_>("price"),
        "quantity":r.get::<Decimal,_>("quantity"),"time":r.get::<chrono::DateTime<chrono::Utc>,_>("created_at")})).collect()))
}
async fn simulation(State(app): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let row = sqlx::query("SELECT count(*) AS total,count(*) FILTER(WHERE last_seen_at > now()-interval '15 seconds') AS active FROM simulated_traders")
        .fetch_one(&app.db).await.map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(
        serde_json::json!({"traders":row.get::<i64,_>("total"),"active":row.get::<i64,_>("active")}),
    ))
}
