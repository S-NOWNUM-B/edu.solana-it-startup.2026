use anchor_lang::{prelude::Pubkey, AccountDeserialize, InstructionData, ToAccountMetas};
use anchor_spl::{
    token::spl_token,
    token_2022::spl_token_2022,
    token_interface::{Mint, TokenAccount},
};
use liquidity_vault::state::{
    UserCard, VaultState, SHARE_MINT_SEED, USER_CARD_SEED, VAULT_SEED, VAULT_TOKENS_SEED,
};
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::{Instruction, Message};
use solana_signer::Signer;
use solana_transaction::Transaction;
use std::{fs, path::PathBuf};

const DECIMALS: u8 = 6;

struct Fixture {
    svm: LiteSVM,
    admin: Keypair,
    mint: Pubkey,
    token_program: Pubkey,
    vault: Pubkey,
    vault_token_account: Pubkey,
    share_mint: Pubkey,
}

fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &liquidity_vault::ID).0
}

fn vault_ix(accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
    Instruction {
        program_id: liquidity_vault::ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

fn send(svm: &mut LiteSVM, payer: &Keypair, signers: &[&Keypair], ixs: &[Instruction]) -> bool {
    let mut all = vec![payer];
    all.extend_from_slice(signers);
    let message = Message::new(ixs, Some(&payer.pubkey()));
    let tx = Transaction::new(&all, message, svm.latest_blockhash());
    let result = svm.send_transaction(tx);
    if let Err(failed) = &result {
        // Логи отклонённой транзакции помогают понять причину отказа
        eprintln!("{:?}\n{}", failed.err, failed.meta.logs.join("\n"));
    }
    let ok = result.is_ok();
    svm.expire_blockhash();
    ok
}

fn read<T: AccountDeserialize>(svm: &LiteSVM, address: &Pubkey) -> T {
    let account = svm.get_account(address).expect("account must exist");
    T::try_deserialize(&mut account.data.as_slice()).unwrap()
}

impl Fixture {
    fn new(token_program: Pubkey) -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/deploy/liquidity_vault.so");
        let bytes = fs::read(path).expect("build the program with cargo build-sbf before tests");
        let mut svm = LiteSVM::new();
        svm.add_program(liquidity_vault::ID, &bytes).unwrap();
        let admin = Keypair::new();
        svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
        let mint = Keypair::new();
        // Базовый mint без расширений занимает 82 байта в обеих token programs
        let space = 82;
        let create = solana_system_interface_create(
            &admin.pubkey(),
            &mint.pubkey(),
            space,
            &token_program,
            &svm,
        );
        let init = if token_program == spl_token::ID {
            spl_token::instruction::initialize_mint2(
                &token_program,
                &mint.pubkey(),
                &admin.pubkey(),
                None,
                DECIMALS,
            )
            .unwrap()
        } else {
            spl_token_2022::instruction::initialize_mint2(
                &token_program,
                &mint.pubkey(),
                &admin.pubkey(),
                None,
                DECIMALS,
            )
            .unwrap()
        };
        assert!(send(&mut svm, &admin, &[&mint], &[create, init]));
        let vault = pda(&[VAULT_SEED, mint.pubkey().as_ref()]);
        let mut fixture = Self {
            svm,
            admin,
            mint: mint.pubkey(),
            token_program,
            vault,
            vault_token_account: pda(&[VAULT_TOKENS_SEED, vault.as_ref()]),
            share_mint: pda(&[SHARE_MINT_SEED, vault.as_ref()]),
        };
        assert!(fixture.initialize(), "initialize_vault must succeed");
        fixture
    }

    fn initialize(&mut self) -> bool {
        let ix = vault_ix(
            liquidity_vault::accounts::InitializeVault {
                admin: self.admin.pubkey(),
                token_mint: self.mint,
                vault: self.vault,
                vault_token_account: self.vault_token_account,
                share_mint: self.share_mint,
                token_program: self.token_program,
                system_program: anchor_lang::system_program::ID,
            },
            liquidity_vault::instruction::InitializeVault {},
        );
        let admin = self.admin.insecure_clone();
        send(&mut self.svm, &admin, &[], &[ix])
    }

    fn user(&mut self) -> Keypair {
        let user = Keypair::new();
        self.svm.airdrop(&user.pubkey(), 2_000_000_000).unwrap();
        user
    }

    fn card_address(&self, user: &Pubkey) -> Pubkey {
        pda(&[USER_CARD_SEED, self.vault.as_ref(), user.as_ref()])
    }

    fn create_card(&mut self, user: &Keypair) -> bool {
        let ix = vault_ix(
            liquidity_vault::accounts::CreateUserCard {
                user: user.pubkey(),
                vault: self.vault,
                user_card: self.card_address(&user.pubkey()),
                system_program: anchor_lang::system_program::ID,
            },
            liquidity_vault::instruction::CreateUserCard {},
        );
        send(&mut self.svm, user, &[], &[ix])
    }
}

/// Создание аккаунта mint через System Program с рентой из LiteSVM.
fn solana_system_interface_create(
    payer: &Pubkey,
    account: &Pubkey,
    space: usize,
    owner: &Pubkey,
    svm: &LiteSVM,
) -> Instruction {
    let lamports = svm.minimum_balance_for_rent_exemption(space);
    anchor_lang::solana_program::system_instruction::create_account(
        payer,
        account,
        lamports,
        space as u64,
        owner,
    )
}

#[test]
fn initialize_creates_program_owned_accounts_for_both_token_programs() {
    for token_program in [spl_token::ID, spl_token_2022::ID] {
        let fixture = Fixture::new(token_program);
        let vault: VaultState = read(&fixture.svm, &fixture.vault);
        assert_eq!(vault.admin, fixture.admin.pubkey());
        assert_eq!(vault.token_mint, fixture.mint);
        assert_eq!(vault.token_program, token_program);
        assert_eq!(vault.vault_token_account, fixture.vault_token_account);
        assert_eq!(vault.share_mint, fixture.share_mint);
        assert_eq!(vault.total_deposited, 0);
        assert_eq!(
            fixture.svm.get_account(&fixture.vault).unwrap().owner,
            liquidity_vault::ID
        );

        let storage: TokenAccount = read(&fixture.svm, &fixture.vault_token_account);
        assert_eq!(
            storage.owner, fixture.vault,
            "storage authority is the vault PDA"
        );
        assert_eq!(storage.mint, fixture.mint);
        assert_eq!(
            fixture
                .svm
                .get_account(&fixture.vault_token_account)
                .unwrap()
                .owner,
            token_program
        );

        let share: Mint = read(&fixture.svm, &fixture.share_mint);
        assert_eq!(share.mint_authority, Some(fixture.vault).into());
        assert_eq!(share.decimals, DECIMALS);
        assert_eq!(share.supply, 0);
    }
}

#[test]
fn vault_is_created_only_once_per_mint() {
    let mut fixture = Fixture::new(spl_token_2022::ID);
    assert!(!fixture.initialize());
}

#[test]
fn every_user_gets_one_card_derived_from_the_wallet() {
    let mut fixture = Fixture::new(spl_token::ID);
    let alice = fixture.user();
    let bob = fixture.user();
    assert!(fixture.create_card(&alice));
    assert!(
        !fixture.create_card(&alice),
        "second card for the same user"
    );
    assert!(fixture.create_card(&bob));

    let card: UserCard = read(&fixture.svm, &fixture.card_address(&alice.pubkey()));
    assert_eq!(card.owner, alice.pubkey());
    assert_eq!(card.vault, fixture.vault);
    assert_eq!(card.deposited, 0);
    assert_ne!(
        fixture.card_address(&alice.pubkey()),
        fixture.card_address(&bob.pubkey())
    );
}

#[test]
fn card_requires_existing_vault() {
    let mut fixture = Fixture::new(spl_token::ID);
    let user = fixture.user();
    let fake_vault = Keypair::new().pubkey();
    let ix = vault_ix(
        liquidity_vault::accounts::CreateUserCard {
            user: user.pubkey(),
            vault: fake_vault,
            user_card: pda(&[USER_CARD_SEED, fake_vault.as_ref(), user.pubkey().as_ref()]),
            system_program: anchor_lang::system_program::ID,
        },
        liquidity_vault::instruction::CreateUserCard {},
    );
    assert!(!send(&mut fixture.svm, &user, &[], &[ix]));
}
