mod generator;
mod load;
mod prices;
mod trader;
// Moves the demo market prices and adds buy and sell orders once a second.
// It only replaces simulator orders, never user orders.

use axum::{Router, extract::State, routing::get};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use sqlx::{Connection, PgPool, Row};
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
    let service = async {
        match env::args().nth(1).as_deref() {
            Some("trader") => trader::run().await,
            Some("load-controller") => load::run().await,
            Some("generate-once") => run_market(true).await,
            _ => run_market(false).await,
        }
    };
    tokio::select! {
        result = service => result,
        _ = exchange_config::shutdown::signal() => Ok(()),
    }
}

async fn run_market(once: bool) -> anyhow::Result<()> {
    // `?` returns an error from this function if loading the URL or connecting fails.
    let db = PgPool::connect_with(exchange_config::database::connection_options()?).await?;
    let seed_text = env::var("SIMULATION_SEED").unwrap_or_else(|_| "42".to_string());
    let seed = seed_text.parse::<u64>()?;
    // Keep this session alive for the full generator lifetime. Losing it stops
    // writes; another replica can then claim ownership without duplicate ticks.
    let mut owner = db.acquire().await?;
    let locked: bool =
        sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended('market-generator',0))")
            .fetch_one(&mut *owner)
            .await?;
    anyhow::ensure!(locked, "market generator already has an active owner");
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
    let liquidity = env::var("LIQUIDITY_NOTIONAL_USD")
        .unwrap_or_else(|_| "100000".into())
        .parse::<Decimal>()?;
    anyhow::ensure!(
        liquidity > Decimal::ZERO && liquidity <= Decimal::from(1_000_000),
        "invalid liquidity notional"
    );
    bootstrap_market_maker(&db).await?;
    if live.is_some() {
        metrics::gauge!("market_price_feed_last_success_timestamp_seconds").set(0.0);
        sqlx::query("UPDATE market_state SET price_source='coinbase',updated_at=LEAST(updated_at,now()-interval '31 seconds'),source_updated_at=NULL").execute(&db).await?;
        sqlx::query("UPDATE orders SET status='cancelled' WHERE is_system=true AND status IN ('open','partially_filled')").execute(&db).await?;
    }
    loop {
        owner.ping().await?;
        let result = tick(&db, seed, live.as_ref(), liquidity).await;
        if once {
            return result;
        }
        if let Err(error) = result {
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

async fn tick(
    db: &PgPool,
    seed: u64,
    live: Option<&reqwest::Client>,
    liquidity: Decimal,
) -> anyhow::Result<()> {
    // Fetch external data only for the explicitly selected live-feed mode.
    let mut live_quotes = std::collections::HashMap::new();
    if let Some(client) = live {
        let symbols = sqlx::query_scalar::<_, String>(
            "SELECT instrument FROM market_state ORDER BY instrument",
        )
        .fetch_all(db)
        .await?;
        for symbol in symbols {
            live_quotes.insert(symbol.clone(), prices::fetch(client, &symbol).await?);
        }
    }
    let mut tx = db.begin().await?;
    let states = sqlx::query("SELECT instrument,reference_price,generator_anchor_price,generator_seed,generator_step FROM market_state ORDER BY instrument FOR UPDATE")
        .fetch_all(&mut *tx).await?;
    for state in states {
        let symbol: String = state.get("instrument");
        let current: Decimal = state.get("reference_price");
        let anchor = state
            .get::<Option<Decimal>, _>("generator_anchor_price")
            .unwrap_or(current);
        let saved_seed = state
            .get::<Option<i64>, _>("generator_seed")
            .unwrap_or_else(|| generator::market_seed(seed, &symbol));
        let step: i64 = state.get("generator_step");
        let (next_price, change, source_time) = if let Some(quote) = live_quotes.remove(&symbol) {
            (quote.0, quote.1, Some(quote.2))
        } else {
            let price = generator::next_price(current, anchor, saved_seed, step);
            (
                price,
                (price - current) / current * Decimal::from(100),
                None,
            )
        };
        if live.is_none() {
            sqlx::query("UPDATE market_state SET generator_anchor_price=$2,generator_seed=$3,generator_step=generator_step+1 WHERE instrument=$1")
                .bind(&symbol).bind(anchor).bind(saved_seed).execute(&mut *tx).await?;
        }
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
        // Equal USD depth across coins, rather than BTC-sized quantities for ETH/SOL.
        let small = (liquidity / next_price).round_dp(8);
        let large = (liquidity * Decimal::from(5) / next_price).round_dp(8);
        for (side, price, quantity) in [
            ("buy", next_price * Decimal::new(998, 3), small),
            ("buy", next_price * Decimal::new(995, 3), large),
            ("sell", next_price * Decimal::new(1002, 3), small),
            ("sell", next_price * Decimal::new(1005, 3), large),
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
