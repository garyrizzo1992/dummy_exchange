# ADR 0001: PostgreSQL outbox rather than Kafka

PostgreSQL is the authoritative state and the transactional outbox captures accepted orders and market events in the same transaction. Workers claim processing work using database row locks/advisory locks. This gives at-least-once delivery and replayability with modest local operational cost. Consumers must be idempotent; the `fills` uniqueness constraint is the primary execution guard.
