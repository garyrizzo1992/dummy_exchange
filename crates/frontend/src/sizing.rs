use rust_decimal::{Decimal, RoundingStrategy};
use std::str::FromStr;

pub fn quantity(
    available: &str,
    price: &str,
    reference: &str,
    buy: bool,
    market: bool,
    percent: u32,
) -> Option<String> {
    if ![25, 50, 75, 100].contains(&percent) {
        return None;
    }
    let available = Decimal::from_str(available).ok()?;
    if available <= Decimal::ZERO {
        return None;
    }
    let unit = if !buy {
        Decimal::ONE
    } else if market {
        Decimal::from_str(reference)
            .ok()?
            .checked_mul(Decimal::new(105, 2))?
    } else {
        Decimal::from_str(price).ok()?
    };
    if unit <= Decimal::ZERO {
        return None;
    }
    let quantity = available
        .checked_mul(Decimal::from(percent))?
        .checked_div(Decimal::from(100))?
        .checked_div(unit)?
        .min(Decimal::from(1_000_000))
        .round_dp_with_strategy(8, RoundingStrategy::ToZero);
    if quantity <= Decimal::ZERO {
        return None;
    }
    Some(quantity.normalize().to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractions_use_available_funds_and_actual_reservation_price() {
        assert_eq!(
            quantity("100", "10", "20", true, false, 25).as_deref(),
            Some("2.5")
        );
        assert_eq!(
            quantity("100", "10", "20", true, false, 50).as_deref(),
            Some("5")
        );
        assert_eq!(
            quantity("100", "10", "20", true, false, 75).as_deref(),
            Some("7.5")
        );
        assert_eq!(
            quantity("100", "10", "20", true, false, 100).as_deref(),
            Some("10")
        );
        let all = quantity("100", "", "3", true, true, 100).unwrap();
        let amount = Decimal::from_str(&all).unwrap();
        assert!(amount * Decimal::new(315, 2) <= Decimal::from(100));
        assert!(Decimal::from(100) - amount * Decimal::new(315, 2) < Decimal::new(4, 8));
        assert_eq!(
            quantity("0.1234567899", "", "", false, true, 100).as_deref(),
            Some("0.12345678")
        );
        assert!(quantity("0", "10", "20", true, false, 100).is_none());
        assert!(quantity("100", "0", "20", true, false, 100).is_none());
        assert_eq!(
            quantity("999999999", "1", "", true, false, 100).as_deref(),
            Some("1000000")
        );
    }
}
