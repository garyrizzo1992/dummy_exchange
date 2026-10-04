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
 before=req('/v1/accounts/balances',token=token)[1]
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
 for _ in range(10): req('/v1/auth/login',{'email':'missing@example.com','password':'incorrect'})
 assert req('/v1/auth/login',{'email':'missing@example.com','password':'incorrect'})[0]==429
 large=urllib.request.Request('http://127.0.0.1:23000/v1/orders',data=b'x'*17000,headers={'Content-Type':'application/json'})
 try:urllib.request.urlopen(large);raise AssertionError('oversized body accepted')
 except urllib.error.HTTPError as e:assert e.code==413,e.code
 trader=start('exchange-simulator'+suffix,{'TRADER_ID':'integration-trader-0','SIMULATOR_METRICS_BIND':'127.0.0.1:23003','TRADER_INTERVAL_MS':'500'},('trader',))
 time.sleep(2)
 uid=sql("SELECT user_id FROM simulated_traders WHERE trader_key='integration-trader-0'");assert uid
 duplicate=start('exchange-simulator'+suffix,{'TRADER_ID':'integration-trader-0','SIMULATOR_METRICS_BIND':'127.0.0.1:23004'},('trader',))
 time.sleep(1)
 assert 'already has an active owner' in Path(__import__("tempfile").gettempdir(),'exchange-test-2.log').read_text()
 baseline=sql("SELECT available+reserved FROM accounts WHERE user_id='"+uid+"' AND currency='USD'")
 trader.terminate();trader.wait();duplicate.terminate();duplicate.wait()
 restarted=start('exchange-simulator'+suffix,{'TRADER_ID':'integration-trader-0','SIMULATOR_METRICS_BIND':'127.0.0.1:23003','TRADER_INTERVAL_MS':'500'},('trader',))
 time.sleep(1)
 assert sql("SELECT user_id FROM simulated_traders WHERE trader_key='integration-trader-0'")==uid
 assert sql("SELECT available+reserved FROM accounts WHERE user_id='"+uid+"' AND currency='USD'")==baseline
 matcher=start('exchange-worker'+suffix,{'WORKER_METRICS_BIND':'127.0.0.1:23001'})
 simulator=start('exchange-simulator'+suffix,{'SIMULATOR_METRICS_BIND':'127.0.0.1:23002'})
 time.sleep(5)
 assert int(sql("SELECT count(*) FROM fills f JOIN orders o ON o.id=f.taker_order_id OR o.id=f.maker_order_id WHERE o.user_id='"+uid+"'"))>0
 assert sql("SELECT count(*) FROM accounts WHERE available < 0 OR reserved < 0")=='0'
 assert sql("SELECT count(*) FROM orders WHERE user_id='"+uid+"' AND is_system")=='0'
 assert sql("SELECT count(*) FROM fills WHERE price<=0 OR quantity<=0")=='0'
 assert req('/v1/simulation')[1]['active']==1
 assert int(sql("SELECT count(*) FROM orders WHERE user_id='"+uid+"' AND trace_context ? 'traceparent'"))>0
 print('Passed: password migration, auth limits, body limits, negative-price exploit, idempotency/refunds, exclusive trader ownership, durable account restart, real fills and nonnegative balances.')
finally:
 for p,log in reversed(processes):
  if p.poll() is None:p.terminate();p.wait()
  log.close()
