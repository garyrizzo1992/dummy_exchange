use anyhow::{Context, ensure};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use std::{str::FromStr, time::Duration};

pub fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .connect_timeout(Duration::from_secs(2))
        .user_agent("dummy-exchange-market-data/1.0")
        .build()?)
}
fn decimal(value: &Value, key: &str) -> anyhow::Result<Decimal> {
    let result = Decimal::from_str(value[key].as_str().context("missing price field")?)?;
    ensure!(
        result > Decimal::ZERO && result <= Decimal::new(1_000_000_000_000, 0),
        "invalid price"
    );
    Ok(result)
}
async fn get(client: &reqwest::Client, symbol: &str, endpoint: &str) -> anyhow::Result<Value> {
    ensure!(
        ["BTC-USD", "ETH-USD", "SOL-USD"].contains(&symbol),
        "unsupported live market"
    );
    let response = client
        .get(format!(
            "https://api.exchange.coinbase.com/products/{symbol}/{endpoint}"
        ))
        .send()
        .await?
        .error_for_status()?;
    let body = response.bytes().await?;
    ensure!(body.len() <= 65536, "oversized provider response");
    Ok(serde_json::from_slice(&body)?)
}
pub fn parse(
    ticker: &Value,
    stats: &Value,
    now: DateTime<Utc>,
) -> anyhow::Result<(Decimal, Decimal, DateTime<Utc>)> {
    let price = decimal(ticker, "price")?;
    let time =
        DateTime::parse_from_rfc3339(ticker["time"].as_str().context("missing trade time")?)?
            .with_timezone(&Utc);
    let age = now.signed_duration_since(time).num_seconds();
    ensure!((-30..=120).contains(&age), "stale provider trade");
    let open = decimal(stats, "open")?;
    let last = decimal(stats, "last")?;
    Ok((price, (last - open) / open * Decimal::from(100), time))
}
pub async fn fetch(
    client: &reqwest::Client,
    symbol: &str,
) -> anyhow::Result<(Decimal, Decimal, DateTime<Utc>)> {
    let (ticker, stats) =
        tokio::try_join!(get(client, symbol, "ticker"), get(client, symbol, "stats"))?;
    parse(&ticker, &stats, Utc::now())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn rejects_invalid_and_stale_quotes() {
        let now = Utc::now();
        let stats = json!({"open":"100","last":"110"});
        let valid = json!({"price":"110","time":now.to_rfc3339()});
        let (price, change, _) = parse(&valid, &stats, now).unwrap();
        assert_eq!(price, Decimal::from(110));
        assert_eq!(change, Decimal::from(10));
        assert!(parse(&json!({"price":"-1","time":now.to_rfc3339()}), &stats, now).is_err());
        assert!(parse(&valid, &json!({"open":"0","last":"110"}), now).is_err());
        assert!(
            parse(
                &json!({"price":"110","time":(now-chrono::Duration::seconds(121)).to_rfc3339()}),
                &stats,
                now
            )
            .is_err()
        );
    }
}
