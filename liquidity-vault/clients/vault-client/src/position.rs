use anchor_lang::prelude::{AccountMeta, Pubkey};
use liquidity_vault::state::Strategy;
use solana_keypair::Keypair;
use solana_message::Instruction;
use solana_signer::Signer;

use crate::{
    pool::{ata, Pair},
    raydium::{self, Pool, Position},
    rpc::{fail, vault_instruction, Client, Result},
    strategy::{self, strategy_address, token_amount},
    vault::{format_amount, parse_amount, Addresses},
};

const COMPUTE_UNITS: u32 = 600_000;

/// Хранилища vault в порядке token0/token1 пула.
fn storages(addresses: &Addresses, strategy: &Strategy, pool: &Pool) -> (Pubkey, Pubkey) {
    if pool.mint_0 == addresses.token_mint {
        (addresses.vault_token_account, strategy.second_token_account)
    } else {
        (strategy.second_token_account, addresses.vault_token_account)
    }
}

fn accounts(
    client: &Client,
    addresses: &Addresses,
    strategy: &Strategy,
    pool: &Pool,
    nft_mint: Pubkey,
    tick_lower: i32,
    tick_upper: i32,
) -> liquidity_vault::accounts::VaultPosition {
    let (token_account_0, token_account_1) = storages(addresses, strategy, pool);
    liquidity_vault::accounts::VaultPosition {
        admin: client.payer.pubkey(),
        vault: addresses.vault,
        strategy: strategy_address(&addresses.vault),
        clmm_program: raydium::CLMM_PROGRAM,
        pool: pool.address,
        position_nft_mint: nft_mint,
        position_nft_account:
            anchor_spl::associated_token::get_associated_token_address_with_program_id(
                &addresses.vault,
                &nft_mint,
                &anchor_spl::token_2022::ID,
            ),
        personal_position: raydium::personal_position(&nft_mint),
        protocol_position: raydium::protocol_position(&pool.address, tick_lower, tick_upper),
        tick_array_lower: raydium::tick_array(
            &pool.address,
            raydium::tick_array_start(tick_lower, pool.tick_spacing),
        ),
        tick_array_upper: raydium::tick_array(
            &pool.address,
            raydium::tick_array_start(tick_upper, pool.tick_spacing),
        ),
        token_account_0,
        token_account_1,
        token_vault_0: pool.vault_0,
        token_vault_1: pool.vault_1,
        vault_0_mint: pool.mint_0,
        vault_1_mint: pool.mint_1,
        rent: anchor_lang::prelude::rent::ID,
        system_program: anchor_lang::system_program::ID,
        token_program: anchor_spl::token::ID,
        token_program_2022: anchor_spl::token_2022::ID,
        associated_token_program: anchor_spl::associated_token::ID,
        memo_program: raydium::MEMO_PROGRAM,
    }
}

fn with_bitmap(mut instruction: Instruction, pool: &Pool) -> Instruction {
    instruction.accounts.push(AccountMeta::new(
        raydium::bitmap_extension(&pool.address),
        false,
    ));
    instruction
}

fn balances(client: &Client, accounts: (Pubkey, Pubkey)) -> Result<(u64, u64)> {
    Ok((
        client.token_balance(&accounts.0)?,
        client.token_balance(&accounts.1)?,
    ))
}

fn current(client: &Client, strategy: &Strategy) -> Result<Position> {
    if strategy.position_nft_mint == Pubkey::default() {
        return fail("Vault has no open position");
    }
    Position::load(client, &strategy.position_nft_mint)?
        .ok_or_else(|| "Position account not found".into())
}

/// Позиция vault ±range% вокруг цены; точная сумма — во втором токене, SNTOK не больше хранилища.
pub fn open(
    client: &Client,
    addresses: &Addresses,
    pair: &Pair,
    range: &str,
    amount: &str,
) -> Result<()> {
    let range: f64 = range.parse::<f64>()? / 100.0;
    if !(range > 0.0 && range < 1.0) {
        return fail("Range must be between 0 and 100 percent");
    }
    let strategy = strategy::load(client, addresses)?;
    let pool = pair.load_pool(client)?;
    let (tick_lower, tick_upper) = raydium::range_ticks(pool.price(), range, &pool);
    let amount = parse_amount(amount, addresses.decimals)?;
    let storages = storages(addresses, &strategy, &pool);
    let before = balances(client, storages)?;
    let second_is_0 = pool.mint_0 == strategy.second_mint;
    let second_balance = if second_is_0 { before.0 } else { before.1 };
    if amount > second_balance {
        return fail(format!(
            "Vault holds only {} second token",
            format_amount(second_balance, addresses.decimals)
        ));
    }
    let (amount_0_max, amount_1_max) = if second_is_0 {
        (amount, before.1)
    } else {
        (before.0, amount)
    };
    let nft_mint = Keypair::new();
    let mut instruction = vault_instruction(
        accounts(
            client,
            addresses,
            &strategy,
            &pool,
            nft_mint.pubkey(),
            tick_lower,
            tick_upper,
        ),
        liquidity_vault::instruction::OpenPosition {
            tick_lower,
            tick_upper,
            tick_array_lower_start: raydium::tick_array_start(tick_lower, pool.tick_spacing),
            tick_array_upper_start: raydium::tick_array_start(tick_upper, pool.tick_spacing),
            amount_0_max,
            amount_1_max,
            base_0: second_is_0,
        },
    );
    // Новый mint NFT подписывает транзакцию: Raydium создаёт его в CPI
    for meta in &mut instruction.accounts {
        if meta.pubkey == nft_mint.pubkey() {
            meta.is_signer = true;
        }
    }
    println!(
        "Open vault position: price {:.6} second token per SNTOK, range ±{:.1}%, ticks {tick_lower}..{tick_upper}",
        pair.quote_per_base(&pool),
        range * 100.0
    );
    client.send(
        "open_position",
        &[
            raydium::compute_budget(COMPUTE_UNITS),
            with_bitmap(instruction, &pool),
        ],
        &[&nft_mint],
    )?;
    let after = balances(client, storages)?;
    let strategy = strategy::load(client, addresses)?;
    let position = current(client, &strategy)?;
    if strategy.position_nft_mint != nft_mint.pubkey() || position.liquidity == 0 {
        return fail("RPC readback: position is not recorded in the strategy");
    }
    println!(
        "PASS RPC readback: vault gave token0 {} + token1 {}; NFT owner is the vault",
        format_amount(before.0 - after.0, pool.decimals_0),
        format_amount(before.1 - after.1, pool.decimals_1)
    );
    show(client, addresses, pair)
}

fn manage(
    client: &Client,
    addresses: &Addresses,
    strategy: &Strategy,
    pool: &Pool,
    position: &Position,
    data: impl anchor_lang::InstructionData,
) -> Instruction {
    with_bitmap(
        vault_instruction(
            accounts(
                client,
                addresses,
                strategy,
                pool,
                position.nft_mint,
                position.tick_lower,
                position.tick_upper,
            ),
            data,
        ),
        pool,
    )
}

pub fn collect(client: &Client, addresses: &Addresses, pair: &Pair) -> Result<()> {
    let strategy = strategy::load(client, addresses)?;
    let pool = pair.load_pool(client)?;
    let position = current(client, &strategy)?;
    let storages = storages(addresses, &strategy, &pool);
    let before = balances(client, storages)?;
    let instruction = manage(
        client,
        addresses,
        &strategy,
        &pool,
        &position,
        liquidity_vault::instruction::CollectFees {},
    );
    client.send(
        "collect_fees",
        &[raydium::compute_budget(COMPUTE_UNITS), instruction],
        &[],
    )?;
    let after = balances(client, storages)?;
    println!(
        "PASS RPC readback: fees token0 +{}, token1 +{}",
        format_amount(after.0 - before.0, pool.decimals_0),
        format_amount(after.1 - before.1, pool.decimals_1)
    );
    show(client, addresses, pair)
}

/// Минимумы закрытия: результат simulation минус slippage.
pub fn close(client: &Client, addresses: &Addresses, pair: &Pair, slippage_bps: u64) -> Result<()> {
    if slippage_bps == 0 || slippage_bps > 1_000 {
        return fail("Slippage must be 1..1000 bps");
    }
    let strategy = strategy::load(client, addresses)?;
    let pool = pair.load_pool(client)?;
    let position = current(client, &strategy)?;
    let storages = storages(addresses, &strategy, &pool);
    let before = balances(client, storages)?;
    let build = |amount_0_min, amount_1_min| {
        [
            raydium::compute_budget(COMPUTE_UNITS),
            manage(
                client,
                addresses,
                &strategy,
                &pool,
                &position,
                liquidity_vault::instruction::ClosePosition {
                    amount_0_min,
                    amount_1_min,
                },
            ),
        ]
    };
    let quoted = client.simulate_accounts(&build(0, 0), &[storages.0, storages.1])?;
    let expected_0 = token_amount(&quoted[0])? - before.0;
    let expected_1 = token_amount(&quoted[1])? - before.1;
    let minimum = |value: u64| value * (10_000 - slippage_bps) / 10_000;
    println!(
        "Close position: expected token0 {}, token1 {}; minimums -{slippage_bps} bps",
        format_amount(expected_0, pool.decimals_0),
        format_amount(expected_1, pool.decimals_1)
    );
    client.send(
        "close_position",
        &build(minimum(expected_0), minimum(expected_1)),
        &[],
    )?;
    let after = balances(client, storages)?;
    let strategy = strategy::load(client, addresses)?;
    if strategy.position_nft_mint != Pubkey::default()
        || client.exists(&raydium::personal_position(&position.nft_mint))?
    {
        return fail("RPC readback: position is still open");
    }
    println!(
        "PASS RPC readback: returned token0 {}, token1 {} (liquidity and fees); position closed",
        format_amount(after.0 - before.0, pool.decimals_0),
        format_amount(after.1 - before.1, pool.decimals_1)
    );
    show(client, addresses, pair)
}

/// Обмен с кошелька напрямую в пуле — как сторонний трейдер, который приносит комиссии.
pub fn trade(
    client: &Client,
    pair: &Pair,
    direction: &str,
    amount: &str,
    slippage_bps: u64,
) -> Result<()> {
    let (input_mint, output_mint) = match direction {
        "sell" => (pair.base, pair.quote),
        "buy" => (pair.quote, pair.base),
        _ => return fail("Direction must be sell (SNTOK -> second) or buy (second -> SNTOK)"),
    };
    let pool = pair.load_pool(client)?;
    let payer = client.payer.pubkey();
    let input = ata(client, &payer, &input_mint)?;
    let output = ata(client, &payer, &output_mint)?;
    let decimals = if input_mint == pool.mint_0 {
        pool.decimals_0
    } else {
        pool.decimals_1
    };
    let amount_in = parse_amount(amount, decimals)?;
    let remaining = pool.swap_remaining(client, input_mint == pool.mint_0)?;
    let build = |minimum| {
        [
            raydium::compute_budget(400_000),
            raydium::swap_v2(
                payer,
                &pool,
                input,
                output,
                input_mint,
                output_mint,
                &remaining,
                amount_in,
                minimum,
            ),
        ]
    };
    let output_before = client.token_balance(&output)?;
    let quoted = client.simulate_accounts(&build(0), &[output])?;
    let expected = token_amount(&quoted[0])? - output_before;
    let minimum = expected * (10_000 - slippage_bps) / 10_000;
    let price_before = pair.quote_per_base(&pool);
    client.send("trade", &build(minimum), &[])?;
    let received = client.token_balance(&output)? - output_before;
    let pool_after = pair.load_pool(client)?;
    println!(
        "PASS RPC readback: {direction} {amount}, received {} (minimum {})\nPool price: {:.6} -> {:.6} second token per SNTOK",
        format_amount(received, decimals),
        format_amount(minimum, decimals),
        price_before,
        pair.quote_per_base(&pool_after)
    );
    Ok(())
}

pub fn show(client: &Client, addresses: &Addresses, pair: &Pair) -> Result<()> {
    strategy::show(client, addresses, pair)?;
    let strategy = strategy::load(client, addresses)?;
    if strategy.position_nft_mint == Pubkey::default() {
        println!("Vault position: none");
        return Ok(());
    }
    let pool = pair.load_pool(client)?;
    let position = current(client, &strategy)?;
    let to_quote = |tick| {
        let price = raydium::tick_to_price(tick, pool.decimals_0, pool.decimals_1);
        if pool.mint_0 == pair.base {
            price
        } else {
            1.0 / price
        }
    };
    let (low, high) = {
        let (a, b) = (to_quote(position.tick_lower), to_quote(position.tick_upper));
        (a.min(b), a.max(b))
    };
    println!(
        "Vault position NFT: {}\n  range {:.6}..{:.6} second token per SNTOK (ticks {}..{}); liquidity {}; in range: {}\n  fees owed at last update: token0 {}, token1 {}",
        client.explorer("address", position.nft_mint),
        low,
        high,
        position.tick_lower,
        position.tick_upper,
        position.liquidity,
        (position.tick_lower..position.tick_upper).contains(&pool.tick_current),
        format_amount(position.fees_owed_0, pool.decimals_0),
        format_amount(position.fees_owed_1, pool.decimals_1)
    );
    Ok(())
}
