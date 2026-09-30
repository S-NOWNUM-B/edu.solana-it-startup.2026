use anchor_lang::prelude::Pubkey;
use counter_client::{
    counter_address, decode_counter, increment_instruction, initialize_instruction,
};
use solana_commitment_config::CommitmentConfig;
use solana_keypair::{read_keypair_file, Keypair};
use solana_message::Instruction;
use solana_rpc_client::rpc_client::RpcClient;
use solana_signer::Signer;
use solana_transaction::Transaction;
use std::{env, error::Error, io, path::PathBuf, process::ExitCode, time::Duration};

const USAGE: &str = "Counter client\n\
Usage: counter-client <initialize|increment|show|demo> [--url <RPC_URL>] [--keypair <PATH>]\n\
Defaults: --url http://127.0.0.1:8899 --keypair ~/.config/solana/id.json\n\
demo initializes a missing counter and increments it three times.";

struct Options {
    command: String,
    url: String,
    keypair: PathBuf,
}

fn parse_options() -> Result<Option<Options>, Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        println!("{USAGE}");
        return Ok(None);
    };
    if command == "--help" || command == "-h" {
        println!("{USAGE}");
        return Ok(None);
    }
    if !matches!(
        command.as_str(),
        "initialize" | "increment" | "show" | "demo"
    ) {
        return Err(io::Error::other(format!("Unknown command: {command}\n{USAGE}")).into());
    }
    let mut url = "http://127.0.0.1:8899".to_string();
    let mut keypair = None;
    while let Some(option) = args.next() {
        if !matches!(option.as_str(), "--url" | "--keypair") {
            return Err(io::Error::other(format!("Unknown option: {option}")).into());
        }
        let value = args
            .next()
            .filter(|value| !value.starts_with("--"))
            .ok_or_else(|| io::Error::other(format!("Missing value for {option}")))?;
        match option.as_str() {
            "--url" => url = value,
            "--keypair" => keypair = Some(PathBuf::from(value)),
            _ => unreachable!(),
        }
    }
    let keypair = match keypair {
        Some(path) => path,
        None => PathBuf::from(
            env::var_os("HOME")
                .ok_or_else(|| io::Error::other("HOME is not set; use --keypair"))?,
        )
        .join(".config/solana/id.json"),
    };
    Ok(Some(Options {
        command,
        url,
        keypair,
    }))
}

fn send(
    client: &RpcClient,
    payer: &Keypair,
    instruction: Instruction,
) -> Result<(), Box<dyn Error>> {
    // Свежий blockhash отличает подпись от предыдущего вызова той же инструкции.
    let current_blockhash = client.get_latest_blockhash()?;
    let blockhash = client.get_new_latest_blockhash(&current_blockhash)?;
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &[payer],
        blockhash,
    );
    let signature = client.send_and_confirm_transaction(&transaction)?;
    println!("Transaction: {signature}");
    Ok(())
}

fn show(client: &RpcClient, authority: &Pubkey) -> Result<(), Box<dyn Error>> {
    let address = counter_address(authority).0;
    let account = client.get_account(&address)?;
    let counter = decode_counter(&address, &account.owner, &account.data, authority)?;
    println!("Count: {}", counter.count);
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let Some(options) = parse_options()? else {
        return Ok(());
    };
    let payer = read_keypair_file(&options.keypair).map_err(|_| {
        io::Error::other(format!(
            "Cannot read keypair: {}",
            options.keypair.display()
        ))
    })?;
    let authority = payer.pubkey();
    let address = counter_address(&authority).0;
    let client = RpcClient::new_with_timeout_and_commitment(
        options.url,
        Duration::from_secs(30),
        CommitmentConfig::confirmed(),
    );
    println!(
        "Program: {}\nAuthority: {authority}\nCounter: {address}",
        counter::ID
    );

    match options.command.as_str() {
        "initialize" => {
            send(&client, &payer, initialize_instruction(authority))?;
            show(&client, &authority)?;
        }
        "increment" => {
            send(&client, &payer, increment_instruction(authority))?;
            show(&client, &authority)?;
        }
        "show" => show(&client, &authority)?,
        "demo" => {
            let account = client
                .get_account_with_commitment(&address, CommitmentConfig::confirmed())?
                .value;
            match account {
                Some(account) => {
                    decode_counter(&address, &account.owner, &account.data, &authority)?;
                }
                None => send(&client, &payer, initialize_instruction(authority))?,
            }
            show(&client, &authority)?;
            for _ in 0..3 {
                send(&client, &payer, increment_instruction(authority))?;
                show(&client, &authority)?;
            }
        }
        _ => unreachable!(),
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
