use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{self, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked},
};

use crate::{
    error::VaultError,
    math::shares_for_deposit,
    state::{UserCard, VaultState, USER_CARD_SEED, VAULT_SEED},
};

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
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
    /// Депозит возможен только со своей карточкой.
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
    #[account(
        mut,
        token::mint = token_mint,
        token::authority = user,
        token::token_program = token_program,
    )]
    pub user_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    /// ATA для share-токенов создаётся при первом депозите.
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = share_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program,
    )]
    pub user_share_account: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    require!(amount > 0, VaultError::ZeroAmount);
    let shares = shares_for_deposit(
        amount,
        ctx.accounts.share_mint.supply,
        ctx.accounts.vault_token_account.amount,
    )?;
    require!(shares > 0, VaultError::ZeroShares);

    // Токены пользователя переводятся в хранилище по его подписи
    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.user_token_account.to_account_info(),
                mint: ctx.accounts.token_mint.to_account_info(),
                to: ctx.accounts.vault_token_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount,
        ctx.accounts.token_mint.decimals,
    )?;

    // Share-токены выпускает программа подписью vault PDA
    let seeds = ctx.accounts.vault.signer_seeds();
    token_interface::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            MintTo {
                mint: ctx.accounts.share_mint.to_account_info(),
                to: ctx.accounts.user_share_account.to_account_info(),
                authority: ctx.accounts.vault.to_account_info(),
            },
            &[&seeds],
        ),
        shares,
    )?;

    let vault = &mut ctx.accounts.vault;
    vault.total_deposited = vault
        .total_deposited
        .checked_add(amount)
        .ok_or(VaultError::MathOverflow)?;
    let card = &mut ctx.accounts.user_card;
    card.deposited = card
        .deposited
        .checked_add(amount)
        .ok_or(VaultError::MathOverflow)?;
    msg!("Deposit: {} tokens -> {} shares", amount, shares);
    Ok(())
}
