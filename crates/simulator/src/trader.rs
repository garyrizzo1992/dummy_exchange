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
    let publisher = exchange_config::kafka::Publisher::from_env()?;
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
        tick(&mut db, identity, user, rng, publisher.as_ref())
            .instrument(span)
            .await?;
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
    let fresh:bool=sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM market_state WHERE price_source='coinbase' AND updated_at < now()-interval '30 seconds')").fetch_one(&mut *tx).await?;
    anyhow::ensure!(
        fresh,
        "waiting for fresh prices before funding a new trader"
    );
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
    sqlx::query("UPDATE users SET initial_equity_usd=(
        SELECT SUM((a.available+a.reserved) * CASE WHEN a.currency='USD' THEN 1 ELSE s.reference_price END)
        FROM accounts a LEFT JOIN instruments i ON i.base_currency=a.currency AND i.quote_currency='USD'
        LEFT JOIN market_state s ON s.instrument=i.symbol WHERE a.user_id=$1
    ), profit_tracking_started_at=now(), profit_baseline_source=(SELECT price_source FROM market_state ORDER BY instrument LIMIT 1) WHERE id=$1")
        .bind(user)
        .execute(&mut *tx)
        .await?;
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
    publisher: Option<&exchange_config::kafka::Publisher>,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE simulated_traders SET last_seen_at=now() WHERE trader_key=$1")
        .bind(identity)
        .execute(&mut *db)
        .await?;
    // Expire only this trader's orders, including orders left by an earlier pod.
    let stale = sqlx::query_scalar::<_,Uuid>("SELECT id FROM orders WHERE user_id=$1 AND status IN ('open','partially_filled') AND created_at < now()-interval '30 seconds' ORDER BY sequence LIMIT 25")
        .bind(user).fetch_all(&mut *db).await?;
    for id in stale {
        if let Some(publisher) = publisher {
            publisher
                .send(
                    identity,
                    user,
                    exchange_config::kafka::Action::Cancel { order_id: id },
                )
                .await?;
            metrics::counter!("simulation_commands_published_total", "action"=>"cancel")
                .increment(1);
            continue;
        }
        match exchange_trading::cancel_order(db, user, id).await {
            Ok(_) => {
                metrics::counter!("simulation_orders_cancelled_total").increment(1);
            }
            Err(exchange_trading::TradingError::NotFound) => (),
            Err(error) => return Err(error.into()),
        }
    }
    let markets =
        sqlx::query("SELECT s.instrument,s.reference_price,i.base_currency,i.quote_currency FROM market_state s JOIN instruments i ON i.symbol=s.instrument WHERE s.price_source='simulated' OR s.updated_at > now()-interval '30 seconds' ORDER BY s.instrument")
            .fetch_all(&mut *db)
            .await?;
    if markets.is_empty() {
        metrics::counter!("simulation_orders_skipped_total", "reason"=>"stale_prices").increment(1);
        return Ok(());
    }
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
    let limit = (reference * Decimal::new(rng.random_range(9960..=10040), 4)).round_dp(2);
    let currency: String = market.get(if side == Side::Buy {
        "quote_currency"
    } else {
        "base_currency"
    });
    let available = sqlx::query_scalar::<_, Decimal>(
        "SELECT available FROM accounts WHERE user_id=$1 AND currency=$2",
    )
    .bind(user)
    .bind(currency)
    .fetch_optional(&mut *db)
    .await?
    .unwrap_or_default();
    let fraction = Decimal::new(rng.random_range(100..=1000), 4);
    // Budget includes the market-buy reservation's 5% price protection.
    let unit_cost = if side == Side::Sell {
        Decimal::ONE
    } else if order_type == OrderType::Limit {
        limit
    } else {
        reference * Decimal::new(105, 2)
    };
    let quantity = (available * fraction / unit_cost)
        .round_dp_with_strategy(8, rust_decimal::RoundingStrategy::ToZero);
    if quantity <= Decimal::ZERO {
        metrics::counter!("simulation_orders_skipped_total", "reason"=>"insufficient_funds")
            .increment(1);
        return Ok(());
    }
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
    if let Some(publisher) = publisher {
        publisher
            .send(
                identity,
                user,
                exchange_config::kafka::Action::Place { order },
            )
            .await?;
        metrics::counter!("simulation_commands_published_total", "action"=>"place").increment(1);
        return Ok(());
    }
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
