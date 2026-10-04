//! One persistent, funded trader per StatefulSet identity. A dedicated PostgreSQL
//! session lock fences duplicate owners; losing that connection stops all its writes.
use exchange_domain::{NewOrder, OrderType, Side};
use rand::{Rng, SeedableRng, rngs::StdRng};
use rust_decimal::Decimal;
use sqlx::{Connection, PgConnection, Row};
use std::env;
use tokio::time::{Duration, sleep};
use tracing::{Instrument, info, warn};
use uuid::Uuid;

pub async fn run() -> anyhow::Result<()> {
    let identity = env::var("TRADER_ID")
        .map_err(|_| anyhow::anyhow!("TRADER_ID must be a stable, unique trader identity"))?;
    anyhow::ensure!(
        !identity.is_empty() && identity.len() <= 128,
        "invalid TRADER_ID"
    );
    let interval = env::var("TRADER_INTERVAL_MS")
        .unwrap_or_else(|_| "2000".into())
        .parse::<u64>()?;
    anyhow::ensure!(interval >= 100, "TRADER_INTERVAL_MS must be at least 100");
    let seed = env::var("SIMULATION_SEED")
        .unwrap_or_else(|_| "42".into())
        .parse::<u64>()?;
    let seed = identity
        .bytes()
        .fold(seed, |s, b| s.wrapping_mul(31).wrapping_add(u64::from(b)));
    let mut rng = StdRng::seed_from_u64(seed);
    loop {
        let result = session(&identity, interval, &mut rng).await;
        metrics::gauge!("simulation_trader_active").set(0.0);
        if let Err(error) = result {
            warn!(%error, trader=%identity, "trader session stopped; reconnecting");
        }
        sleep(Duration::from_secs(3)).await;
    }
}
async fn session(identity: &str, interval: u64, rng: &mut StdRng) -> anyhow::Result<()> {
    let mut db =
        PgConnection::connect_with(&exchange_config::database::connection_options()?).await?;
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended($1,0))")
        .bind(format!("simulated-trader:{identity}"))
        .fetch_one(&mut db)
        .await?;
    anyhow::ensure!(locked, "trader account already has an active owner");
    let user = bootstrap(&mut db, identity).await?;
    info!(trader=%identity, account_id=%user, "trader account claimed");
    metrics::gauge!("simulation_trader_active").set(1.0);
    loop {
        let span = tracing::info_span!("trader.tick", trader=%identity, account_id=%user, trace_id=tracing::field::Empty);
        exchange_config::telemetry::set_parent(&span, &std::collections::HashMap::new());
        tick(&mut db, identity, user, rng).instrument(span).await?;
        sleep(Duration::from_millis(
            interval + rng.random_range(0..=interval / 2),
        ))
        .await;
    }
}
pub async fn bootstrap(db: &mut PgConnection, identity: &str) -> anyhow::Result<Uuid> {
    let mut tx = db.begin().await?;
    if let Some(user) =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM simulated_traders WHERE trader_key=$1")
            .bind(identity)
            .fetch_optional(&mut *tx)
            .await?
    {
        tx.commit().await?;
        return Ok(user);
    }
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email,password_hash) VALUES($1,$2,'disabled')")
        .bind(user)
        .bind(format!("trader-{user}@exchange.internal"))
        .execute(&mut *tx)
        .await?;
    for (currency, amount) in [("USD", 100000), ("BTC", 1), ("ETH", 10), ("SOL", 100)] {
        sqlx::query("INSERT INTO accounts(user_id,currency,available) VALUES($1,$2,$3)")
            .bind(user)
            .bind(currency)
            .bind(Decimal::from(amount))
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("INSERT INTO simulated_traders(trader_key,user_id) VALUES($1,$2)")
        .bind(identity)
        .bind(user)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(user)
}
async fn tick(
    db: &mut PgConnection,
    identity: &str,
    user: Uuid,
    rng: &mut StdRng,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE simulated_traders SET last_seen_at=now() WHERE trader_key=$1")
        .bind(identity)
        .execute(&mut *db)
        .await?;
    // Expire only this trader's orders, including orders left by an earlier pod.
    let stale = sqlx::query_scalar::<_,Uuid>("SELECT id FROM orders WHERE user_id=$1 AND status IN ('open','partially_filled') AND created_at < now()-interval '30 seconds' ORDER BY sequence LIMIT 25")
        .bind(user).fetch_all(&mut *db).await?;
    for id in stale {
        match exchange_trading::cancel_order(db, user, id).await {
            Ok(_) => {
                metrics::counter!("simulation_orders_cancelled_total").increment(1);
            }
            Err(exchange_trading::TradingError::NotFound) => (),
            Err(error) => return Err(error.into()),
        }
    }
    let markets =
        sqlx::query("SELECT instrument,reference_price FROM market_state ORDER BY instrument")
            .fetch_all(&mut *db)
            .await?;
    anyhow::ensure!(!markets.is_empty(), "no markets available");
    let market = &markets[rng.random_range(0..markets.len())];
    let instrument: String = market.get("instrument");
    let reference: Decimal = market.get("reference_price");
    let side = if rng.random_bool(0.5) {
        Side::Buy
    } else {
        Side::Sell
    };
    let order_type = if rng.random_bool(0.5) {
        OrderType::Market
    } else {
        OrderType::Limit
    };
    let quantity = (Decimal::from(rng.random_range(10..=200)) / reference).round_dp(8);
    let limit = (reference * Decimal::new(rng.random_range(9960..=10040), 4)).round_dp(2);
    let order = NewOrder {
        client_order_id: Uuid::new_v4().to_string(),
        instrument: instrument.clone(),
        side,
        order_type,
        quantity,
        limit_price: if order_type == OrderType::Limit {
            Some(limit)
        } else {
            None
        },
    };
    match exchange_trading::place_order(db, user, order).await {
        Ok((_, value)) => {
            metrics::counter!("simulation_orders_placed_total", "instrument"=>instrument)
                .increment(1);
            info!(order_id=%value["id"], "trader order accepted");
        }
        Err(exchange_trading::TradingError::InsufficientFunds) => {
            metrics::counter!("simulation_orders_skipped_total", "reason"=>"insufficient_funds")
                .increment(1);
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
