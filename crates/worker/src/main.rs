//! Matches buy and sell orders, then updates balances in the same transaction.

use axum::{Router, extract::State, routing::get};
use exchange_domain::{BookOrder, OrderType, Side, execution_price, match_taker, price_time};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use sqlx::{PgPool, Row};
use std::env;
use tokio::time::{Duration, sleep};
use tracing::{Instrument, info, warn};
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let _telemetry =
        exchange_config::telemetry::init(if env::args().nth(1).as_deref() == Some("trader") {
            "exchange-trader"
        } else {
            "exchange-matcher"
        })?;
    let metrics = PrometheusBuilder::new().install_recorder()?;
    let metrics_bind = env::var("WORKER_METRICS_BIND").unwrap_or_else(|_| "0.0.0.0:3001".into());
    tokio::spawn(async move {
        if let Err(error) = serve_metrics(metrics, metrics_bind).await {
            warn!(%error, "worker metrics server stopped");
        }
    });
    let db = PgPool::connect_with(exchange_config::database::connection_options()?).await?;
    let worker_id = env::var("WORKER_ID").unwrap_or_else(|_| Uuid::new_v4().to_string());
    loop {
        if let Err(error) = tick(&db, &worker_id).await {
            metrics::counter!("matching_worker_errors_total").increment(1);
            warn!(%error,"worker tick failed")
        }
        sleep(Duration::from_millis(100)).await;
    }
}

async fn tick(db: &PgPool, worker_id: &str) -> anyhow::Result<()> {
    metrics::counter!("matching_worker_ticks_total").increment(1);
    let instruments = sqlx::query("SELECT instrument FROM market_state ORDER BY instrument")
        .fetch_all(db)
        .await?;
    for row in instruments {
        let instrument: String = row.get("instrument");
        if let Err(error) = tick_instrument(db, worker_id, &instrument).await {
            metrics::counter!("matching_instrument_errors_total", "instrument" => instrument.clone()).increment(1);
            warn!(%error,%instrument,"instrument matching failed")
        }
    }
    Ok(())
}

async fn tick_instrument(db: &PgPool, worker_id: &str, instrument: &str) -> anyhow::Result<()> {
    let mut tx = db.begin().await?;
    // Only one worker may match this instrument at a time.
    let lock = sqlx::query("SELECT pg_try_advisory_xact_lock(hashtext($1)) AS locked")
        .bind(instrument)
        .fetch_one(&mut *tx)
        .await?;
    let locked: bool = lock.get("locked");
    if !locked {
        tx.rollback().await?;
        return Ok(());
    }
    let rows = sqlx::query(
        "SELECT id,side,limit_price,remaining,sequence,trace_context,order_type
         FROM orders
         WHERE instrument=$1 AND status IN ('open','partially_filled') AND limit_price IS NOT NULL
         ORDER BY sequence
         FOR UPDATE",
    )
    .bind(instrument)
    .fetch_all(&mut *tx)
    .await?;
    let reference: Decimal =
        sqlx::query_scalar("SELECT reference_price FROM market_state WHERE instrument=$1")
            .bind(instrument)
            .fetch_one(&mut *tx)
            .await?;
    let mut originals = std::collections::HashMap::new();
    let mut contexts = std::collections::HashMap::new();
    let mut buys = Vec::new();
    let mut sells = Vec::new();
    for row in rows {
        let context: serde_json::Value = row.get("trace_context");
        contexts.insert(
            row.get::<Uuid, _>("id"),
            serde_json::from_value::<std::collections::HashMap<String, String>>(context)
                .unwrap_or_default(),
        );
        let order = BookOrder {
            id: row.get("id"),
            side: if row.get::<String, _>("side") == "buy" {
                Side::Buy
            } else {
                Side::Sell
            },
            price: row.get("limit_price"),
            remaining: row.get("remaining"),
            sequence: row.get("sequence"),
        };
        let kind = if row.get::<String, _>("order_type") == "market" {
            OrderType::Market
        } else {
            OrderType::Limit
        };
        originals.insert(order.id, (order.clone(), kind));
        if order.side == Side::Buy {
            buys.push(order)
        } else {
            sells.push(order)
        }
    }
    let mut fills = 0_u64;
    let mut user_buy_fills = 0_u64;
    let mut user_sell_fills = 0_u64;
    let mut fill_notionals = Vec::new();
    buys.sort_by(price_time);
    for mut buy in buys {
        for mut fill in match_taker(&mut buy, &mut sells) {
            let (original_buy, buy_type) = &originals[&fill.taker_id];
            let (original_sell, sell_type) = &originals[&fill.maker_id];
            fill.price = execution_price(
                original_buy,
                original_sell,
                *buy_type,
                *sell_type,
                reference,
            );
            anyhow::ensure!(
                fill.price > Decimal::ZERO,
                "execution price must be positive"
            );
            let inserted = sqlx::query(
                "INSERT INTO fills(maker_order_id,taker_order_id,instrument,price,quantity)
                 VALUES($1,$2,$3,$4,$5)
                 ON CONFLICT DO NOTHING
                 RETURNING id",
            )
            .bind(fill.maker_id)
            .bind(fill.taker_id)
            .bind(instrument)
            .bind(fill.price)
            .bind(fill.quantity)
            .fetch_optional(&mut *tx)
            .await?;
            if inserted.is_some() {
                let span = tracing::info_span!("order.settle", instrument=%instrument,
                    buy_order_id=%fill.taker_id, sell_order_id=%fill.maker_id, trace_id=tracing::field::Empty);
                let parent = contexts
                    .get(&fill.taker_id)
                    .filter(|c| c.contains_key("traceparent"))
                    .or_else(|| contexts.get(&fill.maker_id));
                if let Some(context) = parent {
                    exchange_config::telemetry::set_parent(&span, context);
                }
                if let Some(context) = contexts.get(&fill.maker_id) {
                    exchange_config::telemetry::add_link(&span, context);
                }
                let (buy_is_system, sell_is_system) = apply_fill(
                    &mut tx,
                    fill.maker_id,
                    fill.taker_id,
                    fill.price,
                    fill.quantity,
                )
                .instrument(span)
                .await?;
                fills += 1;
                if !buy_is_system {
                    user_buy_fills += 1;
                }
                if !sell_is_system {
                    user_sell_fills += 1;
                }
                fill_notionals.push((fill.price * fill.quantity).to_f64().unwrap_or_default());
            }
        }
    }
    tx.commit().await?;
    if fills > 0 {
        metrics::counter!("matching_fills_total", "instrument" => instrument.to_owned())
            .increment(fills);
        metrics::counter!("matching_user_buy_fills_total", "instrument" => instrument.to_owned())
            .increment(user_buy_fills);
        metrics::counter!("matching_user_sell_fills_total", "instrument" => instrument.to_owned())
            .increment(user_sell_fills);
        for notional in fill_notionals {
            metrics::histogram!("matching_fill_notional_usd", "instrument" => instrument.to_owned())
                .record(notional);
        }
        info!(worker=%worker_id,%instrument,fills,"orders matched")
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

async fn apply_fill(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    sell_id: Uuid,
    buy_id: Uuid,
    price: Decimal,
    quantity: Decimal,
) -> anyhow::Result<(bool, bool)> {
    // SQLx needs the connection inside the borrowed transaction: &mut **tx.
    let buy =
        sqlx::query("SELECT user_id,instrument,limit_price,is_system FROM orders WHERE id=$1")
            .bind(buy_id)
            .fetch_one(&mut **tx)
            .await?;
    let sell = sqlx::query("SELECT user_id,instrument,is_system FROM orders WHERE id=$1")
        .bind(sell_id)
        .fetch_one(&mut **tx)
        .await?;
    let buyer: Uuid = buy.get("user_id");
    let seller: Uuid = sell.get("user_id");
    let instrument: String = buy.get("instrument");
    let base = instrument.split('-').next().unwrap();
    let quote = instrument.split('-').nth(1).unwrap();
    let reserved_price: Decimal = buy.get("limit_price");
    let reserved = quantity * reserved_price;
    let spent = quantity * price;
    let buy_is_system: bool = buy.get("is_system");
    let sell_is_system: bool = sell.get("is_system");

    // A fill reduces both orders. A fully used order becomes "filled".
    sqlx::query(
        "UPDATE orders
         SET remaining=GREATEST(remaining-$1,0),status=CASE WHEN remaining-$1<=0 THEN 'filled' ELSE 'partially_filled' END
         WHERE id=$2",
    )
        .bind(quantity)
        .bind(buy_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "UPDATE orders
         SET remaining=GREATEST(remaining-$1,0),status=CASE WHEN remaining-$1<=0 THEN 'filled' ELSE 'partially_filled' END
         WHERE id=$2",
    )
        .bind(quantity)
        .bind(sell_id)
        .execute(&mut **tx)
        .await?;
    // Return any unused reserved dollars and credit the purchased cryptocurrency.
    if !buy_is_system {
        sqlx::query("UPDATE accounts SET reserved=reserved-$1,available=available+$2 WHERE user_id=$3 AND currency=$4")
        .bind(reserved)
        .bind(reserved-spent)
        .bind(buyer)
        .bind(quote)
        .execute(&mut **tx)
        .await?;
        sqlx::query("UPDATE accounts SET available=available+$1 WHERE user_id=$2 AND currency=$3")
            .bind(quantity)
            .bind(buyer)
            .bind(base)
            .execute(&mut **tx)
            .await?;
    }
    // Release the seller's reserved cryptocurrency and credit the sale proceeds.
    if !sell_is_system {
        sqlx::query("UPDATE accounts SET reserved=reserved-$1 WHERE user_id=$2 AND currency=$3")
            .bind(quantity)
            .bind(seller)
            .bind(base)
            .execute(&mut **tx)
            .await?;
        sqlx::query("UPDATE accounts SET available=available+$1 WHERE user_id=$2 AND currency=$3")
            .bind(spent)
            .bind(seller)
            .bind(quote)
            .execute(&mut **tx)
            .await?;
    }
    info!(%buy_id, %sell_id, %price, %quantity, "order settlement applied");
    Ok((buy_is_system, sell_is_system))
}
