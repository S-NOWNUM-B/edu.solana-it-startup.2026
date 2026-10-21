use anchor_lang::{prelude::Pubkey, AccountDeserialize, InstructionData, ToAccountMetas};
use anchor_spl::{
    associated_token::{
        get_associated_token_address_with_program_id, spl_associated_token_account,
    },
    token::spl_token,
    token_2022::spl_token_2022,
    token_interface::{Mint, TokenAccount},
};
use liquidity_vault::{
    raydium::{CLMM_PROGRAM_ID, MEMO_PROGRAM_ID},
    state::{
        Strategy, UserCard, VaultState, SECOND_TOKENS_SEED, SHARE_MINT_SEED, STRATEGY_SEED,
        USER_CARD_SEED, VAULT_SEED, VAULT_TOKENS_SEED,
    },
};
use litesvm::LiteSVM;
use solana_account::Account;
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

    fn withdraw_to(&mut self, user: &Keypair, destination: Pubkey, shares: u64) -> bool {
        let ix = vault_ix(
            liquidity_vault::accounts::Withdraw {
                user: user.pubkey(),
                vault: self.vault,
                user_card: self.card_address(&user.pubkey()),
                token_mint: self.mint,
                vault_token_account: self.vault_token_account,
                share_mint: self.share_mint,
                user_token_account: destination,
                user_share_account: self.ata(&user.pubkey(), &self.share_mint),
                token_program: self.token_program,
            },
            liquidity_vault::instruction::Withdraw { shares },
        );
        send(&mut self.svm, user, &[], &[ix])
    }

    fn withdraw(&mut self, user: &Keypair, shares: u64) -> bool {
        let destination = self.ata(&user.pubkey(), &self.mint);
        self.withdraw_to(user, destination, shares)
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

#[test]
fn withdraw_burns_shares_and_returns_tokens_signed_by_vault() {
    for token_program in [spl_token::ID, spl_token_2022::ID] {
        let mut fixture = Fixture::new(token_program);
        let user = fixture.depositor(1_000);
        assert!(fixture.deposit(&user, 1_000));
        assert!(fixture.withdraw(&user, 400));
        assert_eq!(fixture.shares(&user.pubkey()), 600);
        assert_eq!(
            fixture.balance(&fixture.ata(&user.pubkey(), &fixture.mint)),
            400
        );
        assert_eq!(fixture.balance(&fixture.vault_token_account), 600);
        let share: Mint = read(&fixture.svm, &fixture.share_mint);
        assert_eq!(share.supply, 600);
        let card: UserCard = read(&fixture.svm, &fixture.card_address(&user.pubkey()));
        let vault: VaultState = read(&fixture.svm, &fixture.vault);
        assert_eq!(card.deposited, 600);
        assert_eq!(vault.total_deposited, 600);
    }
}

#[test]
fn withdraw_shares_vault_income_proportionally() {
    let mut fixture = Fixture::new(spl_token_2022::ID);
    let early = fixture.depositor(1_000);
    let late = fixture.depositor(110);
    assert!(fixture.deposit(&early, 1_000));
    fixture.earn(100);
    assert!(fixture.deposit(&late, 110));
    assert!(fixture.withdraw(&late, 100));
    assert_eq!(
        fixture.balance(&fixture.ata(&late.pubkey(), &fixture.mint)),
        110
    );
    assert!(fixture.withdraw(&early, 1_000));
    assert_eq!(
        fixture.balance(&fixture.ata(&early.pubkey(), &fixture.mint)),
        1_100
    );
    assert_eq!(fixture.balance(&fixture.vault_token_account), 0);
}

#[test]
fn withdraw_rejects_zero_excess_and_foreign_destination() {
    let mut fixture = Fixture::new(spl_token::ID);
    let user = fixture.depositor(1_000);
    let other = fixture.depositor(1_000);
    assert!(fixture.deposit(&user, 500));
    assert!(fixture.deposit(&other, 500));
    assert!(!fixture.withdraw(&user, 0), "zero shares");
    assert!(!fixture.withdraw(&user, 501), "more than own shares");
    let other_tokens = fixture.ata(&other.pubkey(), &fixture.mint);
    assert!(
        !fixture.withdraw_to(&user, other_tokens, 100),
        "foreign destination"
    );
    let admin = fixture.admin.insecure_clone();
    assert!(
        !fixture.withdraw(&admin, 1),
        "admin has no card and no shares"
    );
    assert_eq!(fixture.balance(&fixture.vault_token_account), 1_000);
}

const POOL_DISCRIMINATOR: [u8; 8] = [247, 237, 227, 245, 215, 195, 222, 70];

/// Стратегия поверх vault: пул Raydium подменён аккаунтом с тем же layout.
struct StrategyFixture {
    base: Fixture,
    second_mint: Pubkey,
    pool: Pubkey,
    strategy: Pubkey,
    second_tokens: Pubkey,
}

impl StrategyFixture {
    fn new() -> Self {
        let mut base = Fixture::new(spl_token_2022::ID);
        let second_mint = Keypair::new();
        let create = solana_system_interface_create(
            &base.admin.pubkey(),
            &second_mint.pubkey(),
            82,
            &spl_token::ID,
            &base.svm,
        );
        let init = spl_token::instruction::initialize_mint2(
            &spl_token::ID,
            &second_mint.pubkey(),
            &base.admin.pubkey(),
            None,
            DECIMALS,
        )
        .unwrap();
        let admin = base.admin.insecure_clone();
        assert!(send(
            &mut base.svm,
            &admin,
            &[&second_mint],
            &[create, init]
        ));
        let pool = fake_pool(
            &mut base.svm,
            CLMM_PROGRAM_ID,
            base.mint,
            second_mint.pubkey(),
        );
        let vault = base.vault;
        Self {
            base,
            second_mint: second_mint.pubkey(),
            pool,
            strategy: pda(&[STRATEGY_SEED, vault.as_ref()]),
            second_tokens: pda(&[SECOND_TOKENS_SEED, vault.as_ref()]),
        }
    }

    fn init(&mut self, signer: &Keypair, pool: Pubkey) -> bool {
        let ix = vault_ix(
            liquidity_vault::accounts::InitStrategy {
                admin: signer.pubkey(),
                vault: self.base.vault,
                strategy: self.strategy,
                pool,
                second_mint: self.second_mint,
                second_token_account: self.second_tokens,
                second_token_program: spl_token::ID,
                system_program: anchor_lang::system_program::ID,
            },
            liquidity_vault::instruction::InitStrategy {},
        );
        send(&mut self.base.svm, signer, &[], &[ix])
    }

    /// Swap отклоняется проверками vault ещё до CPI в Raydium.
    fn swap(&mut self, signer: &Keypair, pool: Pubkey) -> bool {
        let ix = vault_ix(
            liquidity_vault::accounts::Swap {
                admin: signer.pubkey(),
                vault: self.base.vault,
                strategy: self.strategy,
                clmm_program: CLMM_PROGRAM_ID,
                amm_config: Keypair::new().pubkey(),
                pool,
                vault_token_account: self.base.vault_token_account,
                second_token_account: self.second_tokens,
                input_pool_vault: Keypair::new().pubkey(),
                output_pool_vault: Keypair::new().pubkey(),
                observation: Keypair::new().pubkey(),
                token_program: spl_token::ID,
                token_program_2022: spl_token_2022::ID,
                memo_program: MEMO_PROGRAM_ID,
                input_mint: self.base.mint,
                output_mint: self.second_mint,
            },
            liquidity_vault::instruction::Swap {
                amount_in: 1,
                minimum_amount_out: 0,
                sell_vault_token: true,
            },
        );
        send(&mut self.base.svm, signer, &[], &[ix])
    }
}

fn fake_pool(svm: &mut LiteSVM, owner: Pubkey, mint_a: Pubkey, mint_b: Pubkey) -> Pubkey {
    let (mint_0, mint_1) = if mint_a.to_bytes() < mint_b.to_bytes() {
        (mint_a, mint_b)
    } else {
        (mint_b, mint_a)
    };
    let mut data = vec![0; 1544];
    data[..8].copy_from_slice(&POOL_DISCRIMINATOR);
    data[73..105].copy_from_slice(mint_0.as_ref());
    data[105..137].copy_from_slice(mint_1.as_ref());
    let address = Keypair::new().pubkey();
    svm.set_account(
        address,
        Account {
            lamports: svm.minimum_balance_for_rent_exemption(data.len()),
            data,
            owner,
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
    address
}

#[test]
fn strategy_binds_vault_to_one_raydium_pool_with_both_tokens() {
    let mut fixture = StrategyFixture::new();
    let stranger = fixture.base.user();
    assert!(
        !fixture.init(&stranger.insecure_clone(), fixture.pool),
        "only admin"
    );

    let admin = fixture.base.admin.insecure_clone();
    let fake_owner = fake_pool(
        &mut fixture.base.svm,
        Keypair::new().pubkey(),
        fixture.base.mint,
        fixture.second_mint,
    );
    assert!(
        !fixture.init(&admin, fake_owner),
        "pool must be owned by Raydium"
    );
    let other_mint = Keypair::new().pubkey();
    let wrong_pair = fake_pool(
        &mut fixture.base.svm,
        CLMM_PROGRAM_ID,
        fixture.base.mint,
        other_mint,
    );
    assert!(
        !fixture.init(&admin, wrong_pair),
        "pool must contain both tokens"
    );

    assert!(fixture.init(&admin, fixture.pool));
    let strategy: Strategy = read(&fixture.base.svm, &fixture.strategy);
    assert_eq!(strategy.pool, fixture.pool);
    assert_eq!(strategy.second_token_account, fixture.second_tokens);
    assert_eq!(strategy.position_nft_mint, Pubkey::default());
    let storage: TokenAccount = read(&fixture.base.svm, &fixture.second_tokens);
    assert_eq!(
        storage.owner, fixture.base.vault,
        "second storage authority is the vault PDA"
    );
    assert!(
        !fixture.init(&admin, fixture.pool),
        "strategy is created once"
    );
}

#[test]
fn swap_is_allowed_only_for_admin_through_strategy_pool() {
    let mut fixture = StrategyFixture::new();
    let admin = fixture.base.admin.insecure_clone();
    assert!(fixture.init(&admin, fixture.pool));
    let stranger = fixture.base.user();
    assert!(!fixture.swap(&stranger, fixture.pool), "not admin");
    let other_pool = fake_pool(
        &mut fixture.base.svm,
        CLMM_PROGRAM_ID,
        fixture.base.mint,
        fixture.second_mint,
    );
    assert!(!fixture.swap(&admin, other_pool), "pool outside strategy");
}

impl StrategyFixture {
    /// Инструкция позиции с поддельными аккаунтами Raydium: отказ должен случиться до CPI.
    fn position_ix(
        &self,
        admin: Pubkey,
        nft_mint: Pubkey,
        token_account_0: Pubkey,
        data: impl InstructionData,
    ) -> Instruction {
        let mut ix = vault_ix(
            liquidity_vault::accounts::VaultPosition {
                admin,
                vault: self.base.vault,
                strategy: self.strategy,
                clmm_program: CLMM_PROGRAM_ID,
                pool: self.pool,
                position_nft_mint: nft_mint,
                position_nft_account: Keypair::new().pubkey(),
                personal_position: Keypair::new().pubkey(),
                protocol_position: Keypair::new().pubkey(),
                tick_array_lower: Keypair::new().pubkey(),
                tick_array_upper: Keypair::new().pubkey(),
                token_account_0,
                token_account_1: self.second_tokens,
                token_vault_0: Keypair::new().pubkey(),
                token_vault_1: Keypair::new().pubkey(),
                vault_0_mint: self.base.mint,
                vault_1_mint: self.second_mint,
                rent: anchor_lang::prelude::rent::ID,
                system_program: anchor_lang::system_program::ID,
                token_program: spl_token::ID,
                token_program_2022: spl_token_2022::ID,
                associated_token_program: anchor_spl::associated_token::ID,
                memo_program: MEMO_PROGRAM_ID,
            },
            data,
        );
        for meta in &mut ix.accounts {
            if meta.pubkey == nft_mint {
                meta.is_signer = true;
            }
        }
        ix
    }

    fn open_data() -> liquidity_vault::instruction::OpenPosition {
        liquidity_vault::instruction::OpenPosition {
            tick_lower: -60,
            tick_upper: 60,
            tick_array_lower_start: -3_600,
            tick_array_upper_start: 0,
            amount_0_max: 1,
            amount_1_max: 1,
            base_0: true,
        }
    }
}

#[test]
fn position_actions_are_admin_only_and_keep_tokens_in_vault_storages() {
    let mut fixture = StrategyFixture::new();
    let admin = fixture.base.admin.insecure_clone();
    assert!(fixture.init(&admin, fixture.pool));
    let storage = fixture.base.vault_token_account;
    let nft = Keypair::new();

    let stranger = fixture.base.user();
    let ix = fixture.position_ix(
        stranger.pubkey(),
        nft.pubkey(),
        storage,
        StrategyFixture::open_data(),
    );
    assert!(
        !send(&mut fixture.base.svm, &stranger, &[&nft], &[ix]),
        "not admin"
    );

    let own_tokens = fixture.base.fund(&admin.pubkey(), 10);
    let ix = fixture.position_ix(
        admin.pubkey(),
        nft.pubkey(),
        own_tokens,
        StrategyFixture::open_data(),
    );
    assert!(
        !send(&mut fixture.base.svm, &admin, &[&nft], &[ix]),
        "admin wallet is not a vault storage"
    );

    let ix = fixture.position_ix(
        admin.pubkey(),
        nft.pubkey(),
        storage,
        liquidity_vault::instruction::CollectFees {},
    );
    assert!(
        !send(&mut fixture.base.svm, &admin, &[&nft], &[ix]),
        "no open position"
    );
    let ix = fixture.position_ix(
        admin.pubkey(),
        nft.pubkey(),
        storage,
        liquidity_vault::instruction::ClosePosition {
            amount_0_min: 0,
            amount_1_min: 0,
        },
    );
    assert!(
        !send(&mut fixture.base.svm, &admin, &[&nft], &[ix]),
        "no open position"
    );
    let strategy: Strategy = read(&fixture.base.svm, &fixture.strategy);
    assert_eq!(strategy.position_nft_mint, Pubkey::default());
}
