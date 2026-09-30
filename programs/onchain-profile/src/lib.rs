use anchor_lang::prelude::*;

declare_id!("3j2EZTtxkTLmxCjnpBzJhhf3QsavZwueQ9QbQzdxqyzj");

pub const PROFILE_SEED: &[u8] = b"profile";
pub const MAX_NAME_BYTES: usize = 32;
pub const MAX_BIO_BYTES: usize = 160;

#[program]
pub mod onchain_profile {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, display_name: String, bio: String) -> Result<()> {
        validate_fields(&display_name, &bio)?;
        let profile = &mut ctx.accounts.profile;
        profile.authority = ctx.accounts.authority.key();
        profile.display_name = display_name;
        profile.bio = bio;
        profile.bump = ctx.bumps.profile;
        Ok(())
    }

    pub fn update_profile(
        ctx: Context<UpdateProfile>,
        display_name: String,
        bio: String,
    ) -> Result<()> {
        validate_fields(&display_name, &bio)?;
        let profile = &mut ctx.accounts.profile;
        profile.display_name = display_name;
        profile.bio = bio;
        Ok(())
    }
}

pub fn validate_fields(display_name: &str, bio: &str) -> Result<()> {
    require!(!display_name.trim().is_empty(), ProfileError::EmptyName);
    require!(
        display_name.len() <= MAX_NAME_BYTES,
        ProfileError::NameTooLong
    );
    require!(bio.len() <= MAX_BIO_BYTES, ProfileError::BioTooLong);
    Ok(())
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = Profile::DISCRIMINATOR.len() + Profile::INIT_SPACE,
        seeds = [PROFILE_SEED, authority.key().as_ref()],
        bump
    )]
    pub profile: Account<'info, Profile>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateProfile<'info> {
    #[account(
        mut,
        has_one = authority @ ProfileError::Unauthorized,
        seeds = [PROFILE_SEED, profile.authority.as_ref()],
        bump = profile.bump
    )]
    pub profile: Account<'info, Profile>,
    pub authority: Signer<'info>,
}

#[account]
#[derive(InitSpace)]
pub struct Profile {
    pub authority: Pubkey,
    #[max_len(32)]
    pub display_name: String,
    #[max_len(160)]
    pub bio: String,
    pub bump: u8,
}

#[error_code]
pub enum ProfileError {
    #[msg("Only the profile authority can update it")]
    Unauthorized,
    #[msg("Display name must not be empty or whitespace")]
    EmptyName,
    #[msg("Display name exceeds 32 UTF-8 bytes")]
    NameTooLong,
    #[msg("Bio exceeds 160 UTF-8 bytes")]
    BioTooLong,
}
