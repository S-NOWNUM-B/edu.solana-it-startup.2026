use anchor_lang::prelude::*;

#[error_code]
pub enum VaultError {
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Deposit is too small to receive a share")]
    ZeroShares,
    #[msg("Math overflow")]
    MathOverflow,
}
