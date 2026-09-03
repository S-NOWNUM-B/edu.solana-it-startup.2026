use anchor_lang::{prelude::*, solana_program::program_pack::Pack};
use anchor_spl::{
    token_2022::{self, spl_token_2022},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    error::EscrowError,
    state::{DealReceipt, EscrowState, EscrowStatus},
};

#[derive(Accounts)]
#[instruction(deal_id: u64, amount: u64)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub sender: Signer<'info>,
    #[account(constraint = receiver.key() != sender.key() @ EscrowError::SenderEqualsReceiver)]
    pub receiver: SystemAccount<'info>,
    #[account(
        mint::token_program = token_program,
        constraint = mint.to_account_info().data_len() == spl_token_2022::state::Mint::LEN @ EscrowError::UnsupportedMint,
        constraint = mint.freeze_authority.is_none() @ EscrowError::UnsupportedMint,
    )]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        init, payer = sender, space = 8 + DealReceipt::INIT_SPACE,
        seeds = [b"used", sender.key().as_ref(), &deal_id.to_le_bytes()], bump,
    )]
    pub receipt: Account<'info, DealReceipt>,
    #[account(
        init, payer = sender, space = 8 + EscrowState::INIT_SPACE,
        seeds = [b"escrow", sender.key().as_ref(), &deal_id.to_le_bytes()], bump,
    )]
    pub escrow: Account<'info, EscrowState>,
    #[account(
        init, payer = sender,
        seeds = [b"vault", escrow.key().as_ref()], bump,
        token::mint = mint, token::authority = escrow, token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    #[account(address = token_2022::ID @ EscrowError::InvalidTokenProgram)]
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    pub sender: Signer<'info>,
    #[account(
        mut,
        has_one = sender @ EscrowError::UnauthorizedSender,
        has_one = mint @ EscrowError::InvalidMint,
        seeds = [b"escrow", escrow.sender.as_ref(), &escrow.deal_id.to_le_bytes()], bump = escrow.bump,
        constraint = escrow.status == EscrowStatus::Created @ EscrowError::InvalidStatus,
    )]
    pub escrow: Account<'info, EscrowState>,
    #[account(
        mut, seeds = [b"used", escrow.sender.as_ref(), &escrow.deal_id.to_le_bytes()], bump,
        constraint = receipt.status == escrow.status @ EscrowError::InvalidStatus,
    )]
    pub receipt: Account<'info, DealReceipt>,
    #[account(mint::token_program = token_program)]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut, token::mint = mint, token::authority = sender, token::token_program = token_program)]
    pub source: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut, seeds = [b"vault", escrow.key().as_ref()], bump,
        token::mint = mint, token::authority = escrow, token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    #[account(address = token_2022::ID @ EscrowError::InvalidTokenProgram)]
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct Release<'info> {
    #[account(mut)]
    pub sender: Signer<'info>,
    pub receiver: SystemAccount<'info>,
    #[account(
        mut, close = sender,
        has_one = sender @ EscrowError::UnauthorizedSender,
        has_one = receiver @ EscrowError::InvalidReceiver,
        has_one = mint @ EscrowError::InvalidMint,
        seeds = [b"escrow", escrow.sender.as_ref(), &escrow.deal_id.to_le_bytes()], bump = escrow.bump,
        constraint = escrow.status == EscrowStatus::Funded @ EscrowError::InvalidStatus,
    )]
    // Крупные обёртки вынесены в heap, чтобы разбор Accounts помещался в SBF-стек 4 KiB.
    pub escrow: Box<Account<'info, EscrowState>>,
    #[account(
        mut, seeds = [b"used", escrow.sender.as_ref(), &escrow.deal_id.to_le_bytes()], bump,
        constraint = receipt.status == escrow.status @ EscrowError::InvalidStatus,
    )]
    pub receipt: Account<'info, DealReceipt>,
    #[account(mint::token_program = token_program)]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut, seeds = [b"vault", escrow.key().as_ref()], bump,
        token::mint = mint, token::authority = escrow, token::token_program = token_program,
    )]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut, associated_token::mint = mint, associated_token::authority = receiver,
        associated_token::token_program = token_program,
    )]
    pub receiver_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut, associated_token::mint = mint, associated_token::authority = sender,
        associated_token::token_program = token_program,
    )]
    pub sender_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(address = token_2022::ID @ EscrowError::InvalidTokenProgram)]
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct Cancel<'info> {
    #[account(mut)]
    pub sender: Signer<'info>,
    #[account(
        mut, close = sender,
        has_one = sender @ EscrowError::UnauthorizedSender,
        has_one = mint @ EscrowError::InvalidMint,
        seeds = [b"escrow", escrow.sender.as_ref(), &escrow.deal_id.to_le_bytes()], bump = escrow.bump,
        constraint = matches!(escrow.status, EscrowStatus::Created | EscrowStatus::Funded) @ EscrowError::InvalidStatus,
    )]
    pub escrow: Account<'info, EscrowState>,
    #[account(
        mut, seeds = [b"used", escrow.sender.as_ref(), &escrow.deal_id.to_le_bytes()], bump,
        constraint = receipt.status == escrow.status @ EscrowError::InvalidStatus,
    )]
    pub receipt: Account<'info, DealReceipt>,
    #[account(mint::token_program = token_program)]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut, seeds = [b"vault", escrow.key().as_ref()], bump,
        token::mint = mint, token::authority = escrow, token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut, associated_token::mint = mint, associated_token::authority = sender,
        associated_token::token_program = token_program,
    )]
    pub sender_account: InterfaceAccount<'info, TokenAccount>,
    #[account(address = token_2022::ID @ EscrowError::InvalidTokenProgram)]
    pub token_program: Interface<'info, TokenInterface>,
}
