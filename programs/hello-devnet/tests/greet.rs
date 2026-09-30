use anchor_lang::InstructionData;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::{Instruction, Message};
use solana_signer::Signer;
use solana_transaction::Transaction;
use std::{fs, path::PathBuf};

fn program_bytes() -> Vec<u8> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/hello_devnet.so");
    fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "Build the program with `anchor build -p hello_devnet` before running tests. Could not read {}: {error}",
            path.display()
        )
    })
}

#[test]
fn greets_on_chain() {
    let mut svm = LiteSVM::new();
    svm.add_program(hello_devnet::ID, &program_bytes())
        .expect("program must load");

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000)
        .expect("airdrop must succeed");

    let instruction = Instruction {
        program_id: hello_devnet::ID,
        accounts: vec![],
        data: hello_devnet::instruction::Greet {}.data(),
    };
    let message = Message::new(&[instruction], Some(&payer.pubkey()));
    let transaction = Transaction::new(&[&payer], message, svm.latest_blockhash());

    let result = svm
        .send_transaction(transaction)
        .expect("greet must succeed");
    assert!(
        result
            .logs
            .iter()
            .any(|log| log.contains("Hello from Solana Devnet!")),
        "greet must write its message to the transaction log"
    );
}
