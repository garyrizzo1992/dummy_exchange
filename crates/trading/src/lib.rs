//! Order acceptance and cancellation shared by API users and autonomous traders.
use exchange_domain::{NewOrder, OrderType, Side};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use sqlx::{Connection, PgConnection, Row};
use uuid::Uuid;
#[derive(Debug, thiserror::Error)]
pub enum TradingError {
    #[error("invalid order")]
    Invalid,
    #[error("instrument or open order not found")]
    NotFound,
    #[error("insufficient available funds")]
    InsufficientFunds,
    #[error("database operation failed")]
    Unavailable,
}
#[tracing::instrument(skip_all, fields(user_id=%user_id, instrument=%order.instrument))]
pub async fn place_order(
    connection: &mut PgConnection,
    user_id: Uuid,
    order: NewOrder,
) -> Result<(bool, serde_json::Value), TradingError> {
    order.validate().map_err(|_| TradingError::Invalid)?;
    let id = Uuid::new_v4();
    let mut tx = connection
        .begin()
        .await
        .map_err(|_| TradingError::Unavailable)?;
    // Serialize simultaneous retries before checking or reserving balances.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("order-user:{user_id}"))
        .execute(&mut *tx)
        .await
        .map_err(|_| TradingError::Unavailable)?;
    // Reusing a client order ID returns the existing order, without charging twice.
    let found = sqlx::query("SELECT id,status FROM orders WHERE user_id=$1 AND client_order_id=$2")
        .bind(user_id)
        .bind(&order.client_order_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| TradingError::Unavailable)?;
    if let Some(row) = found {
        return Ok((
            false,
            (serde_json::json!({
                "id": row.get::<Uuid, _>("id"),
                "status": row.get::<String, _>("status"),
                "idempotent": true
            })),
        ));
    }
    // Bound matching and history pressure from one publicly registered account.
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM orders WHERE user_id=$1 AND status IN ('open','partially_filled')",
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| TradingError::Unavailable)?;
    if count >= 100 {
        return Err(TradingError::Invalid);
    }
    let state = sqlx::query("SELECT reference_price,(price_source='simulated' OR updated_at > now()-interval '30 seconds') AS fresh FROM market_state WHERE instrument=$1")
        .bind(&order.instrument)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| TradingError::Unavailable)?
        .ok_or(TradingError::NotFound)?;
    if !state.get::<bool, _>("fresh") {
        return Err(TradingError::Unavailable);
    }
    let reference: Decimal = state.get("reference_price");
    // Market buys can pay up to 5% above the reference price. Market sells have no floor.
    let price = if let Some(limit_price) = order.limit_price {
        limit_price
    } else if order.side == Side::Buy {
        reference
            .checked_mul(Decimal::new(105, 2))
            .ok_or(TradingError::Invalid)?
    } else {
        Decimal::ZERO
    };
    let reserve = exchange_domain::balance_amount(if order.side == Side::Buy {
        order
            .quantity
            .checked_mul(price)
            .ok_or(TradingError::Invalid)?
    } else {
        order.quantity
    });
    let base = order
        .instrument
        .split('-')
        .next()
        .ok_or(TradingError::Invalid)?;
    let quote = order
        .instrument
        .split('-')
        .nth(1)
        .ok_or(TradingError::Invalid)?;
    let currency = if order.side == Side::Buy { quote } else { base };
    // Move funds into the reserved balance until the order fills or is cancelled.
    let result = sqlx::query(
        "UPDATE accounts
         SET available=available-$1,reserved=reserved+$1
         WHERE user_id=$2 AND currency=$3 AND available >= $1",
    )
    .bind(reserve)
    .bind(user_id)
    .bind(currency)
    .execute(&mut *tx)
    .await
    .map_err(|_| TradingError::Unavailable)?;
    if result.rows_affected() != 1 {
        return Err(TradingError::InsufficientFunds);
    }
    let side = match order.side {
        Side::Buy => "buy",
        Side::Sell => "sell",
    };
    let order_type = match order.order_type {
        OrderType::Market => "market",
        OrderType::Limit => "limit",
    };
    sqlx::query(
        "INSERT INTO orders(
            id,user_id,client_order_id,instrument,side,order_type,
            quantity,remaining,limit_price,status,trace_context
         )
         VALUES($1,$2,$3,$4,$5,$6,$7,$7,$8,'open',$9)",
    )
    .bind(id)
    .bind(user_id)
    .bind(&order.client_order_id)
    .bind(&order.instrument)
    .bind(side)
    .bind(order_type)
    .bind(order.quantity)
    .bind(price)
    .bind(serde_json::to_value(exchange_config::telemetry::inject()).unwrap_or_default())
    .execute(&mut *tx)
    .await
    .map_err(|_| TradingError::Invalid)?;
    sqlx::query(
        "INSERT INTO outbox_events(kind,aggregate_id,payload) VALUES('order.accepted',$1,$2)",
    )
    .bind(id)
    .bind(serde_json::json!({
        "instrument": order.instrument
    }))
    .execute(&mut *tx)
    .await
    .map_err(|_| TradingError::Unavailable)?;
    tx.commit().await.map_err(|_| TradingError::Unavailable)?;
    metrics::counter!(
        "orders_accepted_total",
        "instrument" => order.instrument.clone(),
        "side" => side,
        "order_type" => order_type
    )
    .increment(1);
    metrics::histogram!(
        "order_notional_usd",
        "instrument" => order.instrument.clone(),
        "side" => side,
        "order_type" => order_type
    )
    .record((order.quantity * price).to_f64().unwrap_or_default());
    Ok((
        true,
        (serde_json::json!({
            "id": id,
            "status": "open"
        })),
    ))
}

#[tracing::instrument(skip_all, fields(user_id=%user_id, order_id=%id))]
pub async fn cancel_order(
    connection: &mut PgConnection,
    user_id: Uuid,
    id: Uuid,
) -> Result<serde_json::Value, TradingError> {
    cancel_orders(connection, user_id, Some(id))
        .await?
        .pop()
        .ok_or(TradingError::NotFound)
}

#[tracing::instrument(skip_all, fields(user_id=%user_id))]
pub async fn cancel_all_orders(
    connection: &mut PgConnection,
    user_id: Uuid,
) -> Result<serde_json::Value, TradingError> {
    let orders = cancel_orders(connection, user_id, None).await?;
    Ok(serde_json::json!({"cancelled":orders.len(),"orders":orders}))
}

async fn cancel_orders(
    connection: &mut PgConnection,
    user_id: Uuid,
    id: Option<Uuid>,
) -> Result<Vec<serde_json::Value>, TradingError> {
    let mut tx = connection
        .begin()
        .await
        .map_err(|_| TradingError::Unavailable)?;
    // Serialize with order placement, and lock remaining quantities against settlement.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("order-user:{user_id}"))
        .execute(&mut *tx)
        .await
        .map_err(|_| TradingError::Unavailable)?;
    let orders = sqlx::query(
        "SELECT id,instrument,side,remaining,limit_price FROM orders
        WHERE user_id=$1 AND ($2::uuid IS NULL OR id=$2) AND status IN ('open','partially_filled')
        ORDER BY sequence FOR UPDATE",
    )
    .bind(user_id)
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| TradingError::Unavailable)?;
    if id.is_some() && orders.is_empty() {
        return Err(TradingError::NotFound);
    }
    let mut result = Vec::new();
    let mut labels = Vec::new();
    for order in orders {
        let id: Uuid = order.get("id");
        let symbol: String = order.get("instrument");
        let side: String = order.get("side");
        let remaining: Decimal = order.get("remaining");
        let price: Decimal = order.get("limit_price");
        let currency = if side == "buy" {
            symbol.split('-').nth(1).unwrap()
        } else {
            symbol.split('-').next().unwrap()
        };
        let release = exchange_domain::balance_amount(if side == "buy" {
            remaining
                .checked_mul(price)
                .ok_or(TradingError::Unavailable)?
        } else {
            remaining
        });
        sqlx::query("UPDATE orders SET status='cancelled' WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|_| TradingError::Unavailable)?;
        sqlx::query("UPDATE accounts SET available=available+$1,reserved=reserved-$1 WHERE user_id=$2 AND currency=$3")
            .bind(release).bind(user_id).bind(currency).execute(&mut *tx).await.map_err(|_|TradingError::Unavailable)?;
        result.push(serde_json::json!({"id":id,"status":"cancelled"}));
        labels.push((symbol, side));
    }
    tx.commit().await.map_err(|_| TradingError::Unavailable)?;
    for (symbol, side) in labels {
        metrics::counter!("orders_cancelled_total","instrument"=>symbol,"side"=>side).increment(1);
    }
    Ok(result)
}
