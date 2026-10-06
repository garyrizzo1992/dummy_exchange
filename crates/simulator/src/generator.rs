//! Deterministic price generation from database state, independent of live feeds.
use rand::{Rng, SeedableRng, rngs::StdRng};
use rust_decimal::Decimal;

pub fn market_seed(seed: u64, symbol: &str) -> i64 {
    symbol
        .bytes()
        .fold(seed, |s, b| s.wrapping_mul(31).wrapping_add(u64::from(b))) as i64
}

pub fn next_price(current: Decimal, anchor: Decimal, seed: i64, step: i64) -> Decimal {
    let mut rng = StdRng::seed_from_u64(
        (seed as u64).wrapping_add((step as u64).wrapping_mul(0x9e3779b97f4a7c15)),
    );
    let noise = Decimal::new(rng.random_range(-15_i64..=15), 4);
    // A weak pull toward the initial simulated valuation prevents an unbounded
    // random walk from collapsing or growing forever. The anchor never follows
    // subsequent cryptocurrency prices.
    let next = current * (Decimal::ONE + noise) + (anchor - current) * Decimal::new(2, 4);
    next.round_dp(2)
        .max((anchor / Decimal::from(2)).max(Decimal::new(1, 2)))
        .min((anchor * Decimal::new(15, 1)).max(Decimal::new(1, 2)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_continues_the_saved_sequence() {
        let anchor = Decimal::from(65000);
        let seed = market_seed(42, "BTC-USD");
        let mut current = anchor;
        for step in 0..100 {
            current = next_price(current, anchor, seed, step);
        }
        let saved = (current, anchor, seed, 100);
        let continued = next_price(current, anchor, seed, 100);
        assert_eq!(continued, next_price(saved.0, saved.1, saved.2, saved.3));
        let continued_sequence: Vec<_> = (100..120)
            .map(|step| next_price(current, anchor, seed, step))
            .collect();
        let restarted_sequence: Vec<_> = (0..20)
            .map(|step| next_price(current, anchor, seed, step))
            .collect();
        assert_ne!(continued_sequence, restarted_sequence);
    }
    #[test]
    fn prices_stay_positive_and_bounded_without_external_quotes() {
        let anchor = Decimal::from(150);
        let mut current = anchor;
        let seed = market_seed(42, "SOL-USD");
        for step in 0..10000 {
            current = next_price(current, anchor, seed, step);
            assert!(current >= Decimal::from(75) && current <= Decimal::from(225));
        }
        assert_ne!(current, anchor);
        assert_ne!(seed, market_seed(42, "ETH-USD"));
    }
}
