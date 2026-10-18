use anchor_lang::{prelude::Pubkey, AccountDeserialize, InstructionData, ToAccountMetas};
use anchor_spl::{
    associated_token::{get_associated_token_address_with_program_id, ID as ASSOCIATED_TOKEN_ID},
    token_interface::TokenAccount,
};
use litesvm::{
    types::{FailedTransactionMetadata, TransactionMetadata},
    LiteSVM,
};
use solana_keypair::Keypair;
use solana_message::{Instruction, Message};
use solana_signer::Signer;
use solana_transaction::Transaction;
use std::{fs, path::PathBuf};

const DECIMALS: u8 = 6;
const SUPPLY: u64 = 10_000_000;
const TOKEN_AMOUNT: u64 = 2_000_000;
const LAMPORTS: u64 = 10_000_000;

struct Fixture {
    svm: LiteSVM,
    payer: Keypair,
    recipient: Pubkey,
    mint: Pubkey,
    source: Pubkey,
    recipient_token: Pubkey,
    token_program: Pubkey,
}

fn instruction(accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
    Instruction {
        program_id: solana_level_1_token_starter::ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

impl Fixture {
    fn new(token_program: Pubkey) -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/deploy/solana_level_1_token_starter.so");
        let bytes =
            fs::read(path).expect("build current program with cargo build-sbf before tests");
        let mut svm = LiteSVM::new();
        svm.add_program(solana_level_1_token_starter::ID, &bytes)
            .expect("program must load");
        let payer = Keypair::new();
        let recipient = Keypair::new().pubkey();
        svm.airdrop(&payer.pubkey(), 5_000_000_000).unwrap();
        svm.airdrop(&recipient, 1_000_000).unwrap();
        let mint = Keypair::new();
        let source = get_associated_token_address_with_program_id(
            &payer.pubkey(),
            &mint.pubkey(),
            &token_program,
        );
        let recipient_token = get_associated_token_address_with_program_id(
            &recipient,
            &mint.pubkey(),
            &token_program,
        );
        let setup = [
            instruction(
                solana_level_1_token_starter::accounts::CreateToken {
                    payer: payer.pubkey(),
                    authority: payer.pubkey(),
                    mint: mint.pubkey(),
                    token_program,
                    system_program: anchor_lang::system_program::ID,
                },
                solana_level_1_token_starter::instruction::CreateToken { decimals: DECIMALS },
            ),
            instruction(
                solana_level_1_token_starter::accounts::CreateTokenAccount {
                    payer: payer.pubkey(),
                    owner: payer.pubkey(),
                    mint: mint.pubkey(),
                    token_account: source,
                    token_program,
                    associated_token_program: ASSOCIATED_TOKEN_ID,
                    system_program: anchor_lang::system_program::ID,
                },
                solana_level_1_token_starter::instruction::CreateTokenAccount {},
            ),
            instruction(
                solana_level_1_token_starter::accounts::MintTokens {
                    authority: payer.pubkey(),
                    mint: mint.pubkey(),
                    destination: source,
                    token_program,
                },
                solana_level_1_token_starter::instruction::MintTokens { amount: SUPPLY },
            ),
        ];
        let transaction = Transaction::new(
            &[&payer, &mint],
            Message::new(&setup, Some(&payer.pubkey())),
            svm.latest_blockhash(),
        );
        svm.send_transaction(transaction)
            .expect("setup must succeed");
        Self {
            svm,
            payer,
            recipient,
            mint: mint.pubkey(),
            source,
            recipient_token,
            token_program,
        }
    }

    fn compose(&self, token_amount: u64, lamports: u64) -> Instruction {
        instruction(
            solana_level_1_token_starter::accounts::ComposeTransfer {
                payer: self.payer.pubkey(),
                recipient: self.recipient,
                mint: self.mint,
                source: self.source,
                recipient_token: self.recipient_token,
                token_program: self.token_program,
                associated_token_program: ASSOCIATED_TOKEN_ID,
                system_program: anchor_lang::system_program::ID,
            },
            solana_level_1_token_starter::instruction::ComposeTransfer {
                token_amount,
                lamports,
            },
        )
    }

    fn send(
        &mut self,
        instruction: Instruction,
    ) -> Result<TransactionMetadata, Box<FailedTransactionMetadata>> {
        let transaction = Transaction::new(
            &[&self.payer],
            Message::new(&[instruction], Some(&self.payer.pubkey())),
            self.svm.latest_blockhash(),
        );
        self.svm.send_transaction(transaction).map_err(Box::new)
    }

    fn token_balance(&self, address: &Pubkey) -> u64 {
        let account = self.svm.get_account(address).expect("token account exists");
        TokenAccount::try_deserialize(&mut account.data.as_slice())
            .unwrap()
            .amount
    }
}

fn supported_programs() -> [Pubkey; 2] {
    [anchor_spl::token::ID, anchor_spl::token_2022::ID]
}

#[test]
fn creates_ata_and_transfers_sol_and_tokens_for_both_token_programs() {
    for token_program in supported_programs() {
        let mut f = Fixture::new(token_program);
        assert!(f.svm.get_account(&f.recipient_token).is_none());
        let payer_before = f.svm.get_balance(&f.payer.pubkey()).unwrap();
        let recipient_before = f.svm.get_balance(&f.recipient).unwrap();
        let result = f.send(f.compose(TOKEN_AMOUNT, LAMPORTS)).unwrap();
        println!("SUCCESS [{token_program}]\n{}", result.logs.join("\n"));
        let recipient_account = f.svm.get_account(&f.recipient_token).unwrap();
        let tokens = TokenAccount::try_deserialize(&mut recipient_account.data.as_slice()).unwrap();
        assert_eq!(recipient_account.owner, token_program);
        assert_eq!(tokens.owner, f.recipient);
        assert_eq!(tokens.mint, f.mint);
        assert_eq!(tokens.amount, TOKEN_AMOUNT);
        assert_eq!(f.token_balance(&f.source), SUPPLY - TOKEN_AMOUNT);
        assert_eq!(
            f.svm.get_balance(&f.recipient).unwrap(),
            recipient_before + LAMPORTS
        );
        assert_eq!(
            f.svm.get_balance(&f.payer.pubkey()).unwrap(),
            payer_before - LAMPORTS - recipient_account.lamports - result.fee
        );
        for marker in ["CPI 1/3", "CPI 2/3", "CPI 3/3"] {
            assert!(result.logs.iter().any(|line| line.contains(marker)));
        }
    }
}

#[test]
fn reuses_existing_ata_without_charging_rent_again() {
    for token_program in supported_programs() {
        let mut f = Fixture::new(token_program);
        f.send(f.compose(TOKEN_AMOUNT, LAMPORTS)).unwrap();
        let rent_before = f.svm.get_balance(&f.recipient_token).unwrap();
        let payer_before = f.svm.get_balance(&f.payer.pubkey()).unwrap();
        f.svm.expire_blockhash();
        let result = f.send(f.compose(TOKEN_AMOUNT, LAMPORTS)).unwrap();
        assert_eq!(f.token_balance(&f.recipient_token), 2 * TOKEN_AMOUNT);
        assert_eq!(f.token_balance(&f.source), SUPPLY - 2 * TOKEN_AMOUNT);
        assert_eq!(f.svm.get_balance(&f.recipient_token).unwrap(), rent_before);
        assert_eq!(
            f.svm.get_balance(&f.payer.pubkey()).unwrap(),
            payer_before - LAMPORTS - result.fee
        );
    }
}

#[test]
fn final_token_cpi_failure_rolls_back_ata_and_sol_but_keeps_transaction_fee() {
    for token_program in supported_programs() {
        let mut f = Fixture::new(token_program);
        let payer_before = f.svm.get_balance(&f.payer.pubkey()).unwrap();
        let recipient_before = f.svm.get_balance(&f.recipient).unwrap();
        let failure = f.send(f.compose(SUPPLY + 1, LAMPORTS)).unwrap_err();
        println!(
            "ROLLBACK [{token_program}]\n{}",
            failure.meta.logs.join("\n")
        );
        assert!(failure
            .meta
            .logs
            .iter()
            .any(|line| line.contains("CPI 2/3")));
        assert!(failure
            .meta
            .logs
            .iter()
            .any(|line| line.contains("insufficient funds")));
        assert!(!failure
            .meta
            .logs
            .iter()
            .any(|line| line.contains("CPI 3/3")));
        assert!(f.svm.get_account(&f.recipient_token).is_none());
        assert_eq!(f.svm.get_balance(&f.recipient).unwrap(), recipient_before);
        assert_eq!(f.token_balance(&f.source), SUPPLY);
        assert_eq!(
            f.svm.get_balance(&f.payer.pubkey()).unwrap(),
            payer_before - failure.meta.fee
        );
        println!(
            "VERIFIED: ATA absent; SOL and source restored; fee={}",
            failure.meta.fee
        );
    }
}

#[test]
fn rejects_zero_token_amount_and_zero_lamports_before_any_cpi() {
    for (token_amount, lamports) in [(0, LAMPORTS), (TOKEN_AMOUNT, 0)] {
        let mut f = Fixture::new(anchor_spl::token_2022::ID);
        let failure = f.send(f.compose(token_amount, lamports)).unwrap_err();
        assert!(failure
            .meta
            .logs
            .iter()
            .any(|line| line.contains("AmountMustBePositive")));
        assert!(!failure
            .meta
            .logs
            .iter()
            .any(|line| line.contains("CPI 1/3")));
        assert!(f.svm.get_account(&f.recipient_token).is_none());
    }
}

#[test]
fn rejects_wrong_recipient_ata_and_same_wallet() {
    for same_wallet in [false, true] {
        let mut f = Fixture::new(anchor_spl::token_2022::ID);
        let mut ix = f.compose(TOKEN_AMOUNT, LAMPORTS);
        let expected_error = if same_wallet {
            ix.accounts[1].pubkey = f.payer.pubkey();
            "PayerEqualsRecipient"
        } else {
            ix.accounts[4].pubkey = f.source;
            "InvalidRecipientTokenAccount"
        };
        let failure = f.send(ix).unwrap_err();
        assert!(
            failure
                .meta
                .logs
                .iter()
                .any(|line| line.contains(expected_error)),
            "{:?}",
            failure
        );
        assert!(!failure
            .meta
            .logs
            .iter()
            .any(|line| line.contains("CPI 1/3")));
        assert_eq!(f.token_balance(&f.source), SUPPLY);
    }
}

#[test]
fn rejects_wrong_mint_source_authority_and_token_program() {
    for invalid_account in ["mint", "authority", "program"] {
        let mut f = Fixture::new(anchor_spl::token_2022::ID);
        let mut ix = f.compose(TOKEN_AMOUNT, LAMPORTS);
        let other = Fixture::new(anchor_spl::token_2022::ID);
        let expected_error = match invalid_account {
            "mint" => {
                let account = other.svm.get_account(&other.mint).unwrap();
                f.svm.set_account(other.mint, account).unwrap();
                ix.accounts[2].pubkey = other.mint;
                "ConstraintTokenMint"
            }
            "authority" => {
                // Сохраняем mint, чтобы проверять именно authority, а не более раннюю проверку mint.
                let mut account = f.svm.get_account(&f.source).unwrap();
                account.data[32..64].copy_from_slice(other.payer.pubkey().as_ref());
                f.svm.set_account(f.source, account).unwrap();
                "ConstraintTokenOwner"
            }
            _ => {
                ix.accounts[5].pubkey = anchor_spl::token::ID;
                "ConstraintMintTokenProgram"
            }
        };
        let recipient_before = f.svm.get_balance(&f.recipient).unwrap();
        let failure = f.send(ix).unwrap_err();
        assert!(
            failure
                .meta
                .logs
                .iter()
                .any(|line| line.contains(expected_error)),
            "{:?}",
            failure
        );
        assert_eq!(f.svm.get_balance(&f.recipient).unwrap(), recipient_before);
        assert!(f.svm.get_account(&f.recipient_token).is_none());
    }
}

#[test]
fn rejects_unsigned_payer_with_separate_fee_payer() {
    let mut f = Fixture::new(anchor_spl::token_2022::ID);
    let fee_payer = Keypair::new();
    f.svm.airdrop(&fee_payer.pubkey(), 1_000_000).unwrap();
    let mut ix = f.compose(TOKEN_AMOUNT, LAMPORTS);
    ix.accounts[0].is_signer = false;
    let transaction = Transaction::new(
        &[&fee_payer],
        Message::new(&[ix], Some(&fee_payer.pubkey())),
        f.svm.latest_blockhash(),
    );
    let failure = f.svm.send_transaction(transaction).unwrap_err();
    assert!(
        failure
            .meta
            .logs
            .iter()
            .any(|line| line.contains("AccountNotSigner")),
        "{:?}",
        failure
    );
    assert!(f.svm.get_account(&f.recipient_token).is_none());
    assert_eq!(f.token_balance(&f.source), SUPPLY);
}
