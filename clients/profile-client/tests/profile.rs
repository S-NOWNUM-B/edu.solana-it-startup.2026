use anchor_lang::{prelude::Pubkey, AccountSerialize, Discriminator, Space};
use litesvm::LiteSVM;
use onchain_profile::{Profile, ProfileError, MAX_BIO_BYTES, MAX_NAME_BYTES};
use profile_client::{decode_profile, initialize_instruction, profile_address, update_instruction};
use solana_keypair::Keypair;
use solana_message::{Instruction, Message};
use solana_signer::Signer;
use solana_transaction::{InstructionError, Transaction, TransactionError};
use std::{fs, path::PathBuf};

fn fixture() -> (LiteSVM, Keypair) {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/onchain_profile.so");
    let program = fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "Run `anchor build --ignore-keys` first; cannot read {}: {error}",
            path.display()
        )
    });
    let mut svm = LiteSVM::new();
    svm.add_program(onchain_profile::ID, &program)
        .expect("program must load");
    let authority = funded_wallet(&mut svm);
    (svm, authority)
}

fn funded_wallet(svm: &mut LiteSVM) -> Keypair {
    let wallet = Keypair::new();
    svm.airdrop(&wallet.pubkey(), 1_000_000_000).unwrap();
    wallet
}

fn send(
    svm: &mut LiteSVM,
    signer: &Keypair,
    instruction: Instruction,
) -> Result<(), TransactionError> {
    // Повторная инструкция должна стать новой транзакцией, а не попасть в duplicate cache.
    svm.expire_blockhash();
    let message = Message::new(&[instruction], Some(&signer.pubkey()));
    let transaction = Transaction::new(&[signer], message, svm.latest_blockhash());
    svm.send_transaction(transaction)
        .map(|_| ())
        .map_err(|failure| failure.err)
}

fn initialize(svm: &mut LiteSVM, authority: &Keypair, name: &str, bio: &str) {
    send(
        svm,
        authority,
        initialize_instruction(authority.pubkey(), name.to_string(), bio.to_string()),
    )
    .unwrap();
}

fn read(svm: &LiteSVM, authority: &Pubkey) -> Profile {
    let address = profile_address(authority).0;
    let account = svm.get_account(&address).expect("profile must exist");
    decode_profile(&address, &account.owner, &account.data, authority).unwrap()
}

fn assert_profile(svm: &LiteSVM, authority: &Pubkey, name: &str, bio: &str) {
    let state = read(svm, authority);
    assert_eq!(state.authority, *authority);
    assert_eq!(state.display_name, name);
    assert_eq!(state.bio, bio);
    assert_eq!(state.bump, profile_address(authority).1);
}

fn assert_custom_error(result: Result<(), TransactionError>, error: ProfileError) {
    assert_eq!(
        result.unwrap_err(),
        TransactionError::InstructionError(0, InstructionError::Custom(error.into()))
    );
}

#[test]
fn initializes_profile_at_authority_pda_with_correct_space() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority, "Alice", "Solana developer");
    let address = profile_address(&authority.pubkey()).0;
    let account = svm.get_account(&address).unwrap();
    assert_eq!(account.owner, onchain_profile::ID);
    assert_eq!(
        account.data.len(),
        Profile::DISCRIMINATOR.len() + Profile::INIT_SPACE
    );
    assert!(!account.executable);
    assert_profile(&svm, &authority.pubkey(), "Alice", "Solana developer");
}

#[test]
fn updates_same_pda_and_reads_shorter_multibyte_fields() {
    let (mut svm, authority) = fixture();
    initialize(
        &mut svm,
        &authority,
        &"a".repeat(MAX_NAME_BYTES),
        &"b".repeat(MAX_BIO_BYTES),
    );
    let address = profile_address(&authority.pubkey()).0;
    let original = svm.get_account(&address).unwrap();
    for (name, bio) in [("Станислав", "Разработчик 🦀"), ("Стас", ""), ("Стас", "")]
    {
        send(
            &mut svm,
            &authority,
            update_instruction(authority.pubkey(), name.to_string(), bio.to_string()),
        )
        .unwrap();
        assert_profile(&svm, &authority.pubkey(), name, bio);
        let account = svm.get_account(&address).unwrap();
        assert_eq!(account.data.len(), original.data.len());
        assert_eq!(account.owner, original.owner);
        assert_eq!(account.lamports, original.lamports);
    }
}

#[test]
fn public_read_requires_no_authority_signature() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority, "Alice", "Public profile");
    let observer = Keypair::new();
    assert_ne!(observer.pubkey(), authority.pubkey());
    // Читателю нужны публичный адрес и данные аккаунта; приватный ключ владельца не передаётся.
    let address = profile_address(&authority.pubkey()).0;
    let account = svm.get_account(&address).unwrap();
    let state = decode_profile(&address, &account.owner, &account.data, &authority.pubkey())
        .expect("public state must be readable");
    assert_eq!(state.display_name, "Alice");
    assert_eq!(state.bio, "Public profile");
}

#[test]
fn repeat_initialize_does_not_reset_existing_profile() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority, "Alice", "Original bio");
    send(
        &mut svm,
        &authority,
        update_instruction(
            authority.pubkey(),
            "Updated name".to_string(),
            "Updated bio".to_string(),
        ),
    )
    .unwrap();
    let address = profile_address(&authority.pubkey()).0;
    let before = svm.get_account(&address).unwrap();
    let error = send(
        &mut svm,
        &authority,
        initialize_instruction(
            authority.pubkey(),
            "Replacement".to_string(),
            "Replacement bio".to_string(),
        ),
    )
    .unwrap_err();
    assert_eq!(
        error,
        TransactionError::InstructionError(0, InstructionError::Custom(0))
    );
    assert_eq!(svm.get_account(&address).unwrap().data, before.data);
    assert_profile(&svm, &authority.pubkey(), "Updated name", "Updated bio");
}

#[test]
fn rejects_update_signed_by_another_wallet() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority, "Alice", "Original bio");
    let address = profile_address(&authority.pubkey()).0;
    let before = svm.get_account(&address).unwrap();
    let attacker = funded_wallet(&mut svm);
    let mut instruction = update_instruction(
        attacker.pubkey(),
        "Attacker".to_string(),
        "Changed bio".to_string(),
    );
    instruction.accounts[0].pubkey = address;
    assert_custom_error(
        send(&mut svm, &attacker, instruction),
        ProfileError::Unauthorized,
    );
    assert_eq!(svm.get_account(&address).unwrap().data, before.data);
}

#[test]
fn requires_authority_signature_for_update() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority, "Alice", "Original bio");
    let address = profile_address(&authority.pubkey()).0;
    let before = svm.get_account(&address).unwrap();
    let payer = funded_wallet(&mut svm);
    let mut instruction = update_instruction(
        authority.pubkey(),
        "Changed name".to_string(),
        "Changed bio".to_string(),
    );
    instruction.accounts[1].is_signer = false;
    assert_eq!(
        send(&mut svm, &payer, instruction).unwrap_err(),
        TransactionError::InstructionError(
            0,
            InstructionError::Custom(anchor_lang::error::ErrorCode::AccountNotSigner.into())
        )
    );
    assert_eq!(svm.get_account(&address).unwrap().data, before.data);
}

#[test]
fn wallets_have_independent_profiles() {
    let (mut svm, first) = fixture();
    let second = funded_wallet(&mut svm);
    initialize(&mut svm, &first, "Alice", "First bio");
    initialize(&mut svm, &second, "Bob", "Second bio");
    send(
        &mut svm,
        &first,
        update_instruction(first.pubkey(), "Alicia".to_string(), "".to_string()),
    )
    .unwrap();
    assert_ne!(
        profile_address(&first.pubkey()).0,
        profile_address(&second.pubkey()).0
    );
    assert_profile(&svm, &first.pubkey(), "Alicia", "");
    assert_profile(&svm, &second.pubkey(), "Bob", "Second bio");
}

fn invalid_fields() -> Vec<(String, String, ProfileError)> {
    vec![
        ("".to_string(), "Bio".to_string(), ProfileError::EmptyName),
        (
            " \t\n".to_string(),
            "Bio".to_string(),
            ProfileError::EmptyName,
        ),
        (
            "a".repeat(MAX_NAME_BYTES + 1),
            "Bio".to_string(),
            ProfileError::NameTooLong,
        ),
        (
            "Valid name".to_string(),
            "b".repeat(MAX_BIO_BYTES + 1),
            ProfileError::BioTooLong,
        ),
    ]
}

#[test]
fn rejects_invalid_fields_without_creating_profile() {
    let (mut svm, _) = fixture();
    for (name, bio, error) in invalid_fields() {
        let authority = funded_wallet(&mut svm);
        let address = profile_address(&authority.pubkey()).0;
        assert_custom_error(
            send(
                &mut svm,
                &authority,
                initialize_instruction(authority.pubkey(), name, bio),
            ),
            error,
        );
        assert!(svm.get_account(&address).is_none());
    }
}

#[test]
fn invalid_update_preserves_both_previous_fields() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority, "Alice", "Original bio");
    let address = profile_address(&authority.pubkey()).0;
    let before = svm.get_account(&address).unwrap();
    for (name, bio, error) in invalid_fields() {
        assert_custom_error(
            send(
                &mut svm,
                &authority,
                update_instruction(authority.pubkey(), name, bio),
            ),
            error,
        );
        assert_eq!(svm.get_account(&address).unwrap().data, before.data);
        assert_profile(&svm, &authority.pubkey(), "Alice", "Original bio");
    }
}

#[test]
fn permits_exact_utf8_byte_limits_and_rejects_one_extra_byte() {
    let (mut svm, authority) = fixture();
    let name = "я".repeat(MAX_NAME_BYTES / 2);
    let bio = "🦀".repeat(MAX_BIO_BYTES / 4);
    assert_eq!(name.len(), MAX_NAME_BYTES);
    assert_eq!(bio.len(), MAX_BIO_BYTES);
    initialize(&mut svm, &authority, &name, &bio);
    assert_profile(&svm, &authority.pubkey(), &name, &bio);
    send(
        &mut svm,
        &authority,
        update_instruction(authority.pubkey(), name.clone(), bio.clone()),
    )
    .unwrap();
    for (too_long_name, too_long_bio, error) in [
        (format!("{name}a"), bio.clone(), ProfileError::NameTooLong),
        (name.clone(), format!("{bio}a"), ProfileError::BioTooLong),
    ] {
        let fresh_authority = funded_wallet(&mut svm);
        assert_custom_error(
            send(
                &mut svm,
                &fresh_authority,
                initialize_instruction(
                    fresh_authority.pubkey(),
                    too_long_name.clone(),
                    too_long_bio.clone(),
                ),
            ),
            error,
        );
        assert!(svm
            .get_account(&profile_address(&fresh_authority.pubkey()).0)
            .is_none());
        let expected_error = if too_long_name.len() > MAX_NAME_BYTES {
            ProfileError::NameTooLong
        } else {
            ProfileError::BioTooLong
        };
        assert_custom_error(
            send(
                &mut svm,
                &authority,
                update_instruction(authority.pubkey(), too_long_name, too_long_bio),
            ),
            expected_error,
        );
        assert_profile(&svm, &authority.pubkey(), &name, &bio);
    }
}

#[test]
fn rejects_valid_profile_data_outside_its_pda() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority, "Alice", "Original bio");
    let original = profile_address(&authority.pubkey()).0;
    let fake = Keypair::new().pubkey();
    let account = svm.get_account(&original).unwrap();
    svm.set_account(fake, account.clone()).unwrap();
    let mut instruction = update_instruction(
        authority.pubkey(),
        "Changed name".to_string(),
        "Changed bio".to_string(),
    );
    instruction.accounts[0].pubkey = fake;
    assert_eq!(
        send(&mut svm, &authority, instruction).unwrap_err(),
        TransactionError::InstructionError(
            0,
            InstructionError::Custom(anchor_lang::error::ErrorCode::ConstraintSeeds.into())
        )
    );
    assert_eq!(svm.get_account(&fake).unwrap().data, account.data);
    assert_eq!(svm.get_account(&original).unwrap().data, account.data);
}

#[test]
fn client_rejects_foreign_owner_address_and_invalid_data() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority, "Alice", "Original bio");
    let address = profile_address(&authority.pubkey()).0;
    let account = svm.get_account(&address).unwrap();
    assert!(decode_profile(
        &address,
        &anchor_lang::system_program::ID,
        &account.data,
        &authority.pubkey()
    )
    .is_err());
    assert!(decode_profile(
        &Keypair::new().pubkey(),
        &account.owner,
        &account.data,
        &authority.pubkey()
    )
    .is_err());
    assert!(decode_profile(&address, &account.owner, &[], &authority.pubkey()).is_err());
    let mut wrong_discriminator = account.data.clone();
    wrong_discriminator[0] ^= 0xff;
    assert!(decode_profile(
        &address,
        &account.owner,
        &wrong_discriminator,
        &authority.pubkey()
    )
    .is_err());
}

#[test]
fn client_rejects_wrong_authority_bump_and_oversized_fields() {
    let (mut svm, authority) = fixture();
    initialize(&mut svm, &authority, "Alice", "Original bio");
    let address = profile_address(&authority.pubkey()).0;
    let account = svm.get_account(&address).unwrap();
    for invalid_case in 0..4 {
        let mut state = read(&svm, &authority.pubkey());
        match invalid_case {
            0 => state.authority = Keypair::new().pubkey(),
            1 => state.bump ^= 0xff,
            2 => state.display_name = "a".repeat(MAX_NAME_BYTES + 1),
            3 => state.bio = "b".repeat(MAX_BIO_BYTES + 1),
            _ => unreachable!(),
        }
        let mut data = Vec::new();
        state.try_serialize(&mut data).unwrap();
        assert!(decode_profile(&address, &account.owner, &data, &authority.pubkey()).is_err());
    }
}
