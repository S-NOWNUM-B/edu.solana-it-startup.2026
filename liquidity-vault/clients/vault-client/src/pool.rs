use anchor_lang::{prelude::Pubkey, solana_program::system_instruction};
use anchor_spl::{
    associated_token::{
        get_associated_token_address_with_program_id, spl_associated_token_account,
    },
    token::spl_token,
};
use solana_keypair::Keypair;
use solana_signer::Signer;

use crate::{
    raydium::{self, OpenPosition, Pool, Position},
    rpc::{fail, Client, Result},
    vault::{format_amount, parse_amount},
};

const QUOTE_DECIMALS: u8 = 6;
const QUOTE_SUPPLY: u64 = 1_000_000_000_000;
const MINT_SPACE: u64 = 82;

/// Пара пула: токен vault (SNTOK) и второй токен.
pub struct Pair {
    pub base: Pubkey,
    pub quote: Pubkey,
}

impl Pair {
    pub fn pool_address(&self) -> Pubkey {
        let (mint_0, mint_1) = raydium::ordered(self.base, self.quote);
        raydium::pool_address(&mint_0, &mint_1)
    }

    /// Цена пула (token1 за token0) -> сколько второго токена за 1 SNTOK.
    pub fn quote_per_base(&self, pool: &Pool) -> f64 {
        if pool.mint_0 == self.base {
            pool.price()
        } else {
            1.0 / pool.price()
        }
    }

    pub fn load_pool(&self, client: &Client) -> Result<Pool> {
        let address = self.pool_address();
        if !client.exists(&address)? {
            return fail(format!("Pool {address} does not exist; run create-pool"));
        }
        Pool::load(client, address)
    }
}

pub fn ata(client: &Client, owner: &Pubkey, mint: &Pubkey) -> Result<Pubkey> {
    let program = client.rpc.get_account(mint)?.owner;
    Ok(get_associated_token_address_with_program_id(
        owner, mint, &program,
    ))
}

/// Второй токен пула: классический SPL Token, весь выпуск — на кошелёк.
pub fn create_token(client: &Client) -> Result<()> {
    let payer = client.payer.pubkey();
    let mint = Keypair::new();
    let account =
        get_associated_token_address_with_program_id(&payer, &mint.pubkey(), &spl_token::ID);
    let instructions = [
        system_instruction::create_account(
            &payer,
            &mint.pubkey(),
            client
                .rpc
                .get_minimum_balance_for_rent_exemption(MINT_SPACE as usize)?,
            MINT_SPACE,
            &spl_token::ID,
        ),
        spl_token::instruction::initialize_mint2(
            &spl_token::ID,
            &mint.pubkey(),
            &payer,
            None,
            QUOTE_DECIMALS,
        )?,
        spl_associated_token_account::instruction::create_associated_token_account_idempotent(
            &payer,
            &payer,
            &mint.pubkey(),
            &spl_token::ID,
        ),
        spl_token::instruction::mint_to(
            &spl_token::ID,
            &mint.pubkey(),
            &account,
            &payer,
            &[],
            QUOTE_SUPPLY,
        )?,
    ];
    client.send("create second token", &instructions, &[&mint])?;
    println!(
        "Second token: {}\nWallet balance: {}",
        client.explorer("address", mint.pubkey()),
        format_amount(client.token_balance(&account)?, QUOTE_DECIMALS)
    );
    Ok(())
}

pub fn create_pool(client: &Client, pair: &Pair, price: &str) -> Result<()> {
    let price: f64 = price.parse()?;
    if !(price.is_finite() && price > 0.0) {
        return fail("Price must be positive");
    }
    let address = pair.pool_address();
    if client.exists(&address)? {
        println!("Pool already exists");
        return show(client, pair, None);
    }
    let (mint_0, mint_1) = raydium::ordered(pair.base, pair.quote);
    let pool_price = if mint_0 == pair.base {
        price
    } else {
        1.0 / price
    };
    let decimals = |mint: &Pubkey| -> Result<u8> { Ok(client.rpc.get_account(mint)?.data[44]) };
    let instruction = raydium::create_pool(
        client.payer.pubkey(),
        mint_0,
        mint_1,
        client.rpc.get_account(&mint_0)?.owner,
        client.rpc.get_account(&mint_1)?.owner,
        raydium::price_to_sqrt_x64(pool_price, decimals(&mint_0)?, decimals(&mint_1)?),
    );
    client.send("create_pool", &[instruction], &[])?;
    show(client, pair, None)
}

/// Позиция с кошелька в диапазоне ±range% вокруг текущей цены.
pub fn open_position(client: &Client, pair: &Pair, range: &str, amount: &str) -> Result<()> {
    let range: f64 = range.parse::<f64>()? / 100.0;
    if !(range > 0.0 && range < 1.0) {
        return fail("Range must be between 0 and 100 percent");
    }
    let pool = pair.load_pool(client)?;
    let payer = client.payer.pubkey();
    let (tick_lower, tick_upper) = raydium::range_ticks(pool.price(), range, &pool);
    let account_0 = ata(client, &payer, &pool.mint_0)?;
    let account_1 = ata(client, &payer, &pool.mint_1)?;
    // Точная сумма задаётся во втором токене, первая сторона ограничена балансом
    let quote_is_0 = pool.mint_0 == pair.quote;
    let quote_decimals = if quote_is_0 {
        pool.decimals_0
    } else {
        pool.decimals_1
    };
    let amount = parse_amount(amount, quote_decimals)?;
    let balance_0 = client.token_balance(&account_0)?;
    let balance_1 = client.token_balance(&account_1)?;
    let nft_mint = Keypair::new();
    let open = OpenPosition {
        payer,
        owner: payer,
        nft_mint: nft_mint.pubkey(),
        token_account_0: account_0,
        token_account_1: account_1,
        tick_lower,
        tick_upper,
        amount_0_max: if quote_is_0 { amount } else { balance_0 },
        amount_1_max: if quote_is_0 { balance_1 } else { amount },
    };
    println!(
        "Open position: ticks {tick_lower}..{tick_upper}, price {:.6}..{:.6} (pool token1 per token0)",
        raydium::tick_to_price(tick_lower, pool.decimals_0, pool.decimals_1),
        raydium::tick_to_price(tick_upper, pool.decimals_0, pool.decimals_1)
    );
    client.send(
        "open_position",
        &[
            raydium::compute_budget(600_000),
            open.instruction(&pool, quote_is_0),
        ],
        &[&nft_mint],
    )?;
    let spent_0 = balance_0 - client.token_balance(&account_0)?;
    let spent_1 = balance_1 - client.token_balance(&account_1)?;
    println!(
        "Deposited: token0 {}, token1 {}",
        format_amount(spent_0, pool.decimals_0),
        format_amount(spent_1, pool.decimals_1)
    );
    show(client, pair, Some(nft_mint.pubkey()))
}

pub fn show(client: &Client, pair: &Pair, position: Option<Pubkey>) -> Result<()> {
    let pool = pair.load_pool(client)?;
    let vault_0 = client.token_balance(&pool.vault_0)?;
    let vault_1 = client.token_balance(&pool.vault_1)?;
    let name = |mint: &Pubkey| {
        if *mint == pair.base {
            "SNTOK"
        } else {
            "second"
        }
    };
    println!("\n=== RAYDIUM CLMM POOL ===");
    println!("Pool: {}", client.explorer("address", pool.address));
    println!(
        "AMM config: {} (fee 0.25%, tick spacing {})",
        raydium::AMM_CONFIG,
        pool.tick_spacing
    );
    println!("token0: {} ({})", pool.mint_0, name(&pool.mint_0));
    println!("token1: {} ({})", pool.mint_1, name(&pool.mint_1));
    println!(
        "Price: {:.6} token1 per token0; {:.6} second token per SNTOK; tick {}",
        pool.price(),
        pair.quote_per_base(&pool),
        pool.tick_current
    );
    println!(
        "Active liquidity: {}; pool vaults: token0 {}, token1 {}",
        pool.liquidity,
        format_amount(vault_0, pool.decimals_0),
        format_amount(vault_1, pool.decimals_1)
    );
    if let Some(nft_mint) = position {
        let Some(position) = Position::load(client, &nft_mint)? else {
            return fail(format!("Position {nft_mint} not found"));
        };
        if position.pool != pool.address {
            return fail("Position belongs to another pool");
        }
        println!(
            "Position NFT: {}\n  ticks {}..{} (price {:.6}..{:.6}); liquidity {}; in range: {}",
            client.explorer("address", position.nft_mint),
            position.tick_lower,
            position.tick_upper,
            raydium::tick_to_price(position.tick_lower, pool.decimals_0, pool.decimals_1),
            raydium::tick_to_price(position.tick_upper, pool.decimals_0, pool.decimals_1),
            position.liquidity,
            (position.tick_lower..position.tick_upper).contains(&pool.tick_current)
        );
    }
    Ok(())
}
