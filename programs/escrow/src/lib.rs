use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, TransferChecked};

pub mod error;
pub mod instructions;
pub mod state;
mod token_cpi;

use error::EscrowError;
pub use instructions::*;
use state::{EscrowState, EscrowStatus};

declare_id!("5yumooWcYbtWgi5KwiJnbMkqsMJjhDvw3JsJGgY25hg2");

#[program]
pub mod escrow {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, deal_id: u64, amount: u64) -> Result<()> {
        require!(amount > 0, EscrowError::AmountMustBePositive);
        ctx.accounts.escrow.set_inner(EscrowState {
            sender: ctx.accounts.sender.key(),
            receiver: ctx.accounts.receiver.key(),
            mint: ctx.accounts.mint.key(),
            amount,
            deal_id,
            bump: ctx.bumps.escrow,
            status: EscrowStatus::Created,
        });
        ctx.accounts.receipt.status = EscrowStatus::Created;
        Ok(())
    }

    pub fn deposit(ctx: Context<Deposit>) -> Result<()> {
        token_interface::transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                TransferChecked {
                    from: ctx.accounts.source.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.vault.to_account_info(),
                    authority: ctx.accounts.sender.to_account_info(),
                },
            ),
            ctx.accounts.escrow.amount,
            ctx.accounts.mint.decimals,
        )?;
        ctx.accounts.escrow.status = EscrowStatus::Funded;
        ctx.accounts.receipt.status = EscrowStatus::Funded;
        Ok(())
    }

    pub fn release(ctx: Context<Release>) -> Result<()> {
        let a = ctx.accounts;
        let surplus = a
            .vault
            .amount
            .checked_sub(a.escrow.amount)
            .ok_or(EscrowError::InvalidVaultBalance)?;
        token_cpi::transfer_from_vault(
            &a.escrow,
            &a.vault,
            &a.mint,
            &a.receiver_account,
            &a.token_program,
            a.escrow.amount,
        )?;
        // Посторонний перевод в vault не должен блокировать закрытие сделки.
        token_cpi::transfer_from_vault(
            &a.escrow,
            &a.vault,
            &a.mint,
            &a.sender_account,
            &a.token_program,
            surplus,
        )?;
        token_cpi::close_vault(&a.escrow, &a.vault, &a.sender, &a.token_program)?;
        a.escrow.status = EscrowStatus::Released;
        a.receipt.status = EscrowStatus::Released;
        Ok(())
    }

    pub fn cancel(ctx: Context<Cancel>) -> Result<()> {
        let a = ctx.accounts;
        if a.escrow.status == EscrowStatus::Funded {
            require!(
                a.vault.amount >= a.escrow.amount,
                EscrowError::InvalidVaultBalance
            );
        }
        token_cpi::transfer_from_vault(
            &a.escrow,
            &a.vault,
            &a.mint,
            &a.sender_account,
            &a.token_program,
            a.vault.amount,
        )?;
        token_cpi::close_vault(&a.escrow, &a.vault, &a.sender, &a.token_program)?;
        a.escrow.status = EscrowStatus::Cancelled;
        a.receipt.status = EscrowStatus::Cancelled;
        Ok(())
    }
}
