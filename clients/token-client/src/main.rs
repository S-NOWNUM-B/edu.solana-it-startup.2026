use serde::Deserialize;
use solana_commitment_config::CommitmentConfig;
use solana_keypair::{read_keypair_file, Keypair};
use solana_message::Instruction;
use solana_pubkey::Pubkey;
use solana_rpc_client::rpc_client::RpcClient;
use solana_signer::Signer;
use solana_system_interface::instruction::{create_account, transfer};
use solana_transaction::Transaction;
use spl_associated_token_account_interface::{
    address::get_associated_token_address_with_program_id,
    instruction::create_associated_token_account_idempotent,
};
use spl_token_2022_interface::{
    extension::{
        metadata_pointer::{instruction as metadata_pointer, MetadataPointer},
        BaseStateWithExtensions, ExtensionType, StateWithExtensions,
    },
    instruction::{initialize_mint2, mint_to_checked, transfer_checked},
    state::{Account, Mint},
};
use spl_token_metadata_interface::{
    instruction as metadata_instruction,
    state::{Field, TokenMetadata},
};
use std::{env, error::Error, io, path::PathBuf, process::ExitCode, time::Duration};

const URI: &str = "https://raw.githubusercontent.com/S-NOWNUM-B/edu.solana-it-startup.2026/task/09-solana-defi/clients/token-client/metadata.json";
const DECIMALS: u8 = 6;
const UNIT: u64 = 1_000_000;
const SUPPLY: u64 = 1_000_000 * UNIT;
const TRANSFER: u64 = 125_000 * UNIT;
const USAGE: &str = "Token-2022 client\n\
Usage: token-client demo --recipient <PUBKEY> [--keypair <PATH>] [--url <RPC_URL>]\n\
       token-client show <MINT> [--url <RPC_URL>]\n\
       token-client update-metadata <MINT> [--keypair <PATH>] [--url <RPC_URL>]\n\
Defaults: http://127.0.0.1:8899, ~/.config/solana/id.json\n\
demo creates a NEW mint on each run; use only localnet or devnet.";

#[derive(Deserialize)]
struct TokenInfo {
    name: String,
    symbol: String,
    description: String,
}

fn token_info() -> Result<TokenInfo, Box<dyn Error>> {
    Ok(serde_json::from_str(include_str!("../metadata.json"))?)
}

struct Options {
    command: String,
    url: String,
    keypair: Option<PathBuf>,
    recipient: Option<Pubkey>,
    mint: Option<Pubkey>,
}

fn parse_options() -> Result<Option<Options>, Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        println!("{USAGE}");
        return Ok(None);
    };
    if matches!(command.as_str(), "--help" | "-h") {
        println!("{USAGE}");
        return Ok(None);
    }
    if !matches!(command.as_str(), "demo" | "show" | "update-metadata") {
        return Err(io::Error::other(format!("Unknown command: {command}\n{USAGE}")).into());
    }
    let mint = if command != "demo" {
        Some(
            args.next()
                .ok_or_else(|| io::Error::other("This command requires a mint address"))?
                .parse()?,
        )
    } else {
        None
    };
    let mut options = Options {
        command,
        url: "http://127.0.0.1:8899".into(),
        keypair: None,
        recipient: None,
        mint,
    };
    while let Some(option) = args.next() {
        if !matches!(option.as_str(), "--url" | "--keypair" | "--recipient") {
            return Err(io::Error::other(format!("Unknown option: {option}")).into());
        }
        let value = args
            .next()
            .filter(|value| !value.starts_with("--"))
            .ok_or_else(|| io::Error::other(format!("Missing value for {option}")))?;
        match option.as_str() {
            "--url" => options.url = value,
            "--keypair" => options.keypair = Some(value.into()),
            "--recipient" => options.recipient = Some(value.parse()?),
            _ => unreachable!(),
        }
    }
    if options.command == "demo" && options.recipient.is_none() {
        return Err(io::Error::other("demo requires --recipient <PUBKEY>").into());
    }
    if options.command == "update-metadata" && options.recipient.is_some() {
        return Err(io::Error::other("update-metadata does not accept --recipient").into());
    }
    if options.command == "show" && (options.recipient.is_some() || options.keypair.is_some()) {
        return Err(io::Error::other("show only accepts --url; reading needs no wallet").into());
    }
    Ok(Some(options))
}

fn send(
    client: &RpcClient,
    payer: &Keypair,
    instructions: &[Instruction],
    mint: Option<&Keypair>,
) -> Result<(), Box<dyn Error>> {
    let mut signers = vec![payer];
    if let Some(mint) = mint {
        signers.push(mint);
    }
    let transaction = Transaction::new_signed_with_payer(
        instructions,
        Some(&payer.pubkey()),
        &signers,
        client.get_latest_blockhash()?,
    );
    println!(
        "Transaction: {}",
        client.send_and_confirm_transaction(&transaction)?
    );
    Ok(())
}

fn show(client: &RpcClient, mint: &Pubkey) -> Result<(Mint, TokenMetadata), Box<dyn Error>> {
    let account = client.get_account(mint)?;
    if account.owner != spl_token_2022_interface::id() {
        return Err(io::Error::other("Mint is not owned by Token-2022").into());
    }
    let state = StateWithExtensions::<Mint>::unpack(&account.data)?;
    let pointer = state.get_extension::<MetadataPointer>()?;
    let metadata = state.get_variable_len_extension::<TokenMetadata>()?;
    if Option::<Pubkey>::from(pointer.metadata_address) != Some(*mint) || metadata.mint != *mint {
        return Err(io::Error::other("Metadata does not belong to this mint").into());
    }
    println!("Mint: {mint}\nProgram: {}\nName: {}\nSymbol: {}\nDecimals: {}\nSupply (raw): {}\nMetadata pointer: {mint}\nURI: {}",
        account.owner, metadata.name, metadata.symbol, state.base.decimals, state.base.supply, metadata.uri);
    if let Some((_, description)) = metadata
        .additional_metadata
        .iter()
        .find(|(key, _)| key == "description")
    {
        println!("Description: {description}");
    }
    Ok((state.base, metadata))
}

fn balance(
    client: &RpcClient,
    address: &Pubkey,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Result<u64, Box<dyn Error>> {
    let account = client.get_account(address)?;
    if account.owner != spl_token_2022_interface::id() {
        return Err(io::Error::other("Token account is not owned by Token-2022").into());
    }
    let state = StateWithExtensions::<Account>::unpack(&account.data)?;
    if state.base.mint != *mint || state.base.owner != *owner {
        return Err(io::Error::other("Unexpected token account mint or owner").into());
    }
    Ok(state.base.amount)
}

fn demo(client: &RpcClient, payer: &Keypair, recipient: Pubkey) -> Result<(), Box<dyn Error>> {
    let info = token_info()?;
    let symbol = &info.symbol;
    let authority = payer.pubkey();
    if authority == recipient {
        return Err(io::Error::other("Recipient must differ from the sender").into());
    }
    let mint = Keypair::new();
    let mint_address = mint.pubkey();
    let program = spl_token_2022_interface::id();
    let metadata = TokenMetadata {
        update_authority: Some(authority).try_into()?,
        mint: mint_address,
        name: info.name.clone(),
        symbol: info.symbol.clone(),
        uri: URI.into(),
        additional_metadata: vec![("description".into(), info.description.clone())],
    };
    let space =
        ExtensionType::try_calculate_account_len::<Mint>(&[ExtensionType::MetadataPointer])?;
    // Metadata увеличивает аккаунт после initialize_mint2; rent оплачивается заранее.
    let rent = client.get_minimum_balance_for_rent_exemption(space + metadata.tlv_size_of()?)?;
    println!("STEP 1  Create mint and onchain metadata");
    send(
        client,
        payer,
        &[
            create_account(&authority, &mint_address, rent, space as u64, &program),
            metadata_pointer::initialize(
                &program,
                &mint_address,
                Some(authority),
                Some(mint_address),
            )?,
            initialize_mint2(&program, &mint_address, &authority, None, DECIMALS)?,
            metadata_instruction::initialize(
                &program,
                &mint_address,
                &authority,
                &mint_address,
                &authority,
                info.name.clone(),
                info.symbol.clone(),
                URI.into(),
            ),
            metadata_instruction::update_field(
                &program,
                &mint_address,
                &authority,
                Field::Key("description".into()),
                info.description.clone(),
            ),
        ],
        Some(&mint),
    )?;
    let (created, stored_metadata) = show(client, &mint_address)?;
    if created.supply != 0 || created.decimals != DECIMALS || stored_metadata != metadata {
        return Err(io::Error::other("Unexpected mint or metadata after creation").into());
    }

    let source = get_associated_token_address_with_program_id(&authority, &mint_address, &program);
    let destination =
        get_associated_token_address_with_program_id(&recipient, &mint_address, &program);
    println!("\nSTEP 2  Mint 1000000 {symbol}\nSender: {authority}\nSender ATA: {source}\nRecipient: {recipient}\nRecipient ATA: {destination}");
    send(
        client,
        payer,
        &[
            create_associated_token_account_idempotent(
                &authority,
                &authority,
                &mint_address,
                &program,
            ),
            create_associated_token_account_idempotent(
                &authority,
                &recipient,
                &mint_address,
                &program,
            ),
            mint_to_checked(
                &program,
                &mint_address,
                &source,
                &authority,
                &[],
                SUPPLY,
                DECIMALS,
            )?,
        ],
        None,
    )?;
    let before_source = balance(client, &source, &mint_address, &authority)?;
    let before_destination = balance(client, &destination, &mint_address, &recipient)?;
    let before_supply = client
        .get_token_supply(&mint_address)?
        .amount
        .parse::<u64>()?;
    if before_source != SUPPLY || before_destination != 0 || before_supply != SUPPLY {
        return Err(io::Error::other("Unexpected balances or supply after minting").into());
    }
    println!("Before transfer: sender = 1000000 {symbol}, recipient = 0 {symbol}");

    println!("\nSTEP 3  Transfer 125000 {symbol} with transfer_checked");
    send(
        client,
        payer,
        &[transfer_checked(
            &program,
            &source,
            &mint_address,
            &destination,
            &authority,
            &[],
            TRANSFER,
            created.decimals,
        )?],
        None,
    )?;
    let after_source = balance(client, &source, &mint_address, &authority)?;
    let after_destination = balance(client, &destination, &mint_address, &recipient)?;
    let (final_mint, final_metadata) = show(client, &mint_address)?;
    if after_source != SUPPLY - TRANSFER
        || after_destination != TRANSFER
        || final_mint.supply != SUPPLY
        || final_metadata != metadata
    {
        return Err(io::Error::other("Unexpected final balances, supply or metadata").into());
    }
    println!("After transfer: sender = {} {symbol}, recipient = {} {symbol}\nSupply = {} {symbol} (unchanged)\nPASS: metadata, minting, transfer and RPC reads",
        after_source / UNIT, after_destination / UNIT, final_mint.supply / UNIT);
    Ok(())
}

fn update_metadata(
    client: &RpcClient,
    payer: &Keypair,
    mint: &Pubkey,
) -> Result<(), Box<dyn Error>> {
    let info = token_info()?;
    let (before, mut expected) = show(client, mint)?;
    if Option::<Pubkey>::from(expected.update_authority) != Some(payer.pubkey()) {
        return Err(io::Error::other("Wallet is not the metadata update authority").into());
    }
    let account = client.get_account(mint)?;
    let old_metadata_size = expected.tlv_size_of()?;
    let mut max_size = account.data.len();
    let fields = [
        (Field::Name, info.name),
        (Field::Symbol, info.symbol),
        (Field::Uri, URI.into()),
        (Field::Key("description".into()), info.description),
    ];
    let mut instructions = Vec::new();
    for (field, value) in fields {
        expected.update(field.clone(), value.clone());
        max_size = max_size.max(account.data.len() - old_metadata_size + expected.tlv_size_of()?);
        instructions.push(metadata_instruction::update_field(
            &spl_token_2022_interface::id(),
            mint,
            &payer.pubkey(),
            field,
            value,
        ));
    }
    let rent = client.get_minimum_balance_for_rent_exemption(max_size)?;
    if rent > account.lamports {
        instructions.insert(0, transfer(&payer.pubkey(), mint, rent - account.lamports));
    }
    send(client, payer, &instructions, None)?;
    let (after, metadata) = show(client, mint)?;
    if after != before || metadata != expected {
        return Err(io::Error::other("Metadata update verification failed").into());
    }
    println!("PASS: metadata updated; mint address, decimals, authorities and supply preserved");
    Ok(())
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
    println!("RPC: {}", options.url);
    if options.command == "show" {
        show(&client, &options.mint.expect("validated mint"))?;
        return Ok(());
    }
    let path = match options.keypair {
        Some(path) => path,
        None => PathBuf::from(
            env::var_os("HOME")
                .ok_or_else(|| io::Error::other("HOME is not set; use --keypair"))?,
        )
        .join(".config/solana/id.json"),
    };
    let payer = read_keypair_file(&path)
        .map_err(|_| io::Error::other(format!("Cannot read keypair: {}", path.display())))?;
    if options.command == "update-metadata" {
        return update_metadata(&client, &payer, &options.mint.expect("validated mint"));
    }
    demo(
        &client,
        &payer,
        options.recipient.expect("validated recipient"),
    )
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
