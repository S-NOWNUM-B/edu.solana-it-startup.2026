use anchor_lang::prelude::*;
use anchor_spl::{token::Token, token_2022::Token2022, token_interface::TokenAccount};

use crate::{
    error::VaultError,
    raydium::{self, readonly, signer, writable},
    state::{Strategy, VaultState, STRATEGY_SEED, VAULT_SEED},
};

#[derive(Accounts)]
pub struct Swap<'info> {
    pub admin: Signer<'info>,
    #[account(
        seeds = [VAULT_SEED, vault.token_mint.as_ref()],
        bump = vault.bump,
        has_one = admin,
        has_one = vault_token_account,
    )]
    pub vault: Box<Account<'info, VaultState>>,
    /// Обмен разрешён только через пул стратегии.
    #[account(
        seeds = [STRATEGY_SEED, vault.key().as_ref()],
        bump = strategy.bump,
        has_one = second_token_account,
        constraint = strategy.pool == pool.key() @ VaultError::WrongPool,
    )]
    pub strategy: Box<Account<'info, Strategy>>,
    /// CHECK: адрес программы Raydium CLMM в devnet.
    #[account(address = raydium::CLMM_PROGRAM_ID)]
    pub clmm_program: UncheckedAccount<'info>,
    /// CHECK: Raydium сверяет с pool.amm_config.
    pub amm_config: UncheckedAccount<'info>,
    /// CHECK: совпадает с пулом стратегии; состояние проверяет Raydium.
    #[account(mut)]
    pub pool: UncheckedAccount<'info>,
    #[account(mut)]
    pub vault_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub second_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: хранилище пула для входного токена, проверяет Raydium.
    #[account(mut)]
    pub input_pool_vault: UncheckedAccount<'info>,
    /// CHECK: хранилище пула для выходного токена, проверяет Raydium.
    #[account(mut)]
    pub output_pool_vault: UncheckedAccount<'info>,
    /// CHECK: oracle пула, проверяет Raydium.
    #[account(mut)]
    pub observation: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
    pub token_program_2022: Program<'info, Token2022>,
    /// CHECK: SPL Memo по адресу.
    #[account(address = raydium::MEMO_PROGRAM_ID)]
    pub memo_program: UncheckedAccount<'info>,
    /// CHECK: mint входного токена, проверяет Raydium.
    pub input_mint: UncheckedAccount<'info>,
    /// CHECK: mint выходного токена, проверяет Raydium.
    pub output_mint: UncheckedAccount<'info>,
}

/// sell_vault_token: true — отдаём токен vault, получаем второй; false — наоборот.
pub fn handler<'info>(
    ctx: Context<'info, Swap<'info>>,
    amount_in: u64,
    minimum_amount_out: u64,
    sell_vault_token: bool,
) -> Result<()> {
    require!(amount_in > 0, VaultError::ZeroAmount);
    let accounts = &ctx.accounts;
    let (input, output) = if sell_vault_token {
        (
            &accounts.vault_token_account,
            &accounts.second_token_account,
        )
    } else {
        (
            &accounts.second_token_account,
            &accounts.vault_token_account,
        )
    };
    let output_before = output.amount;

    let mut data = raydium::SWAP_V2.to_vec();
    data.extend_from_slice(&amount_in.to_le_bytes());
    data.extend_from_slice(&minimum_amount_out.to_le_bytes());
    data.extend_from_slice(&0_u128.to_le_bytes()); // без ограничения цены: защищает минимум
    data.push(1); // amount_in — точная сумма на входе
    let vault_info = accounts.vault.to_account_info();
    let seeds = accounts.vault.signer_seeds();
    raydium::invoke(
        &accounts.clmm_program,
        &[
            signer(&vault_info, false),
            readonly(&accounts.amm_config),
            writable(&accounts.pool),
            writable(&input.to_account_info()),
            writable(&output.to_account_info()),
            writable(&accounts.input_pool_vault),
            writable(&accounts.output_pool_vault),
            writable(&accounts.observation),
            readonly(&accounts.token_program),
            readonly(&accounts.token_program_2022),
            readonly(&accounts.memo_program),
            readonly(&accounts.input_mint),
            readonly(&accounts.output_mint),
        ],
        ctx.remaining_accounts,
        data,
        &seeds,
    )?;

    let output = if sell_vault_token {
        &mut ctx.accounts.second_token_account
    } else {
        &mut ctx.accounts.vault_token_account
    };
    output.reload()?;
    let received = output.amount - output_before;
    require!(received >= minimum_amount_out, VaultError::SlippageExceeded);
    msg!(
        "Swap: {} in -> {} out (minimum {})",
        amount_in,
        received,
        minimum_amount_out
    );
    Ok(())
}
