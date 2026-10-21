use anchor_lang::prelude::*;

#[error_code]
pub enum VaultError {
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Deposit is too small to receive a share")]
    ZeroShares,
    #[msg("Math overflow")]
    MathOverflow,
    #[msg("Not enough share tokens")]
    InsufficientShares,
    #[msg("Account is not a Raydium CLMM pool")]
    NotRaydiumPool,
    #[msg("Pool must contain the vault token and the second token")]
    PoolMintMismatch,
    #[msg("Only the strategy pool is allowed")]
    WrongPool,
    #[msg("Received less than the minimum amount")]
    SlippageExceeded,
    #[msg("Position token accounts must be the vault storages")]
    NotVaultStorage,
    #[msg("Vault already has an open position")]
    PositionAlreadyOpen,
    #[msg("Vault has no open position")]
    NoPosition,
    #[msg("Position does not belong to the vault strategy")]
    WrongPosition,
}
