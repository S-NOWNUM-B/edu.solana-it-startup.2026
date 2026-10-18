use anchor_lang::prelude::*;

pub mod error;
pub mod instructions;

pub use instructions::*;

declare_id!("3pX5NKLru1UBDVckynWQxsgnJeUN3N1viy36Gk9TSn8d");

#[program]
pub mod solana_level_1_token_starter {
    use super::*;

    pub fn create_token(ctx: Context<CreateToken>, decimals: u8) -> Result<()> {
        instructions::create_token::handler(ctx, decimals)
    }

    pub fn create_token_account(ctx: Context<CreateTokenAccount>) -> Result<()> {
        instructions::create_token_account::handler(ctx)
    }

    pub fn mint_tokens(ctx: Context<MintTokens>, amount: u64) -> Result<()> {
        instructions::mint_tokens::handler(ctx, amount)
    }

    pub fn transfer_tokens(ctx: Context<TransferTokens>, amount: u64) -> Result<()> {
        instructions::transfer_tokens::handler(ctx, amount)
    }

    pub fn compose_transfer(
        ctx: Context<ComposeTransfer>,
        token_amount: u64,
        lamports: u64,
    ) -> Result<()> {
        instructions::compose_transfer::handler(ctx, token_amount, lamports)
    }
}
