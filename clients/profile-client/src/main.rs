use anchor_lang::prelude::Pubkey;
use onchain_profile::{validate_fields, Profile};
use profile_client::{decode_profile, initialize_instruction, profile_address, update_instruction};
use solana_commitment_config::CommitmentConfig;
use solana_keypair::{read_keypair_file, Keypair};
use solana_message::Instruction;
use solana_rpc_client::rpc_client::RpcClient;
use solana_signer::Signer;
use solana_transaction::Transaction;
use std::{env, error::Error, io, path::PathBuf, process::ExitCode, time::Duration};

const USAGE: &str = "Profile client\n\
Usage: profile-client <create|update|show|demo> [options]\n\
create/update: --name <NAME> --bio <BIO>\n\
show: [--authority <PUBKEY>]\n\
Common: --url <RPC_URL> --keypair <PATH>\n\
Defaults: http://127.0.0.1:8899, ~/.config/solana/id.json";

struct Options {
    command: String,
    url: String,
    keypair: Option<PathBuf>,
    authority: Option<Pubkey>,
    name: Option<String>,
    bio: Option<String>,
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
    if !matches!(command.as_str(), "create" | "update" | "show" | "demo") {
        return Err(io::Error::other(format!("Unknown command: {command}\n{USAGE}")).into());
    }
    let mut options = Options {
        command,
        url: "http://127.0.0.1:8899".into(),
        keypair: None,
        authority: None,
        name: None,
        bio: None,
    };
    while let Some(option) = args.next() {
        if !matches!(
            option.as_str(),
            "--url" | "--keypair" | "--authority" | "--name" | "--bio"
        ) {
            return Err(io::Error::other(format!("Unknown option: {option}")).into());
        }
        let value = args
            .next()
            .filter(|value| {
                matches!(option.as_str(), "--name" | "--bio") || !value.starts_with("--")
            })
            .ok_or_else(|| io::Error::other(format!("Missing value for {option}")))?;
        match option.as_str() {
            "--url" => options.url = value,
            "--keypair" => options.keypair = Some(value.into()),
            "--authority" => options.authority = Some(value.parse()?),
            "--name" => options.name = Some(value),
            "--bio" => options.bio = Some(value),
            _ => unreachable!(),
        }
    }
    if options.authority.is_some() && options.command != "show" {
        return Err(io::Error::other("--authority is only valid for show").into());
    }
    let editing = matches!(options.command.as_str(), "create" | "update");
    if editing {
        let name = options
            .name
            .as_deref()
            .ok_or_else(|| io::Error::other("--name is required"))?;
        let bio = options
            .bio
            .as_deref()
            .ok_or_else(|| io::Error::other("--bio is required (may be empty)"))?;
        validate_fields(name, bio)?;
    } else if options.name.is_some() || options.bio.is_some() {
        return Err(io::Error::other("--name and --bio are only valid for create/update").into());
    }
    Ok(Some(options))
}

fn payer(options: &Options) -> Result<Keypair, Box<dyn Error>> {
    let path = match &options.keypair {
        Some(path) => path.clone(),
        None => PathBuf::from(
            env::var_os("HOME")
                .ok_or_else(|| io::Error::other("HOME is not set; use --keypair"))?,
        )
        .join(".config/solana/id.json"),
    };
    read_keypair_file(&path)
        .map_err(|_| io::Error::other(format!("Cannot read keypair: {}", path.display())).into())
}

fn send(
    client: &RpcClient,
    payer: &Keypair,
    instruction: Instruction,
) -> Result<(), Box<dyn Error>> {
    let current = client.get_latest_blockhash()?;
    let blockhash = client.get_new_latest_blockhash(&current)?;
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

fn show(client: &RpcClient, authority: &Pubkey) -> Result<Profile, Box<dyn Error>> {
    let address = profile_address(authority).0;
    let account = client.get_account(&address)?;
    let profile = decode_profile(&address, &account.owner, &account.data, authority)?;
    println!(
        "Profile: {address}\nAuthority: {}\nName: {}\nBio: {}\nBump: {}",
        profile.authority, profile.display_name, profile.bio, profile.bump
    );
    Ok(profile)
}

fn run() -> Result<(), Box<dyn Error>> {
    let Some(options) = parse_options()? else {
        return Ok(());
    };
    let client = RpcClient::new_with_timeout_and_commitment(
        options.url.clone(),
        Duration::from_secs(30),
        CommitmentConfig::confirmed(),
    );
    println!("Program: {}", onchain_profile::ID);
    if options.command == "show" {
        let authority = match options.authority {
            Some(authority) => authority,
            None => payer(&options)?.pubkey(),
        };
        show(&client, &authority)?;
        return Ok(());
    }
    let payer = payer(&options)?;
    let authority = payer.pubkey();
    match options.command.as_str() {
        "create" | "update" => {
            let name = options.name.expect("validated name");
            let bio = options.bio.expect("validated bio");
            let instruction = if options.command == "create" {
                initialize_instruction(authority, name, bio)
            } else {
                update_instruction(authority, name, bio)
            };
            send(&client, &payer, instruction)?;
            show(&client, &authority)?;
        }
        "demo" => {
            let address = profile_address(&authority).0;
            let account = client
                .get_account_with_commitment(&address, CommitmentConfig::confirmed())?
                .value;
            let name = match account {
                Some(account) => {
                    decode_profile(&address, &account.owner, &account.data, &authority)?
                        .display_name
                }
                None => {
                    send(
                        &client,
                        &payer,
                        initialize_instruction(
                            authority,
                            "SNOWNUMB".into(),
                            "Solana profile".into(),
                        ),
                    )?;
                    "SNOWNUMB".into()
                }
            };
            show(&client, &authority)?;
            send(
                &client,
                &payer,
                update_instruction(authority, name, "Onchain profile with PDA".into()),
            )?;
            show(&client, &authority)?;
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
