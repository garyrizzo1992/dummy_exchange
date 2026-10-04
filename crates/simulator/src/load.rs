//! Generates bounded load targets, leaving StatefulSet scaling to KEDA.
use rand::Rng;
use std::env;
use tokio::time::{Duration, sleep};

pub async fn run() -> anyhow::Result<()> {
    let min: u32 = env::var("LOAD_MIN_TRADERS")
        .unwrap_or_else(|_| "5".into())
        .parse()?;
    let max: u32 = env::var("LOAD_MAX_TRADERS")
        .unwrap_or_else(|_| "30".into())
        .parse()?;
    let seconds: u64 = env::var("LOAD_INTERVAL_SECONDS")
        .unwrap_or_else(|_| "300".into())
        .parse()?;
    anyhow::ensure!(
        min >= 1 && min <= max && max <= 100 && seconds >= 60,
        "invalid load bounds"
    );
    loop {
        let target = rand::rng().random_range(min..=max);
        metrics::gauge!("simulation_target_traders").set(f64::from(target));
        tracing::info!(target, seconds, "new randomized trader target");
        sleep(Duration::from_secs(seconds)).await;
    }
}
