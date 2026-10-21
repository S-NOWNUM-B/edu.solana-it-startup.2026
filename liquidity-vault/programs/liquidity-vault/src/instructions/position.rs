use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken, token::Token, token_2022::Token2022,
    token_interface::TokenAccount,
};

use crate::{
    error::VaultError,
    raydium::{self, readonly, signer, writable},
    state::{Strategy, VaultState, STRATEGY_SEED, VAULT_SEED},
};

/// Аккаунты позиции vault в пуле стратегии; общие для открытия, комиссий и закрытия.
#[derive(Accounts)]
pub struct VaultPosition<'info> {
    /// Платит ренту новых аккаунтов позиции; токены отдаёт vault.
    #[account(mut)]
    pub admin: Signer<'info>,
    /// mut: при закрытии Raydium возвращает ренту позиции владельцу NFT — vault.
    #[account(
        mut,
        seeds = [VAULT_SEED, vault.token_mint.as_ref()],
        bump = vault.bump,
        has_one = admin,
    )]
    pub vault: Box<Account<'info, VaultState>>,
    #[account(
        mut,
        seeds = [STRATEGY_SEED, vault.key().as_ref()],
        bump = strategy.bump,
        constraint = strategy.pool == pool.key() @ VaultError::WrongPool,
    )]
    pub strategy: Box<Account<'info, Strategy>>,
    /// CHECK: адрес программы Raydium CLMM в devnet.
    #[account(address = raydium::CLMM_PROGRAM_ID)]
    pub clmm_program: UncheckedAccount<'info>,
    /// CHECK: пул стратегии; состояние проверяет Raydium.
    #[account(mut)]
    pub pool: UncheckedAccount<'info>,
    /// CHECK: mint NFT позиции; при открытии — новый keypair, далее сверяется со стратегией.
    #[account(mut)]
    pub position_nft_mint: UncheckedAccount<'info>,
    /// CHECK: ATA NFT позиции, владелец — vault; проверяет Raydium.
    #[account(mut)]
    pub position_nft_account: UncheckedAccount<'info>,
    /// CHECK: PDA позиции Raydium ["position", nft_mint].
    #[account(mut)]
    pub personal_position: UncheckedAccount<'info>,
    /// CHECK: PDA Raydium, проверяет Raydium.
    pub protocol_position: UncheckedAccount<'info>,
    /// CHECK: tick-массив нижней границы, проверяет Raydium.
    #[account(mut)]
    pub tick_array_lower: UncheckedAccount<'info>,
    /// CHECK: tick-массив верхней границы, проверяет Raydium.
    #[account(mut)]
    pub tick_array_upper: UncheckedAccount<'info>,
    /// Хранилище vault для token0 пула.
    #[account(mut)]
    pub token_account_0: Box<InterfaceAccount<'info, TokenAccount>>,
    /// Хранилище vault для token1 пула.
    #[account(mut)]
    pub token_account_1: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: хранилище пула token0, проверяет Raydium.
    #[account(mut)]
    pub token_vault_0: UncheckedAccount<'info>,
    /// CHECK: хранилище пула token1, проверяет Raydium.
    #[account(mut)]
    pub token_vault_1: UncheckedAccount<'info>,
    /// CHECK: mint token0 пула, проверяет Raydium.
    pub vault_0_mint: UncheckedAccount<'info>,
    /// CHECK: mint token1 пула, проверяет Raydium.
    pub vault_1_mint: UncheckedAccount<'info>,
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub token_program_2022: Program<'info, Token2022>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    /// CHECK: SPL Memo по адресу.
    #[account(address = raydium::MEMO_PROGRAM_ID)]
    pub memo_program: UncheckedAccount<'info>,
}

impl<'info> VaultPosition<'info> {
    /// Токены и комиссии ходят только между пулом и хранилищами vault.
    fn require_vault_storages(&self) -> Result<()> {
        let storages = [
            self.vault.vault_token_account,
            self.strategy.second_token_account,
        ];
        let accounts = [self.token_account_0.key(), self.token_account_1.key()];
        require!(
            accounts[0] != accounts[1]
                && storages.contains(&accounts[0])
                && storages.contains(&accounts[1]),
            VaultError::NotVaultStorage
        );
        Ok(())
    }

    fn require_current_position(&self) -> Result<()> {
        require_keys_neq!(
            self.strategy.position_nft_mint,
            Pubkey::default(),
            VaultError::NoPosition
        );
        require_keys_eq!(
            self.position_nft_mint.key(),
            self.strategy.position_nft_mint,
            VaultError::WrongPosition
        );
        Ok(())
    }

    fn increase_liquidity(
        &self,
        remaining: &[AccountInfo<'info>],
        amount_0_max: u64,
        amount_1_max: u64,
        base_0: bool,
    ) -> Result<()> {
        let mut data = raydium::INCREASE_LIQUIDITY_V2.to_vec();
        data.extend_from_slice(&0_u128.to_le_bytes()); // ликвидность считает Raydium из базовой суммы
        data.extend_from_slice(&amount_0_max.to_le_bytes());
        data.extend_from_slice(&amount_1_max.to_le_bytes());
        data.extend_from_slice(&[1, u8::from(base_0)]);
        let vault = self.vault.to_account_info();
        let account_0 = self.token_account_0.to_account_info();
        let account_1 = self.token_account_1.to_account_info();
        raydium::invoke(
            &self.clmm_program,
            &[
                signer(&vault, false),
                readonly(&self.position_nft_account),
                writable(&self.pool),
                readonly(&self.protocol_position),
                writable(&self.personal_position),
                writable(&self.tick_array_lower),
                writable(&self.tick_array_upper),
                writable(&account_0),
                writable(&account_1),
                writable(&self.token_vault_0),
                writable(&self.token_vault_1),
                readonly(&self.token_program),
                readonly(&self.token_program_2022),
                readonly(&self.vault_0_mint),
                readonly(&self.vault_1_mint),
            ],
            remaining,
            data,
            &self.vault.signer_seeds(),
        )
    }

    /// decrease_liquidity_v2: liquidity = 0 выплачивает только накопленные комиссии.
    fn decrease_liquidity(
        &self,
        remaining: &[AccountInfo<'info>],
        liquidity: u128,
        amount_0_min: u64,
        amount_1_min: u64,
    ) -> Result<()> {
        let mut data = raydium::DECREASE_LIQUIDITY_V2.to_vec();
        data.extend_from_slice(&liquidity.to_le_bytes());
        data.extend_from_slice(&amount_0_min.to_le_bytes());
        data.extend_from_slice(&amount_1_min.to_le_bytes());
        let vault = self.vault.to_account_info();
        let account_0 = self.token_account_0.to_account_info();
        let account_1 = self.token_account_1.to_account_info();
        raydium::invoke(
            &self.clmm_program,
            &[
                signer(&vault, false),
                readonly(&self.position_nft_account),
                writable(&self.personal_position),
                writable(&self.pool),
                readonly(&self.protocol_position),
                writable(&self.token_vault_0),
                writable(&self.token_vault_1),
                writable(&self.tick_array_lower),
                writable(&self.tick_array_upper),
                writable(&account_0),
                writable(&account_1),
                readonly(&self.token_program),
                readonly(&self.token_program_2022),
                readonly(&self.memo_program),
                readonly(&self.vault_0_mint),
                readonly(&self.vault_1_mint),
            ],
            remaining,
            data,
            &self.vault.signer_seeds(),
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub fn open_handler<'info>(
    ctx: Context<'info, VaultPosition<'info>>,
    tick_lower: i32,
    tick_upper: i32,
    tick_array_lower_start: i32,
    tick_array_upper_start: i32,
    amount_0_max: u64,
    amount_1_max: u64,
    base_0: bool,
) -> Result<()> {
    let accounts = &ctx.accounts;
    accounts.require_vault_storages()?;
    require_keys_eq!(
        accounts.strategy.position_nft_mint,
        Pubkey::default(),
        VaultError::PositionAlreadyOpen
    );
    require!(
        accounts.position_nft_mint.is_signer,
        VaultError::WrongPosition
    );

    // CPI 1: пустая позиция; ренту платит админ, NFT получает vault PDA
    let mut data = raydium::OPEN_POSITION_WITH_TOKEN22_NFT.to_vec();
    data.extend_from_slice(&tick_lower.to_le_bytes());
    data.extend_from_slice(&tick_upper.to_le_bytes());
    data.extend_from_slice(&tick_array_lower_start.to_le_bytes());
    data.extend_from_slice(&tick_array_upper_start.to_le_bytes());
    data.extend_from_slice(&0_u128.to_le_bytes());
    data.extend_from_slice(&0_u64.to_le_bytes());
    data.extend_from_slice(&0_u64.to_le_bytes());
    data.push(0); // with_metadata = false
    data.push(0); // base_flag = None: ликвидность добавит vault следующим CPI
    let admin = accounts.admin.to_account_info();
    let vault = accounts.vault.to_account_info();
    let nft_mint = accounts.position_nft_mint.to_account_info();
    let account_0 = accounts.token_account_0.to_account_info();
    let account_1 = accounts.token_account_1.to_account_info();
    let rent = accounts.rent.to_account_info();
    let system = accounts.system_program.to_account_info();
    let token = accounts.token_program.to_account_info();
    let associated = accounts.associated_token_program.to_account_info();
    let token_2022 = accounts.token_program_2022.to_account_info();
    raydium::invoke(
        &accounts.clmm_program,
        &[
            signer(&admin, true),
            readonly(&vault),
            signer(&nft_mint, true),
            writable(&accounts.position_nft_account),
            writable(&accounts.pool),
            readonly(&accounts.protocol_position),
            writable(&accounts.tick_array_lower),
            writable(&accounts.tick_array_upper),
            writable(&accounts.personal_position),
            writable(&account_0),
            writable(&account_1),
            writable(&accounts.token_vault_0),
            writable(&accounts.token_vault_1),
            readonly(&rent),
            readonly(&system),
            readonly(&token),
            readonly(&associated),
            readonly(&token_2022),
            readonly(&accounts.vault_0_mint),
            readonly(&accounts.vault_1_mint),
        ],
        ctx.remaining_accounts,
        data,
        &accounts.vault.signer_seeds(),
    )?;

    // CPI 2: ликвидность из хранилищ vault по подписи vault PDA
    accounts.increase_liquidity(ctx.remaining_accounts, amount_0_max, amount_1_max, base_0)?;
    let nft = accounts.position_nft_mint.key();
    ctx.accounts.strategy.position_nft_mint = nft;
    msg!(
        "Position opened: ticks {}..{}, nft {}",
        tick_lower,
        tick_upper,
        nft
    );
    Ok(())
}

pub fn collect_handler<'info>(ctx: Context<'info, VaultPosition<'info>>) -> Result<()> {
    let accounts = &ctx.accounts;
    accounts.require_vault_storages()?;
    accounts.require_current_position()?;
    accounts.decrease_liquidity(ctx.remaining_accounts, 0, 0, 0)?;
    msg!(
        "Fees collected for nft {}",
        accounts.position_nft_mint.key()
    );
    Ok(())
}

pub fn close_handler<'info>(
    ctx: Context<'info, VaultPosition<'info>>,
    amount_0_min: u64,
    amount_1_min: u64,
) -> Result<()> {
    let accounts = &ctx.accounts;
    accounts.require_vault_storages()?;
    accounts.require_current_position()?;
    let liquidity = raydium::position_liquidity(&accounts.personal_position)?;
    if liquidity > 0 {
        accounts.decrease_liquidity(
            ctx.remaining_accounts,
            liquidity,
            amount_0_min,
            amount_1_min,
        )?;
    }

    // NFT сжигается, рента позиции возвращается vault
    let vault = accounts.vault.to_account_info();
    let system = accounts.system_program.to_account_info();
    let token_2022 = accounts.token_program_2022.to_account_info();
    raydium::invoke(
        &accounts.clmm_program,
        &[
            signer(&vault, true),
            writable(&accounts.position_nft_mint),
            writable(&accounts.position_nft_account),
            writable(&accounts.personal_position),
            readonly(&system),
            readonly(&token_2022),
        ],
        &[],
        raydium::CLOSE_POSITION.to_vec(),
        &accounts.vault.signer_seeds(),
    )?;
    ctx.accounts.strategy.position_nft_mint = Pubkey::default();
    msg!("Position closed: liquidity {} returned to vault", liquidity);
    Ok(())
}
