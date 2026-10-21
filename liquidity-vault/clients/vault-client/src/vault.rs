use anchor_lang::prelude::Pubkey;
use anchor_spl::associated_token::{self, get_associated_token_address_with_program_id};
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
    pub decimals: u8,
}

pub fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &liquidity_vault::ID).0
}

pub fn user_card_address(vault: &Pubkey, user: &Pubkey) -> Pubkey {
    pda(&[USER_CARD_SEED, vault.as_ref(), user.as_ref()])
}

impl Addresses {
    pub fn load(client: &Client, token_mint: Pubkey) -> Result<Self> {
        let mint = client.rpc.get_account(&token_mint)?;
        // decimals лежат по смещению 44 в mint обеих token programs
        let decimals = *mint.data.get(44).ok_or("Invalid mint account")?;
        let token_program = mint.owner;
        let vault = pda(&[VAULT_SEED, token_mint.as_ref()]);
        Ok(Self {
            token_mint,
            token_program,
            vault,
            vault_token_account: pda(&[VAULT_TOKENS_SEED, vault.as_ref()]),
            share_mint: pda(&[SHARE_MINT_SEED, vault.as_ref()]),
            decimals,
        })
    }

    pub fn ata(&self, owner: &Pubkey, mint: &Pubkey) -> Pubkey {
        get_associated_token_address_with_program_id(owner, mint, &self.token_program)
    }

    pub fn format(&self, raw: u64) -> String {
        format_amount(raw, self.decimals)
    }
}

/// Переводит десятичную запись в минимальные единицы без float.
pub fn parse_amount(text: &str, decimals: u8) -> Result<u64> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty() && fraction.is_empty()
        || fraction.len() > decimals as usize
        || !whole
            .chars()
            .chain(fraction.chars())
            .all(|c| c.is_ascii_digit())
    {
        return fail(format!("Invalid amount: {text}"));
    }
    let scale = 10_u64.pow(decimals as u32);
    let whole: u64 = if whole.is_empty() { 0 } else { whole.parse()? };
    let fraction: u64 = format!("{fraction:0<width$}", width = decimals as usize)
        .parse()
        .unwrap_or(0);
    whole
        .checked_mul(scale)
        .and_then(|value| value.checked_add(fraction))
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("Amount must be positive and fit u64: {text}").into())
}

pub fn format_amount(raw: u64, decimals: u8) -> String {
    let scale = 10_u64.pow(decimals as u32);
    let fraction = format!("{:0width$}", raw % scale, width = decimals as usize);
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        (raw / scale).to_string()
    } else {
        format!("{}.{fraction}", raw / scale)
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

pub fn deposit(client: &Client, addresses: &Addresses, amount: &str) -> Result<()> {
    let amount = parse_amount(amount, addresses.decimals)?;
    let user = client.payer.pubkey();
    let user_tokens = addresses.ata(&user, &addresses.token_mint);
    let user_shares = addresses.ata(&user, &addresses.share_mint);
    let supply: u64 = client
        .rpc
        .get_token_supply(&addresses.share_mint)?
        .amount
        .parse()?;
    let vault_before = client.token_balance(&addresses.vault_token_account)?;
    let shares_before = client.token_balance(&user_shares)?;
    let expected = liquidity_vault::math::shares_for_deposit(amount, supply, vault_before)?;
    println!(
        "Deposit {} tokens: vault holds {}, share supply {}, expected shares {}",
        addresses.format(amount),
        addresses.format(vault_before),
        addresses.format(supply),
        addresses.format(expected)
    );
    let instruction = vault_instruction(
        liquidity_vault::accounts::Deposit {
            user,
            vault: addresses.vault,
            user_card: user_card_address(&addresses.vault, &user),
            token_mint: addresses.token_mint,
            vault_token_account: addresses.vault_token_account,
            share_mint: addresses.share_mint,
            user_token_account: user_tokens,
            user_share_account: user_shares,
            token_program: addresses.token_program,
            associated_token_program: associated_token::ID,
            system_program: anchor_lang::system_program::ID,
        },
        liquidity_vault::instruction::Deposit { amount },
    );
    client.send("deposit", &[instruction], &[])?;
    let received = client.token_balance(&user_shares)? - shares_before;
    let vault_after = client.token_balance(&addresses.vault_token_account)?;
    if received != expected || vault_after != vault_before + amount {
        return fail("RPC readback does not match the deposit");
    }
    println!(
        "PASS RPC readback: +{} shares, vault holds {}",
        addresses.format(received),
        addresses.format(vault_after)
    );
    show(client, addresses)
}

pub fn withdraw(client: &Client, addresses: &Addresses, shares: &str) -> Result<()> {
    let shares = parse_amount(shares, addresses.decimals)?;
    let user = client.payer.pubkey();
    let user_tokens = addresses.ata(&user, &addresses.token_mint);
    let user_shares = addresses.ata(&user, &addresses.share_mint);
    let supply: u64 = client
        .rpc
        .get_token_supply(&addresses.share_mint)?
        .amount
        .parse()?;
    let vault_before = client.token_balance(&addresses.vault_token_account)?;
    let tokens_before = client.token_balance(&user_tokens)?;
    let expected = liquidity_vault::math::tokens_for_shares(shares, supply, vault_before)?;
    println!(
        "Withdraw {} shares: vault holds {}, share supply {}, expected tokens {}",
        addresses.format(shares),
        addresses.format(vault_before),
        addresses.format(supply),
        addresses.format(expected)
    );
    let instruction = vault_instruction(
        liquidity_vault::accounts::Withdraw {
            user,
            vault: addresses.vault,
            user_card: user_card_address(&addresses.vault, &user),
            token_mint: addresses.token_mint,
            vault_token_account: addresses.vault_token_account,
            share_mint: addresses.share_mint,
            user_token_account: user_tokens,
            user_share_account: user_shares,
            token_program: addresses.token_program,
        },
        liquidity_vault::instruction::Withdraw { shares },
    );
    client.send("withdraw", &[instruction], &[])?;
    let received = client.token_balance(&user_tokens)? - tokens_before;
    let vault_after = client.token_balance(&addresses.vault_token_account)?;
    if received != expected || vault_after != vault_before - expected {
        return fail("RPC readback does not match the withdrawal");
    }
    println!(
        "PASS RPC readback: burned {} shares, +{} tokens, vault holds {}",
        addresses.format(shares),
        addresses.format(received),
        addresses.format(vault_after)
    );
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
    let supply: u64 = client
        .rpc
        .get_token_supply(&vault.share_mint)?
        .amount
        .parse()?;
    println!(
        "Vault balance: {}; share supply: {}; total deposited: {}",
        addresses.format(client.token_balance(&vault.vault_token_account)?),
        addresses.format(supply),
        addresses.format(vault.total_deposited)
    );
    let user = client.payer.pubkey();
    println!(
        "Wallet: {} tokens, {} shares",
        addresses.format(client.token_balance(&addresses.ata(&user, &addresses.token_mint))?),
        addresses.format(client.token_balance(&addresses.ata(&user, &addresses.share_mint))?)
    );
    let card_address = user_card_address(&addresses.vault, &client.payer.pubkey());
    match client.account::<UserCard>(&card_address)? {
        Some(card) => println!(
            "User card: {}\n  owner: {}; deposited: {}; created_at: {}",
            client.explorer("address", card_address),
            card.owner,
            addresses.format(card.deposited),
            card.created_at
        ),
        None => println!("User card: not created ({card_address})"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{format_amount, parse_amount};

    #[test]
    fn parses_and_formats_amounts_without_float() {
        assert_eq!(parse_amount("1000", 6).unwrap(), 1_000_000_000);
        assert_eq!(parse_amount("0.5", 6).unwrap(), 500_000);
        assert_eq!(parse_amount(".000001", 6).unwrap(), 1);
        assert!(parse_amount("0", 6).is_err());
        assert!(parse_amount("1.0000001", 6).is_err());
        assert!(parse_amount("-1", 6).is_err());
        assert!(parse_amount("1e3", 6).is_err());
        assert_eq!(format_amount(1_500_000, 6), "1.5");
        assert_eq!(format_amount(2_000_000, 6), "2");
    }
}
