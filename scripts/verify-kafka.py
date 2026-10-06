"""Kafka delivery checks against disposable exchange-verification-postgres only.
Start the three brokers using compose.kafka.yaml (project exchange-kafka-verification),
migrate the disposable database and build native binaries before running.
"""
import json, os, subprocess, sys, tempfile, time, uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ.get('EXCHANGE_BIN_DIR', ROOT/'target/debug'))
SUFFIX = '.exe' if os.name == 'nt' else ''
ENV = dict(os.environ, PGHOST='127.0.0.1', PGPORT='25432', PGUSER='postgres',
           PGDATABASE='dummy_exchange', PGPASSWORD='test-only-private-password',
           PGSSLMODE='disable', KAFKA_BOOTSTRAP_SERVERS='127.0.0.1:19092,127.0.0.1:19093,127.0.0.1:19094',
           KAFKA_CONSUMER_GROUP='verification-'+str(uuid.uuid4()), RUST_LOG='info',
           OTEL_EXPORTER_OTLP_ENDPOINT='http://127.0.0.1:4318')
processes = []

def sql(query):
    return subprocess.check_output(['docker','exec','exchange-verification-postgres','psql',
        '-U','postgres','-d','dummy_exchange','-Atc',query],text=True).strip()

def start(binary, args=(), extra=None):
    log = tempfile.TemporaryFile(mode='w+')
    proc = subprocess.Popen([str(BIN/(binary+SUFFIX)), *args],env=dict(ENV, **(extra or {})),
                            cwd=ROOT,stdout=log,stderr=log)
    processes.append((proc,log))
    return proc

def publish(payloads):
    lines=''.join(user+'|'+(p if isinstance(p,str) else json.dumps(p))+'\n' for p in payloads)
    subprocess.run(['docker','exec','-i','exchange-kafka-verification-kafka1-1',
        '/opt/kafka/bin/kafka-console-producer.sh','--bootstrap-server','localhost:9092',
        '--topic','exchange.order-commands','--reader-property','parse.key=true',
        '--reader-property','key.separator=|','--command-property','acks=all'],input=lines,text=True,check=True)

def wait(query, expected, seconds=30):
    end=time.monotonic()+seconds
    while time.monotonic()<end:
        if sql(query)==expected:return
        time.sleep(.2)
    raise AssertionError((query,sql(query),expected))

user=str(uuid.uuid4()); identity='verification-'+user
base={'trader_id':identity,'user_id':user,'trace_context':{'traceparent':'00-12345678901234567890123456789012-1234567890123456-01'}}
order={'client_order_id':str(uuid.uuid4()),'instrument':'BTC-USD','side':'buy',
       'order_type':'limit','quantity':'0.01','limit_price':'100'}
place=dict(base,action='place',order=order)
try:
    # The preceding API checks run the Coinbase feed, then stop it. Broker
    # startup can outlast its freshness window. Delivery checks use a simulated
    # price fixture so they do not depend on that stopped external feed.
    sql("UPDATE market_state SET price_source='simulated',updated_at=now()")
    sql("INSERT INTO users(id,email,password_hash) VALUES('"+user+"','"+identity+"@example.com','disabled');"
        "INSERT INTO accounts(user_id,currency,available) VALUES('"+user+"','USD',1000),('"+user+"','BTC',1);"
        "INSERT INTO simulated_traders(trader_key,user_id) VALUES('"+identity+"','"+user+"')")
    # Commands remain durable while no consumer is running.
    publish([place,place,'invalid-json',dict(place,trader_id='wrong-owner'),
        dict(place,order=dict(order,client_order_id=str(uuid.uuid4()),quantity='-1'))])
    assert sql("SELECT count(*) FROM orders WHERE user_id='"+user+"'")=='0'
    worker=start('exchange-worker',extra={'WORKER_METRICS_BIND':'127.0.0.1:23011'})
    wait("SELECT count(*) FROM orders WHERE user_id='"+user+"'",'1')
    wait("SELECT reserved FROM accounts WHERE user_id='"+user+"' AND currency='USD'",'1.0000000000')
    oid=sql("SELECT id FROM orders WHERE user_id='"+user+"'")
    worker.terminate();worker.wait()
    # Replaying a placement across restarts must not charge twice; cancellation
    # follows placement on the same account partition and is also idempotent.
    cancel=dict(base,action='cancel',order_id=oid)
    publish([place,cancel,cancel])
    worker=start('exchange-worker',extra={'WORKER_METRICS_BIND':'127.0.0.1:23011'})
    wait("SELECT status FROM orders WHERE id='"+oid+"'",'cancelled')
    wait("SELECT available FROM accounts WHERE user_id='"+user+"' AND currency='USD'",'1000.0000000000')
    assert sql("SELECT count(*) FROM orders WHERE user_id='"+user+"'")=='1'
    assert sql("SELECT count(*) FROM orders WHERE user_id='"+user+"' AND trace_context ? 'traceparent'")=='1'
    # Old commands cannot spend the fresh balance after a bot reset rotates IDs.
    fresh=str(uuid.uuid4())
    sql("INSERT INTO users(id,email,password_hash) VALUES('"+fresh+"','reset-"+fresh+"@example.com','disabled');"
        "INSERT INTO accounts(user_id,currency,available) VALUES('"+fresh+"','USD',1000),('"+fresh+"','BTC',1);"
        "UPDATE simulated_traders SET user_id='"+fresh+"' WHERE trader_key='"+identity+"'")
    stale=dict(place,order=dict(order,client_order_id=str(uuid.uuid4())))
    valid=dict(stale,user_id=fresh,order=dict(order,client_order_id=str(uuid.uuid4())))
    publish([stale,valid])
    wait("SELECT count(*) FROM orders WHERE user_id='"+fresh+"'",'1')
    assert sql("SELECT count(*) FROM orders WHERE user_id='"+user+"'")=='1'
    assert sql("SELECT available FROM accounts WHERE user_id='"+fresh+"' AND currency='USD'")=='999.0000000000'
    trader_id='kafka-producer-'+str(uuid.uuid4())
    trader=start('exchange-simulator',('trader',),{'TRADER_ID':trader_id,
        'SIMULATOR_METRICS_BIND':'127.0.0.1:23013','TRADER_INTERVAL_MS':'200'})
    end=time.monotonic()+30
    while time.monotonic()<end:
        count=int(sql("SELECT count(*) FROM orders o JOIN simulated_traders t ON t.user_id=o.user_id WHERE t.trader_key='"+trader_id+"'"))
        if count>2:break
        time.sleep(.3)
    else:raise AssertionError('Trader commands did not reach exchange workers')
    assert sql("SELECT count(*) FROM accounts WHERE available<0 OR reserved<0")=='0'
    print('Passed: three-broker durable delivery, duplicate/replayed orders, account identity validation, poison messages, ordered cancellation/refund, trace context and trader producer consumption.')
finally:
    failed = sys.exc_info()[0] is not None
    for proc,log in reversed(processes):
        if proc.poll() is None:proc.terminate();proc.wait()
        log.seek(0)
        if failed or proc.returncode not in (-15,0):print(log.read()[-6000:])
        log.close()
