use anchor_lang::{prelude::Pubkey, AccountDeserialize, InstructionData, ToAccountMetas};
use counter::{Counter, COUNTER_SEED};
use solana_message::Instruction;
use std::{error::Error, io};

pub fn counter_address(authority: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[COUNTER_SEED, authority.as_ref()], &counter::ID)
}

pub fn initialize_instruction(authority: Pubkey) -> Instruction {
    Instruction {
        program_id: counter::ID,
        accounts: counter::accounts::Initialize {
            counter: counter_address(&authority).0,
            authority,
            system_program: anchor_lang::system_program::ID,
        }
        .to_account_metas(None),
        data: counter::instruction::Initialize {}.data(),
    }
}

pub fn increment_instruction(authority: Pubkey) -> Instruction {
    Instruction {
        program_id: counter::ID,
        accounts: counter::accounts::Increment {
            counter: counter_address(&authority).0,
            authority,
        }
        .to_account_metas(None),
        data: counter::instruction::Increment {}.data(),
    }
}

pub fn decode_counter(
    address: &Pubkey,
    owner: &Pubkey,
    data: &[u8],
    authority: &Pubkey,
) -> Result<Counter, Box<dyn Error>> {
    let (expected_address, expected_bump) = counter_address(authority);
    if *owner != counter::ID || *address != expected_address {
        return Err(io::Error::other("Unexpected counter address or program owner").into());
    }
    let counter = Counter::try_deserialize(&mut &data[..])?;
    if counter.authority != *authority || counter.bump != expected_bump {
        return Err(io::Error::other("Unexpected counter authority or PDA bump").into());
    }
    Ok(counter)
}
