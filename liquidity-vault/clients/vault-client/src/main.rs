mod pool;
mod position;
mod raydium;
mod rpc;
mod strategy;
mod vault;

use anchor_lang::prelude::Pubkey;
use std::{path::PathBuf, process::ExitCode};

use pool::Pair;
use rpc::{fail, Client, Result, DEVNET_URL};
use vault::Addresses;

/// SNTOK из задания 7 — токен, который принимает vault.
const SNTOK_MINT: &str = "DhjRbgdts2DNEvbfEVQkuGkpwwioEWW87LdHV11otozW";
/// Второй токен пула Raydium в devnet; для localnet передайте --quote-mint.
const QUOTE_MINT: Option<&str> = Some("YkBaLgL7tK1ffq3vrb5qXqeJZ1rk6LpHSqZvgDXsoFm");
const USAGE: &str = "Liquidity Vault client\n\
Usage: vault-client <command> [--url <RPC_URL>] [--keypair <PATH>] [--mint <PUBKEY>] [--amount <TOKENS>]\n\
       [--quote-mint <PUBKEY>] [--price <QUOTE_PER_SNTOK>] [--range <PERCENT>] [--position <NFT_MINT>]\n\
       [--direction sell|buy] [--slippage-bps <1..1000>] [--reject]\n\
Commands:\n\
  init     create VaultState, vault token account and share mint (admin)\n\
  card     create the user card of the wallet\n\
  deposit  --amount <TOKENS>: deposit tokens and receive share tokens\n\
  withdraw --amount <SHARES>: burn share tokens and receive tokens back\n\
  show     read vault and user card through RPC\n\
  create-token   create the second pool token and mint it to the wallet\n\
  create-pool    --price <P>: create Raydium CLMM pool SNTOK/second token\n\
  open-position  --range <PCT> --amount <SECOND_TOKEN>: wallet position around the price\n\
  pool           [--position <NFT_MINT>]: read pool price, liquidity and a position\n\
  strategy       connect the pool to the vault and create the second vault token account (admin)\n\
  vault-swap     --direction sell|buy --amount <TOKENS> [--slippage-bps 100] [--reject]:\n\
                 swap vault tokens through the pool; --reject doubles the minimum to show a rejection\n\
  position-open    --range <PCT> --amount <SECOND_TOKEN>: vault position around the price (admin)\n\
  position-collect collect position fees into the vault (admin)\n\
  position-close   [--slippage-bps 100]: withdraw all liquidity and close the position (admin)\n\
  rebalance        --range <PCT> --amount <SECOND_TOKEN>: close and reopen around the current price\n\
  position         read the vault position\n\
  trade            --direction sell|buy --amount <TOKENS>: wallet swap in the pool as an outside trader\n\
Defaults: Solana devnet, ../.keys/devnet-payer.json, SNTOK mint";

const COMMANDS: &[&str] = &[
    "init",
    "card",
    "deposit",
    "withdraw",
    "show",
    "create-token",
    "create-pool",
    "open-position",
    "pool",
    "strategy",
    "vault-swap",
    "position-open",
    "position-collect",
    "position-close",
    "rebalance",
    "position",
    "trade",
];

struct Options {
    command: String,
    url: String,
    keypair: PathBuf,
    mint: Pubkey,
    amount: Option<String>,
    quote_mint: Option<Pubkey>,
    price: Option<String>,
    range: Option<String>,
    position: Option<Pubkey>,
    direction: Option<String>,
    slippage_bps: u64,
    reject: bool,
}

fn parse_options(args: impl IntoIterator<Item = String>) -> Result<Option<Options>> {
    let mut args = args.into_iter();
    let Some(command) = args.next().filter(|command| !command.starts_with('-')) else {
        return Ok(None);
    };
    let mut options = Options {
        command,
        url: DEVNET_URL.into(),
        keypair: "../.keys/devnet-payer.json".into(),
        mint: SNTOK_MINT.parse()?,
        amount: None,
        quote_mint: QUOTE_MINT.map(str::parse).transpose()?,
        price: None,
        range: None,
        position: None,
        direction: None,
        slippage_bps: 100,
        reject: false,
    };
    while let Some(option) = args.next() {
        if option == "--reject" {
            options.reject = true;
            continue;
        }
        let Some(value) = args.next() else {
            return fail(format!("Missing value for {option}"));
        };
        match option.as_str() {
            "--url" => options.url = value,
            "--keypair" => options.keypair = value.into(),
            "--mint" => options.mint = value.parse()?,
            "--amount" => options.amount = Some(value),
            "--quote-mint" => options.quote_mint = Some(value.parse()?),
            "--price" => options.price = Some(value),
            "--range" => options.range = Some(value),
            "--position" => options.position = Some(value.parse()?),
            "--direction" => options.direction = Some(value),
            "--slippage-bps" => options.slippage_bps = value.parse()?,
            _ => return fail(format!("Unknown option: {option}")),
        }
    }
    Ok(Some(options))
}

fn run() -> Result<()> {
    let Some(options) = parse_options(std::env::args().skip(1))? else {
        println!("{USAGE}");
        return Ok(());
    };
    if !COMMANDS.contains(&options.command.as_str()) {
        return fail(format!("Unknown command: {}\n{USAGE}", options.command));
    }
    let required = |value: &Option<String>, name: &str| {
        value
            .clone()
            .ok_or_else(|| format!("{} requires --{name}", options.command))
    };
    let amount = || required(&options.amount, "amount");
    let pair = || -> Result<Pair> {
        Ok(Pair {
            base: options.mint,
            quote: options
                .quote_mint
                .ok_or("Pass --quote-mint with the second pool token")?,
        })
    };
    let client = Client::connect(&options.url, &options.keypair)?;
    let addresses = Addresses::load(&client, options.mint)?;
    match options.command.as_str() {
        "init" => vault::init(&client, &addresses),
        "card" => vault::create_card(&client, &addresses),
        "deposit" => vault::deposit(&client, &addresses, &amount()?),
        "withdraw" => vault::withdraw(&client, &addresses, &amount()?),
        "create-token" => pool::create_token(&client),
        "create-pool" => pool::create_pool(&client, &pair()?, &required(&options.price, "price")?),
        "open-position" => pool::open_position(
            &client,
            &pair()?,
            &required(&options.range, "range")?,
            &amount()?,
        ),
        "pool" => pool::show(&client, &pair()?, options.position),
        "strategy" => strategy::init(&client, &addresses, &pair()?),
        "vault-swap" => strategy::swap(
            &client,
            &addresses,
            &pair()?,
            &required(&options.direction, "direction")?,
            &amount()?,
            options.slippage_bps,
            options.reject,
        ),
        "position-open" => position::open(
            &client,
            &addresses,
            &pair()?,
            &required(&options.range, "range")?,
            &amount()?,
        ),
        "position-collect" => position::collect(&client, &addresses, &pair()?),
        "position-close" => position::close(&client, &addresses, &pair()?, options.slippage_bps),
        "rebalance" => {
            let pair = pair()?;
            position::close(&client, &addresses, &pair, options.slippage_bps)?;
            position::open(
                &client,
                &addresses,
                &pair,
                &required(&options.range, "range")?,
                &amount()?,
            )
        }
        "position" => position::show(&client, &addresses, &pair()?),
        "trade" => position::trade(
            &client,
            &pair()?,
            &required(&options.direction, "direction")?,
            &amount()?,
            options.slippage_bps,
        ),
        _ => vault::show(&client, &addresses),
    }
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
    use super::parse_options;
    use crate::rpc::network_name;

    #[test]
    fn allows_only_devnet_and_local_clusters() {
        assert_eq!(
            network_name("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG").unwrap(),
            "devnet"
        );
        assert!(network_name("5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d").is_err());
        assert!(network_name("4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY").is_err());
        assert_eq!(network_name("local").unwrap(), "localnet");
    }

    #[test]
    fn parses_command_and_options() {
        let args = ["show", "--url", "http://127.0.0.1:8899"].map(String::from);
        let options = parse_options(args).unwrap().unwrap();
        assert_eq!(options.command, "show");
        assert_eq!(options.url, "http://127.0.0.1:8899");
        assert!(parse_options(["show", "--bad", "x"].map(String::from)).is_err());
        assert!(parse_options(Vec::<String>::new()).unwrap().is_none());
    }
}
