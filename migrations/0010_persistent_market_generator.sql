-- Capture the last published prices on the first generated tick, never on restart.
ALTER TABLE market_state
    ADD COLUMN generator_anchor_price numeric(30,10),
    ADD COLUMN generator_seed bigint,
    ADD COLUMN generator_step bigint NOT NULL DEFAULT 0 CHECK (generator_step >= 0);
