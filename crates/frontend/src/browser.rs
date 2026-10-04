use gloo_net::http::Request;
use leptos::{prelude::*, task::spawn_local};
use serde_json::{Value, json};
#[derive(Clone, Default)]
struct Snapshot {
    instruments: Vec<Value>,
    ticker: Value,
    book: Value,
    trades: Vec<Value>,
    balances: Vec<Value>,
    orders: Vec<Value>,
    fills: Vec<Value>,
    simulation: Value,
    leaderboard: Value,
}
fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0.0)
}
fn text(value: &Value) -> String {
    value.as_str().unwrap_or("").to_owned()
}
fn money(value: &Value) -> String {
    format!("{:.2}", number(value))
}
async fn api(path: &str, token: &str, body: Option<Value>) -> Result<Value, String> {
    let request = if body.is_some() {
        Request::post(path)
    } else {
        Request::get(path)
    };
    let request = if token.is_empty() {
        request
    } else {
        request.header("Authorization", &format!("Bearer {token}"))
    };
    let response = match body {
        Some(body) => {
            request
                .json(&body)
                .map_err(|_| "Unable to prepare request")?
                .send()
                .await
        }
        None => request.send().await,
    }
    .map_err(|_| "Connection unavailable. Retrying...")?;
    if !response.ok() {
        return Err(match response.status() {
            400 => "Check the fields. Passwords require at least 12 characters.",
            401 => "Sign in again or check your email and password.",
            409 => "This email is already registered.",
            422 => "Insufficient available balance.",
            503 => {
                "Exchange temporarily unavailable. The price feed may be stale; try again shortly."
            }
            429 => "Too many requests. Please wait a minute.",
            _ => "The exchange could not complete this request. Please try again.",
        }
        .into());
    }
    response
        .json()
        .await
        .map_err(|_| "Invalid server response".into())
}
fn array(value: Value) -> Vec<Value> {
    value.as_array().cloned().unwrap_or_default()
}
#[component]
pub fn App() -> impl IntoView {
    let leaderboard_offset = RwSignal::new(0u32);
    let auth_error = RwSignal::new(String::new());
    let auth_message = RwSignal::new(String::new());
    let market = RwSignal::new("BTC-USD".to_owned());
    let snapshot = RwSignal::new(Snapshot::default());
    let token = RwSignal::new(String::new());
    let connected = RwSignal::new(false);
    let error = RwSignal::new(String::new());
    let message = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let side = RwSignal::new("buy".to_owned());
    let kind = RwSignal::new("limit".to_owned());
    let quantity = RwSignal::new("0.001".to_owned());
    let price = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    spawn_local(async move {
        loop {
            let offset = leaderboard_offset.get_untracked();
            let selected = market.get_untracked();
            let auth = token.get_untracked();
            let mut next = snapshot.get_untracked();
            let result: Result<(), String> = async {
                next.instruments = array(api("/v1/instruments", "", None).await?);
                next.ticker = api(&format!("/v1/markets/{selected}/ticker"), "", None).await?;
                next.book = api(&format!("/v1/markets/{selected}/book"), "", None).await?;
                next.trades =
                    array(api(&format!("/v1/markets/{selected}/trades"), "", None).await?);
                next.leaderboard = api(
                    &format!("/v1/accounts/leaderboard?offset={offset}"),
                    "",
                    None,
                )
                .await?;
                next.simulation = api("/v1/simulation", "", None).await?;
                if !auth.is_empty() {
                    next.balances = array(api("/v1/accounts/balances", &auth, None).await?);
                    next.orders = array(api("/v1/orders", &auth, None).await?);
                    next.fills = array(api("/v1/fills", &auth, None).await?);
                } else {
                    next.balances.clear();
                    next.orders.clear();
                    next.fills.clear();
                }
                Ok(())
            }
            .await;
            if selected == market.get_untracked()
                && auth == token.get_untracked()
                && offset == leaderboard_offset.get_untracked()
            {
                match result {
                    Ok(()) => {
                        snapshot.set(next);
                        connected.set(true);
                    }
                    Err(_) => connected.set(false),
                }
            }
            gloo_timers::future::TimeoutFuture::new(2000).await;
        }
    });
    let authenticate = move |register: bool| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        auth_error.set(String::new());
        auth_message.set(String::new());
        let credentials =
            json!({"email":email.get_untracked(),"password":password.get_untracked()});
        spawn_local(async move {
            match api(
                if register {
                    "/v1/auth/register"
                } else {
                    "/v1/auth/login"
                },
                "",
                Some(credentials),
            )
            .await
            {
                Ok(value) => {
                    token.set(text(&value["access_token"]));
                    password.set(String::new());
                    auth_message.set(
                        if register {
                            "Account created. You are signed in with $100,000 in simulated USD."
                        } else {
                            "Signed in. Your simulated balances are ready."
                        }
                        .into(),
                    );
                }
                Err(e) => auth_error.set(e),
            }
            busy.set(false);
        });
    };
    let sized_quantity = move |percent| {
        let current = snapshot.get();
        let buy = side.get() == "buy";
        let selected = market.get();
        let currency = if buy {
            "USD"
        } else {
            selected.split('-').next().unwrap_or("")
        };
        let available = current
            .balances
            .iter()
            .find(|b| b["currency"] == currency)?;
        let available = available["available"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| available["available"].to_string());
        let reference = current.ticker["price"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| current.ticker["price"].to_string());
        crate::sizing::quantity(
            &available,
            &price.get(),
            &reference,
            buy,
            kind.get() == "market",
            percent,
        )
    };
    let cancel_all = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        auth_error.set(String::new());
        auth_message.set(String::new());
        let auth = token.get_untracked();
        spawn_local(async move {
            match api("/v1/orders/cancel-all", &auth, Some(json!({}))).await {
                Ok(value) => {
                    auth_message.set(format!(
                        "{} orders cancelled. Reserved funds released.",
                        value["cancelled"]
                    ));
                    if let Ok(balances) = api("/v1/accounts/balances", &auth, None).await
                        && token.get_untracked() == auth
                    {
                        snapshot.update(|s| {
                            s.balances = array(balances);
                            s.orders.clear();
                        });
                    }
                }
                Err(e) => auth_error.set(e),
            }
            busy.set(false);
        });
    };
    let submit = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(String::new());
        message.set(String::new());
        let mut random = [0u8; 16];
        if web_sys::window()
            .and_then(|w| w.crypto().ok())
            .and_then(|c| c.get_random_values_with_u8_array(&mut random).ok())
            .is_none()
        {
            error.set("Secure random generator unavailable".into());
            busy.set(false);
            return;
        }
        let id = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let body = json!({"client_order_id":id,"instrument":market.get_untracked(),"side":side.get_untracked(),
            "order_type":kind.get_untracked(),"quantity":quantity.get_untracked(),
            "limit_price":if kind.get_untracked()=="limit" {Some(price.get_untracked())} else {None}});
        spawn_local(async move {
            match api("/v1/orders", &token.get_untracked(), Some(body)).await {
                Ok(_) => message.set("Order accepted. Balances update after settlement.".into()),
                Err(e) => error.set(e),
            }
            busy.set(false);
        });
    };
    view! {
        <header><h1>"Dummy Exchange"</h1><div class="row"><span class="badge">"SIMULATED MARKETS"</span>
            <Show when=move || !token.get().is_empty()><button on:click=move |_| {token.set(String::new());snapshot.update(|s|{s.balances.clear();s.orders.clear();s.fills.clear();});}>"Sign out"</button></Show>
        </div></header>
        <div class="notice">"Demo exchange | All balances and orders are simulated. No deposits or real money."</div>
        <main class="layout">
            <section class="panel market"><div class="market-head"><h2>"Spot market"</h2><select prop:value=move ||market.get() on:change=move |ev| {market.set(event_target_value(&ev));price.set(String::new());connected.set(false);snapshot.update(|s|{s.trades.clear();s.book=Value::Null;s.ticker=Value::Null;});}>
                <option value="BTC-USD">"BTC / USD"</option><option value="ETH-USD">"ETH / USD"</option><option value="SOL-USD">"SOL / USD"</option></select></div>
                <div class="price">{move ||if snapshot.get().ticker.is_null() {"Loading price...".to_owned()}else{format!("${}",money(&snapshot.get().ticker["price"]))}}</div>
                <div class="status">{move || if connected.get() {if snapshot.get().ticker["fresh"]==false {"Price feed stale | Order placement paused"}else if snapshot.get().ticker["source"]=="coinbase" {"Coinbase live reference | Updating about every 5 seconds"}else{"Simulated reference | Refreshing every 2 seconds"}} else {"Reconnecting to exchange..."}}</div>
                <svg class="chart" viewBox="0 0 700 230" preserveAspectRatio="none" role="img" aria-label="Recent trade price chart">
                    <line x1="0" y1="50" x2="700" y2="50"/><line x1="0" y1="115" x2="700" y2="115"/><line x1="0" y1="180" x2="700" y2="180"/>
                    <polyline points=move || {let mut prices:Vec<f64>=snapshot.get().trades.iter().map(|t|number(&t["price"])).collect();prices.reverse();
                        let min=prices.iter().copied().fold(f64::INFINITY,f64::min);let max=prices.iter().copied().fold(f64::NEG_INFINITY,f64::max);
                        prices.iter().enumerate().map(|(i,p)|format!("{},{}",i as f64*700.0/(prices.len().max(2)-1) as f64,200.0-(p-min)/(max-min).max(0.01)*170.0)).collect::<Vec<_>>().join(" ")}/>
                </svg><small>"Last 60 trades | Executed prices"</small>
                <p class="muted">{move ||format!("{} active autonomous traders | {} persistent trader accounts",snapshot.get().simulation["active"],snapshot.get().simulation["traders"])}</p>
            </section>
            <section class="panel ticket"><h2>"Place an order"</h2><div class="row tabs"><button class:selected=move ||side.get()=="buy" on:click=move |_|side.set("buy".into())>"Buy"</button><button class:selected=move ||side.get()=="sell" on:click=move |_|side.set("sell".into())>"Sell"</button></div>
                <label>"Order type"<select on:change=move |ev|kind.set(event_target_value(&ev))><option value="limit">"Limit"</option><option value="market">"Market"</option></select></label>
                <label>"Quantity"<input type="number" min="0.0000000001" step="any" prop:value=move ||quantity.get() on:input=move |ev|quantity.set(event_target_value(&ev))/></label>
                <div class="row quantity-shortcuts">{[25u32,50,75,100].into_iter().map(move |percent|view!{
                    <button type="button" disabled=move ||token.get().is_empty()||busy.get()||snapshot.get().ticker["fresh"]==false||sized_quantity(percent).is_none() on:click=move |_|{if let Some(value)=sized_quantity(percent){quantity.set(value);}}>{if percent==100{"All".to_owned()}else{format!("{percent}%")}}</button>
                }).collect_view()}</div><p class="muted">"Uses available balance. Market buys include the 5% price allowance."</p>
                <Show when=move ||kind.get()=="limit"><label>"Limit price (USD)"<input type="number" min="0.01" step="0.01" placeholder="Price" prop:value=move ||price.get() on:input=move |ev|price.set(event_target_value(&ev))/></label><button disabled=move ||snapshot.get().ticker.is_null() on:click=move |_|price.set(money(&snapshot.get().ticker["price"]))>"Use market price"</button></Show>
                <button class="primary" disabled=move ||token.get().is_empty()||busy.get()||snapshot.get().ticker["fresh"]==false on:click=submit>{move ||if busy.get(){"Processing..."}else if token.get().is_empty(){"Sign in to trade"}else{"Submit order"}}</button>
                <p class="error" role="alert">{move ||error.get()}</p><p class="success" role="status">{move ||message.get()}</p>
            </section>
            <section class="panel"><h2>"Order book"</h2><table><thead><tr><th>"Price (USD)"</th><th>"Quantity"</th></tr></thead><tbody>
                {move ||snapshot.get().book["asks"].as_array().cloned().unwrap_or_default().into_iter().take(10).map(|v|view!{<tr class="sell"><td>{money(&v["price"])}</td><td>{format!("{:.6}",number(&v["quantity"]))}</td></tr>}).collect_view()}
                <tr><td colspan="2" class="muted">"Spread"</td></tr>
                {move ||snapshot.get().book["bids"].as_array().cloned().unwrap_or_default().into_iter().take(10).map(|v|view!{<tr class="buy"><td>{money(&v["price"])}</td><td>{format!("{:.6}",number(&v["quantity"]))}</td></tr>}).collect_view()}
            </tbody></table></section>
            <section class="panel"><h2>"Recent trades"</h2><table><thead><tr><th>"Price"</th><th>"Quantity"</th><th>"Time (UTC)"</th></tr></thead><tbody>
                {move ||snapshot.get().trades.into_iter().take(14).map(|v|view!{<tr><td class="buy">{money(&v["price"])}</td><td>{format!("{:.6}",number(&v["quantity"]))}</td><td>{text(&v["time"]).get(11..19).unwrap_or("").to_owned()}</td></tr>}).collect_view()}
            </tbody></table></section>
            <section class="panel"><h2>"Markets"</h2><table><thead><tr><th>"Pair"</th><th>"Reference price"</th></tr></thead><tbody>{move ||snapshot.get().instruments.into_iter().map(|v|view!{<tr><td>{text(&v["symbol"])}</td><td>{money(&v["reference_price"])}</td></tr>}).collect_view()}</tbody></table><p class="muted">"Autonomous traders place real simulated orders and settle against their account balances."</p></section>
            <section class="panel account leaderboard"><h2>"Trader profit leaderboard"</h2>
                <p class="muted">"Ranked by profit percentage since tracking began. Profit includes trading results and changes in crypto value; reserved funds count as holdings."</p>
                <div class="table-scroll"><table><thead><tr><th>"Rank"</th><th>"Account"</th><th>"Profit"</th><th>"Profit (USD)"</th><th>"No. of trades"</th></tr></thead><tbody>
                    {move ||snapshot.get().leaderboard["accounts"].as_array().cloned().unwrap_or_default().into_iter().map(|v|view!{<tr><td>{v["rank"].to_string()}</td><td class="account-id">{v["trader"].as_str().map(str::to_owned).unwrap_or_else(||text(&v["account_id"]))}</td><td>{if v["profit_percent"].is_null(){"N/A".to_owned()}else{format!("{:+.2}%",number(&v["profit_percent"]))}}</td><td>{if v["profit_usd"].is_null(){"N/A".to_owned()}else{format!("{:+.2}",number(&v["profit_usd"]))}}</td><td>{v["trade_count"].as_u64().unwrap_or(0).to_string()}</td></tr>}).collect_view()}
                </tbody></table></div>
                <div class="row"><button disabled=move ||leaderboard_offset.get()==0 on:click=move |_|{leaderboard_offset.update(|v|*v=v.saturating_sub(100));snapshot.update(|s|s.leaderboard=Value::Null);}>"Previous"</button><span>{move ||format!("{} trader accounts",snapshot.get().leaderboard["total"].as_u64().unwrap_or(0))}</span><button disabled=move ||snapshot.get().leaderboard["has_more"]!=true on:click=move |_|{leaderboard_offset.update(|v|*v=v.saturating_add(100));snapshot.update(|s|s.leaderboard=Value::Null);}>"Next"</button></div>
            </section>
            <section class="panel account system-liquidity"><h2>"System liquidity"</h2>
                <p class="muted">"Built-in market-maker inventory supplies simulated liquidity and is excluded from trader rankings."</p>
                <p>{move || {let system=snapshot.get().leaderboard["system_liquidity"].clone();if system.is_null(){"Loading system liquidity...".to_owned()}else{format!("Total inventory value: ${}",money(&system["equity_usd"]))}}}</p>
            </section>
            <section class="panel account"><h2>"Your account"</h2>
                <p class="error" role="alert">{move ||auth_error.get()}</p><p class="success" role="status">{move ||auth_message.get()}</p>
                <Show when=move ||token.get().is_empty() fallback=move ||view!{<div class="account-grid"><div><h3>"Balances"</h3><table><thead><tr><th>"Asset"</th><th>"Available"</th><th>"Reserved"</th></tr></thead><tbody>{move ||snapshot.get().balances.into_iter().map(|v|view!{<tr><td>{text(&v["currency"])}</td><td>{format!("{:.6}",number(&v["available"]))}</td><td>{format!("{:.6}",number(&v["reserved"]))}</td></tr>}).collect_view()}</tbody></table></div><div><div class="row order-actions"><h3>"Open orders"</h3><button type="button" disabled=move ||busy.get()||snapshot.get().orders.is_empty() on:click=cancel_all>"Cancel all orders"</button></div><table><thead><tr><th>"Market / Side"</th><th>"Type"</th><th>"Remaining"</th><th>"Price"</th><th>"Action"</th></tr></thead><tbody>{move ||snapshot.get().orders.into_iter().map(|v|{let id=text(&v["id"]);view!{<tr><td>{format!("{} | {}",text(&v["instrument"]),text(&v["side"]))}</td><td>{if v["order_type"]=="market"{"Market"}else{"Limit"}}</td><td>{format!("{:.6}",number(&v["remaining"]))}</td><td>{if v["order_type"]=="market"{"Market".to_owned()}else{money(&v["limit_price"])}}</td><td><button class="cancel" on:click=move |_|{let id=id.clone();spawn_local(async move{match api(&format!("/v1/orders/{id}/cancel"),&token.get_untracked(),Some(json!({}))).await{Ok(_)=>message.set("Order cancelled.".into()),Err(e)=>error.set(e)}});}>"Cancel"</button></td></tr>}}).collect_view()}</tbody></table><h3>"Recent fills"</h3><table><tbody>{move ||snapshot.get().fills.into_iter().take(10).map(|v|view!{<tr><td>{text(&v["instrument"])}</td><td>{money(&v["price"])}</td><td>{format!("{:.6}",number(&v["quantity"]))}</td></tr>}).collect_view()}</tbody></table></div></div>}>
                    <form class="auth" on:submit=move |ev|{ev.prevent_default();authenticate(false);}>
                        <p class="muted">"Create a demo account with $100,000 in simulated USD."</p><label>"Email"<input type="email" autocomplete="username" required prop:value=move ||email.get() on:input=move |ev|email.set(event_target_value(&ev))/></label>
                        <label>"Password"<input type="password" autocomplete="current-password" placeholder="At least 12 characters to create an account" required maxlength="128" prop:value=move ||password.get() on:input=move |ev|password.set(event_target_value(&ev))/></label>
                        <div class="row"><button type="submit" disabled=move ||busy.get()>"Sign in"</button><button type="button" disabled=move ||busy.get() on:click=move |_|authenticate(true)>{move ||if busy.get(){"Processing..."}else{"Create account"}}</button></div>
                    </form>
                </Show>
            </section>
        </main><footer>"Dummy Exchange | Rust powered | Simulated trading"</footer>
    }
}
