use anchor_lang::prelude::*;

pub mod error;
pub mod instructions;
pub mod math;
pub mod state;

pub use instructions::*;

declare_id!("3XvGNievhhJ9fX5SR46uK4TeBpuCYVV548WpzE5CnmAK");

#[program]
pub mod liquidity_vault {
    use super::*;

    /// Один раз создаёт настройки vault, хранилище токенов и share mint.
    pub fn initialize_vault(ctx: Context<InitializeVault>) -> Result<()> {
        instructions::initialize_vault::handler(ctx)
    }

    /// Пользователь создаёт свою карточку; адрес выводится из vault и пользователя.
    pub fn create_user_card(ctx: Context<CreateUserCard>) -> Result<()> {
        instructions::create_user_card::handler(ctx)
    }

    /// Пользователь вносит токены и получает share-токены по текущему курсу.
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        instructions::deposit::handler(ctx, amount)
    }
}
