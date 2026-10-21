use anchor_lang::prelude::Pubkey;
use liquidity_vault::state::{
    UserCard, VaultState, SHARE_MINT_SEED, USER_CARD_SEED, VAULT_SEED, VAULT_TOKENS_SEED,
};
use solana_signer::Signer;

use crate::rpc::{fail, vault_instruction, Client, Result};

/// Адреса vault для конкретного mint; все PDA вычисляются, а не хранятся.
pub struct Addresses {
    pub token_mint: Pubkey,
    pub token_program: Pubkey,
    pub vault: Pubkey,
    pub vault_token_account: Pubkey,
    pub share_mint: Pubkey,
}

pub fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &liquidity_vault::ID).0
}

pub fn user_card_address(vault: &Pubkey, user: &Pubkey) -> Pubkey {
    pda(&[USER_CARD_SEED, vault.as_ref(), user.as_ref()])
}

impl Addresses {
    pub fn load(client: &Client, token_mint: Pubkey) -> Result<Self> {
        let token_program = client.rpc.get_account(&token_mint)?.owner;
        let vault = pda(&[VAULT_SEED, token_mint.as_ref()]);
        Ok(Self {
            token_mint,
            token_program,
            vault,
            vault_token_account: pda(&[VAULT_TOKENS_SEED, vault.as_ref()]),
            share_mint: pda(&[SHARE_MINT_SEED, vault.as_ref()]),
        })
    }
}

pub fn init(client: &Client, addresses: &Addresses) -> Result<()> {
    if client.exists(&addresses.vault)? {
        println!("Vault already exists");
        return show(client, addresses);
    }
    let instruction = vault_instruction(
        liquidity_vault::accounts::InitializeVault {
            admin: client.payer.pubkey(),
            token_mint: addresses.token_mint,
            vault: addresses.vault,
            vault_token_account: addresses.vault_token_account,
            share_mint: addresses.share_mint,
            token_program: addresses.token_program,
            system_program: anchor_lang::system_program::ID,
        },
        liquidity_vault::instruction::InitializeVault {},
    );
    client.send("initialize_vault", &[instruction], &[])?;
    show(client, addresses)
}

pub fn create_card(client: &Client, addresses: &Addresses) -> Result<()> {
    let card = user_card_address(&addresses.vault, &client.payer.pubkey());
    if client.exists(&card)? {
        println!("User card already exists");
        return show(client, addresses);
    }
    let instruction = vault_instruction(
        liquidity_vault::accounts::CreateUserCard {
            user: client.payer.pubkey(),
            vault: addresses.vault,
            user_card: card,
            system_program: anchor_lang::system_program::ID,
        },
        liquidity_vault::instruction::CreateUserCard {},
    );
    client.send("create_user_card", &[instruction], &[])?;
    show(client, addresses)
}

/// Читает состояние vault и карточки текущего кошелька через RPC.
pub fn show(client: &Client, addresses: &Addresses) -> Result<()> {
    let Some(vault) = client.account::<VaultState>(&addresses.vault)? else {
        return fail("Vault does not exist; run init");
    };
    let owner = |address: &Pubkey| -> Result<Pubkey> { Ok(client.rpc.get_account(address)?.owner) };
    println!("\n=== VAULT ===");
    println!(
        "VaultState: {}",
        client.explorer("address", addresses.vault)
    );
    println!("  owner program: {}", owner(&addresses.vault)?);
    println!("  admin: {}", vault.admin);
    println!(
        "  token mint: {}",
        client.explorer("address", vault.token_mint)
    );
    println!(
        "Vault token account: {} (owner program {})",
        client.explorer("address", vault.vault_token_account),
        owner(&vault.vault_token_account)?
    );
    println!(
        "Share mint: {} (owner program {})",
        client.explorer("address", vault.share_mint),
        owner(&vault.share_mint)?
    );
    println!(
        "Vault balance: {} raw units; total deposited: {}",
        client.token_balance(&vault.vault_token_account)?,
        vault.total_deposited
    );
    let card_address = user_card_address(&addresses.vault, &client.payer.pubkey());
    match client.account::<UserCard>(&card_address)? {
        Some(card) => println!(
            "User card: {}\n  owner: {}; deposited: {}; created_at: {}",
            client.explorer("address", card_address),
            card.owner,
            card.deposited,
            card.created_at
        ),
        None => println!("User card: not created ({card_address})"),
    }
    Ok(())
}
