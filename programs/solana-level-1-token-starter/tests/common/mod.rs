use anchor_lang::{prelude::Pubkey, AccountDeserialize, InstructionData, ToAccountMetas};
use anchor_spl::{
    associated_token::get_associated_token_address_with_program_id,
    token_interface::{Mint, TokenAccount},
};
use litesvm::{types::TransactionResult, LiteSVM};
use solana_keypair::Keypair;
use solana_level_1_token_starter::{accounts, instruction, ID};
use solana_message::{Instruction, Message};
use solana_signer::Signer;
use solana_transaction::{InstructionError, Transaction, TransactionError};
use std::{fs, path::PathBuf, sync::OnceLock};

pub const DECIMALS: u8 = 6;
pub const INITIAL_SUPPLY: u64 = 10_000_000;

fn program_bytes() -> &'static [u8] {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES.get_or_init(|| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/deploy/solana_level_1_token_starter.so");
        fs::read(&path).unwrap_or_else(|error| {
            panic!(
                "Run `anchor build` before tests. Cannot read {}: {error}",
                path.display()
            )
        })
    })
}

pub fn instruction(accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
    Instruction {
        program_id: ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

pub struct TestContext {
    pub svm: LiteSVM,
    pub payer: Keypair,
    pub token_program: Pubkey,
}

impl TestContext {
    pub fn new(token_program: Pubkey) -> Self {
        let mut svm = LiteSVM::new();
        svm.add_program(ID, program_bytes())
            .expect("program must load");
        let payer = Keypair::new();
        svm.airdrop(&payer.pubkey(), 1_000_000_000)
            .expect("airdrop must succeed");
        Self {
            svm,
            payer,
            token_program,
        }
    }

    pub fn send(&mut self, ix: Instruction, signers: &[&Keypair]) -> TransactionResult {
        // Новый blockhash позволяет повторять одинаковые инструкции без AlreadyProcessed.
        self.svm.expire_blockhash();
        let mut all_signers = vec![&self.payer];
        all_signers.extend_from_slice(signers);
        let transaction = Transaction::new(
            &all_signers,
            Message::new(&[ix], Some(&self.payer.pubkey())),
            self.svm.latest_blockhash(),
        );
        self.svm.send_transaction(transaction)
    }

    pub fn create_mint(&mut self, authority: &Keypair, decimals: u8) -> Pubkey {
        let mint = Keypair::new();
        let ix = instruction(
            accounts::CreateToken {
                payer: self.payer.pubkey(),
                authority: authority.pubkey(),
                mint: mint.pubkey(),
                token_program: self.token_program,
                system_program: anchor_lang::system_program::ID,
            },
            instruction::CreateToken { decimals },
        );
        self.send(ix, &[authority, &mint])
            .expect("create_token must succeed");
        mint.pubkey()
    }

    pub fn create_account_instruction(
        &self,
        owner: Pubkey,
        mint: Pubkey,
        token_program: Pubkey,
    ) -> (Pubkey, Instruction) {
        let token_account =
            get_associated_token_address_with_program_id(&owner, &mint, &token_program);
        let ix = instruction(
            accounts::CreateTokenAccount {
                payer: self.payer.pubkey(),
                owner,
                mint,
                token_account,
                token_program,
                associated_token_program: anchor_spl::associated_token::ID,
                system_program: anchor_lang::system_program::ID,
            },
            instruction::CreateTokenAccount {},
        );
        (token_account, ix)
    }

    pub fn create_token_account(&mut self, owner: Pubkey, mint: Pubkey) -> Pubkey {
        let (address, ix) = self.create_account_instruction(owner, mint, self.token_program);
        self.send(ix, &[])
            .expect("create_token_account must succeed");
        address
    }

    pub fn mint_instruction(
        &self,
        authority: Pubkey,
        mint: Pubkey,
        destination: Pubkey,
        amount: u64,
    ) -> Instruction {
        instruction(
            accounts::MintTokens {
                authority,
                mint,
                destination,
                token_program: self.token_program,
            },
            instruction::MintTokens { amount },
        )
    }

    pub fn transfer_instruction(
        &self,
        authority: Pubkey,
        mint: Pubkey,
        source: Pubkey,
        destination: Pubkey,
        amount: u64,
    ) -> Instruction {
        instruction(
            accounts::TransferTokens {
                authority,
                mint,
                source,
                destination,
                token_program: self.token_program,
            },
            instruction::TransferTokens { amount },
        )
    }

    pub fn mint_tokens(
        &mut self,
        authority: &Keypair,
        mint: Pubkey,
        destination: Pubkey,
        amount: u64,
    ) {
        let ix = self.mint_instruction(authority.pubkey(), mint, destination, amount);
        self.send(ix, &[authority])
            .expect("mint_tokens must succeed");
    }

    pub fn burn_instruction(
        &self,
        authority: Pubkey,
        mint: Pubkey,
        source: Pubkey,
        amount: u64,
    ) -> Instruction {
        instruction(
            accounts::BurnTokens {
                authority,
                mint,
                source,
                token_program: self.token_program,
            },
            instruction::BurnTokens { amount },
        )
    }

    pub fn mint(&self, address: Pubkey) -> Mint {
        let account = self.svm.get_account(&address).expect("mint must exist");
        assert_eq!(account.owner, self.token_program);
        Mint::try_deserialize(&mut account.data.as_slice()).expect("mint must deserialize")
    }

    pub fn token_account(&self, address: Pubkey) -> TokenAccount {
        let account = self
            .svm
            .get_account(&address)
            .expect("token account must exist");
        assert_eq!(account.owner, self.token_program);
        TokenAccount::try_deserialize(&mut account.data.as_slice())
            .expect("token account must deserialize")
    }

    pub fn assert_rejected(
        &mut self,
        ix: Instruction,
        signers: &[&Keypair],
        error_code: u32,
        unchanged_accounts: &[Pubkey],
    ) {
        self.assert_rejected_with_error(
            ix,
            signers,
            InstructionError::Custom(error_code),
            unchanged_accounts,
        );
    }

    pub fn assert_rejected_with_error(
        &mut self,
        ix: Instruction,
        signers: &[&Keypair],
        expected_error: InstructionError,
        unchanged_accounts: &[Pubkey],
    ) {
        let before: Vec<_> = unchanged_accounts
            .iter()
            .map(|key| self.svm.get_account(key))
            .collect();
        let failure = self.send(ix, signers).expect_err("instruction must fail");
        assert_eq!(
            failure.err,
            TransactionError::InstructionError(0, expected_error),
            "unexpected failure; logs: {:#?}",
            failure.meta.logs,
        );
        // Сравниваем аккаунты целиком; payer исключён, поскольку комиссия списывается и при ошибке.
        for (address, account_before) in unchanged_accounts.iter().zip(before) {
            assert_eq!(
                self.svm.get_account(address),
                account_before,
                "account {address} changed on failure"
            );
        }
    }
}

pub struct FundedToken {
    pub context: TestContext,
    pub mint_authority: Keypair,
    pub owner: Keypair,
    pub mint: Pubkey,
    pub source: Pubkey,
    pub destination: Pubkey,
}

impl FundedToken {
    pub fn new(token_program: Pubkey) -> Self {
        Self::with_decimals(token_program, DECIMALS)
    }

    pub fn with_decimals(token_program: Pubkey, decimals: u8) -> Self {
        let mut context = TestContext::new(token_program);
        let mint_authority = Keypair::new();
        let owner = Keypair::new();
        let recipient = Keypair::new();
        let mint = context.create_mint(&mint_authority, decimals);
        let source = context.create_token_account(owner.pubkey(), mint);
        let destination = context.create_token_account(recipient.pubkey(), mint);
        context.mint_tokens(&mint_authority, mint, source, INITIAL_SUPPLY);
        Self {
            context,
            mint_authority,
            owner,
            mint,
            source,
            destination,
        }
    }

    pub fn addresses(&self) -> [Pubkey; 3] {
        [self.mint, self.source, self.destination]
    }
}
