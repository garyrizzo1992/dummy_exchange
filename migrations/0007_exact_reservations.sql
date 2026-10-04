-- Reconcile rounding dust against open orders while preserving total holdings.
-- Fail and retry instead of waiting behind an in-flight writer and racing it.
LOCK TABLE orders, accounts IN SHARE ROW EXCLUSIVE MODE NOWAIT;
WITH expected AS (
    SELECT o.user_id,
           CASE WHEN o.side='buy' THEN i.quote_currency ELSE i.base_currency END AS currency,
           SUM(CASE WHEN o.side='buy' THEN round(o.remaining*o.limit_price,10)
                    ELSE o.remaining END) AS amount
    FROM orders o JOIN instruments i ON i.symbol=o.instrument
    WHERE o.status IN ('open','partially_filled') AND NOT o.is_system
    GROUP BY o.user_id,2
), reconciled AS (
    SELECT a.id,COALESCE(e.amount,0) AS amount
    FROM accounts a LEFT JOIN expected e ON e.user_id=a.user_id AND e.currency=a.currency
    WHERE a.user_id <> '00000000-0000-0000-0000-000000000001'::uuid
)
UPDATE accounts a SET available=a.available+a.reserved-r.amount,reserved=r.amount
FROM reconciled r WHERE a.id=r.id AND a.reserved<>r.amount;
