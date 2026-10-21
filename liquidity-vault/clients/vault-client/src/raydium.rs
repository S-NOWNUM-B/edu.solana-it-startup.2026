//! Инструкции и аккаунты Raydium CLMM без крейта Raydium: он закреплён на другой версии Anchor.
//! Порядок аккаунтов и discriminator'ы взяты из raydium-io/raydium-clmm.

use anchor_lang::prelude::{AccountMeta, Pubkey};
use solana_message::Instruction;

use crate::rpc::{fail, Client, Result};

pub const CLMM_PROGRAM: Pubkey =
    Pubkey::from_str_const("DRayAUgENGQBKVaX8owNhgzkEDyoHTGVEGHVJT1E9pfH");
/// Конфиг devnet: индекс 0, комиссия 0.25%, шаг тиков 60.
pub const AMM_CONFIG: Pubkey =
    Pubkey::from_str_const("CD4aJtX11cqTCAc83nxSPkkh5JW2yjD6uwHeovjqQ1qu");
pub const MEMO_PROGRAM: Pubkey =
    Pubkey::from_str_const("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
const COMPUTE_BUDGET_PROGRAM: Pubkey =
    Pubkey::from_str_const("ComputeBudget111111111111111111111111111111");
const TOKEN_PROGRAM: Pubkey = anchor_spl::token::ID;
const TOKEN_2022_PROGRAM: Pubkey = anchor_spl::token_2022::ID;
const TICK_ARRAY_SIZE: i32 = 60;
const POOL_DISCRIMINATOR: [u8; 8] = [247, 237, 227, 245, 215, 195, 222, 70];
const POSITION_DISCRIMINATOR: [u8; 8] = [70, 111, 150, 126, 230, 15, 25, 117];
const SWAP_V2: [u8; 8] = [43, 4, 237, 11, 26, 201, 30, 98];
const CREATE_POOL: [u8; 8] = [233, 146, 209, 142, 207, 104, 64, 188];
const OPEN_POSITION_WITH_TOKEN22_NFT: [u8; 8] = [77, 255, 174, 82, 125, 29, 201, 46];

fn clmm_pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &CLMM_PROGRAM).0
}

pub fn pool_address(mint_0: &Pubkey, mint_1: &Pubkey) -> Pubkey {
    clmm_pda(&[
        b"pool",
        AMM_CONFIG.as_ref(),
        mint_0.as_ref(),
        mint_1.as_ref(),
    ])
}

pub fn pool_vault(pool: &Pubkey, mint: &Pubkey) -> Pubkey {
    clmm_pda(&[b"pool_vault", pool.as_ref(), mint.as_ref()])
}

pub fn observation(pool: &Pubkey) -> Pubkey {
    clmm_pda(&[b"observation", pool.as_ref()])
}

pub fn bitmap_extension(pool: &Pubkey) -> Pubkey {
    clmm_pda(&[b"pool_tick_array_bitmap_extension", pool.as_ref()])
}

pub fn tick_array(pool: &Pubkey, start_index: i32) -> Pubkey {
    clmm_pda(&[b"tick_array", pool.as_ref(), &start_index.to_be_bytes()])
}

pub fn personal_position(nft_mint: &Pubkey) -> Pubkey {
    clmm_pda(&[b"position", nft_mint.as_ref()])
}

pub fn protocol_position(pool: &Pubkey, tick_lower: i32, tick_upper: i32) -> Pubkey {
    clmm_pda(&[
        b"position",
        pool.as_ref(),
        &tick_lower.to_be_bytes(),
        &tick_upper.to_be_bytes(),
    ])
}

/// Raydium хранит токены пула в порядке возрастания адресов mint.
pub fn ordered(mint_a: Pubkey, mint_b: Pubkey) -> (Pubkey, Pubkey) {
    if mint_a.to_bytes() < mint_b.to_bytes() {
        (mint_a, mint_b)
    } else {
        (mint_b, mint_a)
    }
}

pub fn tick_array_start(tick: i32, tick_spacing: u16) -> i32 {
    let span = TICK_ARRAY_SIZE * i32::from(tick_spacing);
    tick.div_euclid(span) * span
}

/// Цена token1 за token0 в целых токенах -> тик (вниз).
pub fn price_to_tick(price: f64, decimals_0: u8, decimals_1: u8) -> i32 {
    let raw = price * 10_f64.powi(decimals_1 as i32 - decimals_0 as i32);
    (raw.ln() / 1.0001_f64.ln()).floor() as i32
}

pub fn tick_to_price(tick: i32, decimals_0: u8, decimals_1: u8) -> f64 {
    1.0001_f64.powi(tick) / 10_f64.powi(decimals_1 as i32 - decimals_0 as i32)
}

pub fn price_to_sqrt_x64(price: f64, decimals_0: u8, decimals_1: u8) -> u128 {
    let raw = price * 10_f64.powi(decimals_1 as i32 - decimals_0 as i32);
    (raw.sqrt() * 2_f64.powi(64)) as u128
}

/// Нижняя граница округляется вниз, верхняя вверх до шага тиков.
pub fn range_ticks(price: f64, range: f64, pool: &Pool) -> (i32, i32) {
    let spacing = i32::from(pool.tick_spacing);
    let lower = price_to_tick(price * (1.0 - range), pool.decimals_0, pool.decimals_1);
    let upper = price_to_tick(price * (1.0 + range), pool.decimals_0, pool.decimals_1);
    (
        lower.div_euclid(spacing) * spacing,
        (upper.div_euclid(spacing) + 1) * spacing,
    )
}

fn read_pubkey(data: &[u8], offset: usize) -> Pubkey {
    Pubkey::new_from_array(data[offset..offset + 32].try_into().unwrap())
}

fn read_u128(data: &[u8], offset: usize) -> u128 {
    u128::from_le_bytes(data[offset..offset + 16].try_into().unwrap())
}

fn read_i32(data: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn read_u64(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
}

/// Нужные поля PoolState (zero-copy, packed).
pub struct Pool {
    pub address: Pubkey,
    pub mint_0: Pubkey,
    pub mint_1: Pubkey,
    pub vault_0: Pubkey,
    pub vault_1: Pubkey,
    pub observation: Pubkey,
    pub decimals_0: u8,
    pub decimals_1: u8,
    pub tick_spacing: u16,
    pub liquidity: u128,
    pub sqrt_price_x64: u128,
    pub tick_current: i32,
}

impl Pool {
    pub fn parse(address: Pubkey, data: &[u8]) -> Result<Self> {
        if data.len() < 273 || data[..8] != POOL_DISCRIMINATOR {
            return fail(format!("{address} is not a Raydium CLMM pool"));
        }
        Ok(Self {
            address,
            mint_0: read_pubkey(data, 73),
            mint_1: read_pubkey(data, 105),
            vault_0: read_pubkey(data, 137),
            vault_1: read_pubkey(data, 169),
            observation: read_pubkey(data, 201),
            decimals_0: data[233],
            decimals_1: data[234],
            tick_spacing: u16::from_le_bytes([data[235], data[236]]),
            liquidity: read_u128(data, 237),
            sqrt_price_x64: read_u128(data, 253),
            tick_current: read_i32(data, 269),
        })
    }

    pub fn load(client: &Client, address: Pubkey) -> Result<Self> {
        let account = client.rpc.get_account(&address)?;
        if account.owner != CLMM_PROGRAM {
            return fail(format!("{address} is not owned by Raydium CLMM"));
        }
        Self::parse(address, &account.data)
    }

    /// Цена token1 за token0 в целых токенах.
    pub fn price(&self) -> f64 {
        let sqrt = self.sqrt_price_x64 as f64 / 2_f64.powi(64);
        sqrt * sqrt / 10_f64.powi(self.decimals_1 as i32 - self.decimals_0 as i32)
    }

    /// Хранилище пула для mint.
    pub fn vault_for(&self, mint: &Pubkey) -> Pubkey {
        if *mint == self.mint_0 {
            self.vault_0
        } else {
            self.vault_1
        }
    }

    /// Remaining accounts swap_v2: bitmap-расширение и существующие tick-массивы
    /// по направлению обмена, начиная с текущего.
    pub fn swap_remaining(&self, client: &Client, zero_for_one: bool) -> Result<Vec<AccountMeta>> {
        let span = TICK_ARRAY_SIZE * i32::from(self.tick_spacing);
        let start = tick_array_start(self.tick_current, self.tick_spacing);
        let step = if zero_for_one { -span } else { span };
        let candidates: Vec<_> = (0..4)
            .map(|i| tick_array(&self.address, start + step * i))
            .collect();
        let accounts = client.rpc.get_multiple_accounts(&candidates)?;
        let mut remaining = vec![AccountMeta::new_readonly(
            bitmap_extension(&self.address),
            false,
        )];
        remaining.extend(
            candidates
                .into_iter()
                .zip(accounts)
                .filter(|(_, account)| account.is_some())
                .map(|(address, _)| AccountMeta::new(address, false)),
        );
        if remaining.len() == 1 {
            return fail("Pool has no initialized tick arrays near the current price");
        }
        Ok(remaining)
    }
}

/// Нужные поля PersonalPositionState (Borsh).
pub struct Position {
    pub nft_mint: Pubkey,
    pub pool: Pubkey,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub liquidity: u128,
    pub fees_owed_0: u64,
    pub fees_owed_1: u64,
}

impl Position {
    pub fn load(client: &Client, nft_mint: &Pubkey) -> Result<Option<Self>> {
        let address = personal_position(nft_mint);
        let Some(account) = client
            .rpc
            .get_account_with_commitment(&address, client.rpc.commitment())?
            .value
        else {
            return Ok(None);
        };
        let data = account.data;
        if data.len() < 145 || data[..8] != POSITION_DISCRIMINATOR {
            return fail(format!("{address} is not a Raydium position"));
        }
        Ok(Some(Self {
            nft_mint: read_pubkey(&data, 9),
            pool: read_pubkey(&data, 41),
            tick_lower: read_i32(&data, 73),
            tick_upper: read_i32(&data, 77),
            liquidity: read_u128(&data, 81),
            fees_owed_0: read_u64(&data, 129),
            fees_owed_1: read_u64(&data, 137),
        }))
    }
}

pub fn compute_budget(units: u32) -> Instruction {
    let mut data = vec![2];
    data.extend_from_slice(&units.to_le_bytes());
    Instruction {
        program_id: COMPUTE_BUDGET_PROGRAM,
        accounts: vec![],
        data,
    }
}

pub fn create_pool(
    creator: Pubkey,
    mint_0: Pubkey,
    mint_1: Pubkey,
    token_program_0: Pubkey,
    token_program_1: Pubkey,
    sqrt_price_x64: u128,
) -> Instruction {
    let pool = pool_address(&mint_0, &mint_1);
    let mut data = CREATE_POOL.to_vec();
    data.extend_from_slice(&sqrt_price_x64.to_le_bytes());
    data.extend_from_slice(&0_u64.to_le_bytes());
    Instruction {
        program_id: CLMM_PROGRAM,
        accounts: vec![
            AccountMeta::new(creator, true),
            AccountMeta::new_readonly(AMM_CONFIG, false),
            AccountMeta::new(pool, false),
            AccountMeta::new_readonly(mint_0, false),
            AccountMeta::new_readonly(mint_1, false),
            AccountMeta::new(pool_vault(&pool, &mint_0), false),
            AccountMeta::new(pool_vault(&pool, &mint_1), false),
            AccountMeta::new(observation(&pool), false),
            AccountMeta::new(bitmap_extension(&pool), false),
            AccountMeta::new_readonly(token_program_0, false),
            AccountMeta::new_readonly(token_program_1, false),
            AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
            AccountMeta::new_readonly(anchor_lang::prelude::rent::ID, false),
        ],
        data,
    }
}

/// swap_v2 с кошелька: используется как обмен стороннего трейдера.
#[allow(clippy::too_many_arguments)]
pub fn swap_v2(
    payer: Pubkey,
    pool: &Pool,
    input_account: Pubkey,
    output_account: Pubkey,
    input_mint: Pubkey,
    output_mint: Pubkey,
    remaining: &[AccountMeta],
    amount_in: u64,
    minimum_amount_out: u64,
) -> Instruction {
    let mut data = SWAP_V2.to_vec();
    data.extend_from_slice(&amount_in.to_le_bytes());
    data.extend_from_slice(&minimum_amount_out.to_le_bytes());
    data.extend_from_slice(&0_u128.to_le_bytes());
    data.push(1);
    let mut accounts = vec![
        AccountMeta::new_readonly(payer, true),
        AccountMeta::new_readonly(AMM_CONFIG, false),
        AccountMeta::new(pool.address, false),
        AccountMeta::new(input_account, false),
        AccountMeta::new(output_account, false),
        AccountMeta::new(pool.vault_for(&input_mint), false),
        AccountMeta::new(pool.vault_for(&output_mint), false),
        AccountMeta::new(pool.observation, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM, false),
        AccountMeta::new_readonly(TOKEN_2022_PROGRAM, false),
        AccountMeta::new_readonly(MEMO_PROGRAM, false),
        AccountMeta::new_readonly(input_mint, false),
        AccountMeta::new_readonly(output_mint, false),
    ];
    accounts.extend_from_slice(remaining);
    Instruction {
        program_id: CLMM_PROGRAM,
        accounts,
        data,
    }
}

pub struct OpenPosition {
    pub payer: Pubkey,
    pub owner: Pubkey,
    pub nft_mint: Pubkey,
    pub token_account_0: Pubkey,
    pub token_account_1: Pubkey,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub amount_0_max: u64,
    pub amount_1_max: u64,
}

impl OpenPosition {
    pub fn nft_account(&self) -> Pubkey {
        anchor_spl::associated_token::get_associated_token_address_with_program_id(
            &self.owner,
            &self.nft_mint,
            &TOKEN_2022_PROGRAM,
        )
    }

    /// Аккаунты open_position_with_token22_nft; NFT позиции — Token-2022.
    pub fn accounts(&self, pool: &Pool) -> Vec<AccountMeta> {
        let lower_start = tick_array_start(self.tick_lower, pool.tick_spacing);
        let upper_start = tick_array_start(self.tick_upper, pool.tick_spacing);
        vec![
            AccountMeta::new(self.payer, true),
            AccountMeta::new_readonly(self.owner, false),
            AccountMeta::new(self.nft_mint, true),
            AccountMeta::new(self.nft_account(), false),
            AccountMeta::new(pool.address, false),
            AccountMeta::new_readonly(
                protocol_position(&pool.address, self.tick_lower, self.tick_upper),
                false,
            ),
            AccountMeta::new(tick_array(&pool.address, lower_start), false),
            AccountMeta::new(tick_array(&pool.address, upper_start), false),
            AccountMeta::new(personal_position(&self.nft_mint), false),
            AccountMeta::new(self.token_account_0, false),
            AccountMeta::new(self.token_account_1, false),
            AccountMeta::new(pool.vault_0, false),
            AccountMeta::new(pool.vault_1, false),
            AccountMeta::new_readonly(anchor_lang::prelude::rent::ID, false),
            AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
            AccountMeta::new_readonly(TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(anchor_spl::associated_token::ID, false),
            AccountMeta::new_readonly(TOKEN_2022_PROGRAM, false),
            AccountMeta::new_readonly(pool.mint_0, false),
            AccountMeta::new_readonly(pool.mint_1, false),
            AccountMeta::new(bitmap_extension(&pool.address), false),
        ]
    }

    /// Позиция с кошелька: ликвидность считается из точной суммы token0 или token1.
    pub fn instruction(&self, pool: &Pool, base_0: bool) -> Instruction {
        let mut data = OPEN_POSITION_WITH_TOKEN22_NFT.to_vec();
        data.extend_from_slice(&self.tick_lower.to_le_bytes());
        data.extend_from_slice(&self.tick_upper.to_le_bytes());
        data.extend_from_slice(&tick_array_start(self.tick_lower, pool.tick_spacing).to_le_bytes());
        data.extend_from_slice(&tick_array_start(self.tick_upper, pool.tick_spacing).to_le_bytes());
        data.extend_from_slice(&0_u128.to_le_bytes());
        data.extend_from_slice(&self.amount_0_max.to_le_bytes());
        data.extend_from_slice(&self.amount_1_max.to_le_bytes());
        data.push(0); // with_metadata = false
        data.extend_from_slice(&[1, u8::from(base_0)]); // base_flag = Some(base_0)
        Instruction {
            program_id: CLMM_PROGRAM,
            accounts: self.accounts(pool),
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_array_start_rounds_toward_negative_infinity() {
        assert_eq!(tick_array_start(0, 60), 0);
        assert_eq!(tick_array_start(3_599, 60), 0);
        assert_eq!(tick_array_start(3_600, 60), 3_600);
        assert_eq!(tick_array_start(-1, 60), -3_600);
        assert_eq!(tick_array_start(-3_600, 60), -3_600);
    }

    #[test]
    fn price_and_tick_round_trip() {
        assert_eq!(price_to_tick(1.0, 6, 6), 0);
        let tick = price_to_tick(2.0, 6, 6);
        assert_eq!(tick, 6_931);
        assert!((tick_to_price(tick, 6, 6) - 2.0).abs() < 0.001);
        assert_eq!(price_to_sqrt_x64(1.0, 6, 6), 1_u128 << 64);
    }

    #[test]
    fn mints_are_ordered_by_bytes() {
        let a = Pubkey::new_from_array([1; 32]);
        let b = Pubkey::new_from_array([2; 32]);
        assert_eq!(ordered(b, a), (a, b));
        assert_eq!(ordered(a, b), (a, b));
    }
}
