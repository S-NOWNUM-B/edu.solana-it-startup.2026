use anchor_lang::prelude::*;

pub mod error;
pub mod instructions;
pub mod math;
pub mod raydium;
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

    /// Пользователь возвращает share-токены и забирает свою долю токенов vault.
    pub fn withdraw(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
        instructions::withdraw::handler(ctx, shares)
    }

    /// Админ подключает пул Raydium CLMM и создаёт второе хранилище vault.
    pub fn init_strategy(ctx: Context<InitStrategy>) -> Result<()> {
        instructions::init_strategy::handler(ctx)
    }

    /// Админ обменивает токены vault через пул стратегии; подписывает vault PDA.
    pub fn swap<'info>(
        ctx: Context<'info, Swap<'info>>,
        amount_in: u64,
        minimum_amount_out: u64,
        sell_vault_token: bool,
    ) -> Result<()> {
        instructions::swap::handler(ctx, amount_in, minimum_amount_out, sell_vault_token)
    }
}
