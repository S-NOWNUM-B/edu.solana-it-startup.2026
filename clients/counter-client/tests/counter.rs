use anchor_lang::{prelude::Pubkey, AccountSerialize, Discriminator, Space};
use counter::{Counter, CounterError};
use counter_client::{
    counter_address, decode_counter, increment_instruction, initialize_instruction,
};
use litesvm::LiteSVM;
use solana_account::Account;
use solana_keypair::Keypair;
use solana_message::{Instruction, Message};
use solana_signer::Signer;
use solana_transaction::{InstructionError, Transaction, TransactionError};
use std::{fs, path::PathBuf};

fn fixture() -> (LiteSVM, Keypair) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/counter.so");
    let program = fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "Run `anchor build --ignore-keys` first; cannot read {}: {error}",
            path.display()
        )
    });
    let mut svm = LiteSVM::new();
    svm.add_program(counter::ID, &program)
        .expect("program must load");
    let authority = Keypair::new();
    svm.airdrop(&authority.pubkey(), 1_000_000_000).unwrap();
    (svm, authority)
}

fn send(
    svm: &mut LiteSVM,
    signer: &Keypair,
    instruction: Instruction,
) -> Result<(), TransactionError> {
    // Одинаковые инструкции должны создавать новые транзакции, а не попадать в duplicate cache.
    svm.expire_blockhash();
    let message = Message::new(&[instruction], Some(&signer.pubkey()));
    let transaction = Transaction::new(&[signer], message, svm.latest_blockhash());
    svm.send_transaction(transaction)
        .map(|_| ())
        .map_err(|failure| failure.err)
}

fn read(svm: &LiteSVM, authority: &Pubkey) -> Counter {
    let address = counter_address(authority).0;
    let account = svm.get_account(&address).expect("counter must exist");
    decode_counter(&address, &account.owner, &account.data, authority).unwrap()
}

fn initialize(svm: &mut LiteSVM, authority: &Keypair) {
    send(svm, authority, initialize_instruction(authority.pubkey())).unwrap();
}

fn assert_custom_error(result: Result<(), TransactionError>, error: CounterError) {
    assert_eq!(
        result.unwrap_err(),
        TransactionError::InstructionError(0, InstructionError::Custom(error.into()))
    );
}

#[test]
fn initializes_zero_at_authority_pda() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority);
    let (address, bump) = counter_address(&authority.pubkey());
    let account = svm.get_account(&address).unwrap();
    assert_eq!(account.owner, counter::ID);
    assert_eq!(
        account.data.len(),
        Counter::DISCRIMINATOR.len() + Counter::INIT_SPACE
    );
    assert!(!account.executable);
    let state = read(&svm, &authority.pubkey());
    assert_eq!(state.authority, authority.pubkey());
    assert_eq!(state.count, 0);
    assert_eq!(state.bump, bump);
}

#[test]
fn client_increments_and_reads_persisted_value() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority);
    for expected in 1..=3 {
        send(
            &mut svm,
            &authority,
            increment_instruction(authority.pubkey()),
        )
        .unwrap();
        assert_eq!(read(&svm, &authority.pubkey()).count, expected);
    }
}

#[test]
fn repeat_initialize_does_not_reset_value() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority);
    send(
        &mut svm,
        &authority,
        increment_instruction(authority.pubkey()),
    )
    .unwrap();
    let error = send(
        &mut svm,
        &authority,
        initialize_instruction(authority.pubkey()),
    )
    .unwrap_err();
    // System Program отклоняет попытку allocate уже существующего PDA.
    assert_eq!(
        error,
        TransactionError::InstructionError(0, InstructionError::Custom(0))
    );
    assert_eq!(read(&svm, &authority.pubkey()).count, 1);
}

#[test]
fn rejects_another_wallet_as_authority() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority);
    let attacker = Keypair::new();
    svm.airdrop(&attacker.pubkey(), 1_000_000_000).unwrap();
    let mut instruction = increment_instruction(attacker.pubkey());
    instruction.accounts[0].pubkey = counter_address(&authority.pubkey()).0;
    assert_custom_error(
        send(&mut svm, &attacker, instruction),
        CounterError::Unauthorized,
    );
    assert_eq!(read(&svm, &authority.pubkey()).count, 0);
}

#[test]
fn requires_authority_signature_on_chain() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority);
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let mut instruction = increment_instruction(authority.pubkey());
    instruction.accounts[1].is_signer = false;
    let error = send(&mut svm, &payer, instruction).unwrap_err();
    assert_eq!(
        error,
        TransactionError::InstructionError(
            0,
            InstructionError::Custom(anchor_lang::error::ErrorCode::AccountNotSigner.into())
        )
    );
    assert_eq!(read(&svm, &authority.pubkey()).count, 0);
}

#[test]
fn wallets_have_independent_counters() {
    let (mut svm, first) = fixture();
    let second = Keypair::new();
    svm.airdrop(&second.pubkey(), 1_000_000_000).unwrap();
    initialize(&mut svm, &first);
    initialize(&mut svm, &second);
    send(&mut svm, &first, increment_instruction(first.pubkey())).unwrap();
    assert_ne!(
        counter_address(&first.pubkey()).0,
        counter_address(&second.pubkey()).0
    );
    assert_eq!(read(&svm, &first.pubkey()).count, 1);
    assert_eq!(read(&svm, &second.pubkey()).count, 0);
}

#[test]
fn overflow_preserves_maximum_value() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority);
    let address = counter_address(&authority.pubkey()).0;
    let mut account = svm.get_account(&address).unwrap();
    let mut state = read(&svm, &authority.pubkey());
    state.count = u64::MAX;
    let mut data = Vec::new();
    state.try_serialize(&mut data).unwrap();
    account.data = data;
    svm.set_account(address, account).unwrap();
    assert_custom_error(
        send(
            &mut svm,
            &authority,
            increment_instruction(authority.pubkey()),
        ),
        CounterError::Overflow,
    );
    assert_eq!(read(&svm, &authority.pubkey()).count, u64::MAX);
}

#[test]
fn rejects_valid_counter_data_outside_its_pda() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority);
    let original = counter_address(&authority.pubkey()).0;
    let fake = Keypair::new().pubkey();
    let account = svm.get_account(&original).unwrap();
    svm.set_account(fake, account.clone()).unwrap();
    let mut instruction = increment_instruction(authority.pubkey());
    instruction.accounts[0].pubkey = fake;
    let error = send(&mut svm, &authority, instruction).unwrap_err();
    assert_eq!(
        error,
        TransactionError::InstructionError(
            0,
            InstructionError::Custom(anchor_lang::error::ErrorCode::ConstraintSeeds.into())
        )
    );
    assert_eq!(svm.get_account(&fake).unwrap().data, account.data);
    assert_eq!(read(&svm, &authority.pubkey()).count, 0);
}

#[test]
fn client_rejects_foreign_owner_and_invalid_account_data() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority);
    let address = counter_address(&authority.pubkey()).0;
    let account = svm.get_account(&address).unwrap();
    let foreign = Account {
        owner: anchor_lang::system_program::ID,
        ..account.clone()
    };
    assert!(decode_counter(&address, &foreign.owner, &foreign.data, &authority.pubkey()).is_err());
    assert!(decode_counter(&address, &account.owner, &[], &authority.pubkey()).is_err());
    let mut wrong_discriminator = account.data.clone();
    wrong_discriminator[0] ^= 0xff;
    assert!(decode_counter(
        &address,
        &account.owner,
        &wrong_discriminator,
        &authority.pubkey()
    )
    .is_err());
}
