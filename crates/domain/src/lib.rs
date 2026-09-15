use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Buy,
    Sell,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrderType {
    Market,
    Limit,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewOrder {
    pub client_order_id: String,
    pub instrument: String,
    pub side: Side,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub limit_price: Option<Decimal>,
}
#[derive(Debug, Clone)]
pub struct BookOrder {
    pub id: Uuid,
    pub side: Side,
    pub price: Decimal,
    pub remaining: Decimal,
    pub sequence: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub maker_id: Uuid,
    pub taker_id: Uuid,
    pub price: Decimal,
    pub quantity: Decimal,
}
#[derive(Debug, Error)]
pub enum OrderError {
    #[error("quantity must be positive")]
    Quantity,
    #[error("limit order requires positive limit_price")]
    LimitPrice,
}
impl NewOrder {
    pub fn validate(&self) -> Result<(), OrderError> {
        if self.quantity <= Decimal::ZERO {
            return Err(OrderError::Quantity);
        };
        if self.order_type == OrderType::Limit
            && self.limit_price.unwrap_or(Decimal::ZERO) <= Decimal::ZERO
        {
            return Err(OrderError::LimitPrice);
        };
        Ok(())
    }
}
pub fn crosses(taker: &BookOrder, maker: &BookOrder) -> bool {
    match taker.side {
        Side::Buy => taker.price >= maker.price,
        Side::Sell => taker.price <= maker.price,
    }
}
pub fn price_time(a: &BookOrder, b: &BookOrder) -> Ordering {
    let p = match a.side {
        Side::Buy => b.price.cmp(&a.price),
        Side::Sell => a.price.cmp(&b.price),
    };
    p.then(a.sequence.cmp(&b.sequence))
}
pub fn match_taker(taker: &mut BookOrder, makers: &mut [BookOrder]) -> Vec<Match> {
    makers.sort_by(price_time);
    let mut fills = vec![];
    for maker in makers {
        if taker.remaining <= Decimal::ZERO || !crosses(taker, maker) {
            break;
        }
        let q = taker.remaining.min(maker.remaining);
        maker.remaining -= q;
        taker.remaining -= q;
        fills.push(Match {
            maker_id: maker.id,
            taker_id: taker.id,
            price: maker.price,
            quantity: q,
        });
    }
    fills
}
#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    fn o(side: Side, p: Decimal, s: i64) -> BookOrder {
        BookOrder {
            id: Uuid::new_v4(),
            side,
            price: p,
            remaining: dec!(2),
            sequence: s,
        }
    }
    #[test]
    fn price_then_time_matching() {
        let mut taker = o(Side::Buy, dec!(101), 3);
        let mut makers = vec![o(Side::Sell, dec!(100), 2), o(Side::Sell, dec!(99), 1)];
        let f = match_taker(&mut taker, &mut makers);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].price, dec!(99));
        assert_eq!(taker.remaining, Decimal::ZERO);
    }
    #[test]
    fn rejects_bad_limit() {
        assert!(
            NewOrder {
                client_order_id: "x".into(),
                instrument: "BTC-USD".into(),
                side: Side::Buy,
                order_type: OrderType::Limit,
                quantity: dec!(1),
                limit_price: None
            }
            .validate()
            .is_err()
        );
    }
}
