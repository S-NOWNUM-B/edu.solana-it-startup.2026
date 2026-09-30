use anchor_lang::prelude::*;

declare_id!("dtAv1kjUCA3BTjbs75nMn1fV9vfZ23WEq1AZaDJbNex");

#[program]
pub mod hello_devnet {
    use super::*;

    pub fn greet(_ctx: Context<Greet>) -> Result<()> {
        msg!("Hello from Solana Devnet!");
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Greet {}
