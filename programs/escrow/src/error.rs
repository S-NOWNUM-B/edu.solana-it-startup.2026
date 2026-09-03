use anchor_lang::prelude::*;

#[error_code]
pub enum EscrowError {
    #[msg("Amount must be greater than zero")]
    AmountMustBePositive,
    #[msg("Receiver must be different from sender")]
    SenderEqualsReceiver,
    #[msg("Only the original sender can manage this deal")]
    UnauthorizedSender,
    #[msg("Receiver does not match the deal")]
    InvalidReceiver,
    #[msg("Mint does not match the deal")]
    InvalidMint,
    #[msg("Only Token-2022 is supported")]
    InvalidTokenProgram,
    #[msg("This operation is not allowed in the current deal status")]
    InvalidStatus,
    #[msg("Only mints without extensions and freeze authority are supported")]
    UnsupportedMint,
    #[msg("Vault balance is smaller than the funded amount")]
    InvalidVaultBalance,
}
