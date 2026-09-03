use anchor_lang::{
    prelude::Pubkey, solana_program::program_pack::Pack, AccountDeserialize, InstructionData,
    ToAccountMetas,
};
use anchor_spl::{
    associated_token::{
        get_associated_token_address_with_program_id, spl_associated_token_account,
    },
    token_2022::{
        self,
        spl_token_2022::{
            self,
            state::{Account as TokenAccount, Mint},
        },
    },
};
use escrow::{
    accounts, instruction,
    state::{DealReceipt, EscrowState},
    ID,
};
use litesvm::{
    types::{FailedTransactionMetadata, TransactionMetadata},
    LiteSVM,
};
use solana_keypair::Keypair;
use solana_message::{Instruction, Message};
use solana_signer::Signer;
use solana_transaction::{InstructionError, Transaction, TransactionError};
use std::{fs, path::PathBuf, sync::OnceLock};

pub const AMOUNT: u64 = 1_234_567;
pub const SUPPLY: u64 = 10_000_000;
pub const DEAL_ID: u64 = 42;
pub const DECIMALS: u8 = 6;

pub fn instruction(accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
    Instruction {
        program_id: ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

#[derive(Clone, Copy)]
pub struct Deal {
    pub id: u64,
    pub state: Pubkey,
    pub vault: Pubkey,
    pub receipt: Pubkey,
    pub bump: u8,
}

impl Deal {
    pub fn new(sender: Pubkey, id: u64) -> Self {
        let (state, bump) =
            Pubkey::find_program_address(&[b"escrow", sender.as_ref(), &id.to_le_bytes()], &ID);
        let (vault, _) = Pubkey::find_program_address(&[b"vault", state.as_ref()], &ID);
        let (receipt, _) =
            Pubkey::find_program_address(&[b"used", sender.as_ref(), &id.to_le_bytes()], &ID);
        Self {
            id,
            state,
            vault,
            receipt,
            bump,
        }
    }
}

pub struct TestContext {
    pub svm: LiteSVM,
    pub payer: Keypair,
    pub tracked: Vec<Pubkey>,
}

impl TestContext {
    pub fn new() -> Self {
        static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
        let bytes = BYTES.get_or_init(|| {
            let path =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/escrow.so");
            fs::read(path).expect("Run anchor build before cargo test")
        });
        let mut svm = LiteSVM::new();
        svm.add_program(ID, bytes).unwrap();
        let payer = Keypair::new();
        svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();
        Self {
            svm,
            payer,
            tracked: vec![],
        }
    }

    pub fn wallet(&mut self) -> Keypair {
        let wallet = Keypair::new();
        self.svm.airdrop(&wallet.pubkey(), 1_000_000_000).unwrap();
        self.tracked.push(wallet.pubkey());
        wallet
    }

    pub fn send(
        &mut self,
        instructions: &[Instruction],
        signers: &[&Keypair],
    ) -> Result<TransactionMetadata, Box<FailedTransactionMetadata>> {
        self.svm.expire_blockhash();
        let mut all_signers = vec![&self.payer];
        all_signers.extend_from_slice(signers);
        let tx = Transaction::new(
            &all_signers,
            Message::new(instructions, Some(&self.payer.pubkey())),
            self.svm.latest_blockhash(),
        );
        self.svm.send_transaction(tx).map_err(Box::new)
    }

    pub fn create_mint(
        &mut self,
        authority: &Keypair,
        program: Pubkey,
        freeze: Option<Pubkey>,
    ) -> Pubkey {
        let mint = Keypair::new();
        let instructions = [
            solana_system_interface::instruction::create_account(
                &self.payer.pubkey(),
                &mint.pubkey(),
                self.svm.minimum_balance_for_rent_exemption(Mint::LEN),
                Mint::LEN as u64,
                &program,
            ),
            spl_token_2022::instruction::initialize_mint2(
                &program,
                &mint.pubkey(),
                &authority.pubkey(),
                freeze.as_ref(),
                DECIMALS,
            )
            .unwrap(),
        ];
        self.send(&instructions, &[&mint]).unwrap();
        self.tracked.push(mint.pubkey());
        mint.pubkey()
    }

    pub fn ata(&mut self, owner: Pubkey, mint: Pubkey) -> Pubkey {
        let address = get_associated_token_address_with_program_id(&owner, &mint, &token_2022::ID);
        let ix = spl_associated_token_account::instruction::create_associated_token_account(
            &self.payer.pubkey(),
            &owner,
            &mint,
            &token_2022::ID,
        );
        self.send(&[ix], &[]).unwrap();
        self.tracked.push(address);
        address
    }

    pub fn token_account(&mut self, owner: Pubkey, mint: Pubkey) -> Pubkey {
        let account = Keypair::new();
        let instructions = [
            solana_system_interface::instruction::create_account(
                &self.payer.pubkey(),
                &account.pubkey(),
                self.svm
                    .minimum_balance_for_rent_exemption(TokenAccount::LEN),
                TokenAccount::LEN as u64,
                &token_2022::ID,
            ),
            spl_token_2022::instruction::initialize_account3(
                &token_2022::ID,
                &account.pubkey(),
                &mint,
                &owner,
            )
            .unwrap(),
        ];
        self.send(&instructions, &[&account]).unwrap();
        self.tracked.push(account.pubkey());
        account.pubkey()
    }

    pub fn mint_to(&mut self, authority: &Keypair, mint: Pubkey, to: Pubkey, amount: u64) {
        let ix = spl_token_2022::instruction::mint_to_checked(
            &token_2022::ID,
            &mint,
            &to,
            &authority.pubkey(),
            &[],
            amount,
            DECIMALS,
        )
        .unwrap();
        self.send(&[ix], &[authority]).unwrap();
    }

    pub fn tokens(&self, address: Pubkey) -> TokenAccount {
        let account = self.svm.get_account(&address).unwrap();
        assert_eq!(account.owner, token_2022::ID);
        spl_token_2022::extension::StateWithExtensions::<TokenAccount>::unpack(&account.data)
            .unwrap()
            .base
    }

    pub fn transfer(
        &mut self,
        owner: &Keypair,
        source: Pubkey,
        mint: Pubkey,
        destination: Pubkey,
        amount: u64,
    ) {
        let ix = spl_token_2022::instruction::transfer_checked(
            &token_2022::ID,
            &source,
            &mint,
            &destination,
            &owner.pubkey(),
            &[],
            amount,
            DECIMALS,
        )
        .unwrap();
        self.send(&[ix], &[owner]).unwrap();
    }

    pub fn supply(&self, mint: Pubkey) -> u64 {
        let account = self.svm.get_account(&mint).unwrap();
        Mint::unpack(&account.data).unwrap().supply
    }

    pub fn state(&self, deal: Deal) -> EscrowState {
        let account = self.svm.get_account(&deal.state).unwrap();
        assert_eq!(account.owner, ID);
        EscrowState::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    pub fn receipt(&self, deal: Deal) -> DealReceipt {
        let account = self.svm.get_account(&deal.receipt).unwrap();
        assert_eq!(account.owner, ID);
        DealReceipt::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    pub fn reject(&mut self, ix: Instruction, signers: &[&Keypair], error: impl Into<u32>) {
        self.reject_with_error(ix, signers, InstructionError::Custom(error.into()));
    }

    pub fn reject_with_error(
        &mut self,
        ix: Instruction,
        signers: &[&Keypair],
        error: InstructionError,
    ) {
        self.reject_transaction(&[ix], signers, 0, error);
    }

    pub fn reject_transaction(
        &mut self,
        instructions: &[Instruction],
        signers: &[&Keypair],
        failed_index: u8,
        error: InstructionError,
    ) {
        let mut keys = self.tracked.clone();
        keys.extend(
            instructions
                .iter()
                .flat_map(|ix| ix.accounts.iter().map(|a| a.pubkey)),
        );
        keys.sort();
        keys.dedup();
        keys.retain(|key| *key != self.payer.pubkey());
        let before: Vec<_> = keys.iter().map(|key| self.svm.get_account(key)).collect();
        let failure = self
            .send(instructions, signers)
            .expect_err("transaction must fail");
        assert_eq!(
            failure.err,
            TransactionError::InstructionError(failed_index, error),
            "logs: {:#?}",
            failure.meta.logs
        );
        // Включаем sender/rent, receipt и несуществующие аккаунты; только fee payer исключён.
        for (key, account) in keys.iter().zip(before) {
            assert_eq!(
                self.svm.get_account(key),
                account,
                "account {key} changed after rejection"
            );
        }
    }
}

pub struct Fixture {
    pub context: TestContext,
    pub sender: Keypair,
    pub receiver: Keypair,
    pub authority: Keypair,
    pub mint: Pubkey,
    pub source: Pubkey,
    pub destination: Pubkey,
    pub deal: Deal,
}

impl Fixture {
    pub fn new() -> Self {
        let mut context = TestContext::new();
        let sender = context.wallet();
        let receiver = context.wallet();
        let authority = context.wallet();
        let mint = context.create_mint(&authority, token_2022::ID, None);
        let source = context.ata(sender.pubkey(), mint);
        let destination = context.ata(receiver.pubkey(), mint);
        context.mint_to(&authority, mint, source, SUPPLY);
        let deal = Deal::new(sender.pubkey(), DEAL_ID);
        context
            .tracked
            .extend([deal.state, deal.vault, deal.receipt]);
        Self {
            context,
            sender,
            receiver,
            authority,
            mint,
            source,
            destination,
            deal,
        }
    }

    pub fn initialize_ix(&self, amount: u64) -> Instruction {
        instruction(
            accounts::Initialize {
                sender: self.sender.pubkey(),
                receiver: self.receiver.pubkey(),
                mint: self.mint,
                receipt: self.deal.receipt,
                escrow: self.deal.state,
                vault: self.deal.vault,
                token_program: token_2022::ID,
                system_program: anchor_lang::system_program::ID,
            },
            instruction::Initialize {
                deal_id: self.deal.id,
                amount,
            },
        )
    }

    pub fn deposit_ix(&self) -> Instruction {
        instruction(
            accounts::Deposit {
                sender: self.sender.pubkey(),
                escrow: self.deal.state,
                receipt: self.deal.receipt,
                mint: self.mint,
                source: self.source,
                vault: self.deal.vault,
                token_program: token_2022::ID,
            },
            instruction::Deposit {},
        )
    }

    pub fn release_ix(&self) -> Instruction {
        instruction(
            accounts::Release {
                sender: self.sender.pubkey(),
                receiver: self.receiver.pubkey(),
                escrow: self.deal.state,
                receipt: self.deal.receipt,
                mint: self.mint,
                vault: self.deal.vault,
                receiver_account: self.destination,
                sender_account: self.source,
                token_program: token_2022::ID,
            },
            instruction::Release {},
        )
    }

    pub fn cancel_ix(&self) -> Instruction {
        instruction(
            accounts::Cancel {
                sender: self.sender.pubkey(),
                escrow: self.deal.state,
                receipt: self.deal.receipt,
                mint: self.mint,
                vault: self.deal.vault,
                sender_account: self.source,
                token_program: token_2022::ID,
            },
            instruction::Cancel {},
        )
    }

    pub fn initialize(&mut self) {
        self.context
            .send(&[self.initialize_ix(AMOUNT)], &[&self.sender])
            .unwrap();
    }

    pub fn deposit(&mut self) {
        self.context
            .send(&[self.deposit_ix()], &[&self.sender])
            .unwrap();
    }

    pub fn funded() -> Self {
        let mut f = Self::new();
        f.initialize();
        f.deposit();
        f
    }

    pub fn assert_closed(
        &self,
        status: escrow::state::EscrowStatus,
        rent: u64,
        sender_lamports_before: u64,
    ) {
        for key in [self.deal.state, self.deal.vault] {
            assert!(
                self.context
                    .svm
                    .get_account(&key)
                    .is_none_or(|a| a.lamports == 0 && a.data.is_empty()),
                "{key} must be closed"
            );
        }
        assert_eq!(self.context.receipt(self.deal).status, status);
        assert_eq!(
            self.context.svm.get_balance(&self.sender.pubkey()).unwrap(),
            sender_lamports_before + rent
        );
        assert_eq!(self.context.supply(self.mint), SUPPLY);
    }

    pub fn rent(&self) -> u64 {
        self.context.svm.get_balance(&self.deal.state).unwrap()
            + self.context.svm.get_balance(&self.deal.vault).unwrap()
    }
}

pub fn replace(ix: &mut Instruction, from: Pubkey, to: Pubkey) {
    ix.accounts
        .iter_mut()
        .find(|a| a.pubkey == from)
        .unwrap()
        .pubkey = to;
}
