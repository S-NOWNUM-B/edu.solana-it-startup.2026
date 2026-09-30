use anchor_lang::{prelude::Pubkey, AccountDeserialize, InstructionData, ToAccountMetas};
use onchain_profile::{validate_fields, Profile, PROFILE_SEED};
use solana_message::Instruction;
use std::{error::Error, io};

pub fn profile_address(authority: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[PROFILE_SEED, authority.as_ref()], &onchain_profile::ID)
}

pub fn initialize_instruction(authority: Pubkey, display_name: String, bio: String) -> Instruction {
    Instruction {
        program_id: onchain_profile::ID,
        accounts: onchain_profile::accounts::Initialize {
            profile: profile_address(&authority).0,
            authority,
            system_program: anchor_lang::system_program::ID,
        }
        .to_account_metas(None),
        data: onchain_profile::instruction::Initialize { display_name, bio }.data(),
    }
}

pub fn update_instruction(authority: Pubkey, display_name: String, bio: String) -> Instruction {
    Instruction {
        program_id: onchain_profile::ID,
        accounts: onchain_profile::accounts::UpdateProfile {
            profile: profile_address(&authority).0,
            authority,
        }
        .to_account_metas(None),
        data: onchain_profile::instruction::UpdateProfile { display_name, bio }.data(),
    }
}

pub fn decode_profile(
    address: &Pubkey,
    owner: &Pubkey,
    data: &[u8],
    authority: &Pubkey,
) -> Result<Profile, Box<dyn Error>> {
    let (expected_address, expected_bump) = profile_address(authority);
    if *owner != onchain_profile::ID || *address != expected_address {
        return Err(io::Error::other("Unexpected profile address or program owner").into());
    }
    let profile = Profile::try_deserialize(&mut &data[..])?;
    if profile.authority != *authority || profile.bump != expected_bump {
        return Err(io::Error::other("Unexpected profile authority or PDA bump").into());
    }
    validate_fields(&profile.display_name, &profile.bio)?;
    Ok(profile)
}
