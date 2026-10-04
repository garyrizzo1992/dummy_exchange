"""Integration checks against a disposable, empty exchange-verification-postgres container.
Create it with PostgreSQL on 127.0.0.1:25432, migrate, and build workspace binaries first.
Never point this script at an existing exchange database.
"""
import subprocess,os,time,json,urllib.request,urllib.error,uuid,hashlib
from pathlib import Path
root=Path(__file__).resolve().parents[1]
env=os.environ.copy();env.update(PGHOST="127.0.0.1",PGPORT="25432",PGUSER="postgres",PGDATABASE="dummy_exchange",PGPASSWORD="test-only-private-password",PGSSLMODE="disable",JWT_SECRET="integration-only-strong-secret-123456789",API_BIND="127.0.0.1:23000",OTEL_EXPORTER_OTLP_ENDPOINT="http://127.0.0.1:4318")
suffix=".exe" if os.name=="nt" else ""
processes=[]
def start(binary,extra=None,args=()):
 e=env.copy();e.update(extra or {})
 log=open(Path(__import__("tempfile").gettempdir())/('exchange-test-'+str(len(processes))+'.log'),'w')
 p=subprocess.Popen([str(root/'target/debug'/binary),*args],cwd=root,env=e,stdout=log,stderr=log);processes.append((p,log));return p
def sql(query):
 return subprocess.check_output(['docker','exec','exchange-verification-postgres','psql','-U','postgres','-d','dummy_exchange','-Atc',query],text=True).strip()
def req(path,body=None,token=None):
 headers={'Content-Type':'application/json'}
 if token: headers['Authorization']='Bearer '+token
 request=urllib.request.Request('http://127.0.0.1:23000'+path,data=None if body is None else json.dumps(body).encode(),headers=headers)
 try:
  with urllib.request.urlopen(request) as r: return r.status,json.loads(r.read()),r.headers
 except urllib.error.HTTPError as e:return e.code,None,e.headers
try:
 api=start('exchange-api'+suffix);time.sleep(1)
 status,value,headers=req('/v1/auth/register',{'email':'integration@example.com','password':'integration-password-123'})
 assert status==200,(status,value);token=value['access_token']
 assert headers['X-Content-Type-Options']=='nosniff'
 assert sql("SELECT password_hash LIKE '$argon2id$%' FROM users WHERE email='integration@example.com'")=='t'
 assert req('/v1/auth/login',{'email':'integration@example.com','password':'incorrect'})[0]==401
 old=hashlib.sha256(b'old-password').hexdigest()
 sql("INSERT INTO users(email,password_hash) VALUES('legacy@example.com','"+old+"')")
 assert req('/v1/auth/login',{'email':'legacy@example.com','password':'old-password'})[0]==200
 assert sql("SELECT password_hash LIKE '$argon2id$%' FROM users WHERE email='legacy@example.com'")=='t'
 # Leaderboard values crypto at reference prices and includes reserved funds.
 uid=sql("SELECT id FROM users WHERE email='integration@example.com'")
 sql("UPDATE accounts SET available=99990,reserved=10 WHERE user_id='"+uid+"' AND currency='USD'")
 sql("UPDATE accounts SET available=1,reserved=0.5 WHERE user_id='"+uid+"' AND currency='BTC'")
 board=req('/v1/accounts/leaderboard')[1]
 mine=next(r for r in board['accounts'] if r['account_id']==uid)
 expected=float(sql("SELECT 100000+1.5*reference_price FROM market_state WHERE instrument='BTC-USD'"))
 assert float(mine['equity_usd'])==expected,(mine,expected)
 assert float(mine['initial_equity_usd'])==100000 and mine['tracking_started_at']
 assert float(mine['profit_usd'])==expected-100000
 assert abs(float(mine['profit_percent'])-(expected-100000)/1000)<0.000001
 # A small account with a higher return outranks a larger portfolio.
 tiny=str(uuid.uuid4())
 sql("INSERT INTO users(id,email,password_hash,initial_equity_usd,profit_tracking_started_at) VALUES('"+tiny+"','profit-check@example.com','disabled',50,now())")
 sql("INSERT INTO accounts(user_id,currency,available) VALUES('"+tiny+"','USD',100)")
 winner=req('/v1/accounts/leaderboard')[1]['accounts'][0]
 assert winner['account_id']==tiny and float(winner['profit_percent'])==100
 sql("DELETE FROM accounts WHERE user_id='"+tiny+"'; DELETE FROM users WHERE id='"+tiny+"'")

 assert 'integration@example.com' not in json.dumps(board)
 assert board['accounts'][0]['rank']==1
 assert board['system_liquidity'] is None
 system='00000000-0000-0000-0000-000000000001'
 sql("INSERT INTO users(id,email,password_hash) VALUES('"+system+"','market-maker@exchange.internal','disabled')")
 sql("INSERT INTO accounts(user_id,currency,available) VALUES('"+system+"','USD',1000000000)")
 separated=req('/v1/accounts/leaderboard')[1]
 assert separated['system_liquidity']['label']=='System liquidity'
 assert float(separated['system_liquidity']['equity_usd'])==1000000000
 assert all(r['account_id']!=system for r in separated['accounts'])
 assert separated['accounts'][0]['rank']==1 and separated['total']==board['total']
 empty=req('/v1/accounts/leaderboard?offset=9999')[1]
 assert empty['accounts']==[] and empty['total']==board['total'] and empty['system_liquidity']

 sql("INSERT INTO users(email,password_hash) SELECT 'leaderboard-test-'||n||'@example.com','disabled' FROM generate_series(1,101) n")
 first_page=req('/v1/accounts/leaderboard')[1];last_page=req('/v1/accounts/leaderboard?offset=100')[1]
 assert len(first_page['accounts'])==100 and first_page['has_more']
 assert last_page['accounts'][0]['rank']==101 and not last_page['has_more']
 assert not {r['account_id'] for r in first_page['accounts']} & {r['account_id'] for r in last_page['accounts']}
 assert req('/v1/accounts/leaderboard?offset=-1')[0]==400
 sql("DELETE FROM users WHERE email LIKE 'leaderboard-test-%'")
 sql("UPDATE accounts SET available=100000,reserved=0 WHERE user_id='"+uid+"' AND currency='USD'")
 sql("UPDATE accounts SET available=0,reserved=0 WHERE user_id='"+uid+"' AND currency='BTC'")
 before=req('/v1/accounts/balances',token=token)[1]
 sql("UPDATE market_state SET price_source='coinbase',updated_at=now()-interval '31 seconds' WHERE instrument='BTC-USD'")
 paused={'client_order_id':'stale-feed','instrument':'BTC-USD','side':'buy','order_type':'market','quantity':'0.001','limit_price':None}
 assert req('/v1/orders',paused,token)[0]==503
 assert req('/v1/accounts/balances',token=token)[1]==before
 assert req('/v1/markets/BTC-USD/ticker')[1]['fresh'] is False
 sql("UPDATE market_state SET price_source='simulated',updated_at=now() WHERE instrument='BTC-USD'")

 bad={'client_order_id':'exploit','instrument':'BTC-USD','side':'buy','order_type':'market','quantity':'1','limit_price':'-1'}
 assert req('/v1/orders',bad,token)[0]==400
 assert req('/v1/accounts/balances',token=token)[1]==before
 bad['limit_price']=None;bad['quantity']='999999999999999999999'
 assert req('/v1/orders',bad,token)[0]==400
 valid=dict(bad,quantity='0.001',order_type='limit',limit_price='1',client_order_id='idempotent')
 first=req('/v1/orders',valid,token);second=req('/v1/orders',valid,token)
 assert first[0]==201 and second[0]==200 and first[1]['id']==second[1]['id'],(first,second)
 assert req('/v1/orders/'+first[1]['id']+'/cancel',{},token)[0]==200
 assert req('/v1/accounts/balances',token=token)[1]==before
 # Market orders expose a type, not internal reservation/sentinel prices.
 market=dict(valid,client_order_id='market-display-buy',order_type='market',limit_price=None)
 placed=req('/v1/orders',market,token)[1]['id']
 shown=next(o for o in req('/v1/orders',token=token)[1] if o['id']==placed)
 assert shown['order_type']=='market' and shown['limit_price'] is None
 assert req('/v1/orders/'+placed+'/cancel',{},token)[0]==200
 sql("UPDATE accounts SET available=1 WHERE user_id='"+uid+"' AND currency='ETH'")
 market.update(client_order_id='market-display-sell',instrument='ETH-USD',side='sell',quantity='0.1')
 placed=req('/v1/orders',market,token)[1]['id']
 shown=next(o for o in req('/v1/orders',token=token)[1] if o['id']==placed)
 assert shown['order_type']=='market' and shown['limit_price'] is None
 assert req('/v1/orders/'+placed+'/cancel',{},token)[0]==200
 sql("UPDATE accounts SET available=0 WHERE user_id='"+uid+"' AND currency='ETH'")
 # Bulk cancellation releases both quote and base reservations, only for the owner.
 sql("UPDATE accounts SET available=1 WHERE user_id='"+uid+"' AND currency='ETH'")
 funded=req('/v1/accounts/balances',token=token)[1]
 other=req('/v1/auth/register',{'email':'other-owner@example.com','password':'integration-password-123'})[1]['access_token']
 buy=dict(valid,client_order_id='bulk-buy')
 sell=dict(valid,client_order_id='bulk-sell',instrument='ETH-USD',side='sell',quantity='0.1',limit_price='1000000')
 assert req('/v1/orders',buy,token)[0]==201
 assert req('/v1/orders',sell,token)[0]==201
 foreign=req('/v1/orders',dict(valid,client_order_id='foreign-order'),other)[1]['id']
 assert req('/v1/orders/cancel-all',{})[0]==401
 assert len(req('/v1/orders',token=token)[1])==2
 assert req('/v1/orders/cancel-all',{},token)[1]['cancelled']==2
 assert req('/v1/orders',token=token)[1]==[]
 assert req('/v1/accounts/balances',token=token)[1]==funded
 assert req('/v1/orders',token=other)[1][0]['id']==foreign
 assert req('/v1/orders/cancel-all',{},token)[1]['cancelled']==0
 assert req('/v1/orders/cancel-all',{},other)[1]['cancelled']==1
 sql("UPDATE accounts SET available=0 WHERE user_id='"+uid+"' AND currency='ETH'")
 for _ in range(10): req('/v1/auth/login',{'email':'missing@example.com','password':'incorrect'})
 assert req('/v1/auth/login',{'email':'missing@example.com','password':'incorrect'})[0]==429
 large=urllib.request.Request('http://127.0.0.1:23000/v1/orders',data=b'x'*17000,headers={'Content-Type':'application/json'})
 try:urllib.request.urlopen(large);raise AssertionError('oversized body accepted')
 except urllib.error.HTTPError as e:assert e.code==413,e.code
 trader=start('exchange-simulator'+suffix,{'TRADER_ID':'integration-trader-0','SIMULATOR_METRICS_BIND':'127.0.0.1:23003','TRADER_INTERVAL_MS':'500'},('trader',))
 time.sleep(2)
 uid=sql("SELECT user_id FROM simulated_traders WHERE trader_key='integration-trader-0'");assert uid
 initial=sql("SELECT initial_equity_usd FROM users WHERE id='"+uid+"'");assert float(initial)>100000

 duplicate=start('exchange-simulator'+suffix,{'TRADER_ID':'integration-trader-0','SIMULATOR_METRICS_BIND':'127.0.0.1:23004'},('trader',))
 time.sleep(1)
 assert 'already has an active owner' in Path(__import__("tempfile").gettempdir(),'exchange-test-2.log').read_text()
 baseline=sql("SELECT available+reserved FROM accounts WHERE user_id='"+uid+"' AND currency='USD'")
 trader.terminate();trader.wait();duplicate.terminate();duplicate.wait()
 restarted=start('exchange-simulator'+suffix,{'TRADER_ID':'integration-trader-0','SIMULATOR_METRICS_BIND':'127.0.0.1:23003','TRADER_INTERVAL_MS':'500'},('trader',))
 time.sleep(1)
 assert sql("SELECT user_id FROM simulated_traders WHERE trader_key='integration-trader-0'")==uid
 assert sql("SELECT initial_equity_usd FROM users WHERE id='"+uid+"'")==initial

 assert sql("SELECT available+reserved FROM accounts WHERE user_id='"+uid+"' AND currency='USD'")==baseline
 matcher=start('exchange-worker'+suffix,{'WORKER_METRICS_BIND':'127.0.0.1:23001'})
 simulator=start('exchange-simulator'+suffix,{'SIMULATOR_METRICS_BIND':'127.0.0.1:23002'})
 time.sleep(5)
 assert int(sql("SELECT count(*) FROM fills f JOIN orders o ON o.id=f.taker_order_id OR o.id=f.maker_order_id WHERE o.user_id='"+uid+"'"))>0
 assert sql("SELECT count(*) FROM accounts WHERE available < 0 OR reserved < 0")=='0'
 assert sql("SELECT count(*) FROM orders WHERE user_id='"+uid+"' AND is_system")=='0'
 assert sql("SELECT count(*) FROM fills WHERE price<=0 OR quantity<=0")=='0'
 # Large market buys/sells must fill quickly in every market, including ETH/SOL.
 from decimal import Decimal, ROUND_DOWN
 for symbol in ['BTC-USD','ETH-USD','SOL-USD']:
  price=Decimal(sql("SELECT reference_price FROM market_state WHERE instrument='"+symbol+"'"))
  quantity=str((Decimal(50000)/price).quantize(Decimal('0.00000001'),rounding=ROUND_DOWN))
  for side in ['buy','sell']:
   body={'client_order_id':str(uuid.uuid4()),'instrument':symbol,'side':side,'order_type':'market','quantity':quantity,'limit_price':None}
   accepted=req('/v1/orders',body,token);assert accepted[0]==201,accepted
   oid=accepted[1]['id']
   for _ in range(40):
    if sql("SELECT status FROM orders WHERE id='"+oid+"'")=='filled':break
    time.sleep(0.2)
   else:raise AssertionError('Market order did not fill promptly: '+symbol+' '+side)
   assert sql("SELECT count(*) FROM fills WHERE (maker_order_id='"+oid+"' OR taker_order_id='"+oid+"') AND price<=0")=='0'
 assert sql("SELECT count(*) FROM accounts WHERE available<0 OR reserved<0")=='0'
 assert req('/v1/simulation')[1]['active']==1
 assert int(sql("SELECT count(*) FROM orders WHERE user_id='"+uid+"' AND trace_context ? 'traceparent'"))>0
 print('Passed: password migration, auth limits, body limits, negative-price exploit, idempotency/refunds, exclusive trader ownership, durable account restart, real fills and nonnegative balances.')
finally:
 for p,log in reversed(processes):
  if p.poll() is None:p.terminate();p.wait()
  log.close()
