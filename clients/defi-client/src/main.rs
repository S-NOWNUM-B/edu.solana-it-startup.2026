use orca_whirlpools::{
    create_splash_pool_instructions, open_full_range_position_instructions, order_mints,
    swap_instructions, CreateSplashPoolConfig, IncreaseLiquidityParam, OpenFullRangePositionConfig,
    SwapConfig, SwapInstructions, SwapQuote, SwapType, WhirlpoolDeployment,
    SPLASH_POOL_TICK_SPACING,
};
use orca_whirlpools_client::{get_position_address, get_whirlpool_address, Position, Whirlpool};
use solana_commitment_config::CommitmentConfig;
use solana_keypair::{read_keypair_file, Keypair};
use solana_message::Instruction;
use solana_pubkey::Pubkey;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_api::config::{
    RpcSimulateTransactionAccountsConfig, RpcSimulateTransactionConfig,
};
use solana_signer::Signer;
use solana_transaction::Transaction;
use spl_associated_token_account_interface::address::get_associated_token_address_with_program_id;
use spl_token_2022_interface::{extension::StateWithExtensions, state::Account as TokenAccount};
use std::{env, error::Error, fs, io, path::PathBuf, process::ExitCode, time::Duration};

const SNTOK_MINT: &str = "DhjRbgdts2DNEvbfEVQkuGkpwwioEWW87LdHV11otozW";
const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";
const DEVNET_GENESIS_HASH: &str = "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG";
const LIQUIDITY_LAMPORTS: u64 = 5_000_000;
const LIQUIDITY_TOKENS: u64 = 500_000_000;
const SETUP_BUDGET_LAMPORTS: u64 = 180_000_000;
const DEFAULT_SWAP_LAMPORTS: u64 = 100_000;
const USAGE: &str = "SNTOK Orca client\n\
Usage: defi-client <setup|quote|swap> [options]\n\
Common: --url <RPC_URL> --keypair <PATH>\n\
setup: --position-keypair <PATH> (default .keys/defi-position.json)\n\
quote/swap: --input-lamports <INTEGER> --slippage-bps <1..1000>\n\
quote: --owner <PUBKEY> (read without a wallet file)\n\
Defaults: Solana devnet, ~/.config/solana/id.json, input 100000 lamports, slippage 100 bps\n\
setup reuses the pool and position; adds up to 0.005 SOL and 500 SNTOK once.";

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct Options {
    command: String,
    url: String,
    keypair: Option<PathBuf>,
    position_keypair: PathBuf,
    owner: Option<Pubkey>,
    input_lamports: u64,
    slippage_bps: u16,
}

fn parse_options(args: impl IntoIterator<Item = String>) -> Result<Option<Options>> {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return Ok(None);
    };
    if matches!(command.as_str(), "--help" | "-h") {
        return Ok(None);
    }
    if !matches!(command.as_str(), "setup" | "quote" | "swap") {
        return Err(io::Error::other(format!("Unknown command: {command}")).into());
    }
    let mut options = Options {
        command,
        url: "https://api.devnet.solana.com".into(),
        keypair: None,
        position_keypair: ".keys/defi-position.json".into(),
        owner: None,
        input_lamports: DEFAULT_SWAP_LAMPORTS,
        slippage_bps: 100,
    };
    while let Some(option) = args.next() {
        let allowed = matches!(option.as_str(), "--url" | "--keypair")
            || (options.command == "setup" && option == "--position-keypair")
            || (options.command != "setup"
                && matches!(option.as_str(), "--input-lamports" | "--slippage-bps"))
            || (options.command == "quote" && option == "--owner");
        if !allowed {
            return Err(io::Error::other(format!(
                "Invalid option for {}: {option}",
                options.command
            ))
            .into());
        }
        let value = args
            .next()
            .filter(|value| !value.starts_with("--"))
            .ok_or_else(|| io::Error::other(format!("Missing value for {option}")))?;
        match option.as_str() {
            "--url" => options.url = value,
            "--keypair" => options.keypair = Some(value.into()),
            "--position-keypair" => options.position_keypair = value.into(),
            "--owner" => options.owner = Some(value.parse()?),
            "--input-lamports" => options.input_lamports = value.parse()?,
            "--slippage-bps" => options.slippage_bps = value.parse()?,
            _ => unreachable!(),
        }
    }
    if options.input_lamports == 0 || !(1..=1000).contains(&options.slippage_bps) {
        return Err(
            io::Error::other("Input must be positive; slippage must be 1..1000 bps").into(),
        );
    }
    if options.owner.is_some() && options.keypair.is_some() {
        return Err(io::Error::other("Use either --owner or --keypair for quote").into());
    }
    Ok(Some(options))
}

fn read_payer(options: &Options) -> Result<Keypair> {
    let path = match &options.keypair {
        Some(path) => path.clone(),
        None => {
            PathBuf::from(env::var_os("HOME").ok_or_else(|| io::Error::other("Use --keypair"))?)
                .join(".config/solana/id.json")
        }
    };
    read_keypair_file(&path)
        .map_err(|_| io::Error::other(format!("Cannot read keypair: {}", path.display())).into())
}

async fn assert_devnet(client: &RpcClient) -> Result<()> {
    if client.get_genesis_hash().await?.to_string() != DEVNET_GENESIS_HASH {
        return Err(io::Error::other("RPC is not Solana devnet").into());
    }
    Ok(())
}

fn units(amount: u64, decimals: u32) -> String {
    let unit = 10_u64.pow(decimals);
    let fraction = format!("{:0width$}", amount % unit, width = decimals as usize);
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        (amount / unit).to_string()
    } else {
        format!("{}.{fraction}", amount / unit)
    }
}

async fn read_pool(client: &RpcClient, address: Pubkey) -> Result<Whirlpool> {
    let account = client.get_account(&address).await?;
    let deployment = WhirlpoolDeployment::devnet();
    let pool = Whirlpool::from_bytes(&account.data)?;
    let [token_a, token_b] = order_mints(WSOL_MINT.parse()?, SNTOK_MINT.parse()?);
    if account.owner != deployment.id()
        || pool.whirlpools_config != deployment.config_address()
        || pool.token_mint_a != token_a
        || pool.token_mint_b != token_b
        || pool.tick_spacing != SPLASH_POOL_TICK_SPACING
    {
        return Err(io::Error::other("Unexpected SNTOK pool account").into());
    }
    Ok(pool)
}

async fn token_balance(client: &RpcClient, owner: Pubkey) -> Result<u64> {
    let mint: Pubkey = SNTOK_MINT.parse()?;
    let program = spl_token_2022_interface::id();
    let address = get_associated_token_address_with_program_id(&owner, &mint, &program);
    let Some(account) = client
        .get_account_with_commitment(&address, CommitmentConfig::confirmed())
        .await?
        .value
    else {
        return Ok(0);
    };
    let state = StateWithExtensions::<TokenAccount>::unpack(&account.data)?;
    if account.owner != program || state.base.mint != mint || state.base.owner != owner {
        return Err(io::Error::other("Unexpected SNTOK token account").into());
    }
    Ok(state.base.amount)
}

async fn send(
    client: &RpcClient,
    payer: &Keypair,
    instructions: &[Instruction],
    additional_signers: &[Keypair],
    budget: u64,
) -> Result<u64> {
    assert_devnet(client).await?;
    let before = client.get_balance(&payer.pubkey()).await?;
    let mut signers = vec![payer];
    signers.extend(additional_signers.iter());
    let transaction = Transaction::new_signed_with_payer(
        instructions,
        Some(&payer.pubkey()),
        &signers,
        client.get_latest_blockhash().await?,
    );
    let simulation = client
        .simulate_transaction_with_config(
            &transaction,
            RpcSimulateTransactionConfig {
                sig_verify: true,
                commitment: Some(CommitmentConfig::confirmed()),
                accounts: Some(RpcSimulateTransactionAccountsConfig {
                    encoding: None,
                    addresses: vec![payer.pubkey().to_string()],
                }),
                ..Default::default()
            },
        )
        .await?;
    if let Some(error) = simulation.value.err {
        for line in simulation.value.logs.unwrap_or_default() {
            eprintln!("{line}");
        }
        return Err(io::Error::other(format!("Simulation failed: {error}")).into());
    }
    let simulated_balance = simulation
        .value
        .accounts
        .as_ref()
        .and_then(|accounts| accounts.first())
        .and_then(Option::as_ref)
        .ok_or_else(|| io::Error::other("Simulation did not return payer balance"))?
        .lamports;
    let simulated_spend = before.saturating_sub(simulated_balance);
    if simulated_spend > budget {
        return Err(io::Error::other(format!(
            "Simulated spend {simulated_spend} exceeds budget {budget} lamports"
        ))
        .into());
    }
    println!(
        "Simulation: PASS, estimated spend {} SOL",
        units(simulated_spend, 9)
    );
    let signature = client.send_and_confirm_transaction(&transaction).await?;
    println!("Transaction: {signature}");
    let after = client.get_balance(&payer.pubkey()).await?;
    Ok(before.saturating_sub(after))
}

fn save_position_keypair(keypair: &Keypair, path: &PathBuf) -> Result<()> {
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    serde_json::to_writer(file, &keypair.to_bytes().to_vec())?;
    Ok(())
}

async fn setup(
    client: &RpcClient,
    payer: &Keypair,
    options: &Options,
    pool_address: Pubkey,
) -> Result<()> {
    assert_devnet(client).await?;
    // Файл никогда не перезаписывается; существующая позиция повторно не пополняется.
    if options.position_keypair.exists() {
        let position_mint = read_keypair_file(&options.position_keypair)
            .map_err(|_| io::Error::other("Cannot read the position keypair"))?
            .pubkey();
        let address =
            get_position_address(&position_mint, Some(WhirlpoolDeployment::devnet().id()))?.0;
        let account = client.get_account(&address).await?;
        let position = Position::from_bytes(&account.data)?;
        let nft_ata = get_associated_token_address_with_program_id(
            &payer.pubkey(),
            &position_mint,
            &spl_token_2022_interface::id(),
        );
        let ownership = client.get_token_account_balance(&nft_ata).await?;
        if account.owner != WhirlpoolDeployment::devnet().id()
            || position.whirlpool != pool_address
            || position.position_mint != position_mint
            || position.liquidity == 0
            || ownership.amount != "1"
        {
            return Err(
                io::Error::other("Saved position does not match this wallet and pool").into(),
            );
        }
        read_pool(client, pool_address).await?;
        println!("Pool: {pool_address}\nPosition mint: {position_mint}\nLiquidity: {}\nSetup: existing position reused", position.liquidity);
        return Ok(());
    }
    if token_balance(client, payer.pubkey()).await? < LIQUIDITY_TOKENS {
        return Err(io::Error::other("Setup requires at least 500 SNTOK").into());
    }
    let mut budget = SETUP_BUDGET_LAMPORTS;
    let [token_a, token_b] = order_mints(WSOL_MINT.parse()?, SNTOK_MINT.parse()?);
    if client
        .get_account_with_commitment(&pool_address, CommitmentConfig::confirmed())
        .await?
        .value
        .is_none()
    {
        println!("Create SNTOK/wSOL Splash pool");
        let pool = create_splash_pool_instructions(
            client,
            token_a,
            token_b,
            CreateSplashPoolConfig {
                initial_price: Some(if token_a == WSOL_MINT.parse()? {
                    100_000.0
                } else {
                    0.00001
                }),
                funder: Some(payer.pubkey()),
                whirlpool_deployment: Some(WhirlpoolDeployment::devnet()),
            },
        )
        .await?;
        let spent = send(
            client,
            payer,
            &pool.instructions,
            &pool.additional_signers,
            budget,
        )
        .await?;
        budget = budget
            .checked_sub(spent)
            .ok_or_else(|| io::Error::other("Setup budget exceeded"))?;
    }
    read_pool(client, pool_address).await?;
    println!("Pool: {pool_address}\nAdd up to 0.005 SOL and 500 SNTOK");
    let (token_max_a, token_max_b) = if token_a == WSOL_MINT.parse()? {
        (LIQUIDITY_LAMPORTS, LIQUIDITY_TOKENS)
    } else {
        (LIQUIDITY_TOKENS, LIQUIDITY_LAMPORTS)
    };
    let position = open_full_range_position_instructions(
        client,
        pool_address,
        IncreaseLiquidityParam {
            token_max_a,
            token_max_b,
        },
        OpenFullRangePositionConfig {
            slippage_tolerance_bps: Some(100),
            funder: Some(payer.pubkey()),
            whirlpool_deployment: Some(WhirlpoolDeployment::devnet()),
        },
    )
    .await?;
    println!("Position mint: {}", position.position_mint);
    let position_keypair = position
        .additional_signers
        .iter()
        .find(|signer| signer.pubkey() == position.position_mint)
        .ok_or_else(|| io::Error::other("Position mint signer missing"))?;
    // Сохраняем до отправки, чтобы сбой RPC после подтверждения не создал вторую позицию.
    save_position_keypair(position_keypair, &options.position_keypair)?;
    send(
        client,
        payer,
        &position.instructions,
        &position.additional_signers,
        budget,
    )
    .await?;
    let address = get_position_address(
        &position.position_mint,
        Some(WhirlpoolDeployment::devnet().id()),
    )?
    .0;
    let state = Position::from_bytes(&client.get_account(&address).await?.data)?;
    if state.whirlpool != pool_address || state.liquidity == 0 {
        return Err(io::Error::other("Liquidity position verification failed").into());
    }
    println!(
        "Liquidity: {}\nPosition keypair: {}\nSetup: PASS",
        state.liquidity,
        options.position_keypair.display()
    );
    Ok(())
}

fn print_quote(swap: &SwapInstructions, input_lamports: u64, slippage_bps: u16) -> Result<u64> {
    let SwapQuote::ExactIn(quote) = &swap.quote else {
        return Err(io::Error::other("Expected an exact input quote").into());
    };
    if quote.token_in != input_lamports
        || quote.token_min_out == 0
        || quote.token_min_out > quote.token_est_out
    {
        return Err(io::Error::other("Invalid swap quote").into());
    }
    println!("Input: {} SOL\nEstimated output: {} SNTOK\nMinimum output: {} SNTOK\nSlippage: {slippage_bps} bps\nTrade fee: {} SOL",
        units(quote.token_in, 9), units(quote.token_est_out, 6), units(quote.token_min_out, 6), units(quote.trade_fee, 9));
    Ok(quote.token_min_out)
}

async fn run() -> Result<()> {
    let Some(options) = parse_options(env::args().skip(1))? else {
        println!("{USAGE}");
        return Ok(());
    };
    let client = RpcClient::new_with_timeout_and_commitment(
        options.url.clone(),
        Duration::from_secs(30),
        CommitmentConfig::confirmed(),
    );
    assert_devnet(&client).await?;
    let deployment = WhirlpoolDeployment::devnet();
    let [token_a, token_b] = order_mints(WSOL_MINT.parse()?, SNTOK_MINT.parse()?);
    let pool_address = get_whirlpool_address(
        &token_a,
        &token_b,
        SPLASH_POOL_TICK_SPACING,
        Some(deployment),
    )?
    .0;
    println!(
        "RPC: {}\nProgram: {}\nPool: {pool_address}",
        options.url,
        deployment.id()
    );
    if options.command == "setup" {
        return setup(&client, &read_payer(&options)?, &options, pool_address).await;
    }
    read_pool(&client, pool_address).await?;
    let payer = if options.owner.is_none() {
        Some(read_payer(&options)?)
    } else {
        None
    };
    let owner = options
        .owner
        .unwrap_or_else(|| payer.as_ref().expect("loaded payer").pubkey());
    let swap = swap_instructions(
        &client,
        pool_address,
        options.input_lamports,
        WSOL_MINT.parse()?,
        SwapType::ExactIn,
        SwapConfig {
            slippage_tolerance_bps: Some(options.slippage_bps),
            signer: Some(owner),
            whirlpool_deployment: Some(deployment),
        },
    )
    .await?;
    let min_out = print_quote(&swap, options.input_lamports, options.slippage_bps)?;
    if options.command == "quote" {
        return Ok(());
    }
    let payer = payer.expect("swap requires a wallet");
    let before_tokens = token_balance(&client, owner).await?;
    let budget = options
        .input_lamports
        .checked_add(100_000)
        .ok_or_else(|| io::Error::other("Input amount is too large"))?;
    let spent = send(
        &client,
        &payer,
        &swap.instructions,
        &swap.additional_signers,
        budget,
    )
    .await?;
    let after_tokens = token_balance(&client, owner).await?;
    let received = after_tokens
        .checked_sub(before_tokens)
        .ok_or_else(|| io::Error::other("SNTOK balance decreased"))?;
    if received < min_out {
        return Err(io::Error::other("Received tokens are below the minimum output").into());
    }
    println!(
        "SNTOK balance: {} -> {}\nReceived: {} SNTOK\nSOL spent including fees: {}\nSwap: PASS",
        units(before_tokens, 6),
        units(after_tokens, 6),
        units(received, 6),
        units(spent, 9)
    );
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
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

    fn parse(args: &[&str]) -> Result<Option<Options>> {
        parse_options(args.iter().map(|value| (*value).to_string()))
    }

    #[test]
    fn rejects_unsafe_amounts_and_command_options() {
        for args in [
            vec!["swap", "--input-lamports", "0"],
            vec!["swap", "--input-lamports", "0.0001"],
            vec!["swap", "--input-lamports", "-1"],
            vec!["swap", "--slippage-bps", "1001"],
            vec!["swap", "--slippage-bps", "0"],
            vec!["setup", "--input-lamports", "100"],
            vec!["swap", "--owner", WSOL_MINT],
            vec!["quote", "--owner", WSOL_MINT, "--keypair", "wallet.json"],
        ] {
            assert!(parse(&args).is_err(), "Accepted invalid args: {args:?}");
        }
    }

    #[test]
    fn quote_accepts_public_owner_without_loading_a_wallet() {
        let options = parse(&["quote", "--owner", WSOL_MINT, "--input-lamports", "123456"])
            .unwrap()
            .unwrap();
        assert_eq!(options.owner, Some(WSOL_MINT.parse().unwrap()));
        assert!(options.keypair.is_none());
        assert_eq!(options.input_lamports, 123456);
    }

    #[test]
    fn formats_token_quantities_without_float_rounding() {
        assert_eq!(units(100_000, 9), "0.0001");
        assert_eq!(units(875_000_123_456, 6), "875000.123456");
        assert_eq!(units(500_000_000, 6), "500");
        assert_eq!(units(0, 6), "0");
    }
}
