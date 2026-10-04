# Public demo security

This is a simulated exchange. It accepts no real deposits and must not be used for real money.

Five verified threats and their fixes:

1. **Offline password cracking after a database leak.** Unsalted SHA-256 was fast to crack. New passwords use salted Argon2id; successful legacy logins upgrade the stored hash. Passwords are limited to 128 bytes and new passwords require 12 bytes. Hashing runs off the async executor with four concurrent slots.
2. **Forged sessions when JWT configuration is missing.** The API previously used a known development signing secret. Startup now requires a secret of at least 32 bytes and rejects the old default. Kubernetes supplies it from the existing Secret. Rotate that Secret if it has been disclosed.
3. **Credential stuffing and automated account creation.** Each API instance permits ten login attempts and five registrations per client IP per minute, with bounded limiter storage and Retry-After responses. The dev tunnel explicitly enables trust in Cloudflare's overwritten CF-Connecting-IP header; other installations use the actual connection address. Do not enable this flag on an origin directly accessible outside Cloudflare. These are per-instance limits; an edge limit is needed for a strict global budget across replicas.
4. **Balance manipulation through order inputs and retry races.** A market buy with an explicitly supplied negative price could credit available USD. Market orders now reject any explicit limit price. Limits, quantities, decimal precision and identifiers are bounded; arithmetic is checked. Market sells execute against the counterparty price rather than the internal zero-price sentinel. A transaction lock per account serializes acceptance so concurrent retries reserve funds once. Users can cancel and read only their own orders.
5. **Resource exhaustion through oversized requests and growing responses.** JSON bodies are capped at 16 KiB, requests time out after ten seconds, the API pool permits ten database connections, clients have an API request budget, account open orders are capped at 100, and order/fill/trade/book responses have fixed limits. Authentication hashing also has a concurrency cap. These protections reduce application load; they do not replace an upstream network DDoS service.

The UI keeps bearer tokens in memory and renders dynamic values using Leptos escaping. Responses add a restrictive Content-Security-Policy, deny framing, prevent MIME sniffing, and disable caching. The public tunnel exposes only `/v1`; operational endpoints, database services, and OTLP ingestion remain private. Trace data excludes credentials and request bodies.

Run `cargo test --workspace --locked` and `python scripts/verify-exchange.py` against its documented disposable PostgreSQL container to verify protections. CI runs both native and WebAssembly checks and the integration flow.
