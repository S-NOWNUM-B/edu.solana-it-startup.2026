use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::{
    error::VaultError,
    raydium,
    state::{Strategy, VaultState, SECOND_TOKENS_SEED, STRATEGY_SEED, VAULT_SEED},
};

#[derive(Accounts)]
pub struct InitStrategy<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        seeds = [VAULT_SEED, vault.token_mint.as_ref()],
        bump = vault.bump,
        has_one = admin,
    )]
    pub vault: Box<Account<'info, VaultState>>,
    /// Одна стратегия на vault: пул и второе хранилище задаются один раз.
    #[account(
        init,
        payer = admin,
        space = 8 + Strategy::INIT_SPACE,
        seeds = [STRATEGY_SEED, vault.key().as_ref()],
        bump,
    )]
    pub strategy: Box<Account<'info, Strategy>>,
    /// CHECK: владелец, discriminator и mint'ы проверяются в handler.
    pub pool: UncheckedAccount<'info>,
    #[account(mint::token_program = second_token_program)]
    pub second_mint: Box<InterfaceAccount<'info, Mint>>,
    /// Второе хранилище: authority — vault PDA, как у основного.
    #[account(
        init,
        payer = admin,
        seeds = [SECOND_TOKENS_SEED, vault.key().as_ref()],
        bump,
        token::mint = second_mint,
        token::authority = vault,
        token::token_program = second_token_program,
    )]
    pub second_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    pub second_token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<InitStrategy>) -> Result<()> {
    let mints = raydium::pool_mints(&ctx.accounts.pool)?;
    let vault_mint = ctx.accounts.vault.token_mint;
    let second_mint = ctx.accounts.second_mint.key();
    require!(
        vault_mint != second_mint && mints.contains(&vault_mint) && mints.contains(&second_mint),
        VaultError::PoolMintMismatch
    );
    let strategy = &mut ctx.accounts.strategy;
    strategy.vault = ctx.accounts.vault.key();
    strategy.pool = ctx.accounts.pool.key();
    strategy.second_mint = second_mint;
    strategy.second_token_account = ctx.accounts.second_token_account.key();
    strategy.position_nft_mint = Pubkey::default();
    strategy.bump = ctx.bumps.strategy;
    msg!(
        "Strategy: pool {}, second mint {}",
        strategy.pool,
        second_mint
    );
    Ok(())
}
