use anchor_lang::prelude::*;

#[error_code]
pub enum TokenStarterError {
    #[msg("Amount must be greater than zero")]
    AmountMustBePositive,
    #[msg("Source and destination token accounts must be different")]
    SourceEqualsDestination,
    #[msg("Payer and recipient wallets must be different")]
    PayerEqualsRecipient,
    #[msg("Recipient token account must be its associated token address")]
    InvalidRecipientTokenAccount,
}
