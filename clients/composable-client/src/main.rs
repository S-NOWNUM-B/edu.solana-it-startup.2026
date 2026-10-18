use anchor_lang::{prelude::Pubkey, InstructionData, ToAccountMetas};
use anchor_spl::{associated_token, token_2022};
use solana_commitment_config::CommitmentConfig;
use solana_keypair::Keypair;
use solana_message::Instruction;
use solana_rpc_client::rpc_client::RpcClient;
use solana_signer::Signer;
use solana_transaction::Transaction;
use std::{env, error::Error, io, process::ExitCode, thread, time::Duration};

const RPC: &str = "http://127.0.0.1:8899";
const UNIT: u64 = 1_000_000;
const SUPPLY: u64 = 1_000 * UNIT;
const AMOUNT: u64 = 125 * UNIT;
const LAMPORTS: u64 = 10_000_000;

fn is_public_cluster(genesis: &str) -> bool {
    matches!(
        genesis,
        "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"
            | "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"
            | "4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY"
    )
}

fn check(condition: bool, message: &str) -> Result<(), Box<dyn Error>> {
    if !condition {
        return Err(io::Error::other(message).into());
    }
    Ok(())
}

fn instruction(accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
    Instruction {
        program_id: solana_level_1_token_starter::ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

fn transaction(
    rpc: &RpcClient,
    payer: &Keypair,
    extra_signers: &[&Keypair],
    instructions: &[Instruction],
) -> Result<Transaction, Box<dyn Error>> {
    let mut signers = vec![payer];
    signers.extend_from_slice(extra_signers);
    let current = rpc.get_latest_blockhash()?;
    let blockhash = rpc.get_new_latest_blockhash(&current)?;
    Ok(Transaction::new_signed_with_payer(
        instructions,
        Some(&payer.pubkey()),
        &signers,
        blockhash,
    ))
}

fn simulate(rpc: &RpcClient, transaction: &Transaction) -> Result<bool, Box<dyn Error>> {
    let result = rpc.simulate_transaction(transaction)?.value;
    for line in result.logs.unwrap_or_default() {
        println!("{line}");
    }
    println!(
        "Simulation units: {:?}; error: {:?}",
        result.units_consumed, result.err
    );
    Ok(result.err.is_none())
}

fn token_balance(rpc: &RpcClient, account: &Pubkey) -> Result<u64, Box<dyn Error>> {
    Ok(rpc.get_token_account_balance(account)?.amount.parse()?)
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args == ["--help"] || args == ["-h"] {
        println!("Usage: composable-client demo\nLocalnet only: {RPC}\nUses in-memory wallets and a new local mint; no private keys are printed or saved.");
        return Ok(());
    }
    check(
        args == ["demo"],
        "Usage: composable-client demo (localnet only)",
    )?;
    let rpc = RpcClient::new_with_timeout_and_commitment(
        RPC,
        Duration::from_secs(30),
        CommitmentConfig::confirmed(),
    );
    let genesis = rpc.get_genesis_hash()?.to_string();
    check(
        !is_public_cluster(&genesis),
        "Public clusters are not allowed; run solana-test-validator",
    )?;
    let program = rpc.get_account(&solana_level_1_token_starter::ID)?;
    check(
        program.executable,
        "Load the compiled program into the local validator first",
    )?;
    let payer = Keypair::new();
    let recipient = Keypair::new().pubkey();
    let mint = Keypair::new();
    let token_program = token_2022::ID;
    let source = associated_token::get_associated_token_address_with_program_id(
        &payer.pubkey(),
        &mint.pubkey(),
        &token_program,
    );
    let destination = associated_token::get_associated_token_address_with_program_id(
        &recipient,
        &mint.pubkey(),
        &token_program,
    );
    let airdrop = rpc.request_airdrop(&payer.pubkey(), 2_000_000_000)?;
    let mut confirmed = false;
    for _ in 0..60 {
        if rpc.confirm_transaction(&airdrop)? {
            confirmed = true;
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }
    check(confirmed, "Local airdrop did not confirm")?;
    println!("RPC: {RPC}\nNetwork: localnet\nProgram: {}\nPayer: {}\nRecipient: {recipient}\nMint: {}\nSource ATA: {source}\nRecipient ATA: {destination}", solana_level_1_token_starter::ID, payer.pubkey(), mint.pubkey());
    let setup = [
        instruction(
            solana_level_1_token_starter::accounts::CreateToken {
                payer: payer.pubkey(),
                authority: payer.pubkey(),
                mint: mint.pubkey(),
                token_program,
                system_program: anchor_lang::system_program::ID,
            },
            solana_level_1_token_starter::instruction::CreateToken { decimals: 6 },
        ),
        instruction(
            solana_level_1_token_starter::accounts::CreateTokenAccount {
                payer: payer.pubkey(),
                owner: payer.pubkey(),
                mint: mint.pubkey(),
                token_account: source,
                token_program,
                associated_token_program: associated_token::ID,
                system_program: anchor_lang::system_program::ID,
            },
            solana_level_1_token_starter::instruction::CreateTokenAccount {},
        ),
        instruction(
            solana_level_1_token_starter::accounts::MintTokens {
                authority: payer.pubkey(),
                mint: mint.pubkey(),
                destination: source,
                token_program,
            },
            solana_level_1_token_starter::instruction::MintTokens { amount: SUPPLY },
        ),
    ];
    let setup_tx = transaction(&rpc, &payer, &[&mint], &setup)?;
    check(simulate(&rpc, &setup_tx)?, "Setup simulation failed")?;
    println!(
        "Setup transaction: {}",
        rpc.send_and_confirm_transaction(&setup_tx)?
    );
    let compose = |token_amount| {
        instruction(
            solana_level_1_token_starter::accounts::ComposeTransfer {
                payer: payer.pubkey(),
                recipient,
                mint: mint.pubkey(),
                source,
                recipient_token: destination,
                token_program,
                associated_token_program: associated_token::ID,
                system_program: anchor_lang::system_program::ID,
            },
            solana_level_1_token_starter::instruction::ComposeTransfer {
                token_amount,
                lamports: LAMPORTS,
            },
        )
    };
    println!("\n=== NEGATIVE SIMULATION: insufficient tokens ===");
    let before_sol = rpc.get_balance(&recipient)?;
    let rejected = transaction(&rpc, &payer, &[], &[compose(SUPPLY + 1)])?;
    check(
        !simulate(&rpc, &rejected)?,
        "Excessive transfer unexpectedly simulated successfully",
    )?;
    check(
        rpc.get_account_with_commitment(&destination, CommitmentConfig::confirmed())?
            .value
            .is_none(),
        "Recipient ATA appeared after simulation",
    )?;
    check(
        rpc.get_balance(&recipient)? == before_sol && token_balance(&rpc, &source)? == SUPPLY,
        "Simulation changed persisted state",
    )?;
    println!("PASS negative simulation: recipient ATA absent, SOL and token balances unchanged");
    for round in 1..=2_u64 {
        println!("\n=== COMPOSE TRANSFER round {round} ===");
        let tx = transaction(&rpc, &payer, &[], &[compose(AMOUNT)])?;
        check(simulate(&rpc, &tx)?, "Compose simulation failed")?;
        println!(
            "Compose transaction: {}",
            rpc.send_and_confirm_transaction(&tx)?
        );
        let actual_source = token_balance(&rpc, &source)?;
        let actual_destination = token_balance(&rpc, &destination)?;
        let actual_sol = rpc.get_balance(&recipient)?;
        check(
            actual_source == SUPPLY - AMOUNT * round && actual_destination == AMOUNT * round,
            "Token balances do not match composition",
        )?;
        check(
            actual_sol == before_sol + LAMPORTS * round,
            "Recipient SOL does not match composition",
        )?;
        check(
            rpc.get_account(&destination)?.owner == token_program,
            "Wrong destination program owner",
        )?;
        println!(
            "PASS RPC readback: source={} tokens, recipient={} tokens, recipient SOL={} lamports",
            actual_source / UNIT,
            actual_destination / UNIT,
            actual_sol
        );
    }
    println!("\nPASS: ATA + System + Token-2022 CPI; 750 / 250 tokens; recipient received 0.02 SOL; existing ATA reused");
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
    use super::is_public_cluster;

    #[test]
    fn rejects_full_public_genesis_hashes_even_through_loopback_rpc() {
        for genesis in [
            "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d",
            "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG",
            "4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY",
        ] {
            assert!(is_public_cluster(genesis));
        }
        assert!(!is_public_cluster("11111111111111111111111111111111"));
    }
}
