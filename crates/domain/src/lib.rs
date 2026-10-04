//! Shared order types and matching rules used by the API and worker.
//! Decimal keeps prices exact; floating-point numbers can introduce rounding errors.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use thiserror::Error;
use uuid::Uuid;

/// Account columns use ten decimal places; match PostgreSQL numeric rounding.
pub fn balance_amount(amount: Decimal) -> Decimal {
    amount.round_dp_with_strategy(10, rust_decimal::RoundingStrategy::MidpointAwayFromZero)
}

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
    #[error("invalid client order ID or instrument")]
    Identifier,
    #[error("limit order requires positive limit_price")]
    LimitPrice,
}

impl NewOrder {
    pub fn validate(&self) -> Result<(), OrderError> {
        if self.client_order_id.is_empty()
            || self.client_order_id.len() > 128
            || self.instrument.len() > 32
            || !self.instrument.contains('-')
        {
            return Err(OrderError::Identifier);
        }
        if self.quantity <= Decimal::ZERO
            || self.quantity > Decimal::from(1_000_000)
            || self.quantity.scale() > 10
        {
            return Err(OrderError::Quantity);
        }
        match (self.order_type, self.limit_price) {
            (OrderType::Market, None) => {}
            (OrderType::Limit, Some(price))
                if price > Decimal::ZERO
                    && price <= Decimal::from(1_000_000_000_000_i64)
                    && price.scale() <= 10 => {}
            _ => return Err(OrderError::LimitPrice),
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
        if maker.remaining <= Decimal::ZERO {
            continue;
        }
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

#[cfg(test)]
mod security_tests {
    use super::*;
    fn order() -> NewOrder {
        NewOrder {
            client_order_id: "test".into(),
            instrument: "BTC-USD".into(),
            side: Side::Buy,
            order_type: OrderType::Market,
            quantity: Decimal::ONE,
            limit_price: None,
        }
    }
    #[test]
    fn market_price_cannot_credit_balance() {
        for price in [-1, 0, 1] {
            let mut order = order();
            order.limit_price = Some(Decimal::from(price));
            assert!(order.validate().is_err());
        }
        assert!(order().validate().is_ok());
    }
    #[test]
    fn excessive_precision_and_size_are_rejected() {
        let mut order = order();
        order.quantity = Decimal::MAX;
        assert!(order.validate().is_err());
        order.quantity = Decimal::new(1, 11);
        assert!(order.validate().is_err());
        order.quantity = Decimal::ONE;
        order.client_order_id = "x".repeat(129);
        assert!(order.validate().is_err());
    }
    #[test]
    fn exhausted_makers_never_emit_zero_fills() {
        let mut taker = BookOrder {
            id: Uuid::new_v4(),
            side: Side::Buy,
            price: Decimal::ONE,
            remaining: Decimal::ONE,
            sequence: 2,
        };
        let mut makers = [BookOrder {
            id: Uuid::new_v4(),
            side: Side::Sell,
            price: Decimal::ONE,
            remaining: Decimal::ZERO,
            sequence: 1,
        }];
        assert!(match_taker(&mut taker, &mut makers).is_empty());
    }
}

/// Market sell sentinel prices are never execution prices. Use the resting limit
/// counterparty, or the reference when both sides are market orders.
pub fn execution_price(
    buy: &BookOrder,
    sell: &BookOrder,
    buy_type: OrderType,
    sell_type: OrderType,
    reference: Decimal,
) -> Decimal {
    match (buy_type, sell_type) {
        (OrderType::Market, OrderType::Market) => reference.min(buy.price),
        (_, OrderType::Market) => buy.price,
        (OrderType::Market, _) => sell.price,
        _ if buy.sequence < sell.sequence => buy.price,
        _ => sell.price,
    }
}
#[cfg(test)]
mod execution_tests {
    use super::*;
    #[test]
    fn partial_reservation_releases_sum_to_the_original_amount() {
        let price = Decimal::new(85217526712345, 10);
        let mut remaining = Decimal::new(123456789, 8);
        let original = balance_amount(remaining * price);
        let mut released = Decimal::ZERO;
        for quantity in [Decimal::new(1234567, 8), Decimal::new(33333333, 8)] {
            released +=
                balance_amount(remaining * price) - balance_amount((remaining - quantity) * price);
            remaining -= quantity;
        }
        released += balance_amount(remaining * price);
        assert_eq!(released, original);
    }
    #[test]
    fn market_sells_receive_the_resting_bid_not_zero() {
        let buy = BookOrder {
            id: Uuid::new_v4(),
            side: Side::Buy,
            price: Decimal::from(65_000),
            remaining: Decimal::ONE,
            sequence: 1,
        };
        let sell = BookOrder {
            id: Uuid::new_v4(),
            side: Side::Sell,
            price: Decimal::ZERO,
            remaining: Decimal::ONE,
            sequence: 2,
        };
        assert_eq!(
            execution_price(
                &buy,
                &sell,
                OrderType::Limit,
                OrderType::Market,
                Decimal::from(64_000)
            ),
            buy.price
        );
        assert_eq!(
            execution_price(
                &buy,
                &sell,
                OrderType::Market,
                OrderType::Market,
                Decimal::from(64_000)
            ),
            Decimal::from(64_000)
        );
    }
}
