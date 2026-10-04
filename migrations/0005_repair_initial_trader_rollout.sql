-- Correct only autonomous simulation fills made by the legacy matcher during
-- the first mixed-version rollout. Preserve every correction in audit_log.
CREATE TEMP TABLE simulation_fill_corrections ON COMMIT DROP AS
SELECT f.id, f.price AS old_price, f.quantity, b.user_id AS buyer, s.user_id AS seller,
       b.is_system AS buyer_is_system,
       split_part(b.instrument, '-', 2) AS currency,
       CASE WHEN b.order_type='market' THEN round(b.limit_price / 1.05, 10)
            ELSE b.limit_price END AS new_price
FROM fills f
JOIN orders b ON b.id=f.taker_order_id
JOIN orders s ON s.id=f.maker_order_id
WHERE f.price=0 AND f.quantity>0 AND s.order_type='market'
  AND EXISTS(SELECT 1 FROM simulated_traders t WHERE t.user_id=s.user_id)
  AND (b.is_system OR EXISTS(SELECT 1 FROM simulated_traders t WHERE t.user_id=b.user_id));

-- Refuse corrections that could overdraw an account or use an invalid price.
DO $$
BEGIN
  IF EXISTS(SELECT 1 FROM simulation_fill_corrections WHERE new_price<=0) THEN
    RAISE EXCEPTION 'invalid simulation correction price';
  END IF;
  IF EXISTS(
    SELECT 1 FROM (
      SELECT buyer,currency,sum(round(quantity*new_price,10)) AS debit
      FROM simulation_fill_corrections WHERE NOT buyer_is_system GROUP BY buyer,currency
    ) c LEFT JOIN accounts a ON a.user_id=c.buyer AND a.currency=c.currency
    WHERE a.id IS NULL OR a.available<c.debit
  ) THEN RAISE EXCEPTION 'simulation correction would overdraw an account'; END IF;
END $$;

INSERT INTO audit_log(action,entity_id,metadata)
SELECT 'simulation.fill_price_corrected',id,
       jsonb_build_object('old_price',old_price,'new_price',new_price,'quantity',quantity,
                          'buyer',buyer,'seller',seller,'reason','legacy matcher during initial trader rollout')
FROM simulation_fill_corrections;

UPDATE accounts a SET available=a.available-c.debit
FROM (
  SELECT buyer,currency,sum(round(quantity*new_price,10)) AS debit
  FROM simulation_fill_corrections WHERE NOT buyer_is_system GROUP BY buyer,currency
) c WHERE a.user_id=c.buyer AND a.currency=c.currency;
UPDATE accounts a SET available=a.available+c.credit
FROM (
  SELECT seller,currency,sum(round(quantity*new_price,10)) AS credit
  FROM simulation_fill_corrections GROUP BY seller,currency
) c WHERE a.user_id=c.seller AND a.currency=c.currency;
UPDATE fills f SET price=c.new_price FROM simulation_fill_corrections c WHERE f.id=c.id;

-- Zero-quantity records did not settle any money. Keep their original contents
-- in the audit log, then remove them from the execution history.
INSERT INTO audit_log(action,entity_id,metadata)
SELECT 'simulation.zero_quantity_fill_removed',f.id,to_jsonb(f)
FROM fills f JOIN orders b ON b.id=f.taker_order_id JOIN orders s ON s.id=f.maker_order_id
WHERE f.quantity=0 AND EXISTS(SELECT 1 FROM simulated_traders t WHERE t.user_id IN(b.user_id,s.user_id));
DELETE FROM fills f USING orders b,orders s
WHERE b.id=f.taker_order_id AND s.id=f.maker_order_id AND f.quantity=0
  AND EXISTS(SELECT 1 FROM simulated_traders t WHERE t.user_id IN(b.user_id,s.user_id));
