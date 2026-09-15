use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use exchange_domain::{NewOrder, OrderType, Side};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use metrics_exporter_prometheus::PrometheusBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::{env, sync::Arc};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;
#[derive(Clone)]
struct App {
    db: PgPool,
    jwt: Arc<String>,
    metrics: Arc<String>,
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
#[derive(Deserialize)]
struct OrderRequest {
    client_order_id: String,
    instrument: String,
    side: Side,
    order_type: OrderType,
    quantity: rust_decimal::Decimal,
    limit_price: Option<rust_decimal::Decimal>,
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let db = PgPool::connect(&env::var("DATABASE_URL")?).await?;
    sqlx::migrate!("../../migrations").run(&db).await?;
    let metrics = PrometheusBuilder::new().install_recorder()?.render();
    let app = App {
        db,
        jwt: Arc::new(
            env::var("JWT_SECRET")
                .unwrap_or_else(|_| "development-secret-change-me-32bytes".into()),
        ),
        metrics: Arc::new(metrics),
    };
    let router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .route("/metrics", get(metric))
        .route("/v1/auth/register", post(register))
        .route("/v1/auth/login", post(login))
        .route("/v1/orders", post(place_order))
        .route("/v1/orders/{id}/cancel", post(cancel))
        .route("/v1/orders", get(open_orders))
        .route("/v1/accounts/balances", get(balances))
        .route("/v1/fills", get(fills))
        .route("/v1/instruments", get(instruments))
        .route("/v1/markets/{symbol}/ticker", get(ticker))
        .route("/v1/markets/{symbol}/book", get(order_book))
        .with_state(app)
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(TraceLayer::new_for_http());
    let bind = env::var("API_BIND").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, router).await?;
    Ok(())
}
async fn ready(State(a): State<App>) -> impl IntoResponse {
    if sqlx::query("SELECT 1").execute(&a.db).await.is_ok() {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "database unavailable")
    }
}
async fn metric(State(a): State<App>) -> String {
    (*a.metrics).clone()
}
fn hash(p: &str) -> String {
    format!("{:x}", Sha256::digest(p.as_bytes()))
}
fn user(headers: &HeaderMap, a: &App) -> Result<Uuid, StatusCode> {
    let value = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let c = decode::<Claims>(
        value,
        &DecodingKey::from_secret(a.jwt.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;
    Uuid::parse_str(&c.claims.sub).map_err(|_| StatusCode::UNAUTHORIZED)
}
async fn register(
    State(a): State<App>,
    Json(c): Json<Credentials>,
) -> Result<Json<Token>, StatusCode> {
    let mut tx =
        a.db.begin()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email,password_hash) VALUES($1,$2,$3)")
        .bind(id)
        .bind(&c.email)
        .bind(hash(&c.password))
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    for cur in ["USD", "BTC", "ETH", "SOL"] {
        sqlx::query("INSERT INTO accounts(user_id,currency,available) VALUES($1,$2,$3)")
            .bind(id)
            .bind(cur)
            .bind(if cur == "USD" {
                rust_decimal::Decimal::new(100000, 0)
            } else {
                rust_decimal::Decimal::ZERO
            })
            .execute(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(token(id, &a.jwt)))
}
async fn login(
    State(a): State<App>,
    Json(c): Json<Credentials>,
) -> Result<Json<Token>, StatusCode> {
    let row = sqlx::query("SELECT id FROM users WHERE email=$1 AND password_hash=$2")
        .bind(c.email)
        .bind(hash(&c.password))
        .fetch_optional(&a.db)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    Ok(Json(token(row.get("id"), &a.jwt)))
}
fn token(id: Uuid, secret: &str) -> Token {
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
    State(a): State<App>,
    headers: HeaderMap,
    Json(r): Json<OrderRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let uid = user(&headers, &a)?;
    let o = NewOrder {
        client_order_id: r.client_order_id,
        instrument: r.instrument,
        side: r.side,
        order_type: r.order_type,
        quantity: r.quantity,
        limit_price: r.limit_price,
    };
    o.validate().map_err(|_| StatusCode::BAD_REQUEST)?;
    let id = Uuid::new_v4();
    let mut tx =
        a.db.begin()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let found = sqlx::query("SELECT id,status FROM orders WHERE user_id=$1 AND client_order_id=$2")
        .bind(uid)
        .bind(&o.client_order_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(row) = found {
        return Ok((
            StatusCode::OK,
            Json(
                serde_json::json!({"id":row.get::<Uuid,_>("id"),"status":row.get::<String,_>("status"),"idempotent":true}),
            ),
        ));
    }
    let state = sqlx::query("SELECT reference_price FROM market_state WHERE instrument=$1")
        .bind(&o.instrument)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let reference: rust_decimal::Decimal = state.get("reference_price");
    // A market order persists a protective execution price so matching can remain deterministic.
    let price = o.limit_price.unwrap_or_else(|| {
        if o.side == Side::Buy {
            reference * rust_decimal::Decimal::new(105, 2)
        } else {
            rust_decimal::Decimal::ZERO
        }
    });
    let reserve = if o.side == Side::Buy {
        o.quantity * price
    } else {
        o.quantity
    };
    let base = o
        .instrument
        .split('-')
        .next()
        .ok_or(StatusCode::BAD_REQUEST)?;
    let quote = o
        .instrument
        .split('-')
        .nth(1)
        .ok_or(StatusCode::BAD_REQUEST)?;
    let currency = if o.side == Side::Buy { quote } else { base };
    let result=sqlx::query("UPDATE accounts SET available=available-$1,reserved=reserved+$1 WHERE user_id=$2 AND currency=$3 AND available >= $1").bind(reserve).bind(uid).bind(currency).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() != 1 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    sqlx::query("INSERT INTO orders(id,user_id,client_order_id,instrument,side,order_type,quantity,remaining,limit_price,status) VALUES($1,$2,$3,$4,$5,$6,$7,$7,$8,'open')").bind(id).bind(uid).bind(&o.client_order_id).bind(&o.instrument).bind(format!("{:?}",o.side).to_lowercase()).bind(format!("{:?}",o.order_type).to_lowercase()).bind(o.quantity).bind(price).execute(&mut *tx).await.map_err(|_|StatusCode::BAD_REQUEST)?;
    sqlx::query(
        "INSERT INTO outbox_events(kind,aggregate_id,payload) VALUES('order.accepted',$1,$2)",
    )
    .bind(id)
    .bind(serde_json::json!({"instrument":o.instrument}))
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    metrics::counter!("orders_accepted_total").increment(1);
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"id":id,"status":"open"})),
    ))
}
async fn cancel(
    State(a): State<App>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let u = user(&headers, &a)?;
    let mut tx =
        a.db.begin()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let order=sqlx::query("SELECT instrument,side,remaining,limit_price FROM orders WHERE id=$1 AND user_id=$2 AND status IN ('open','partially_filled') FOR UPDATE").bind(id).bind(u).fetch_optional(&mut *tx).await.map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?.ok_or(StatusCode::NOT_FOUND)?;
    let symbol: String = order.get("instrument");
    let side: String = order.get("side");
    let remaining: rust_decimal::Decimal = order.get("remaining");
    let price: rust_decimal::Decimal = order.get("limit_price");
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
    sqlx::query("UPDATE accounts SET available=available+$1,reserved=reserved-$1 WHERE user_id=$2 AND currency=$3").bind(release).bind(u).bind(currency).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"id":id,"status":"cancelled"})))
}
async fn open_orders(
    State(a): State<App>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let u = user(&headers, &a)?;
    let rows=sqlx::query("SELECT id,instrument,side,quantity,remaining,limit_price,status,created_at FROM orders WHERE user_id=$1 AND status IN ('open','partially_filled') ORDER BY created_at DESC").bind(u).fetch_all(&a.db).await.map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(rows.into_iter().map(|r|serde_json::json!({"id":r.get::<Uuid,_>("id"),"instrument":r.get::<String,_>("instrument"),"side":r.get::<String,_>("side"),"quantity":r.get::<rust_decimal::Decimal,_>("quantity"),"remaining":r.get::<rust_decimal::Decimal,_>("remaining"),"limit_price":r.get::<Option<rust_decimal::Decimal>,_>("limit_price"),"status":r.get::<String,_>("status")})).collect()))
}
async fn balances(
    State(a): State<App>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let u = user(&headers, &a)?;
    let rows = sqlx::query(
        "SELECT currency,available,reserved FROM accounts WHERE user_id=$1 ORDER BY currency",
    )
    .bind(u)
    .fetch_all(&a.db)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(rows.into_iter().map(|r| serde_json::json!({
        "currency":r.get::<String,_>("currency"), "available":r.get::<rust_decimal::Decimal,_>("available"), "reserved":r.get::<rust_decimal::Decimal,_>("reserved")
    })).collect()))
}
async fn fills(
    State(a): State<App>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let u = user(&headers, &a)?;
    let rows=sqlx::query("SELECT f.id,f.instrument,f.price,f.quantity,f.created_at FROM fills f JOIN orders o ON o.id IN (f.maker_order_id,f.taker_order_id) WHERE o.user_id=$1 ORDER BY f.created_at DESC").bind(u).fetch_all(&a.db).await.map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(rows.into_iter().map(|r|serde_json::json!({"id":r.get::<Uuid,_>("id"),"instrument":r.get::<String,_>("instrument"),"price":r.get::<rust_decimal::Decimal,_>("price"),"quantity":r.get::<rust_decimal::Decimal,_>("quantity")})).collect()))
}
async fn instruments(State(a): State<App>) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let rows=sqlx::query("SELECT i.symbol,i.base_currency,i.quote_currency,i.tick_size,s.reference_price,s.updated_at FROM instruments i JOIN market_state s ON s.instrument=i.symbol WHERE i.enabled ORDER BY i.symbol").fetch_all(&a.db).await.map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(rows.into_iter().map(|r|serde_json::json!({"symbol":r.get::<String,_>("symbol"),"base":r.get::<String,_>("base_currency"),"quote":r.get::<String,_>("quote_currency"),"tick_size":r.get::<rust_decimal::Decimal,_>("tick_size"),"reference_price":r.get::<rust_decimal::Decimal,_>("reference_price")})).collect()))
}
async fn ticker(
    State(a): State<App>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let row=sqlx::query("SELECT instrument,reference_price,change_24h,updated_at FROM market_state WHERE instrument=$1").bind(symbol).fetch_optional(&a.db).await.map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?.ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(
        serde_json::json!({"instrument":row.get::<String,_>("instrument"),"price":row.get::<rust_decimal::Decimal,_>("reference_price"),"change_24h":row.get::<rust_decimal::Decimal,_>("change_24h"),"updated_at":row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")}),
    ))
}
async fn order_book(
    State(a): State<App>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let rows=sqlx::query("SELECT side,limit_price,SUM(remaining) AS quantity FROM orders WHERE instrument=$1 AND status IN ('open','partially_filled') GROUP BY side,limit_price ORDER BY side,limit_price").bind(&symbol).fetch_all(&a.db).await.map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;
    let mut bids = Vec::new();
    let mut asks = Vec::new();
    for r in rows {
        let level = serde_json::json!({"price":r.get::<rust_decimal::Decimal,_>("limit_price"),"quantity":r.get::<rust_decimal::Decimal,_>("quantity")});
        if r.get::<String, _>("side") == "buy" {
            bids.push(level)
        } else {
            asks.push(level)
        }
    }
    Ok(Json(
        serde_json::json!({"instrument":symbol,"bids":bids,"asks":asks}),
    ))
}
