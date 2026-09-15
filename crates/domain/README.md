# Exchange Domain Library

`exchange-domain` is a shared Rust library, not a hosted service. It defines order types/statuses, validation, crossing rules, price-time priority, and the pure matching calculation used by the API and matching worker.

It has no network listener, database connection, environment variables, Docker image, or independent scaling model. It is compiled into the API and worker, ensuring both use exactly the same exchange rules.
