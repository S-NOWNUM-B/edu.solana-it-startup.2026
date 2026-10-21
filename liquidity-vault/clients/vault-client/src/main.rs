mod rpc;
mod vault;

use anchor_lang::prelude::Pubkey;
use std::{path::PathBuf, process::ExitCode};

use rpc::{fail, Client, Result, DEVNET_URL};
use vault::Addresses;

/// SNTOK из задания 7 — токен, который принимает vault.
const SNTOK_MINT: &str = "DhjRbgdts2DNEvbfEVQkuGkpwwioEWW87LdHV11otozW";
const USAGE: &str = "Liquidity Vault client\n\
Usage: vault-client <command> [--url <RPC_URL>] [--keypair <PATH>] [--mint <PUBKEY>]\n\
Commands:\n\
  init   create VaultState, vault token account and share mint (admin)\n\
  card   create the user card of the wallet\n\
  show   read vault and user card through RPC\n\
Defaults: Solana devnet, ../.keys/devnet-payer.json, SNTOK mint";

struct Options {
    command: String,
    url: String,
    keypair: PathBuf,
    mint: Pubkey,
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
    };
    while let Some(option) = args.next() {
        let Some(value) = args.next() else {
            return fail(format!("Missing value for {option}"));
        };
        match option.as_str() {
            "--url" => options.url = value,
            "--keypair" => options.keypair = value.into(),
            "--mint" => options.mint = value.parse()?,
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
    if !matches!(options.command.as_str(), "init" | "card" | "show") {
        return fail(format!("Unknown command: {}\n{USAGE}", options.command));
    }
    let client = Client::connect(&options.url, &options.keypair)?;
    let addresses = Addresses::load(&client, options.mint)?;
    match options.command.as_str() {
        "init" => vault::init(&client, &addresses),
        "card" => vault::create_card(&client, &addresses),
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
