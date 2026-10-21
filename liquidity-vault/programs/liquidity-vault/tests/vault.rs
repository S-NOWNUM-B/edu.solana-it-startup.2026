use anchor_lang::{prelude::Pubkey, AccountDeserialize, InstructionData, ToAccountMetas};
use anchor_spl::{
    associated_token::{
        get_associated_token_address_with_program_id, spl_associated_token_account,
    },
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

    fn ata(&self, owner: &Pubkey, mint: &Pubkey) -> Pubkey {
        get_associated_token_address_with_program_id(owner, mint, &self.token_program)
    }

    /// Создаёт ATA владельца и начисляет ему токены vault от имени mint authority.
    fn fund(&mut self, owner: &Pubkey, amount: u64) -> Pubkey {
        let account = self.ata(owner, &self.mint);
        let create =
            spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                &self.admin.pubkey(),
                owner,
                &self.mint,
                &self.token_program,
            );
        let mint_to = spl_token_2022::instruction::mint_to(
            &self.token_program,
            &self.mint,
            &account,
            &self.admin.pubkey(),
            &[],
            amount,
        )
        .unwrap();
        let admin = self.admin.insecure_clone();
        assert!(send(&mut self.svm, &admin, &[], &[create, mint_to]));
        account
    }

    fn balance(&self, account: &Pubkey) -> u64 {
        match self.svm.get_account(account) {
            Some(_) => read::<TokenAccount>(&self.svm, account).amount,
            None => 0,
        }
    }

    fn shares(&self, owner: &Pubkey) -> u64 {
        self.balance(&self.ata(owner, &self.share_mint))
    }

    fn deposit_from(&mut self, user: &Keypair, source: Pubkey, amount: u64) -> bool {
        let ix = vault_ix(
            liquidity_vault::accounts::Deposit {
                user: user.pubkey(),
                vault: self.vault,
                user_card: self.card_address(&user.pubkey()),
                token_mint: self.mint,
                vault_token_account: self.vault_token_account,
                share_mint: self.share_mint,
                user_token_account: source,
                user_share_account: self.ata(&user.pubkey(), &self.share_mint),
                token_program: self.token_program,
                associated_token_program: anchor_spl::associated_token::ID,
                system_program: anchor_lang::system_program::ID,
            },
            liquidity_vault::instruction::Deposit { amount },
        );
        send(&mut self.svm, user, &[], &[ix])
    }

    fn deposit(&mut self, user: &Keypair, amount: u64) -> bool {
        let source = self.ata(&user.pubkey(), &self.mint);
        self.deposit_from(user, source, amount)
    }

    /// Доход vault: токены поступают в хранилище без выпуска share-токенов.
    fn earn(&mut self, amount: u64) {
        let mint_to = spl_token_2022::instruction::mint_to(
            &self.token_program,
            &self.mint,
            &self.vault_token_account,
            &self.admin.pubkey(),
            &[],
            amount,
        )
        .unwrap();
        let admin = self.admin.insecure_clone();
        assert!(send(&mut self.svm, &admin, &[], &[mint_to]));
    }

    /// Пользователь с карточкой и токенами на кошельке.
    fn depositor(&mut self, tokens: u64) -> Keypair {
        let user = self.user();
        assert!(self.create_card(&user));
        self.fund(&user.pubkey(), tokens);
        user
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

#[test]
fn first_deposit_moves_tokens_and_mints_shares_one_to_one() {
    for token_program in [spl_token::ID, spl_token_2022::ID] {
        let mut fixture = Fixture::new(token_program);
        let user = fixture.depositor(5_000);
        assert!(fixture.deposit(&user, 1_000));
        assert_eq!(
            fixture.balance(&fixture.ata(&user.pubkey(), &fixture.mint)),
            4_000
        );
        assert_eq!(fixture.balance(&fixture.vault_token_account), 1_000);
        assert_eq!(fixture.shares(&user.pubkey()), 1_000);
        let vault: VaultState = read(&fixture.svm, &fixture.vault);
        let card: UserCard = read(&fixture.svm, &fixture.card_address(&user.pubkey()));
        assert_eq!(vault.total_deposited, 1_000);
        assert_eq!(card.deposited, 1_000);
    }
}

#[test]
fn late_depositor_gets_fewer_shares_after_vault_income() {
    let mut fixture = Fixture::new(spl_token_2022::ID);
    let early = fixture.depositor(1_000);
    let late = fixture.depositor(110);
    assert!(fixture.deposit(&early, 1_000));
    fixture.earn(100);
    assert!(fixture.deposit(&late, 110));
    assert_eq!(fixture.shares(&late.pubkey()), 100);
    assert_eq!(fixture.balance(&fixture.vault_token_account), 1_210);
    let share: Mint = read(&fixture.svm, &fixture.share_mint);
    assert_eq!(share.supply, 1_100);
}

#[test]
fn deposit_rejects_zero_dust_foreign_accounts_and_missing_card() {
    let mut fixture = Fixture::new(spl_token::ID);
    let user = fixture.depositor(1_000);
    assert!(!fixture.deposit(&user, 0), "zero amount");

    assert!(fixture.deposit(&user, 10));
    fixture.earn(1_000);
    assert!(!fixture.deposit(&user, 1), "dust gives zero shares");

    let victim = fixture.depositor(500);
    let victim_tokens = fixture.ata(&victim.pubkey(), &fixture.mint);
    assert!(
        !fixture.deposit_from(&user, victim_tokens, 100),
        "foreign source"
    );

    let no_card = fixture.user();
    fixture.fund(&no_card.pubkey(), 100);
    assert!(!fixture.deposit(&no_card, 100), "card is required");
    assert_eq!(fixture.balance(&victim_tokens), 500);
}
