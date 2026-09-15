import http from 'k6/http'; import { check } from 'k6';
export const options={vus:10,duration:'30s'};
export default function(){const res=http.post(`${__ENV.BASE_URL||'http://127.0.0.1:3000'}/v1/orders`,JSON.stringify({client_order_id:`k6-${__VU}-${__ITER}`,instrument:'BTC-USD',side:'buy',order_type:'limit',quantity:'0.01',limit_price:'64000'}),{headers:{'Content-Type':'application/json',Authorization:`Bearer ${__ENV.TOKEN}`}});check(res,{'accepted':r=>r.status===201});}
