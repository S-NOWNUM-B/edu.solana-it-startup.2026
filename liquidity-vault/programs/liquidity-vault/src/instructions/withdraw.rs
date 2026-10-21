use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    self, Burn, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::{
    error::VaultError,
    math::{mul_div, tokens_for_shares},
    state::{UserCard, VaultState, USER_CARD_SEED, VAULT_SEED},
};

#[derive(Accounts)]
pub struct Withdraw<'info> {
    pub user: Signer<'info>,
    #[account(
        mut,
        seeds = [VAULT_SEED, token_mint.key().as_ref()],
        bump = vault.bump,
        has_one = token_mint,
        has_one = vault_token_account,
        has_one = share_mint,
        has_one = token_program,
    )]
    pub vault: Box<Account<'info, VaultState>>,
    #[account(
        mut,
        seeds = [USER_CARD_SEED, vault.key().as_ref(), user.key().as_ref()],
        bump = user_card.bump,
    )]
    pub user_card: Box<Account<'info, UserCard>>,
    pub token_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut)]
    pub vault_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub share_mint: Box<InterfaceAccount<'info, Mint>>,
    /// Токены возвращаются только на свой счёт пользователя.
    #[account(
        mut,
        token::mint = token_mint,
        token::authority = user,
        token::token_program = token_program,
    )]
    pub user_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = share_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program,
    )]
    pub user_share_account: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handler(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
    require!(shares > 0, VaultError::ZeroAmount);
    let user_shares = ctx.accounts.user_share_account.amount;
    require!(shares <= user_shares, VaultError::InsufficientShares);
    let amount = tokens_for_shares(
        shares,
        ctx.accounts.share_mint.supply,
        ctx.accounts.vault_token_account.amount,
    )?;
    require!(amount > 0, VaultError::ZeroAmount);

    // CPI 1: сжечь share-токены пользователя по его подписи
    token_interface::burn(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            Burn {
                mint: ctx.accounts.share_mint.to_account_info(),
                from: ctx.accounts.user_share_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        shares,
    )?;

    // CPI 2: отдать токены из хранилища подписью vault PDA
    let seeds = ctx.accounts.vault.signer_seeds();
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.vault_token_account.to_account_info(),
                mint: ctx.accounts.token_mint.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.vault.to_account_info(),
            },
            &[&seeds],
        ),
        amount,
        ctx.accounts.token_mint.decimals,
    )?;

    // Внесённая сумма уменьшается пропорционально возвращённой доле
    let card = &mut ctx.accounts.user_card;
    let principal = mul_div(card.deposited, shares, user_shares)?;
    card.deposited -= principal;
    let vault = &mut ctx.accounts.vault;
    vault.total_deposited = vault.total_deposited.saturating_sub(principal);
    msg!("Withdraw: {} shares -> {} tokens", shares, amount);
    Ok(())
}
