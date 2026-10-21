use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::state::{VaultState, SHARE_MINT_SEED, VAULT_SEED, VAULT_TOKENS_SEED};

#[derive(Accounts)]
pub struct InitializeVault<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mint::token_program = token_program)]
    pub token_mint: InterfaceAccount<'info, Mint>,
    #[account(
        init,
        payer = admin,
        space = 8 + VaultState::INIT_SPACE,
        seeds = [VAULT_SEED, token_mint.key().as_ref()],
        bump,
    )]
    pub vault: Account<'info, VaultState>,
    /// Хранилище: владелец аккаунта — Token Program, authority — vault PDA.
    #[account(
        init,
        payer = admin,
        seeds = [VAULT_TOKENS_SEED, vault.key().as_ref()],
        bump,
        token::mint = token_mint,
        token::authority = vault,
        token::token_program = token_program,
    )]
    pub vault_token_account: InterfaceAccount<'info, TokenAccount>,
    /// Share mint: выпускать может только vault PDA, то есть только программа.
    #[account(
        init,
        payer = admin,
        seeds = [SHARE_MINT_SEED, vault.key().as_ref()],
        bump,
        mint::decimals = token_mint.decimals,
        mint::authority = vault,
        mint::token_program = token_program,
    )]
    pub share_mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<InitializeVault>) -> Result<()> {
    let vault = &mut ctx.accounts.vault;
    vault.admin = ctx.accounts.admin.key();
    vault.token_mint = ctx.accounts.token_mint.key();
    vault.token_program = ctx.accounts.token_program.key();
    vault.vault_token_account = ctx.accounts.vault_token_account.key();
    vault.share_mint = ctx.accounts.share_mint.key();
    vault.total_deposited = 0;
    vault.bump = ctx.bumps.vault;
    msg!(
        "Vault {} created for mint {}",
        vault.key(),
        vault.token_mint
    );
    Ok(())
}
