//! Commit Kafka offsets only after the corresponding database operation completes.
use exchange_config::kafka::{self, Action, Command};
use rdkafka::{
    ClientConfig, Message,
    consumer::{CommitMode, Consumer, StreamConsumer},
};
use sqlx::PgPool;
use tokio::time::{Duration, sleep};
use tracing::{Instrument, info, warn};

pub async fn run(db: PgPool, brokers: String) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", kafka::group())
        .set("enable.auto.commit", "false")
        .set("enable.auto.offset.store", "false")
        .set("auto.offset.reset", "earliest")
        .set("max.poll.interval.ms", "300000")
        .set("session.timeout.ms", "10000")
        .set("heartbeat.interval.ms", "3000")
        .set("partition.assignment.strategy", "cooperative-sticky")
        .set("allow.auto.create.topics", "false")
        .create()?;
    consumer.subscribe(&[&kafka::topic()])?;
    loop {
        let message = consumer.recv().await?;
        let command = message
            .payload()
            .and_then(|p| serde_json::from_slice::<Command>(p).ok());
        let Some(command) = command else {
            // Poison messages are rejected explicitly, never block all later commands.
            metrics::counter!("kafka_commands_rejected_total", "reason"=>"invalid_payload")
                .increment(1);
            warn!(
                partition = message.partition(),
                offset = message.offset(),
                "invalid Kafka command rejected"
            );
            consumer.commit_message(&message, CommitMode::Sync)?;
            continue;
        };
        let span = tracing::info_span!("kafka.command", account_id=%command.user_id,
            partition=message.partition(), offset=message.offset(), trace_id=tracing::field::Empty);
        exchange_config::telemetry::set_parent(&span, &command.trace_context);
        // On a transient DB failure restart the consumer at its committed offset.
        // Do not process or commit subsequent messages out of account order.
        if let Err(error) = apply(&db, &command).instrument(span).await {
            metrics::counter!("kafka_command_errors_total").increment(1);
            return Err(error);
        }
        consumer.commit_message(&message, CommitMode::Sync)?;
        metrics::counter!("kafka_commands_processed_total").increment(1);
    }
}

async fn apply(db: &PgPool, command: &Command) -> anyhow::Result<()> {
    let mut connection = db.acquire().await?;
    let owns: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM simulated_traders WHERE trader_key=$1 AND user_id=$2)",
    )
    .bind(&command.trader_id)
    .bind(command.user_id)
    .fetch_one(&mut *connection)
    .await?;
    if !owns {
        metrics::counter!("kafka_commands_rejected_total", "reason"=>"account_identity")
            .increment(1);
        return Ok(());
    }
    let result = match &command.action {
        Action::Place { order } => {
            exchange_trading::place_order(&mut connection, command.user_id, order.clone())
                .await
                .map(|(_, v)| v)
        }
        Action::Cancel { order_id } => {
            exchange_trading::cancel_order(&mut connection, command.user_id, *order_id).await
        }
    };
    match result {
        Ok(value) => {
            info!(result=%value, "Kafka command applied");
            Ok(())
        }
        // Price-feed staleness also returns Unavailable; keep commands until fresh.
        Err(exchange_trading::TradingError::Unavailable) => {
            anyhow::bail!("command temporarily unavailable")
        }
        Err(error) => {
            metrics::counter!("kafka_commands_rejected_total", "reason"=>error.to_string())
                .increment(1);
            info!(%error, "Kafka command rejected by trading rules");
            Ok(())
        }
    }
}

pub async fn supervise(db: PgPool, brokers: String) {
    loop {
        if let Err(error) = run(db.clone(), brokers.clone()).await {
            warn!(%error, "Kafka consumer stopped; restarting at committed offset");
            metrics::counter!("kafka_consumer_restarts_total").increment(1);
        }
        sleep(Duration::from_secs(3)).await;
    }
}
