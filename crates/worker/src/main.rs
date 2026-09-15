use exchange_domain::{BookOrder, Side, match_taker};
use rust_decimal::Decimal;
use sqlx::{PgPool, Row};
use std::env;
use tokio::time::{Duration, sleep};
use tracing::{info, warn};
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().json().init();
    let db = PgPool::connect(&env::var("DATABASE_URL")?).await?;
    let worker = env::var("WORKER_ID").unwrap_or_else(|_| Uuid::new_v4().to_string());
    loop {
        if let Err(error) = tick(&db, &worker).await {
            warn!(%error,"worker tick failed")
        };
        sleep(Duration::from_millis(100)).await;
    }
}
async fn tick(db: &PgPool, worker: &str) -> anyhow::Result<()> {
    let instruments = sqlx::query("SELECT instrument FROM market_state ORDER BY instrument")
        .fetch_all(db)
        .await?;
    for row in instruments {
        let instrument: String = row.get("instrument");
        if let Err(error) = tick_instrument(db, worker, &instrument).await {
            warn!(%error,%instrument,"instrument matching failed")
        };
    }
    Ok(())
}
async fn tick_instrument(db: &PgPool, worker: &str, instrument: &str) -> anyhow::Result<()> {
    let mut tx = db.begin().await?;
    if !sqlx::query("SELECT pg_try_advisory_xact_lock(hashtext($1)) AS locked")
        .bind(instrument)
        .fetch_one(&mut *tx)
        .await?
        .get::<bool, _>("locked")
    {
        tx.rollback().await?;
        return Ok(());
    }
    let rows=sqlx::query("SELECT id,side,limit_price,remaining,sequence FROM orders WHERE instrument=$1 AND status IN ('open','partially_filled') AND limit_price IS NOT NULL ORDER BY sequence FOR UPDATE").bind(instrument).fetch_all(&mut *tx).await?;
    let (mut buys, mut sells) = (vec![], vec![]);
    for row in rows {
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
        if order.side == Side::Buy {
            buys.push(order)
        } else {
            sells.push(order)
        }
    }
    let mut fills = 0;
    for mut buy in buys {
        for fill in match_taker(&mut buy, &mut sells) {
            let inserted=sqlx::query("INSERT INTO fills(maker_order_id,taker_order_id,instrument,price,quantity) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING RETURNING id").bind(fill.maker_id).bind(fill.taker_id).bind(instrument).bind(fill.price).bind(fill.quantity).fetch_optional(&mut *tx).await?;
            if inserted.is_some() {
                apply_fill(
                    &mut tx,
                    fill.maker_id,
                    fill.taker_id,
                    fill.price,
                    fill.quantity,
                )
                .await?;
                fills += 1;
            }
        }
    }
    tx.commit().await?;
    if fills > 0 {
        info!(%worker,%instrument,fills,"orders matched")
    };
    Ok(())
}
async fn apply_fill(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    sell_id: Uuid,
    buy_id: Uuid,
    price: Decimal,
    quantity: Decimal,
) -> anyhow::Result<()> {
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
    sqlx::query("UPDATE orders SET remaining=GREATEST(remaining-$1,0),status=CASE WHEN remaining-$1<=0 THEN 'filled' ELSE 'partially_filled' END WHERE id=$2").bind(quantity).bind(buy_id).execute(&mut **tx).await?;
    sqlx::query("UPDATE orders SET remaining=GREATEST(remaining-$1,0),status=CASE WHEN remaining-$1<=0 THEN 'filled' ELSE 'partially_filled' END WHERE id=$2").bind(quantity).bind(sell_id).execute(&mut **tx).await?;
    if !buy.get::<bool, _>("is_system") {
        sqlx::query("UPDATE accounts SET reserved=reserved-$1,available=available+$2 WHERE user_id=$3 AND currency=$4").bind(reserved).bind(reserved-spent).bind(buyer).bind(quote).execute(&mut **tx).await?;
        sqlx::query("UPDATE accounts SET available=available+$1 WHERE user_id=$2 AND currency=$3")
            .bind(quantity)
            .bind(buyer)
            .bind(base)
            .execute(&mut **tx)
            .await?;
    }
    if !sell.get::<bool, _>("is_system") {
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
    Ok(())
}
