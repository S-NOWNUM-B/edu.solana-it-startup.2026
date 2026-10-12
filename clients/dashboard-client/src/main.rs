use async_trait::async_trait;
use serde_json::{json, Value};
use solana_commitment_config::CommitmentConfig;
use solana_pubkey::Pubkey;
use solana_rpc_client::{
    http_sender::HttpSender,
    rpc_client::{GetConfirmedSignaturesForAddress2Config, RpcClient, RpcClientConfig},
    rpc_sender::{RpcSender, RpcTransportStats},
};
use solana_rpc_client_api::{
    client_error::Result as RpcResult,
    request::{RpcRequest, TokenAccountsFilter},
};
use std::{env, error::Error, io, process::ExitCode, time::Duration};
use tiny_http::{Header, Method, Response, Server};

const DEFAULT_WALLET: &str = "9KmHj9ZwNcPysvJM3CYAuc7Jp1hzVbEC7QaBAT7uD7N2";
const SNTOK_MINT: &str = "DhjRbgdts2DNEvbfEVQkuGkpwwioEWW87LdHV11otozW";
const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";
const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const TOKEN_2022_PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
const ORCA_PROGRAM: &str = "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc";
const DEVNET_GENESIS: &str = "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG";
const USAGE: &str = "SNTOK RPC dashboard\nUsage: dashboard-client [--port <1024..65535>] [--url <RPC_URL>] [--wallet <PUBKEY>]\nDefaults: http://127.0.0.1:8787, Solana devnet";
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const RPC_TIMEOUT: Duration = Duration::from_secs(8);

struct BoundedSender(HttpSender);

#[async_trait]
impl RpcSender for BoundedSender {
    async fn send(&self, request: RpcRequest, params: Value) -> RpcResult<Value> {
        // Ограничиваем весь RPC-вызов, включая ожидание Retry-After при HTTP 429.
        tokio::time::timeout(RPC_TIMEOUT, self.0.send(request, params))
            .await
            .map_err(|_| {
                io::Error::new(io::ErrorKind::TimedOut, "RPC request exceeded 8 seconds")
            })?
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
    url: String,
    wallet: Pubkey,
}

fn parse_options(args: impl IntoIterator<Item = String>) -> Result<Option<Options>> {
    let mut args = args.into_iter();
    let mut options = Options {
        port: 8787,
        url: "https://api.devnet.solana.com".into(),
        wallet: DEFAULT_WALLET.parse()?,
    };
    while let Some(option) = args.next() {
        if matches!(option.as_str(), "--help" | "-h") {
            return Ok(None);
        }
        if !matches!(option.as_str(), "--port" | "--url" | "--wallet") {
            return Err(io::Error::other(format!("Unknown option: {option}")).into());
        }
        let value = args
            .next()
            .filter(|value| !value.starts_with("--"))
            .ok_or_else(|| io::Error::other(format!("Missing value for {option}")))?;
        match option.as_str() {
            "--port" => options.port = value.parse()?,
            "--url" => options.url = value,
            "--wallet" => options.wallet = value.parse()?,
            _ => unreachable!(),
        }
    }
    if options.port < 1024 || options.url.trim().is_empty() {
        return Err(io::Error::other("Port must be 1024..65535; RPC URL must not be empty").into());
    }
    Ok(Some(options))
}

fn query_wallet(query: &str, default: Pubkey) -> Result<Pubkey> {
    if query.is_empty() {
        return Ok(default);
    }
    let address = query
        .strip_prefix("wallet=")
        .filter(|address| !address.is_empty() && !address.contains('&') && !address.contains('='))
        .ok_or_else(|| io::Error::other("Expected a single wallet=<PUBKEY> query parameter"))?;
    address
        .parse()
        .map_err(|_| io::Error::other("Invalid wallet address").into())
}

fn exact_units(amount: u64, decimals: u8) -> String {
    let digits = amount.to_string();
    let decimals = usize::from(decimals);
    if decimals == 0 {
        return digits;
    }
    let value = if digits.len() <= decimals {
        format!("0.{}{}", "0".repeat(decimals - digits.len()), digits)
    } else {
        let (whole, fraction) = digits.split_at(digits.len() - decimals);
        format!("{whole}.{fraction}")
    };
    value
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn token_row(
    account: &str,
    account_program: &str,
    data: &Value,
    owner: Pubkey,
    program: &str,
    token: &Value,
) -> Result<Value> {
    let _: Pubkey = account.parse()?;
    let info = &data["parsed"]["info"];
    if account_program != program
        || data["parsed"]["type"] != "account"
        || info["owner"].as_str() != Some(owner.to_string().as_str())
    {
        return Err(io::Error::other("Unexpected token account program or owner").into());
    }
    let mint = info["mint"]
        .as_str()
        .ok_or_else(|| io::Error::other("Missing token mint"))?;
    let _: Pubkey = mint.parse()?;
    let amount = info["tokenAmount"]["amount"]
        .as_str()
        .ok_or_else(|| io::Error::other("Missing raw token amount"))?
        .parse::<u64>()?;
    let decimals = u8::try_from(
        info["tokenAmount"]["decimals"]
            .as_u64()
            .ok_or_else(|| io::Error::other("Missing token decimals"))?,
    )?;
    let (name, symbol) = if mint == SNTOK_MINT {
        (
            token["name"]
                .as_str()
                .unwrap_or("SNOWNUMB TOKEN")
                .to_string(),
            token["symbol"].as_str().unwrap_or("SNTOK").to_string(),
        )
    } else if mint == WSOL_MINT {
        ("Wrapped SOL".into(), "SOL".into())
    } else {
        (
            "Token".into(),
            format!("{}…{}", &mint[..4], &mint[mint.len() - 4..]),
        )
    };
    Ok(
        json!({"account": account, "mint": mint, "program": program, "name": name, "symbol": symbol,
        "amount": amount.to_string(), "decimals": decimals}),
    )
}

fn tokens(client: &RpcClient, owner: Pubkey, token: &Value) -> Result<Value> {
    let mut rows = Vec::new();
    for program in [TOKEN_PROGRAM, TOKEN_2022_PROGRAM] {
        let accounts = client.get_token_accounts_by_owner_with_commitment(
            &owner,
            TokenAccountsFilter::ProgramId(program.parse()?),
            CommitmentConfig::confirmed(),
        )?;
        for account in accounts.value {
            let data = serde_json::to_value(account.account.data)?;
            rows.push(token_row(
                &account.pubkey,
                &account.account.owner,
                &data,
                owner,
                program,
                token,
            )?);
        }
    }
    rows.sort_by(|left, right| {
        left["mint"]
            .as_str()
            .cmp(&right["mint"].as_str())
            .then_with(|| left["account"].as_str().cmp(&right["account"].as_str()))
    });
    Ok(Value::Array(rows))
}

fn transactions(client: &RpcClient, address: Pubkey) -> Result<Value> {
    let signatures = client.get_signatures_for_address_with_config(
        &address,
        GetConfirmedSignaturesForAddress2Config {
            limit: Some(6),
            commitment: Some(CommitmentConfig::confirmed()),
            ..Default::default()
        },
    )?;
    Ok(Value::Array(
        signatures
            .into_iter()
            .map(|signature| {
                let status = signature
                    .confirmation_status
                    .and_then(|status| serde_json::to_value(status).ok())
                    .and_then(|status| status.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "unknown".into());
                json!({"signature": signature.signature, "slot": signature.slot,
            "success": signature.err.is_none(), "confirmation_status": status})
            })
            .collect(),
    ))
}

fn partial(result: Result<Value>, section: &str, errors: &mut Vec<Value>) -> Value {
    match result {
        Ok(value) => value,
        Err(error) => {
            errors.push(json!({"section": section, "message": error.to_string()}));
            Value::Null
        }
    }
}

fn dashboard(client: &RpcClient, options: &Options, wallet: Pubkey, token: &Value) -> Value {
    let mut errors = Vec::new();
    let balance = partial(
        (|| {
            let response =
                client.get_balance_with_commitment(&wallet, CommitmentConfig::confirmed())?;
            Ok(
                json!({"lamports": response.value.to_string(), "sol": exact_units(response.value, 9), "slot": response.context.slot}),
            )
        })(),
        "balance",
        &mut errors,
    );
    let tokens = partial(tokens(client, wallet, token), "tokens", &mut errors);
    let transactions = partial(transactions(client, wallet), "transactions", &mut errors);
    let programs = [("Orca Whirlpools", ORCA_PROGRAM), ("Token-2022", TOKEN_2022_PROGRAM)]
        .into_iter().map(|(name, address)| {
            match self::transactions(client, address.parse().expect("valid program id")) {
                Ok(transactions) => json!({"name": name, "address": address, "transactions": transactions, "error": null}),
                Err(error) => {
                    let message = error.to_string();
                    errors.push(json!({"section": "programs", "message": format!("{name}: {message}")}));
                    json!({"name": name, "address": address, "transactions": null, "error": message})
                }
            }
        }).collect::<Vec<_>>();
    json!({"network": "devnet", "rpc_url": options.url, "wallet": wallet.to_string(), "commitment": "confirmed",
        "balance": balance, "tokens": tokens, "transactions": transactions, "programs": programs, "errors": errors})
}

fn run() -> Result<()> {
    let Some(options) = parse_options(env::args().skip(1))? else {
        println!("{USAGE}");
        return Ok(());
    };
    let token: Value = serde_json::from_str(include_str!("../../token-client/metadata.json"))?;
    let client = RpcClient::new_sender(
        BoundedSender(HttpSender::new_with_timeout(
            options.url.clone(),
            RPC_TIMEOUT,
        )),
        RpcClientConfig::with_commitment(CommitmentConfig::confirmed()),
    );
    if client.get_genesis_hash()?.to_string() != DEVNET_GENESIS {
        return Err(io::Error::other("RPC is not Solana devnet").into());
    }
    let server = Server::http(("127.0.0.1", options.port))
        .map_err(|error| io::Error::other(error.to_string()))?;
    println!(
        "Dashboard: http://127.0.0.1:{}\nRPC: {}\nWallet: {}",
        options.port, options.url, options.wallet
    );
    for request in server.incoming_requests() {
        let (path, query) = request.url().split_once('?').unwrap_or((request.url(), ""));
        let (status, content_type, body) = if request.method() != &Method::Get {
            (
                405,
                "application/json; charset=utf-8",
                json!({"error": "Only GET is supported"}).to_string(),
            )
        } else {
            match path {
                "/" => (200, "text/html; charset=utf-8", include_str!("../web/index.html").to_string()),
                "/app.js" => (200, "text/javascript; charset=utf-8", include_str!("../web/app.js").to_string()),
                "/styles.css" => (200, "text/css; charset=utf-8", include_str!("../web/styles.css").to_string()),
                "/api/config" if query.is_empty() => (200, "application/json; charset=utf-8", json!({
                    "wallet": options.wallet.to_string(), "network": "devnet", "rpc_url": options.url,
                    "mint": SNTOK_MINT, "token": token,
                }).to_string()),
                "/api/dashboard" => match query_wallet(query, options.wallet) {
                    Ok(wallet) => (200, "application/json; charset=utf-8", dashboard(&client, &options, wallet, &token).to_string()),
                    Err(error) => (400, "application/json; charset=utf-8", json!({"error": error.to_string()}).to_string()),
                },
                "/api/config" => (400, "application/json; charset=utf-8", json!({"error": "Config accepts no query parameters"}).to_string()),
                _ => (404, "application/json; charset=utf-8", json!({"error": "Not found"}).to_string()),
            }
        };
        let response = Response::from_string(body)
            .with_status_code(status)
            .with_header(Header::from_bytes("Content-Type", content_type).expect("valid header"))
            .with_header(Header::from_bytes("Cache-Control", "no-store").expect("valid header"));
        if let Err(error) = request.respond(response) {
            eprintln!("HTTP response failed: {error}");
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_wallet_query_before_rpc() {
        let wallet = DEFAULT_WALLET.parse().unwrap();
        assert_eq!(query_wallet("", wallet).unwrap(), wallet);
        assert_eq!(
            query_wallet(&format!("wallet={DEFAULT_WALLET}"), wallet).unwrap(),
            wallet
        );
        for query in [
            "wallet=",
            "wallet=abc",
            "wallet=%39KmH",
            "wallet=a&wallet=b",
            "owner=abc",
            "wallet=a&x=b",
        ] {
            assert!(query_wallet(query, wallet).is_err());
        }
    }

    #[test]
    fn formats_large_balances_without_float_precision_loss() {
        assert_eq!(exact_units(u64::MAX, 9), "18446744073.709551615");
        assert_eq!(exact_units(9_007_199_254_740_993, 6), "9007199254.740993");
        assert_eq!(exact_units(1, 18), "0.000000000000000001");
        assert_eq!(exact_units(0, 9), "0");
    }

    #[test]
    fn preserves_token_raw_amount_and_rejects_wrong_owner() {
        let owner: Pubkey = DEFAULT_WALLET.parse().unwrap();
        let data = json!({"parsed": {"type": "account", "info": {"owner": DEFAULT_WALLET, "mint": SNTOK_MINT,
            "tokenAmount": {"amount": "9007199254740993", "decimals": 6}}}});
        let row = token_row(
            DEFAULT_WALLET,
            TOKEN_2022_PROGRAM,
            &data,
            owner,
            TOKEN_2022_PROGRAM,
            &json!({"name":"SNOWNUMB TOKEN","symbol":"SNTOK"}),
        )
        .unwrap();
        assert_eq!(row["amount"], "9007199254740993");
        assert_eq!(row["decimals"], 6);
        assert!(token_row(
            DEFAULT_WALLET,
            TOKEN_2022_PROGRAM,
            &data,
            SNTOK_MINT.parse().unwrap(),
            TOKEN_2022_PROGRAM,
            &json!({})
        )
        .is_err());
    }
}
