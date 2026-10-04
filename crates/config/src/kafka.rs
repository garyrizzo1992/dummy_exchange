//! Durable trader commands. Account keys keep placement and cancellation ordered.
use exchange_domain::NewOrder;
use rdkafka::{
    ClientConfig,
    producer::{FutureProducer, FutureRecord},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env, time::Duration};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Command {
    pub trader_id: String,
    pub user_id: Uuid,
    pub trace_context: HashMap<String, String>,
    #[serde(flatten)]
    pub action: Action,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    Place { order: NewOrder },
    Cancel { order_id: Uuid },
}

pub fn brokers() -> Option<String> {
    env::var("KAFKA_BOOTSTRAP_SERVERS")
        .ok()
        .filter(|s| !s.trim().is_empty())
}
pub fn topic() -> String {
    env::var("KAFKA_COMMAND_TOPIC").unwrap_or_else(|_| "exchange.order-commands".into())
}
pub fn group() -> String {
    env::var("KAFKA_CONSUMER_GROUP").unwrap_or_else(|_| "exchange-workers".into())
}

pub struct Publisher {
    producer: FutureProducer,
    topic: String,
}
impl Publisher {
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        brokers()
            .map(|brokers| {
                let producer = ClientConfig::new()
                    .set("bootstrap.servers", brokers)
                    .set("enable.idempotence", "true")
                    .set("acks", "all")
                    .set("message.timeout.ms", "10000")
                    .set("allow.auto.create.topics", "false")
                    .create()?;
                Ok(Self {
                    producer,
                    topic: topic(),
                })
            })
            .transpose()
    }
    pub async fn send(&self, trader_id: &str, user_id: Uuid, action: Action) -> anyhow::Result<()> {
        let command = Command {
            trader_id: trader_id.into(),
            user_id,
            trace_context: crate::telemetry::inject(),
            action,
        };
        let payload = serde_json::to_string(&command)?;
        let key = user_id.to_string();
        // Retain the same client order ID after ambiguous delivery failures.
        // PostgreSQL's idempotency check also covers producer restarts/redelivery.
        loop {
            match self
                .producer
                .send(
                    FutureRecord::to(&self.topic).key(&key).payload(&payload),
                    Duration::from_secs(1),
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err((error, _)) => {
                    tracing::warn!(%error, "Kafka command delivery failed; retrying same command");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }
}
