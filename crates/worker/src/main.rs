use exchange_domain::{BookOrder, Side, match_taker};
use rand::{Rng, SeedableRng, rngs::StdRng};
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
    let id = env::var("WORKER_ID").unwrap_or_else(|_| Uuid::new_v4().to_string());
    let mut rng = StdRng::seed_from_u64(42);
    loop {
        if let Err(e) = tick(&db, &id, &mut rng).await {
            warn!(error=%e,"worker tick failed")
        };
        sleep(Duration::from_millis(100)).await
    }
}
async fn tick(db: &PgPool, worker: &str, rng: &mut StdRng) -> anyhow::Result<()> {
    let mut tx = db.begin().await?;
    let lease = sqlx::query("SELECT pg_try_advisory_xact_lock(hashtext('BTC-USD')) AS locked")
        .fetch_one(&mut *tx)
        .await?
        .get::<bool, _>("locked");
    if !lease {
        tx.rollback().await?;
        return Ok(());
    }
    let rows=sqlx::query("SELECT id,side,limit_price,remaining,sequence FROM orders WHERE instrument='BTC-USD' AND status IN ('open','partially_filled') AND limit_price IS NOT NULL ORDER BY sequence FOR UPDATE").fetch_all(&mut *tx).await?;
    let mut buys = vec![];
    let mut sells = vec![];
    for r in rows {
        let o = BookOrder {
            id: r.get("id"),
            side: match r.get::<String, _>("side").as_str() {
                "buy" => Side::Buy,
                _ => Side::Sell,
            },
            price: r.get("limit_price"),
            remaining: r.get("remaining"),
            sequence: r.get("sequence"),
        };
        match o.side {
            Side::Buy => buys.push(o),
            Side::Sell => sells.push(o),
        }
    }
    let mut fills = 0;
    for mut bid in buys {
        let fs = match_taker(&mut bid, &mut sells);
        for f in fs {
            let exists=sqlx::query("INSERT INTO fills(maker_order_id,taker_order_id,instrument,price,quantity) VALUES($1,$2,'BTC-USD',$3,$4) ON CONFLICT DO NOTHING RETURNING id").bind(f.maker_id).bind(f.taker_id).bind(f.price).bind(f.quantity).fetch_optional(&mut *tx).await?;
            if exists.is_some() {
                fills += 1;
                sqlx::query("UPDATE orders SET remaining=GREATEST(remaining-$1,0),status=CASE WHEN remaining-$1<=0 THEN 'filled' ELSE 'partially_filled' END WHERE id=$2").bind(f.quantity).bind(f.maker_id).execute(&mut *tx).await?;
                sqlx::query("UPDATE orders SET remaining=GREATEST(remaining-$1,0),status=CASE WHEN remaining-$1<=0 THEN 'filled' ELSE 'partially_filled' END WHERE id=$2").bind(f.quantity).bind(f.taker_id).execute(&mut *tx).await?;
            }
        }
    } // deterministic quote heartbeat, useful to consumers even without client flow
    let mid = Decimal::new(65000 + rng.random_range(-100..100), 0);
    sqlx::query(
        "INSERT INTO outbox_events(kind,aggregate_id,payload) VALUES('market.quote',$1,$2)",
    )
    .bind(Uuid::nil())
    .bind(serde_json::json!({"instrument":"BTC-USD","mid":mid,"worker":worker}))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    if fills > 0 {
        info!(worker, fills, "matched orders")
    };
    Ok(())
}
