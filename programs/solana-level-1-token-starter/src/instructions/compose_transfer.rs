use anchor_lang::{prelude::*, system_program};
use anchor_spl::{
    associated_token::{
        self, get_associated_token_address_with_program_id, AssociatedToken, Create,
    },
    token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked},
};

use crate::error::TokenStarterError;

#[derive(Accounts)]
pub struct ComposeTransfer<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        mut,
        constraint = recipient.key() != payer.key() @ TokenStarterError::PayerEqualsRecipient,
    )]
    pub recipient: SystemAccount<'info>,
    #[account(mint::token_program = token_program)]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = payer,
        token::token_program = token_program,
    )]
    pub source: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: Адрес проверяется по recipient/mint/token_program; ATA CPI проверяет содержимое.
    #[account(
        mut,
        address = get_associated_token_address_with_program_id(
            &recipient.key(), &mint.key(), &token_program.key(),
        ) @ TokenStarterError::InvalidRecipientTokenAccount,
    )]
    pub recipient_token: UncheckedAccount<'info>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<ComposeTransfer>, token_amount: u64, lamports: u64) -> Result<()> {
    require!(token_amount > 0, TokenStarterError::AmountMustBePositive);
    require!(lamports > 0, TokenStarterError::AmountMustBePositive);

    associated_token::create_idempotent(CpiContext::new(
        ctx.accounts.associated_token_program.key(),
        Create {
            payer: ctx.accounts.payer.to_account_info(),
            associated_token: ctx.accounts.recipient_token.to_account_info(),
            authority: ctx.accounts.recipient.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
            token_program: ctx.accounts.token_program.to_account_info(),
        },
    ))?;
    msg!("CPI 1/3: recipient ATA ready");

    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.key(),
            system_program::Transfer {
                from: ctx.accounts.payer.to_account_info(),
                to: ctx.accounts.recipient.to_account_info(),
            },
        ),
        lamports,
    )?;
    msg!("CPI 2/3: transferred {} lamports", lamports);

    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.source.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.recipient_token.to_account_info(),
                authority: ctx.accounts.payer.to_account_info(),
            },
        ),
        token_amount,
        ctx.accounts.mint.decimals,
    )?;
    msg!("CPI 3/3: transferred {} token units", token_amount);
    Ok(())
}
