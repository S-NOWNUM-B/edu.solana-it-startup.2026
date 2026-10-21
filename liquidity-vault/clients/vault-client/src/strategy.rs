use anchor_lang::prelude::Pubkey;
use liquidity_vault::state::{Strategy, SECOND_TOKENS_SEED, STRATEGY_SEED};
use solana_message::Instruction;
use solana_signer::Signer;

use crate::{
    pool::Pair,
    raydium,
    rpc::{fail, vault_instruction, Client, Result},
    vault::{format_amount, parse_amount, pda, Addresses},
};

const TOKEN_AMOUNT_OFFSET: usize = 64;

pub fn strategy_address(vault: &Pubkey) -> Pubkey {
    pda(&[STRATEGY_SEED, vault.as_ref()])
}

pub fn second_tokens_address(vault: &Pubkey) -> Pubkey {
    pda(&[SECOND_TOKENS_SEED, vault.as_ref()])
}

pub fn load(client: &Client, addresses: &Addresses) -> Result<Strategy> {
    client
        .account::<Strategy>(&strategy_address(&addresses.vault))?
        .ok_or_else(|| "Strategy does not exist; run strategy".into())
}

pub fn token_amount(data: &[u8]) -> Result<u64> {
    let bytes = data
        .get(TOKEN_AMOUNT_OFFSET..TOKEN_AMOUNT_OFFSET + 8)
        .ok_or("Invalid token account")?;
    Ok(u64::from_le_bytes(bytes.try_into()?))
}

pub fn init(client: &Client, addresses: &Addresses, pair: &Pair) -> Result<()> {
    let strategy = strategy_address(&addresses.vault);
    if !client.exists(&strategy)? {
        let pool = pair.load_pool(client)?;
        let instruction = vault_instruction(
            liquidity_vault::accounts::InitStrategy {
                admin: client.payer.pubkey(),
                vault: addresses.vault,
                strategy,
                pool: pool.address,
                second_mint: pair.quote,
                second_token_account: second_tokens_address(&addresses.vault),
                second_token_program: client.rpc.get_account(&pair.quote)?.owner,
                system_program: anchor_lang::system_program::ID,
            },
            liquidity_vault::instruction::InitStrategy {},
        );
        client.send("init_strategy", &[instruction], &[])?;
    } else {
        println!("Strategy already exists");
    }
    show(client, addresses, pair)
}

/// Обмен токенов vault через пул: ожидаемая сумма берётся из simulation той же инструкции.
pub fn swap(
    client: &Client,
    addresses: &Addresses,
    pair: &Pair,
    direction: &str,
    amount: &str,
    slippage_bps: u64,
    reject: bool,
) -> Result<()> {
    let sell_vault_token = match direction {
        "sell" => true,
        "buy" => false,
        _ => return fail("Direction must be sell (SNTOK -> second) or buy (second -> SNTOK)"),
    };
    if slippage_bps == 0 || slippage_bps > 1_000 {
        return fail("Slippage must be 1..1000 bps");
    }
    let strategy = load(client, addresses)?;
    let pool = pair.load_pool(client)?;
    let (input, output) = if sell_vault_token {
        (addresses.vault_token_account, strategy.second_token_account)
    } else {
        (strategy.second_token_account, addresses.vault_token_account)
    };
    let input_mint = if sell_vault_token {
        addresses.token_mint
    } else {
        strategy.second_mint
    };
    let amount_in = parse_amount(amount, addresses.decimals)?;
    let remaining = pool.swap_remaining(client, input_mint == pool.mint_0)?;
    // Инструкция swap vault; minimum_amount_out подставляется после котировки
    let build = |minimum_amount_out: u64| -> Instruction {
        let (input_mint, output_mint) = if sell_vault_token {
            (addresses.token_mint, strategy.second_mint)
        } else {
            (strategy.second_mint, addresses.token_mint)
        };
        let mut instruction = vault_instruction(
            liquidity_vault::accounts::Swap {
                admin: client.payer.pubkey(),
                vault: addresses.vault,
                strategy: strategy_address(&addresses.vault),
                clmm_program: raydium::CLMM_PROGRAM,
                amm_config: raydium::AMM_CONFIG,
                pool: pool.address,
                vault_token_account: addresses.vault_token_account,
                second_token_account: strategy.second_token_account,
                input_pool_vault: pool.vault_for(&input_mint),
                output_pool_vault: pool.vault_for(&output_mint),
                observation: pool.observation,
                token_program: anchor_spl::token::ID,
                token_program_2022: anchor_spl::token_2022::ID,
                memo_program: raydium::MEMO_PROGRAM,
                input_mint,
                output_mint,
            },
            liquidity_vault::instruction::Swap {
                amount_in,
                minimum_amount_out,
                sell_vault_token,
            },
        );
        instruction.accounts.extend_from_slice(&remaining);
        instruction
    };
    let input_before = client.token_balance(&input)?;
    let output_before = client.token_balance(&output)?;
    let price_before = pair.quote_per_base(&pool);

    let quoted =
        client.simulate_accounts(&[raydium::compute_budget(400_000), build(0)], &[output])?;
    let expected = token_amount(&quoted[0])? - output_before;
    let minimum = if reject {
        expected * 2
    } else {
        expected * (10_000 - slippage_bps) / 10_000
    };
    println!(
        "Swap {} {direction}: expected {}, minimum {} ({})",
        format_amount(amount_in, addresses.decimals),
        format_amount(expected, addresses.decimals),
        format_amount(minimum, addresses.decimals),
        if reject {
            "doubled to show slippage protection".to_string()
        } else {
            format!("expected - {slippage_bps} bps")
        }
    );
    let instructions = [raydium::compute_budget(400_000), build(minimum)];
    if reject {
        client.send_expecting_failure("swap rejected by slippage", &instructions)?;
        if client.token_balance(&input)? != input_before
            || client.token_balance(&output)? != output_before
        {
            return fail("Vault balances changed after the rejected swap");
        }
        println!("PASS RPC readback: vault balances unchanged after rejection");
        return show(client, addresses, pair);
    }
    client.send("swap", &instructions, &[])?;
    let spent = input_before - client.token_balance(&input)?;
    let received = client.token_balance(&output)? - output_before;
    if spent != amount_in || received < minimum {
        return fail("RPC readback does not match the swap");
    }
    let pool_after = pair.load_pool(client)?;
    println!(
        "PASS RPC readback: spent {}, received {} (minimum {})\nPool price: {:.6} -> {:.6} second token per SNTOK ({:+.3}%)",
        format_amount(spent, addresses.decimals),
        format_amount(received, addresses.decimals),
        format_amount(minimum, addresses.decimals),
        price_before,
        pair.quote_per_base(&pool_after),
        (pair.quote_per_base(&pool_after) / price_before - 1.0) * 100.0
    );
    show(client, addresses, pair)
}

pub fn show(client: &Client, addresses: &Addresses, pair: &Pair) -> Result<()> {
    let strategy = load(client, addresses)?;
    let pool = pair.load_pool(client)?;
    println!("\n=== STRATEGY ===");
    println!(
        "Strategy: {}",
        client.explorer("address", strategy_address(&addresses.vault))
    );
    println!("Pool: {}", strategy.pool);
    println!(
        "Second token account: {}",
        client.explorer("address", strategy.second_token_account)
    );
    println!(
        "Vault holds: {} SNTOK + {} second token; pool price {:.6} second token per SNTOK",
        format_amount(
            client.token_balance(&addresses.vault_token_account)?,
            addresses.decimals
        ),
        format_amount(
            client.token_balance(&strategy.second_token_account)?,
            addresses.decimals
        ),
        pair.quote_per_base(&pool)
    );
    Ok(())
}
