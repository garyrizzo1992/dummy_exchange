-- PostgreSQL 16+ provides gen_random_uuid() natively; no uuid-ossp extension is required.
ALTER TABLE users ALTER COLUMN id SET DEFAULT gen_random_uuid();
ALTER TABLE accounts ALTER COLUMN id SET DEFAULT gen_random_uuid();
ALTER TABLE orders ALTER COLUMN id SET DEFAULT gen_random_uuid();
ALTER TABLE fills ALTER COLUMN id SET DEFAULT gen_random_uuid();
ALTER TABLE outbox_events ALTER COLUMN id SET DEFAULT gen_random_uuid();

DROP EXTENSION IF EXISTS "uuid-ossp";
