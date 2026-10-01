//! Shared order types and matching rules used by the API and worker.
//! Decimal keeps prices exact; floating-point numbers can introduce rounding errors.

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
        }
        if self.order_type == OrderType::Limit
            && self.limit_price.unwrap_or(Decimal::ZERO) <= Decimal::ZERO
        {
            return Err(OrderError::LimitPrice);
        }
        Ok(())
    }
}

pub fn crosses(taker: &BookOrder, maker: &BookOrder) -> bool {
    match taker.side {
        Side::Buy => taker.price >= maker.price,
        Side::Sell => taker.price <= maker.price,
    }
}

// Better prices come first. At the same price, the older order comes first.
pub fn price_time(first: &BookOrder, second: &BookOrder) -> Ordering {
    let price_order = match first.side {
        Side::Buy => second.price.cmp(&first.price),
        Side::Sell => first.price.cmp(&second.price),
    };

    if price_order == Ordering::Equal {
        first.sequence.cmp(&second.sequence)
    } else {
        price_order
    }
}

// The taker is the order being filled. Makers are orders already in the book.
pub fn match_taker(taker: &mut BookOrder, makers: &mut [BookOrder]) -> Vec<Match> {
    makers.sort_by(price_time);
    let mut fills = Vec::new();
    for maker in makers {
        if taker.remaining <= Decimal::ZERO || !crosses(taker, maker) {
            break;
        }
        let quantity = taker.remaining.min(maker.remaining);
        maker.remaining -= quantity;
        taker.remaining -= quantity;
        fills.push(Match {
            maker_id: maker.id,
            taker_id: taker.id,
            price: maker.price,
            quantity,
        });
    }
    fills
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    fn book_order(side: Side, price: Decimal, sequence: i64) -> BookOrder {
        BookOrder {
            id: Uuid::new_v4(),
            side,
            price,
            remaining: dec!(2),
            sequence,
        }
    }
    #[test]
    fn price_then_time_matching() {
        let mut taker = book_order(Side::Buy, dec!(101), 3);
        let mut makers = vec![
            book_order(Side::Sell, dec!(100), 2),
            book_order(Side::Sell, dec!(99), 1),
        ];
        let fills = match_taker(&mut taker, &mut makers);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].price, dec!(99));
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

    #[test]
    fn older_orders_fill_first_at_the_same_price() {
        let mut taker = book_order(Side::Buy, dec!(100), 3);
        let older = book_order(Side::Sell, dec!(100), 1);
        let newer = book_order(Side::Sell, dec!(100), 2);
        let older_id = older.id;
        let mut makers = vec![newer, older];

        let fills = match_taker(&mut taker, &mut makers);

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].maker_id, older_id);
    }

    #[test]
    fn sell_orders_take_the_highest_buy_price_first() {
        let mut taker = book_order(Side::Sell, dec!(99), 3);
        let mut makers = vec![
            book_order(Side::Buy, dec!(100), 1),
            book_order(Side::Buy, dec!(101), 2),
        ];

        let fills = match_taker(&mut taker, &mut makers);

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].price, dec!(101));
    }

    #[test]
    fn partial_fill_leaves_the_rest_of_the_maker_order() {
        let mut taker = book_order(Side::Buy, dec!(100), 2);
        taker.remaining = dec!(1);
        let mut makers = vec![book_order(Side::Sell, dec!(99), 1)];

        let fills = match_taker(&mut taker, &mut makers);

        assert_eq!(fills[0].quantity, dec!(1));
        assert_eq!(taker.remaining, Decimal::ZERO);
        assert_eq!(makers[0].remaining, dec!(1));
    }

    #[test]
    fn prices_that_do_not_cross_leave_orders_unchanged() {
        let mut taker = book_order(Side::Buy, dec!(99), 2);
        let mut makers = vec![book_order(Side::Sell, dec!(100), 1)];

        let fills = match_taker(&mut taker, &mut makers);

        assert!(fills.is_empty());
        assert_eq!(taker.remaining, dec!(2));
        assert_eq!(makers[0].remaining, dec!(2));
    }
}
