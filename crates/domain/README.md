# Exchange domain library

`exchange-domain` contains the shared exchange rules: order types and statuses,
validation, when prices cross, which orders match first, and the matching
calculation. The API and worker use this library.

It compiles into the API and worker so both use the same rules. There is no
separate service to run or scale, and it has no network listener, database
connection, environment variables or Docker image.
