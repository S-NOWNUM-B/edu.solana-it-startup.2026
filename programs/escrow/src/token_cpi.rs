use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    self, CloseAccount, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::state::EscrowState;

pub fn transfer_from_vault<'info>(
    escrow: &Account<'info, EscrowState>,
    vault: &InterfaceAccount<'info, TokenAccount>,
    mint: &InterfaceAccount<'info, Mint>,
    destination: &InterfaceAccount<'info, TokenAccount>,
    token_program: &Interface<'info, TokenInterface>,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let deal_id = escrow.deal_id.to_le_bytes();
    let bump = [escrow.bump];
    let seeds: &[&[u8]] = &[b"escrow", escrow.sender.as_ref(), &deal_id, &bump];
    token_interface::transfer_checked(
        CpiContext::new(
            token_program.key(),
            TransferChecked {
                from: vault.to_account_info(),
                mint: mint.to_account_info(),
                to: destination.to_account_info(),
                authority: escrow.to_account_info(),
            },
        )
        .with_signer(&[seeds]),
        amount,
        mint.decimals,
    )
}

pub fn close_vault<'info>(
    escrow: &Account<'info, EscrowState>,
    vault: &InterfaceAccount<'info, TokenAccount>,
    sender: &Signer<'info>,
    token_program: &Interface<'info, TokenInterface>,
) -> Result<()> {
    let deal_id = escrow.deal_id.to_le_bytes();
    let bump = [escrow.bump];
    let seeds: &[&[u8]] = &[b"escrow", escrow.sender.as_ref(), &deal_id, &bump];
    token_interface::close_account(
        CpiContext::new(
            token_program.key(),
            CloseAccount {
                account: vault.to_account_info(),
                destination: sender.to_account_info(),
                authority: escrow.to_account_info(),
            },
        )
        .with_signer(&[seeds]),
    )
}
