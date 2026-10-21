use anchor_lang::prelude::*;

pub const VAULT_SEED: &[u8] = b"vault";
pub const VAULT_TOKENS_SEED: &[u8] = b"vault_tokens";
pub const SHARE_MINT_SEED: &[u8] = b"share_mint";
pub const USER_CARD_SEED: &[u8] = b"user_card";
pub const STRATEGY_SEED: &[u8] = b"strategy";
pub const SECOND_TOKENS_SEED: &[u8] = b"second_tokens";

/// Настройки vault. PDA ["vault", token_mint] — authority хранилища и share mint.
#[account]
#[derive(InitSpace)]
pub struct VaultState {
    pub admin: Pubkey,
    pub token_mint: Pubkey,
    pub token_program: Pubkey,
    pub vault_token_account: Pubkey,
    pub share_mint: Pubkey,
    /// Сколько токенов внесено пользователями (без дохода vault).
    pub total_deposited: u64,
    pub bump: u8,
}

impl VaultState {
    /// Подпись программы от имени vault PDA.
    pub fn signer_seeds(&self) -> [&[u8]; 3] {
        [
            VAULT_SEED,
            self.token_mint.as_ref(),
            std::slice::from_ref(&self.bump),
        ]
    }
}

/// Карточка пользователя. PDA ["user_card", vault, owner].
#[account]
#[derive(InitSpace)]
pub struct UserCard {
    pub vault: Pubkey,
    pub owner: Pubkey,
    /// Сколько токенов пользователь внёс и ещё не вывел.
    pub deposited: u64,
    pub created_at: i64,
    pub bump: u8,
}

/// Стратегия vault. PDA ["strategy", vault]: пул Raydium и второе хранилище.
#[account]
#[derive(InitSpace)]
pub struct Strategy {
    pub vault: Pubkey,
    pub pool: Pubkey,
    pub second_mint: Pubkey,
    pub second_token_account: Pubkey,
    /// NFT открытой позиции vault в пуле; default — позиции нет.
    pub position_nft_mint: Pubkey,
    pub bump: u8,
}
