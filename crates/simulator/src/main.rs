mod prices;
mod trader;
// Moves the demo market prices and adds buy and sell orders once a second.
// It only replaces simulator orders, never user orders.

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
    let _telemetry =
        exchange_config::telemetry::init(if env::args().nth(1).as_deref() == Some("trader") {
            "exchange-trader"
        } else {
            "exchange-market-simulator"
        })?;
    let metrics = PrometheusBuilder::new().install_recorder()?;
    let metrics_bind = env::var("SIMULATOR_METRICS_BIND").unwrap_or_else(|_| "0.0.0.0:3002".into());
    tokio::spawn(async move {
        if let Err(error) = serve_metrics(metrics, metrics_bind).await {
            warn!(%error, "simulator metrics server stopped");
        }
    });
    if env::args().nth(1).as_deref() == Some("trader") {
        return trader::run().await;
    }
    // `?` returns an error from this function if loading the URL or connecting fails.
    let db = PgPool::connect_with(exchange_config::database::connection_options()?).await?;
    let seed_text = env::var("SIMULATION_SEED").unwrap_or_else(|_| "42".to_string());
    let seed = seed_text.parse().unwrap_or(42);
    let mut rng = StdRng::seed_from_u64(seed);
    let source = env::var("PRICE_SOURCE").unwrap_or_else(|_| "simulated".into());
    anyhow::ensure!(
        ["simulated", "coinbase"].contains(&source.as_str()),
        "unsupported PRICE_SOURCE"
    );
    let live = if source == "coinbase" {
        Some(prices::client()?)
    } else {
        None
    };
    bootstrap_market_maker(&db).await?;
    if live.is_some() {
        metrics::gauge!("market_price_feed_last_success_timestamp_seconds").set(0.0);
        sqlx::query("UPDATE market_state SET price_source='coinbase',updated_at=LEAST(updated_at,now()-interval '31 seconds'),source_updated_at=NULL").execute(&db).await?;
        sqlx::query("UPDATE orders SET status='cancelled' WHERE is_system=true AND status IN ('open','partially_filled')").execute(&db).await?;
    }
    loop {
        if let Err(error) = tick(&db, &mut rng, live.as_ref()).await {
            metrics::counter!("market_simulator_errors_total").increment(1);
            warn!(%error, "simulation tick failed");
            metrics::counter!("market_price_feed_errors_total").increment(1);
            if live.is_some() {
                let _=sqlx::query("UPDATE orders SET status='cancelled' WHERE is_system=true AND status IN ('open','partially_filled') AND instrument IN (SELECT instrument FROM market_state WHERE updated_at < now()-interval '30 seconds')").execute(&db).await;
            }
        }
        sleep(Duration::from_secs(if live.is_some() { 5 } else { 1 })).await;
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

async fn tick(db: &PgPool, rng: &mut StdRng, live: Option<&reqwest::Client>) -> anyhow::Result<()> {
    let states =
        sqlx::query("SELECT instrument,reference_price FROM market_state ORDER BY instrument")
            .fetch_all(db)
            .await?;
    let mut quotes = Vec::new();
    for state in states {
        let symbol: String = state.get("instrument");
        let current_price: Decimal = state.get("reference_price");
        let (next_price, change, source_time) = if let Some(client) = live {
            let quote = prices::fetch(client, &symbol).await?;
            (quote.0, quote.1, Some(quote.2))
        } else {
            let change = Decimal::new(rng.random_range(-35_i64..=35), 4);
            (
                (current_price * (Decimal::ONE + change))
                    .round_dp(2)
                    .max(Decimal::new(1, 2)),
                change * Decimal::from(100),
                None,
            )
        };
        quotes.push((symbol, next_price, change, source_time));
    }
    // Publish all markets, liquidity and transition baselines atomically.
    let mut tx = db.begin().await?;
    for (symbol, next_price, change, source_time) in quotes {
        metrics::counter!("market_simulator_updates_total", "instrument" => symbol.clone())
            .increment(1);
        metrics::gauge!("market_reference_price", "instrument" => symbol.clone())
            .set(next_price.to_f64().unwrap_or_default());
        sqlx::query(
            "UPDATE market_state
             SET reference_price=$1,
                 change_24h=$3, price_source=$4, source_updated_at=$5,
                 updated_at=now()
             WHERE instrument=$2",
        )
        .bind(next_price)
        .bind(&symbol)
        .bind(change)
        .bind(if live.is_some() {
            "coinbase"
        } else {
            "simulated"
        })
        .bind(source_time)
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
        info!(instrument=%symbol,price=%next_price,"market updated");
    }
    if live.is_some() {
        // Avoid counting the switch from invented prices to live prices as profit.
        sqlx::query("WITH totals AS (
            SELECT a.user_id,SUM((a.available+a.reserved)*CASE WHEN a.currency='USD' THEN 1 ELSE s.reference_price END) AS equity
            FROM accounts a LEFT JOIN instruments i ON i.base_currency=a.currency AND i.quote_currency='USD'
            LEFT JOIN market_state s ON s.instrument=i.symbol GROUP BY a.user_id
        ) UPDATE users u SET initial_equity_usd=t.equity,profit_tracking_started_at=now(),profit_baseline_source='coinbase'
          FROM totals t WHERE t.user_id=u.id AND u.profit_baseline_source='simulated'
          AND u.id <> '00000000-0000-0000-0000-000000000001'::uuid")
            .execute(&mut *tx).await?;
        metrics::gauge!("market_price_feed_last_success_timestamp_seconds")
            .set(chrono::Utc::now().timestamp() as f64);
    }
    tx.commit().await?;
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
