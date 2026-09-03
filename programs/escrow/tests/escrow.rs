mod common;

use anchor_lang::{error::ErrorCode, solana_program::program_pack::Pack, AccountSerialize};
use anchor_spl::token_2022::{
    self,
    spl_token_2022::{self, error::TokenError},
};
use common::{replace, Deal, Fixture, AMOUNT, DEAL_ID, SUPPLY};
use escrow::{error::EscrowError, state::EscrowStatus};
use solana_keypair::Keypair;
use solana_signer::Signer;
use solana_transaction::InstructionError;

#[test]
fn release_end_to_end() {
    let mut f = Fixture::new();
    f.initialize();
    let state = f.context.state(f.deal);
    assert_eq!(state.sender, f.sender.pubkey());
    assert_eq!(state.receiver, f.receiver.pubkey());
    assert_eq!(state.mint, f.mint);
    assert_eq!(state.amount, AMOUNT);
    assert_eq!(state.deal_id, DEAL_ID);
    assert_eq!(state.bump, f.deal.bump);
    assert_eq!(state.status, EscrowStatus::Created);
    let vault = f.context.tokens(f.deal.vault);
    assert_eq!(vault.owner, f.deal.state);
    assert_eq!(vault.mint, f.mint);
    assert_eq!(vault.amount, 0);
    assert!(vault.delegate.is_none());
    assert!(vault.close_authority.is_none());
    f.deposit();
    assert_eq!(f.context.state(f.deal).status, EscrowStatus::Funded);
    assert_eq!(f.context.receipt(f.deal).status, EscrowStatus::Funded);
    assert_eq!(f.context.tokens(f.source).amount, SUPPLY - AMOUNT);
    assert_eq!(f.context.tokens(f.deal.vault).amount, AMOUNT);
    assert_eq!(f.context.tokens(f.destination).amount, 0);
    assert_eq!(f.context.supply(f.mint), SUPPLY);
    let rent = f.rent();
    let sender_before = f.context.svm.get_balance(&f.sender.pubkey()).unwrap();
    f.context.send(&[f.release_ix()], &[&f.sender]).unwrap();
    assert_eq!(f.context.tokens(f.source).amount, SUPPLY - AMOUNT);
    assert_eq!(f.context.tokens(f.destination).amount, AMOUNT);
    f.assert_closed(EscrowStatus::Released, rent, sender_before);
}

#[test]
fn cancel_end_to_end() {
    let mut f = Fixture::funded();
    let rent = f.rent();
    let sender_before = f.context.svm.get_balance(&f.sender.pubkey()).unwrap();
    f.context.send(&[f.cancel_ix()], &[&f.sender]).unwrap();
    assert_eq!(f.context.tokens(f.source).amount, SUPPLY);
    assert_eq!(f.context.tokens(f.destination).amount, 0);
    f.assert_closed(EscrowStatus::Cancelled, rent, sender_before);
}

#[test]
fn cancels_created_deal_without_deposit() {
    let mut f = Fixture::new();
    f.initialize();
    let rent = f.rent();
    let sender_before = f.context.svm.get_balance(&f.sender.pubkey()).unwrap();
    f.context.send(&[f.cancel_ix()], &[&f.sender]).unwrap();
    assert_eq!(f.context.tokens(f.source).amount, SUPPLY);
    f.assert_closed(EscrowStatus::Cancelled, rent, sender_before);
}

#[test]
fn rejects_zero_amount() {
    let mut f = Fixture::new();
    f.context.reject(
        f.initialize_ix(0),
        &[&f.sender],
        EscrowError::AmountMustBePositive,
    );
}

#[test]
fn rejects_sender_as_receiver() {
    let mut f = Fixture::new();
    let mut ix = f.initialize_ix(AMOUNT);
    replace(&mut ix, f.receiver.pubkey(), f.sender.pubkey());
    f.context
        .reject(ix, &[&f.sender], EscrowError::SenderEqualsReceiver);
}

#[test]
fn rejects_missing_signatures_in_every_instruction() {
    for action in 0..4 {
        let mut f = Fixture::new();
        if action > 0 {
            f.initialize();
        }
        if action >= 2 {
            f.deposit();
        }
        let mut ix = match action {
            0 => f.initialize_ix(AMOUNT),
            1 => f.deposit_ix(),
            2 => f.release_ix(),
            _ => f.cancel_ix(),
        };
        ix.accounts
            .iter_mut()
            .find(|a| a.pubkey == f.sender.pubkey())
            .unwrap()
            .is_signer = false;
        f.context.reject(ix, &[], ErrorCode::AccountNotSigner);
    }
}

#[test]
fn rejects_wrong_sender_in_deposit_release_and_cancel() {
    for action in 0..3 {
        let mut f = Fixture::new();
        f.initialize();
        if action > 0 {
            f.deposit();
        }
        let mut ix = match action {
            0 => f.deposit_ix(),
            1 => f.release_ix(),
            _ => f.cancel_ix(),
        };
        replace(&mut ix, f.sender.pubkey(), f.receiver.pubkey());
        f.context
            .reject(ix, &[&f.receiver], EscrowError::UnauthorizedSender);
    }
}

#[test]
fn rejects_mint_substitution_in_every_transition() {
    for action in 0..3 {
        let mut f = Fixture::new();
        f.initialize();
        if action > 0 {
            f.deposit();
        }
        let other_mint = f.context.create_mint(&f.authority, token_2022::ID, None);
        let mut ix = match action {
            0 => f.deposit_ix(),
            1 => f.release_ix(),
            _ => f.cancel_ix(),
        };
        replace(&mut ix, f.mint, other_mint);
        f.context.reject(ix, &[&f.sender], EscrowError::InvalidMint);
    }
}

#[test]
fn rejects_receiver_substitution() {
    let mut f = Fixture::funded();
    let other = f.context.wallet();
    let other_ata = f.context.ata(other.pubkey(), f.mint);
    let mut ix = f.release_ix();
    replace(&mut ix, f.receiver.pubkey(), other.pubkey());
    replace(&mut ix, f.destination, other_ata);
    f.context
        .reject(ix, &[&f.sender], EscrowError::InvalidReceiver);
}

#[test]
fn rejects_source_with_wrong_owner() {
    let mut f = Fixture::new();
    f.initialize();
    let mut ix = f.deposit_ix();
    replace(&mut ix, f.source, f.destination);
    f.context
        .reject(ix, &[&f.sender], ErrorCode::ConstraintTokenOwner);
}

#[test]
fn rejects_source_with_wrong_mint() {
    let mut f = Fixture::new();
    f.initialize();
    let mint = f.context.create_mint(&f.authority, token_2022::ID, None);
    let source = f.context.ata(f.sender.pubkey(), mint);
    let mut ix = f.deposit_ix();
    replace(&mut ix, f.source, source);
    f.context
        .reject(ix, &[&f.sender], ErrorCode::ConstraintTokenMint);
}

#[test]
fn rejects_non_ata_receiver_account() {
    let mut f = Fixture::funded();
    let non_ata = f.context.token_account(f.receiver.pubkey(), f.mint);
    let mut ix = f.release_ix();
    replace(&mut ix, f.destination, non_ata);
    f.context
        .reject(ix, &[&f.sender], ErrorCode::ConstraintAssociated);
}

#[test]
fn rejects_non_ata_refund_account() {
    let mut f = Fixture::funded();
    let non_ata = f.context.token_account(f.sender.pubkey(), f.mint);
    for mut ix in [f.cancel_ix(), f.release_ix()] {
        replace(&mut ix, f.source, non_ata);
        f.context
            .reject(ix, &[&f.sender], ErrorCode::ConstraintAssociated);
    }
}

#[test]
fn rejects_insufficient_balance() {
    let mut f = Fixture::new();
    f.context
        .send(&[f.initialize_ix(SUPPLY + 1)], &[&f.sender])
        .unwrap();
    f.context.reject(
        f.deposit_ix(),
        &[&f.sender],
        TokenError::InsufficientFunds as u32,
    );
}

#[test]
fn rejects_deposit_twice() {
    let mut f = Fixture::funded();
    f.context
        .reject(f.deposit_ix(), &[&f.sender], EscrowError::InvalidStatus);
}

#[test]
fn rejects_release_before_funding() {
    let mut f = Fixture::new();
    f.initialize();
    f.context
        .reject(f.release_ix(), &[&f.sender], EscrowError::InvalidStatus);
}

#[test]
fn rejects_repeated_deal_id_while_active_and_after_both_terminal_states() {
    for terminal in 0..3 {
        let mut f = Fixture::funded();
        if terminal > 0 {
            let ix = if terminal == 1 {
                f.release_ix()
            } else {
                f.cancel_ix()
            };
            f.context.send(&[ix], &[&f.sender]).unwrap();
        }
        // System Program запрещает init уже занятого receipt PDA.
        f.context
            .reject(f.initialize_ix(AMOUNT), &[&f.sender], 0u32);
    }
}

#[test]
fn rejects_every_operation_after_release_or_cancel() {
    for release in [true, false] {
        let mut f = Fixture::funded();
        let finish = if release {
            f.release_ix()
        } else {
            f.cancel_ix()
        };
        f.context.send(&[finish], &[&f.sender]).unwrap();
        for ix in [f.deposit_ix(), f.release_ix(), f.cancel_ix()] {
            f.context
                .reject(ix, &[&f.sender], ErrorCode::AccountNotInitialized);
        }
    }
}

#[test]
fn rejects_vault_of_another_deal() {
    let mut f = Fixture::funded();
    let first = f.deal;
    f.deal = Deal::new(f.sender.pubkey(), DEAL_ID + 1);
    f.initialize();
    let other_vault = f.deal.vault;
    f.deal = first;
    for mut ix in [f.release_ix(), f.cancel_ix()] {
        replace(&mut ix, first.vault, other_vault);
        f.context
            .reject(ix, &[&f.sender], ErrorCode::ConstraintSeeds);
    }
}

#[test]
fn rejects_wrong_receipt_pda() {
    let mut f = Fixture::funded();
    let first = f.deal;
    f.deal = Deal::new(f.sender.pubkey(), DEAL_ID + 1);
    f.initialize();
    let other_receipt = f.deal.receipt;
    f.deal = first;
    let mut ix = f.release_ix();
    replace(&mut ix, first.receipt, other_receipt);
    f.context
        .reject(ix, &[&f.sender], ErrorCode::ConstraintSeeds);
}

#[test]
fn rejects_wrong_initialize_seeds() {
    let mut f = Fixture::new();
    let other = Deal::new(f.sender.pubkey(), DEAL_ID + 1);
    for (from, to) in [
        (f.deal.state, other.state),
        (f.deal.receipt, other.receipt),
        (f.deal.vault, other.vault),
    ] {
        let mut ix = f.initialize_ix(AMOUNT);
        replace(&mut ix, from, to);
        f.context
            .reject(ix, &[&f.sender], ErrorCode::ConstraintSeeds);
    }
}

#[test]
fn rejects_corrupted_stored_bump() {
    let mut f = Fixture::funded();
    let mut state = f.context.state(f.deal);
    state.bump = state.bump.wrapping_sub(1);
    let mut account = f.context.svm.get_account(&f.deal.state).unwrap();
    state
        .try_serialize(&mut account.data.as_mut_slice())
        .unwrap();
    f.context.svm.set_account(f.deal.state, account).unwrap();
    f.context
        .reject(f.release_ix(), &[&f.sender], ErrorCode::ConstraintSeeds);
}

#[test]
fn rejects_wrong_token_program() {
    let mut f = Fixture::funded();
    for mut ix in [f.release_ix(), f.cancel_ix()] {
        replace(&mut ix, token_2022::ID, anchor_spl::token::ID);
        // Mint проверяет владельца до ограничения адреса token_program.
        f.context
            .reject(ix, &[&f.sender], ErrorCode::ConstraintMintTokenProgram);
    }
    let mut f = Fixture::new();
    f.initialize();
    let mut ix = f.deposit_ix();
    replace(&mut ix, token_2022::ID, anchor_spl::token::ID);
    f.context
        .reject(ix, &[&f.sender], ErrorCode::ConstraintMintTokenProgram);
}

#[test]
fn rejects_legacy_token_mint_at_initialize() {
    let mut f = Fixture::new();
    f.mint = f
        .context
        .create_mint(&f.authority, anchor_spl::token::ID, None);
    let mut ix = f.initialize_ix(AMOUNT);
    replace(&mut ix, token_2022::ID, anchor_spl::token::ID);
    f.context
        .reject(ix, &[&f.sender], EscrowError::InvalidTokenProgram);
}

#[test]
fn rejects_arbitrary_program_instead_of_token_program() {
    let mut f = Fixture::funded();
    let mut ix = f.release_ix();
    replace(&mut ix, token_2022::ID, anchor_lang::system_program::ID);
    f.context
        .reject(ix, &[&f.sender], ErrorCode::InvalidProgramId);
}

#[test]
fn rejects_wrong_vault_token_authority() {
    let mut f = Fixture::funded();
    // Искусственная фикстура проверяет constraint отдельно от PDA seeds.
    let mut account = f.context.svm.get_account(&f.deal.vault).unwrap();
    let mut token = f.context.tokens(f.deal.vault);
    token.owner = f.sender.pubkey();
    spl_token_2022::state::Account::pack(token, &mut account.data).unwrap();
    f.context.svm.set_account(f.deal.vault, account).unwrap();
    f.context.reject(
        f.release_ix(),
        &[&f.sender],
        ErrorCode::ConstraintTokenOwner,
    );
}

#[test]
fn rejects_wrong_vault_program_owner() {
    let mut f = Fixture::funded();
    let mut account = f.context.svm.get_account(&f.deal.vault).unwrap();
    account.owner = anchor_spl::token::ID;
    f.context.svm.set_account(f.deal.vault, account).unwrap();
    f.context.reject(
        f.release_ix(),
        &[&f.sender],
        ErrorCode::ConstraintTokenTokenProgram,
    );
}

#[test]
fn rejects_freezable_mint() {
    let mut f = Fixture::new();
    f.mint = f
        .context
        .create_mint(&f.authority, token_2022::ID, Some(f.authority.pubkey()));
    f.context.reject(
        f.initialize_ix(AMOUNT),
        &[&f.sender],
        EscrowError::UnsupportedMint,
    );
}

#[test]
fn unsolicited_tokens_do_not_block_release_or_cancel() {
    for release in [true, false] {
        let mut f = Fixture::funded();
        let donation = 13;
        let donor = f.context.wallet();
        let donor_account = f.context.ata(donor.pubkey(), f.mint);
        f.context
            .transfer(&f.sender, f.source, f.mint, donor_account, donation);
        // Сам перевод в vault подписан посторонним владельцем токенов.
        f.context
            .transfer(&donor, donor_account, f.mint, f.deal.vault, donation);
        let rent = f.rent();
        let sender_before = f.context.svm.get_balance(&f.sender.pubkey()).unwrap();
        let finish = if release {
            f.release_ix()
        } else {
            f.cancel_ix()
        };
        f.context.send(&[finish], &[&f.sender]).unwrap();
        let received = if release { AMOUNT } else { 0 };
        assert_eq!(f.context.tokens(f.destination).amount, received);
        assert_eq!(f.context.tokens(f.source).amount, SUPPLY - received);
        f.assert_closed(
            if release {
                EscrowStatus::Released
            } else {
                EscrowStatus::Cancelled
            },
            rent,
            sender_before,
        );
    }
}

#[test]
fn rolls_back_successful_release_when_next_instruction_fails() {
    let mut f = Fixture::funded();
    f.context.reject_transaction(
        &[f.release_ix(), f.cancel_ix()],
        &[&f.sender],
        1,
        InstructionError::Custom(ErrorCode::AccountNotInitialized.into()),
    );
}

#[test]
fn independent_deals_do_not_share_balances() {
    let mut f = Fixture::funded();
    let first = f.deal;
    let before = f.context.svm.get_account(&first.vault);
    f.deal = Deal::new(f.sender.pubkey(), DEAL_ID + 1);
    f.initialize();
    f.deposit();
    f.context.send(&[f.cancel_ix()], &[&f.sender]).unwrap();
    assert_eq!(f.context.svm.get_account(&first.vault), before);
    assert_eq!(f.context.state(first).status, EscrowStatus::Funded);
    f.deal = first;
    f.context.send(&[f.release_ix()], &[&f.sender]).unwrap();
    assert_eq!(f.context.tokens(f.destination).amount, AMOUNT);
    assert_eq!(f.context.tokens(f.source).amount, SUPPLY - AMOUNT);
}

#[test]
fn same_deal_id_is_independent_for_different_senders() {
    let mut f = Fixture::funded();
    let first = f.deal;
    f.sender = f.context.wallet();
    f.deal = Deal::new(f.sender.pubkey(), DEAL_ID);
    assert_ne!(f.deal.state, first.state);
    assert_ne!(f.deal.vault, first.vault);
    assert_ne!(f.deal.receipt, first.receipt);
    f.initialize();
    assert_eq!(f.context.state(first).status, EscrowStatus::Funded);
    assert_eq!(f.context.state(f.deal).status, EscrowStatus::Created);
}

#[test]
fn rejects_mint_extensions() {
    use spl_token_2022::{extension::ExtensionType, state::Mint};
    let mut f = Fixture::new();
    let mint = Keypair::new();
    let size =
        ExtensionType::try_calculate_account_len::<Mint>(&[ExtensionType::MintCloseAuthority])
            .unwrap();
    let instructions = [
        solana_system_interface::instruction::create_account(
            &f.context.payer.pubkey(),
            &mint.pubkey(),
            f.context.svm.minimum_balance_for_rent_exemption(size),
            size as u64,
            &token_2022::ID,
        ),
        spl_token_2022::instruction::initialize_mint_close_authority(
            &token_2022::ID,
            &mint.pubkey(),
            Some(&f.authority.pubkey()),
        )
        .unwrap(),
        spl_token_2022::instruction::initialize_mint2(
            &token_2022::ID,
            &mint.pubkey(),
            &f.authority.pubkey(),
            None,
            common::DECIMALS,
        )
        .unwrap(),
    ];
    f.context.send(&instructions, &[&mint]).unwrap();
    f.mint = mint.pubkey();
    f.context.reject(
        f.initialize_ix(AMOUNT),
        &[&f.sender],
        EscrowError::UnsupportedMint,
    );
}

#[test]
fn direct_transfer_does_not_fund_created_deal_and_can_be_refunded() {
    let mut f = Fixture::new();
    f.initialize();
    f.context
        .transfer(&f.sender, f.source, f.mint, f.deal.vault, AMOUNT);
    f.context
        .reject(f.release_ix(), &[&f.sender], EscrowError::InvalidStatus);
    let rent = f.rent();
    let sender_before = f.context.svm.get_balance(&f.sender.pubkey()).unwrap();
    f.context.send(&[f.cancel_ix()], &[&f.sender]).unwrap();
    assert_eq!(f.context.tokens(f.source).amount, SUPPLY);
    f.assert_closed(EscrowStatus::Cancelled, rent, sender_before);
}

#[test]
fn rolls_back_receiver_transfer_when_refund_cpi_fails() {
    let mut f = Fixture::funded();
    f.context
        .transfer(&f.sender, f.source, f.mint, f.deal.vault, 1);
    // Искусственно замораживаем ATA sender: первый CPI получателю успешен,
    // второй CPI возврата излишка падает. Вся инструкция обязана откатиться.
    let mut account = f.context.svm.get_account(&f.source).unwrap();
    let mut token = f.context.tokens(f.source);
    token.state = spl_token_2022::state::AccountState::Frozen;
    spl_token_2022::state::Account::pack(
        token,
        &mut account.data[..spl_token_2022::state::Account::LEN],
    )
    .unwrap();
    f.context.svm.set_account(f.source, account).unwrap();
    f.context.reject(
        f.release_ix(),
        &[&f.sender],
        TokenError::AccountFrozen as u32,
    );
}

#[test]
fn rejects_funded_vault_shortfall() {
    let mut f = Fixture::funded();
    let mut account = f.context.svm.get_account(&f.deal.vault).unwrap();
    let mut token = f.context.tokens(f.deal.vault);
    token.amount = AMOUNT - 1;
    spl_token_2022::state::Account::pack(token, &mut account.data).unwrap();
    f.context.svm.set_account(f.deal.vault, account).unwrap();
    for ix in [f.release_ix(), f.cancel_ix()] {
        f.context
            .reject(ix, &[&f.sender], EscrowError::InvalidVaultBalance);
    }
}
