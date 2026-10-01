//! Moves the demo market prices and adds buy and sell orders once a second.
//! It only replaces simulator orders, never user orders.

use axum::{Router, extract::State, routing::get};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use rand::{Rng, SeedableRng, rngs::StdRng};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use sqlx::{PgPool, Row};
use std::env;
use tokio::time::{Duration, sleep};
use tracing::{info, warn};
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().json().init();
    let metrics = PrometheusBuilder::new().install_recorder()?;
    let metrics_bind = env::var("SIMULATOR_METRICS_BIND").unwrap_or_else(|_| "0.0.0.0:3002".into());
    tokio::spawn(async move {
        if let Err(error) = serve_metrics(metrics, metrics_bind).await {
            warn!(%error, "simulator metrics server stopped");
        }
    });
    // `?` returns an error from this function if loading the URL or connecting fails.
    let database_url = env::var("DATABASE_URL")?;
    let db = PgPool::connect(&database_url).await?;
    let seed_text = env::var("SIMULATION_SEED").unwrap_or_else(|_| "42".to_string());
    let seed = seed_text.parse().unwrap_or(42);
    let mut rng = StdRng::seed_from_u64(seed);
    bootstrap_market_maker(&db).await?;
    loop {
        if let Err(error) = tick(&db, &mut rng).await {
            metrics::counter!("market_simulator_errors_total").increment(1);
            warn!(%error, "simulation tick failed");
        }
        sleep(Duration::from_secs(1)).await;
    }
}

async fn bootstrap_market_maker(db: &PgPool) -> anyhow::Result<()> {
    // A transaction saves all these changes together, or none of them.
    let mut tx = db.begin().await?;
    let id = Uuid::parse_str("00000000-0000-0000-0000-000000000001")?;
    sqlx::query(
        "INSERT INTO users(id,email,password_hash)
         VALUES($1,'market-maker@exchange.internal','disabled')
         ON CONFLICT(email) DO NOTHING",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    for (currency, amount) in [
        ("USD", Decimal::new(1_000_000_000, 0)),
        ("BTC", Decimal::new(100_000, 0)),
        ("ETH", Decimal::new(1_000_000, 0)),
        ("SOL", Decimal::new(10_000_000, 0)),
    ] {
        sqlx::query(
            "INSERT INTO accounts(user_id,currency,available) VALUES($1,$2,$3)
             ON CONFLICT(user_id,currency) DO NOTHING",
        )
        .bind(id)
        .bind(currency)
        .bind(amount)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn tick(db: &PgPool, rng: &mut StdRng) -> anyhow::Result<()> {
    let states =
        sqlx::query("SELECT instrument,reference_price FROM market_state ORDER BY instrument")
            .fetch_all(db)
            .await?;
    for state in states {
        let symbol: String = state.get("instrument");
        let current_price: Decimal = state.get("reference_price");
        let price_change = Decimal::new(rng.random_range(-35_i64..=35), 4);
        let next_price = (current_price * (Decimal::ONE + price_change))
            .round_dp(2)
            .max(Decimal::new(1, 2));
        metrics::counter!("market_simulator_updates_total", "instrument" => symbol.clone())
            .increment(1);
        metrics::gauge!("market_reference_price", "instrument" => symbol.clone())
            .set(next_price.to_f64().unwrap_or_default());
        let mut tx = db.begin().await?;
        sqlx::query(
            "UPDATE market_state
             SET reference_price=$1,
                 change_24h=(($1-reference_price)/reference_price)*100,
                 updated_at=now()
             WHERE instrument=$2",
        )
        .bind(next_price)
        .bind(&symbol)
        .execute(&mut *tx)
        .await?;
        // Replace only stale simulator liquidity. User orders are never touched.
        sqlx::query(
            "UPDATE orders SET status='cancelled'
             WHERE instrument=$1 AND is_system=true
               AND status IN ('open','partially_filled')",
        )
        .bind(&symbol)
        .execute(&mut *tx)
        .await?;
        let base = symbol.split('-').next().unwrap();
        for (side, price, quantity) in [
            ("buy", next_price * Decimal::new(998, 3), Decimal::new(2, 2)),
            ("buy", next_price * Decimal::new(995, 3), Decimal::new(5, 2)),
            (
                "sell",
                next_price * Decimal::new(1002, 3),
                Decimal::new(2, 2),
            ),
            (
                "sell",
                next_price * Decimal::new(1005, 3),
                Decimal::new(5, 2),
            ),
        ] {
            let id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO orders(
                    id,user_id,client_order_id,instrument,side,order_type,
                    quantity,remaining,limit_price,status,is_system
                 ) VALUES(
                    $1,'00000000-0000-0000-0000-000000000001',
                    $2,$3,$4,'limit',$5,$5,$6,'open',true
                 )",
            )
            .bind(id)
            .bind(format!("sim-{symbol}-{id}"))
            .bind(&symbol)
            .bind(side)
            .bind(quantity)
            .bind(price.round_dp(2))
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO outbox_events(kind,aggregate_id,payload) VALUES('market.ticker',$1,$2)",
        )
        .bind(Uuid::nil())
        .bind(serde_json::json!({"instrument": symbol,"price": next_price,"base": base}))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        info!(instrument=%symbol,price=%next_price,"market updated");
    }
    Ok(())
}

async fn serve_metrics(handle: PrometheusHandle, bind: String) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/metrics", get(render_metrics))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(handle);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn render_metrics(State(handle): State<PrometheusHandle>) -> String {
    handle.render()
}
