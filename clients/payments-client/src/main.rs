mod model;
mod verification;

use async_trait::async_trait;
use model::{Store, DECIMALS, GENESIS, MINT, TOKEN};
use qrcode::{render::svg, QrCode};
use serde::Deserialize;
use serde_json::{json, Value};
use solana_commitment_config::CommitmentConfig;
use solana_pubkey::Pubkey;
use solana_rpc_client::{
    http_sender::HttpSender,
    rpc_client::{RpcClient, RpcClientConfig},
    rpc_sender::{RpcSender, RpcTransportStats},
};
use solana_rpc_client_api::{client_error::Result as RpcResult, request::RpcRequest};
use std::{
    collections::HashSet,
    env,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::PathBuf,
    process::ExitCode,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tiny_http::{Header, Method, Request, Response, Server};
use verification::{verify_transaction, ExpectedPayment};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_WALLET: &str = "9KmHj9ZwNcPysvJM3CYAuc7Jp1hzVbEC7QaBAT7uD7N2";
const USAGE: &str = "SNOWNUMB Pay\nUsage: payments-client [--port <1024..65535>] [--wallet <PUBKEY>] [--url <DEVNET_RPC>] [--store <PATH>]\nDefaults: 127.0.0.1:8790; Solana devnet; .anchor/payments-store.json\nOperator token: PAYMENTS_ADMIN_TOKEN or .anchor/payments-admin-token.txt";
struct BoundedSender(HttpSender);
#[async_trait]
impl RpcSender for BoundedSender {
    async fn send(&self, request: RpcRequest, params: Value) -> RpcResult<Value> {
        tokio::time::timeout(TIMEOUT, self.0.send(request, params))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "RPC timeout"))?
    }
    fn get_transport_stats(&self) -> RpcTransportStats {
        self.0.get_transport_stats()
    }
    fn url(&self) -> String {
        self.0.url()
    }
}
struct Options {
    port: u16,
    wallet: String,
    url: String,
    store: PathBuf,
}
fn options(args: impl IntoIterator<Item = String>) -> Result<Option<Options>> {
    let mut options = Options {
        port: 8790,
        wallet: DEFAULT_WALLET.into(),
        url: "https://api.devnet.solana.com".into(),
        store: ".anchor/payments-store.json".into(),
    };
    let mut args = args.into_iter();
    while let Some(key) = args.next() {
        if key == "--help" || key == "-h" {
            return Ok(None);
        }
        let value = args.next().ok_or("Ожидается значение параметра")?;
        match key.as_str() {
            "--port" => options.port = value.parse()?,
            "--wallet" => options.wallet = value,
            "--url" => options.url = value,
            "--store" => options.store = value.into(),
            _ => return Err(format!("Неизвестный параметр {key}").into()),
        }
    }
    if options.port < 1024 {
        return Err("Порт должен быть не меньше 1024".into());
    }
    options.wallet.parse::<Pubkey>()?;
    if !(options.url.starts_with("https://") || options.url.starts_with("http://")) {
        return Err("Требуется HTTP(S) RPC".into());
    }
    Ok(Some(options))
}
fn operator_token() -> Result<String> {
    if let Ok(value) = env::var("PAYMENTS_ADMIN_TOKEN") {
        if value.len() < 24
            || value.len() > 128
            || !value
                .bytes()
                .all(|v| v.is_ascii_alphanumeric() || v == b'-')
        {
            return Err("PAYMENTS_ADMIN_TOKEN: ожидается 24–128 букв, цифр или дефисов".into());
        }
        return Ok(value);
    }
    let path = PathBuf::from(".anchor/payments-admin-token.txt");
    if path.exists() {
        let value = fs::read_to_string(&path)?;
        let value = value.trim();
        if value.len() < 24 || value.len() > 128 {
            return Err("Некорректный файл токена оператора".into());
        }
        return Ok(value.into());
    }
    fs::create_dir_all(".anchor")?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    let token = model::random_id();
    file.write_all(token.as_bytes())?;
    file.sync_all()?;
    Ok(token)
}
fn validate_network(client: &RpcClient) -> Result<()> {
    if client.get_genesis_hash()?.to_string() != GENESIS {
        return Err("RPC не является Solana devnet".into());
    }
    let account: Value = client.send(
        RpcRequest::GetAccountInfo,
        json!([MINT,{"encoding":"jsonParsed","commitment":"finalized"}]),
    )?;
    let mint = &account["value"];
    if mint["owner"] != TOKEN
        || mint["data"]["parsed"]["type"] != "mint"
        || mint["data"]["parsed"]["info"]["decimals"] != DECIMALS
        || mint["data"]["parsed"]["info"]["isInitialized"] != true
    {
        return Err("RPC не подтвердил ожидаемый USDC mint, Token Program и decimals".into());
    }
    Ok(())
}
struct App {
    store: Mutex<Store>,
    rpc: RpcClient,
    scanning: Mutex<()>,
    token: String,
    host: String,
    wallet: String,
}
impl App {
    fn check(&self, id: &str) -> std::result::Result<Value, String> {
        let initial = self
            .store
            .lock()
            .map_err(|_| "Storage lock failed")?
            .get(id)
            .ok_or("Счёт не найден")?;
        let Ok(_scan) = self.scanning.try_lock() else {
            return Ok(
                json!({"invoice":initial,"checked":0,"rpc_error":"Сверка уже выполняется; повторите запрос"}),
            );
        };
        let known: HashSet<String> = initial
            .attempts
            .iter()
            .map(|a| a.signature.clone())
            .collect();
        let started = Instant::now();
        let mut before: Option<String> = None;
        let mut checked = 0;
        let mut failure: Option<String> = None;
        'pages: for page in 0..3 {
            let mut config = json!({"commitment":"finalized","limit":10});
            if let Some(ref cursor) = before {
                config["before"] = json!(cursor);
            }
            let rows: Value = match self.rpc.send(
                RpcRequest::GetSignaturesForAddress,
                json!([initial.reference, config]),
            ) {
                Ok(v) => v,
                Err(e) => {
                    failure = Some(format!("RPC: {e}"));
                    break;
                }
            };
            let Some(rows) = rows.as_array() else {
                failure = Some("RPC вернул некорректный список транзакций".into());
                break;
            };
            for row in rows {
                if started.elapsed() >= Duration::from_secs(20) {
                    failure = Some("Сверка не завершена за один запрос; повторите её".into());
                    break 'pages;
                }
                let Some(signature) = row["signature"].as_str() else {
                    failure = Some("RPC не вернул подпись".into());
                    break 'pages;
                };
                before = Some(signature.into());
                if known.contains(signature) {
                    continue;
                }
                let transaction: Value = match self.rpc.send(RpcRequest::GetTransaction, json!([signature,{"encoding":"json","commitment":"finalized","maxSupportedTransactionVersion":1}])) { Ok(v) => v, Err(e) => { failure = Some(format!("RPC: {e}")); break 'pages; } };
                if transaction.is_null() {
                    failure = Some("RPC пока не вернул данные транзакции; статус сохранён".into());
                    break 'pages;
                }
                let amount_raw = initial
                    .amount_raw
                    .parse::<u64>()
                    .map_err(|_| "Повреждена сумма счёта")?;
                let expected = ExpectedPayment {
                    signature,
                    mint: MINT,
                    token_program: TOKEN,
                    merchant_wallet: &initial.merchant_wallet,
                    recipient_ata: &initial.recipient_ata,
                    reference: &initial.reference,
                    amount_raw,
                    decimals: DECIMALS,
                };
                let result = verify_transaction(&transaction, &expected).map(|v| {
                    debug_assert_eq!(v.signature, signature);
                    v.slot
                });
                self.store
                    .lock()
                    .map_err(|_| "Storage lock failed")?
                    .record(id, signature, result)?;
                checked += 1;
            }
            if rows.len() < 10 {
                break;
            }
            if page == 2 {
                failure = Some("За запрос просмотрено 30 кандидатов; для старой истории нужна отдельная сверка".into());
            }
        }
        let invoice = self
            .store
            .lock()
            .map_err(|_| "Storage lock failed")?
            .get(id)
            .ok_or("Счёт не найден")?;
        Ok(json!({"invoice":invoice,"checked":checked,"rpc_error":failure}))
    }
    fn admin(&self, request: &Request) -> bool {
        let expected = format!("Bearer {}", self.token);
        let supplied = header(request, "Authorization");
        expected.len() == supplied.len()
            && expected
                .bytes()
                .zip(supplied.bytes())
                .fold(0u8, |diff, (a, b)| diff | (a ^ b))
                == 0
    }
}
fn header(request: &Request, name: &str) -> String {
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_default()
}
fn input(request: &mut Request) -> std::result::Result<Value, String> {
    if header(request, "Content-Type").split(';').next() != Some("application/json") {
        return Err("Требуется Content-Type application/json".into());
    }
    if request.body_length().is_none_or(|n| n > 16_384) {
        return Err("Тело запроса отсутствует или превышает 16 KiB".into());
    }
    let mut bytes = Vec::new();
    request
        .as_reader()
        .take(16_385)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16_384 {
        return Err("Тело запроса превышает 16 KiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Некорректный JSON".into())
}
fn empty_input(request: &mut Request) -> std::result::Result<(), String> {
    if input(request)? != json!({}) {
        return Err("Ожидается пустой JSON-объект; состояние платежа определяет RPC".into());
    }
    Ok(())
}
fn json_response(status: u16, value: Value) -> (u16, &'static str, String) {
    (status, "application/json; charset=utf-8", value.to_string())
}
fn error(status: u16, message: impl ToString) -> (u16, &'static str, String) {
    json_response(status, json!({"error":message.to_string()}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateInvoice {
    title: String,
    amount: String,
}
fn handle(app: &App, request: &mut Request) -> (u16, &'static str, String) {
    if header(request, "Host") != app.host {
        return error(403, "Недопустимый Host");
    }
    let origin = header(request, "Origin");
    if !origin.is_empty() && origin != format!("http://{}", app.host) {
        return error(403, "Недопустимый Origin");
    }
    if header(request, "Sec-Fetch-Site") == "cross-site" {
        return error(403, "Cross-site запрос отклонён");
    }
    if request.url().contains('?') {
        return error(400, "Query-параметры не поддерживаются");
    }
    let path = request.url().to_owned();
    if request.method() == &Method::Get {
        match path.as_str() {
            "/" => {
                return (
                    200,
                    "text/html; charset=utf-8",
                    include_str!("../web/index.html").into(),
                )
            }
            "/styles.css" => {
                return (
                    200,
                    "text/css; charset=utf-8",
                    include_str!("../web/styles.css").into(),
                )
            }
            "/app.js" => {
                return (
                    200,
                    "text/javascript; charset=utf-8",
                    include_str!("../web/app.js").into(),
                )
            }
            "/api/config" => {
                return json_response(
                    200,
                    json!({"network":"devnet","mint":MINT,"decimals":DECIMALS,"merchant_wallet":app.wallet,"token_program":TOKEN,"storage":"local","commitment":"finalized"}),
                )
            }
            "/api/invoices" => {
                if !app.admin(request) {
                    return error(401, "Нужен токен оператора");
                }
                return match app.store.lock() {
                    Ok(store) => json_response(200, json!({"invoices":store.list()})),
                    Err(_) => error(500, "Storage lock failed"),
                };
            }
            _ => {}
        }
        if let Some(id) = path.strip_prefix("/pay/") {
            if app.store.lock().ok().and_then(|s| s.get(id)).is_some() {
                return (
                    200,
                    "text/html; charset=utf-8",
                    include_str!("../web/index.html").into(),
                );
            }
            return error(404, "Счёт не найден");
        }
    } else if request.method() == &Method::Post && path == "/api/invoices" {
        if !app.admin(request) {
            return error(401, "Нужен токен оператора");
        }
        let key = header(request, "Idempotency-Key");
        let body: CreateInvoice = match input(request).and_then(|v| {
            serde_json::from_value(v)
                .map_err(|_| "Требуются только строковые title и amount".into())
        }) {
            Ok(v) => v,
            Err(e) => return error(400, e),
        };
        return match app
            .store
            .lock()
            .map_err(|_| "Storage lock failed".to_string())
            .and_then(|mut s| s.create(&body.title, &body.amount, &key))
        {
            Ok((invoice, created)) => {
                json_response(if created { 201 } else { 200 }, json!({"invoice":invoice}))
            }
            Err(e) => error(
                if e.contains("Idempotency-Key уже") {
                    409
                } else {
                    400
                },
                e,
            ),
        };
    }
    if let Some(tail) = path.strip_prefix("/api/invoices/") {
        let mut parts = tail.split('/');
        let id = parts.next().unwrap_or("");
        let action = parts.next();
        if parts.next().is_some() || id.parse::<Pubkey>().is_err() {
            return error(404, "Счёт не найден");
        }
        let invoice = match app.store.lock().ok().and_then(|s| s.get(id)) {
            Some(v) => v,
            None => return error(404, "Счёт не найден"),
        };
        if request.method() == &Method::Get {
            return match action {
                None => json_response(200, json!({"invoice":invoice})),
                Some("qr.svg") => match QrCode::new(invoice.solana_pay_url.as_bytes()) {
                    Ok(qr) => (
                        200,
                        "image/svg+xml",
                        qr.render::<svg::Color>()
                            .min_dimensions(320, 320)
                            .quiet_zone(true)
                            .build(),
                    ),
                    Err(e) => error(500, e),
                },
                Some("receipt") => match invoice.receipt {
                    Some(receipt) => json_response(200, json!({"invoice":id,"receipt":receipt})),
                    None => error(409, "Квитанция доступна только после проверенной оплаты"),
                },
                _ => error(404, "Не найдено"),
            };
        }
        if request.method() == &Method::Post {
            if !matches!(action, Some("check" | "close")) {
                return error(404, "Не найдено");
            }
            if action == Some("close") && !app.admin(request) {
                return error(401, "Нужен токен оператора");
            }
            if let Err(e) = empty_input(request) {
                return error(400, e);
            }
            return match action {
                Some("check") => match app.check(id) {
                    Ok(v) => json_response(200, v),
                    Err(e) => error(503, e),
                },
                Some("close") => match app
                    .store
                    .lock()
                    .map_err(|_| "Storage lock failed".to_string())
                    .and_then(|mut s| s.close(id))
                {
                    Ok(invoice) => json_response(200, json!({"invoice":invoice})),
                    Err(e) => error(409, e),
                },
                _ => unreachable!(),
            };
        }
    }
    error(
        if request.method() == &Method::Get || request.method() == &Method::Post {
            404
        } else {
            405
        },
        "Маршрут или HTTP-метод не поддерживается",
    )
}
fn run() -> Result<()> {
    let Some(options) = options(env::args().skip(1))? else {
        println!("{USAGE}");
        return Ok(());
    };
    let rpc = RpcClient::new_sender(
        BoundedSender(HttpSender::new_with_timeout(options.url.clone(), TIMEOUT)),
        RpcClientConfig::with_commitment(CommitmentConfig::finalized()),
    );
    validate_network(&rpc)?;
    let store = Store::open(options.store.clone(), &options.wallet).map_err(io::Error::other)?;
    let app = Arc::new(App {
        store: Mutex::new(store),
        rpc,
        scanning: Mutex::new(()),
        token: operator_token()?,
        host: format!("127.0.0.1:{}", options.port),
        wallet: options.wallet,
    });
    let server =
        Server::http(("127.0.0.1", options.port)).map_err(|e| io::Error::other(e.to_string()))?;
    println!("SNOWNUMB Pay: http://{}\nNetwork: devnet; mint: {}\nStorage: {}\nOperator token: PAYMENTS_ADMIN_TOKEN or .anchor/payments-admin-token.txt\nRPC genesis and USDC mint validated; server never signs transactions",app.host,MINT,options.store.display());
    let worker = Arc::clone(&app);
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(10));
        let invoices = worker.store.lock().map(|s| s.list()).unwrap_or_default();
        for invoice in invoices {
            let _ = worker.check(&invoice.public_id);
            thread::sleep(Duration::from_millis(200));
        }
    });
    for mut request in server.incoming_requests() {
        let (status, content_type, body) = handle(&app, &mut request);
        let mut response = Response::from_string(body).with_status_code(status);
        for (name,value) in [("Content-Type",content_type),("Cache-Control","no-store"),("X-Content-Type-Options","nosniff"),("Referrer-Policy","no-referrer"),("Content-Security-Policy","default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'")] {
            response.add_header(Header::from_bytes(name,value).expect("valid header"));
        }
        if let Err(e) = request.respond(response) {
            eprintln!("HTTP response failed: {e}");
        }
    }
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_cli_without_access_to_keys() {
        assert!(options(["--port".into(), "80".into()]).is_err());
        assert!(options(["--wallet".into(), "invalid".into()]).is_err());
        assert!(options(["--url".into(), "file:///tmp/key".into()]).is_err());
        assert!(options(["--help".into()]).unwrap().is_none());
    }
    struct FixtureRpc {
        invoice: model::Invoice,
        unavailable: bool,
    }
    #[async_trait]
    impl RpcSender for FixtureRpc {
        async fn send(&self, request: RpcRequest, params: Value) -> RpcResult<Value> {
            if self.unavailable {
                return Err(io::Error::other("Controlled RPC outage").into());
            }
            assert_eq!(params[1]["commitment"], "finalized");
            match request {
                RpcRequest::GetSignaturesForAddress => {
                    Ok(json!([{"signature":"fixture-signature","err":null}]))
                }
                RpcRequest::GetTransaction => {
                    assert_eq!(params[1]["encoding"], "json");
                    let payer = Pubkey::new_from_array([1; 32]).to_string();
                    let source = Pubkey::new_from_array([2; 32]).to_string();
                    let keys = [
                        &payer,
                        &source,
                        &self.invoice.recipient_ata,
                        MINT,
                        &self.invoice.reference,
                        TOKEN,
                        &self.invoice.merchant_wallet,
                    ];
                    let raw = self.invoice.amount_raw.parse::<u64>().unwrap();
                    let mut data = vec![12];
                    data.extend(raw.to_le_bytes());
                    data.push(6);
                    let balance = |index, owner: &str, amount: u64| json!({"accountIndex":index,"mint":MINT,"owner":owner,"programId":TOKEN,"uiTokenAmount":{"amount":amount.to_string(),"decimals":6}});
                    Ok(
                        json!({"version":"legacy","slot":123,"transaction":{"signatures":["fixture-signature"],"message":{"header":{"numRequiredSignatures":1,"numReadonlySignedAccounts":0,"numReadonlyUnsignedAccounts":4},"accountKeys":keys,"instructions":[{"programIdIndex":5,"accounts":[1,3,2,0,4],"data":bs58::encode(data).into_string()}]}},"meta":{"err":null,"innerInstructions":[],"preTokenBalances":[balance(1,&payer,raw*2),balance(2,&self.invoice.merchant_wallet,0)],"postTokenBalances":[balance(1,&payer,raw),balance(2,&self.invoice.merchant_wallet,raw)]}}),
                    )
                }
                _ => panic!("Unexpected RPC call"),
            }
        }
        fn get_transport_stats(&self) -> RpcTransportStats {
            RpcTransportStats::default()
        }
        fn url(&self) -> String {
            "http://controlled-fixture.invalid".into()
        }
    }
    fn fixture_app(unavailable: bool) -> (App, model::Invoice, PathBuf) {
        let path = env::temp_dir().join(format!("payments-http-{}.json", model::random_id()));
        let mut store = Store::open(path.clone(), DEFAULT_WALLET).unwrap();
        let invoice = store
            .create("Fixture invoice", "125.50", "fixture-request")
            .unwrap()
            .0;
        let rpc = RpcClient::new_sender(
            FixtureRpc {
                invoice: invoice.clone(),
                unavailable,
            },
            RpcClientConfig::with_commitment(CommitmentConfig::finalized()),
        );
        (
            App {
                store: Mutex::new(store),
                rpc,
                scanning: Mutex::new(()),
                token: "operator-test-token-for-request".into(),
                host: "127.0.0.1:8790".into(),
                wallet: DEFAULT_WALLET.into(),
            },
            invoice,
            path,
        )
    }
    fn test_request(path: &str, method: Method, body: &'static str) -> tiny_http::TestRequest {
        tiny_http::TestRequest::new()
            .with_method(method)
            .with_path(path)
            .with_body(body)
            .with_header(Header::from_bytes("Host", "127.0.0.1:8790").unwrap())
            .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
    }
    fn cleanup(app: App, path: PathBuf) {
        drop(app);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("lock"));
    }
    #[test]
    fn full_rpc_verification_creates_one_receipt_and_repeated_check_is_idempotent() {
        let (app, invoice, path) = fixture_app(false);
        let check = format!("/api/invoices/{}/check", invoice.public_id);
        let response = handle(&app, &mut test_request(&check, Method::Post, "{}").into());
        assert_eq!(response.0, 200);
        let payload: Value = serde_json::from_str(&response.2).unwrap();
        assert_eq!(payload["invoice"]["state"], "paid");
        assert_eq!(payload["invoice"]["receipt"]["commitment"], "finalized");
        handle(&app, &mut test_request(&check, Method::Post, "{}").into());
        assert_eq!(
            app.store
                .lock()
                .unwrap()
                .get(&invoice.public_id)
                .unwrap()
                .attempts
                .len(),
            1
        );
        let receipt = format!("/api/invoices/{}/receipt", invoice.public_id);
        assert_eq!(
            handle(&app, &mut test_request(&receipt, Method::Get, "").into()).0,
            200
        );
        cleanup(app, path);
    }
    #[test]
    fn rpc_outage_and_client_supplied_paid_never_create_a_receipt() {
        let (app, invoice, path) = fixture_app(true);
        let check = format!("/api/invoices/{}/check", invoice.public_id);
        let response = handle(&app, &mut test_request(&check, Method::Post, "{}").into());
        let body: Value = serde_json::from_str(&response.2).unwrap();
        assert_eq!(body["invoice"]["state"], "open");
        assert!(body["rpc_error"].is_string());
        assert!(body["invoice"]["receipt"].is_null());
        assert_eq!(
            handle(
                &app,
                &mut test_request(&check, Method::Post, "{\"paid\":true}").into()
            )
            .0,
            400
        );
        let receipt = format!("/api/invoices/{}/receipt", invoice.public_id);
        assert_eq!(
            handle(&app, &mut test_request(&receipt, Method::Get, "").into()).0,
            409
        );
        cleanup(app, path);
    }
    #[test]
    fn admin_routes_require_token_and_reject_cross_origin_and_rebinding() {
        let (app, _, path) = fixture_app(true);
        assert_eq!(
            handle(
                &app,
                &mut test_request("/api/invoices", Method::Get, "").into()
            )
            .0,
            401
        );
        let auth =
            Header::from_bytes("Authorization", "Bearer operator-test-token-for-request").unwrap();
        assert_eq!(
            handle(
                &app,
                &mut test_request("/api/invoices", Method::Get, "")
                    .with_header(auth)
                    .into()
            )
            .0,
            200
        );
        let evil = test_request("/api/config", Method::Get, "")
            .with_header(Header::from_bytes("Origin", "https://attacker.invalid").unwrap());
        assert_eq!(handle(&app, &mut evil.into()).0, 403);
        let evil = tiny_http::TestRequest::new()
            .with_path("/api/config")
            .with_header(Header::from_bytes("Host", "attacker.invalid").unwrap());
        assert_eq!(handle(&app, &mut evil.into()).0, 403);
        cleanup(app, path);
    }
}
