use anchor_lang::prelude::*;

use crate::state::{UserCard, VaultState, USER_CARD_SEED, VAULT_SEED};

#[derive(Accounts)]
pub struct CreateUserCard<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(seeds = [VAULT_SEED, vault.token_mint.as_ref()], bump = vault.bump)]
    pub vault: Account<'info, VaultState>,
    /// Адрес выводится из vault и пользователя: список пользователей хранить не нужно.
    #[account(
        init,
        payer = user,
        space = 8 + UserCard::INIT_SPACE,
        seeds = [USER_CARD_SEED, vault.key().as_ref(), user.key().as_ref()],
        bump,
    )]
    pub user_card: Account<'info, UserCard>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<CreateUserCard>) -> Result<()> {
    let card = &mut ctx.accounts.user_card;
    card.vault = ctx.accounts.vault.key();
    card.owner = ctx.accounts.user.key();
    card.deposited = 0;
    card.created_at = Clock::get()?.unix_timestamp;
    card.bump = ctx.bumps.user_card;
    msg!("User card {} created for {}", card.key(), card.owner);
    Ok(())
}
