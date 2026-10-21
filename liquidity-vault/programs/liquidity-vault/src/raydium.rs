//! Ручной CPI в Raydium CLMM: крейт Raydium закреплён на другой версии Anchor.
//! Порядок аккаунтов и discriminator'ы взяты из raydium-io/raydium-clmm.

use anchor_lang::{
    prelude::*,
    solana_program::{instruction::Instruction, program::invoke_signed},
};

use crate::error::VaultError;

pub const CLMM_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("DRayAUgENGQBKVaX8owNhgzkEDyoHTGVEGHVJT1E9pfH");
pub const MEMO_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
const POOL_DISCRIMINATOR: [u8; 8] = [247, 237, 227, 245, 215, 195, 222, 70];
pub const SWAP_V2: [u8; 8] = [43, 4, 237, 11, 26, 201, 30, 98];

/// Mint'ы пула из PoolState после проверки владельца и discriminator.
pub fn pool_mints(pool: &AccountInfo) -> Result<[Pubkey; 2]> {
    require_keys_eq!(*pool.owner, CLMM_PROGRAM_ID, VaultError::NotRaydiumPool);
    let data = pool.try_borrow_data()?;
    require!(
        data.len() >= 137 && data[..8] == POOL_DISCRIMINATOR,
        VaultError::NotRaydiumPool
    );
    let mint =
        |offset: usize| Pubkey::new_from_array(data[offset..offset + 32].try_into().unwrap());
    Ok([mint(73), mint(105)])
}

/// Аккаунт инструкции Raydium и его права в CPI.
pub struct Meta<'a, 'info> {
    pub info: &'a AccountInfo<'info>,
    pub writable: bool,
    pub signer: bool,
}

pub fn readonly<'a, 'info>(info: &'a AccountInfo<'info>) -> Meta<'a, 'info> {
    Meta {
        info,
        writable: false,
        signer: false,
    }
}

pub fn writable<'a, 'info>(info: &'a AccountInfo<'info>) -> Meta<'a, 'info> {
    Meta {
        info,
        writable: true,
        signer: false,
    }
}

pub fn signer<'a, 'info>(info: &'a AccountInfo<'info>, writable: bool) -> Meta<'a, 'info> {
    Meta {
        info,
        writable,
        signer: true,
    }
}

/// Вызов Raydium с подписью vault PDA; remaining accounts передаются как есть.
pub fn invoke<'info>(
    program: &AccountInfo<'info>,
    accounts: &[Meta<'_, 'info>],
    remaining: &[AccountInfo<'info>],
    data: Vec<u8>,
    signer_seeds: &[&[u8]],
) -> Result<()> {
    let mut metas = Vec::with_capacity(accounts.len() + remaining.len());
    let mut infos = Vec::with_capacity(accounts.len() + remaining.len() + 1);
    for meta in accounts {
        metas.push(AccountMeta {
            pubkey: meta.info.key(),
            is_signer: meta.signer,
            is_writable: meta.writable,
        });
        infos.push(meta.info.clone());
    }
    for info in remaining {
        metas.push(AccountMeta {
            pubkey: info.key(),
            is_signer: false,
            is_writable: info.is_writable,
        });
        infos.push(info.clone());
    }
    infos.push(program.clone());
    let instruction = Instruction {
        program_id: program.key(),
        accounts: metas,
        data,
    };
    invoke_signed(&instruction, &infos, &[signer_seeds]).map_err(Into::into)
}
