mod common;

use anchor_lang::{error::ErrorCode, prelude::Pubkey, solana_program::program_option::COption};
use anchor_spl::token_2022::spl_token_2022::{error::TokenError, state::AccountState};
use common::{FundedToken, TestContext, DECIMALS, INITIAL_SUPPLY};
use solana_keypair::Keypair;
use solana_level_1_token_starter::error::TokenStarterError;
use solana_signer::Signer;
use solana_transaction::InstructionError;

// Каждый сценарий запускается отдельно для обеих поддерживаемых token-программ.
macro_rules! test_both_programs {
    ($($scenario:ident),+ $(,)?) => {
        $(mod $scenario {
            #[test]
            fn token_2022() { super::$scenario(anchor_spl::token_2022::ID); }

            #[test]
            fn token_program() { super::$scenario(anchor_spl::token::ID); }
        })+
    };
}

test_both_programs!(
    creates_mint,
    creates_associated_token_account,
    mints_tokens,
    transfers_tokens,
    rejects_zero_mint,
    rejects_zero_transfer,
    rejects_wrong_mint_authority,
    rejects_wrong_transfer_authority,
    rejects_mint_without_authority_signature,
    rejects_transfer_without_authority_signature,
    rejects_mint_to_another_mint,
    rejects_transfer_from_another_mint,
    rejects_transfer_to_another_mint,
    rejects_transfer_to_itself,
    rejects_account_with_wrong_token_program,
    rejects_mint_with_wrong_token_program,
    rejects_transfer_with_wrong_token_program,
    rejects_insufficient_balance,
    burns_tokens,
    rejects_zero_burn,
    rejects_wrong_burn_authority,
    rejects_burn_without_authority_signature,
    rejects_burn_from_another_mint,
    rejects_burn_with_wrong_token_program,
    rejects_burn_account_with_wrong_token_program,
    rejects_burn_with_non_token_program,
    rejects_burn_insufficient_balance,
);

fn burns_tokens(token_program: Pubkey) {
    // Decimals не передаются клиентом; проверяем, что CPI берёт их из mint.
    for decimals in [0, DECIMALS, 9] {
        let mut f = FundedToken::with_decimals(token_program, decimals);
        f.context
            .mint_tokens(&f.mint_authority, f.mint, f.destination, 100);
        let destination_before = f.context.svm.get_account(&f.destination);
        // Сначала частичное сжигание, затем весь остаток. Чужие 100 токенов остаются.
        for amount in [1_234_567, INITIAL_SUPPLY - 1_234_567] {
            let balance_before = f.context.token_account(f.source).amount;
            let supply_before = f.context.mint(f.mint).supply;
            let ix = f
                .context
                .burn_instruction(f.owner.pubkey(), f.mint, f.source, amount);
            f.context
                .send(ix, &[&f.owner])
                .expect("burn_tokens must succeed");
            assert_eq!(
                f.context.token_account(f.source).amount,
                balance_before - amount
            );
            assert_eq!(f.context.mint(f.mint).supply, supply_before - amount);
            assert_eq!(
                f.context.svm.get_account(&f.destination),
                destination_before
            );
        }
        assert_eq!(f.context.token_account(f.source).amount, 0);
        assert_eq!(f.context.mint(f.mint).supply, 100);
    }
}

fn rejects_zero_burn(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let ix = f
        .context
        .burn_instruction(f.owner.pubkey(), f.mint, f.source, 0);
    f.context.assert_rejected(
        ix,
        &[&f.owner],
        TokenStarterError::AmountMustBePositive.into(),
        &f.addresses(),
    );
}

fn rejects_wrong_burn_authority(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    // Право выпуска токенов не даёт права сжигать баланс другого владельца.
    let ix = f
        .context
        .burn_instruction(f.mint_authority.pubkey(), f.mint, f.source, 1);
    f.context.assert_rejected(
        ix,
        &[&f.mint_authority],
        ErrorCode::ConstraintTokenOwner.into(),
        &f.addresses(),
    );
}

fn rejects_burn_without_authority_signature(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let mut ix = f
        .context
        .burn_instruction(f.owner.pubkey(), f.mint, f.source, 1);
    ix.accounts
        .iter_mut()
        .find(|meta| meta.pubkey == f.owner.pubkey())
        .unwrap()
        .is_signer = false;
    f.context
        .assert_rejected(ix, &[], ErrorCode::AccountNotSigner.into(), &f.addresses());
}

fn rejects_burn_from_another_mint(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let other_mint = f.context.create_mint(&f.mint_authority, DECIMALS);
    let ix = f
        .context
        .burn_instruction(f.owner.pubkey(), other_mint, f.source, 1);
    f.context.assert_rejected(
        ix,
        &[&f.owner],
        ErrorCode::ConstraintTokenMint.into(),
        &[f.mint, f.source, f.destination, other_mint],
    );
}

fn rejects_burn_with_wrong_token_program(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let mut ix = f
        .context
        .burn_instruction(f.owner.pubkey(), f.mint, f.source, 1);
    ix.accounts
        .iter_mut()
        .find(|meta| meta.pubkey == token_program)
        .unwrap()
        .pubkey = other_token_program(token_program);
    f.context.assert_rejected(
        ix,
        &[&f.owner],
        ErrorCode::ConstraintMintTokenProgram.into(),
        &f.addresses(),
    );
}

fn rejects_burn_account_with_wrong_token_program(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    // Изолируем token::token_program: данные mint/authority оставляем правильными,
    // но в искусственной LiteSVM-фикстуре меняем программу — владельца source.
    let mut source = f.context.svm.get_account(&f.source).unwrap();
    source.owner = other_token_program(token_program);
    f.context.svm.set_account(f.source, source).unwrap();
    let ix = f
        .context
        .burn_instruction(f.owner.pubkey(), f.mint, f.source, 1);
    f.context.assert_rejected(
        ix,
        &[&f.owner],
        ErrorCode::ConstraintTokenTokenProgram.into(),
        &f.addresses(),
    );
}

fn rejects_burn_with_non_token_program(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let mut ix = f
        .context
        .burn_instruction(f.owner.pubkey(), f.mint, f.source, 1);
    ix.accounts
        .iter_mut()
        .find(|meta| meta.pubkey == token_program)
        .unwrap()
        .pubkey = anchor_lang::system_program::ID;
    f.context.assert_rejected(
        ix,
        &[&f.owner],
        ErrorCode::InvalidProgramId.into(),
        &f.addresses(),
    );
}

fn rejects_burn_insufficient_balance(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    // Supply больше запрошенного amount: отказ обязан зависеть от баланса source.
    f.context
        .mint_tokens(&f.mint_authority, f.mint, f.destination, INITIAL_SUPPLY);
    let ix = f
        .context
        .burn_instruction(f.owner.pubkey(), f.mint, f.source, INITIAL_SUPPLY + 1);
    f.context.assert_rejected(
        ix,
        &[&f.owner],
        TokenError::InsufficientFunds as u32,
        &f.addresses(),
    );
}

fn creates_mint(token_program: Pubkey) {
    let mut context = TestContext::new(token_program);
    let authority = Keypair::new();
    for decimals in [0, DECIMALS, 9] {
        let address = context.create_mint(&authority, decimals);
        let mint = context.mint(address);
        assert_eq!(mint.decimals, decimals);
        assert_eq!(mint.mint_authority, COption::Some(authority.pubkey()));
        assert_eq!(mint.freeze_authority, COption::Some(authority.pubkey()));
        assert_eq!(mint.supply, 0);
        assert!(mint.is_initialized);
    }
}

fn creates_associated_token_account(token_program: Pubkey) {
    let mut context = TestContext::new(token_program);
    let authority = Keypair::new();
    let owner = Keypair::new();
    let mint = context.create_mint(&authority, DECIMALS);
    let address = context.create_token_account(owner.pubkey(), mint);
    let account = context.token_account(address);
    assert_eq!(account.owner, owner.pubkey());
    assert_eq!(account.mint, mint);
    assert_eq!(account.amount, 0);
    assert_eq!(account.state, AccountState::Initialized);
    assert_eq!(context.mint(mint).supply, 0);
}

fn mints_tokens(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    assert_eq!(f.context.token_account(f.source).amount, INITIAL_SUPPLY);
    assert_eq!(f.context.mint(f.mint).supply, INITIAL_SUPPLY);
    // Повторный выпуск проверяет сложение с существующим балансом и supply.
    for count in 1..=2 {
        f.context
            .mint_tokens(&f.mint_authority, f.mint, f.source, 1_234_567);
        let expected = INITIAL_SUPPLY + count * 1_234_567;
        assert_eq!(f.context.token_account(f.source).amount, expected);
        assert_eq!(f.context.token_account(f.destination).amount, 0);
        assert_eq!(f.context.mint(f.mint).supply, expected);
    }
}

fn transfers_tokens(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    f.context
        .mint_tokens(&f.mint_authority, f.mint, f.destination, 100);
    let supply = f.context.mint(f.mint).supply;
    let mut transferred = 0;
    // Дробная сумма в base units, затем перевод всего остатка.
    for amount in [1_234_567, INITIAL_SUPPLY - 1_234_567] {
        let ix = f.context.transfer_instruction(
            f.owner.pubkey(),
            f.mint,
            f.source,
            f.destination,
            amount,
        );
        f.context
            .send(ix, &[&f.owner])
            .expect("transfer_tokens must succeed");
        transferred += amount;
        assert_eq!(
            f.context.token_account(f.source).amount,
            INITIAL_SUPPLY - transferred
        );
        assert_eq!(
            f.context.token_account(f.destination).amount,
            100 + transferred
        );
        assert_eq!(f.context.mint(f.mint).supply, supply);
    }
}

fn rejects_zero_mint(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let ix = f
        .context
        .mint_instruction(f.mint_authority.pubkey(), f.mint, f.source, 0);
    f.context.assert_rejected(
        ix,
        &[&f.mint_authority],
        TokenStarterError::AmountMustBePositive.into(),
        &f.addresses(),
    );
}

fn rejects_zero_transfer(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let ix = f
        .context
        .transfer_instruction(f.owner.pubkey(), f.mint, f.source, f.destination, 0);
    f.context.assert_rejected(
        ix,
        &[&f.owner],
        TokenStarterError::AmountMustBePositive.into(),
        &f.addresses(),
    );
}

fn rejects_wrong_mint_authority(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let impostor = Keypair::new();
    let ix = f
        .context
        .mint_instruction(impostor.pubkey(), f.mint, f.source, 1);
    f.context.assert_rejected(
        ix,
        &[&impostor],
        ErrorCode::ConstraintMintMintAuthority.into(),
        &f.addresses(),
    );
}

fn rejects_wrong_transfer_authority(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    // Mint authority не имеет права тратить токены другого владельца.
    let ix = f.context.transfer_instruction(
        f.mint_authority.pubkey(),
        f.mint,
        f.source,
        f.destination,
        1,
    );
    f.context.assert_rejected(
        ix,
        &[&f.mint_authority],
        ErrorCode::ConstraintTokenOwner.into(),
        &f.addresses(),
    );
}

fn rejects_mint_without_authority_signature(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let mut ix = f
        .context
        .mint_instruction(f.mint_authority.pubkey(), f.mint, f.source, 1);
    ix.accounts
        .iter_mut()
        .find(|meta| meta.pubkey == f.mint_authority.pubkey())
        .unwrap()
        .is_signer = false;
    f.context
        .assert_rejected(ix, &[], ErrorCode::AccountNotSigner.into(), &f.addresses());
}

fn rejects_transfer_without_authority_signature(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let mut ix =
        f.context
            .transfer_instruction(f.owner.pubkey(), f.mint, f.source, f.destination, 1);
    ix.accounts
        .iter_mut()
        .find(|meta| meta.pubkey == f.owner.pubkey())
        .unwrap()
        .is_signer = false;
    f.context
        .assert_rejected(ix, &[], ErrorCode::AccountNotSigner.into(), &f.addresses());
}

fn rejects_mint_to_another_mint(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let other_mint = f.context.create_mint(&f.mint_authority, DECIMALS);
    let other_account = f.context.create_token_account(f.owner.pubkey(), other_mint);
    let ix = f
        .context
        .mint_instruction(f.mint_authority.pubkey(), f.mint, other_account, 1);
    f.context.assert_rejected(
        ix,
        &[&f.mint_authority],
        ErrorCode::ConstraintTokenMint.into(),
        &[f.mint, f.source, f.destination, other_mint, other_account],
    );
}

fn rejects_transfer_from_another_mint(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let other_mint = f.context.create_mint(&f.mint_authority, DECIMALS);
    let other_destination = f
        .context
        .create_token_account(Keypair::new().pubkey(), other_mint);
    let ix = f.context.transfer_instruction(
        f.owner.pubkey(),
        other_mint,
        f.source,
        other_destination,
        1,
    );
    f.context.assert_rejected(
        ix,
        &[&f.owner],
        ErrorCode::ConstraintTokenMint.into(),
        &[
            f.mint,
            f.source,
            f.destination,
            other_mint,
            other_destination,
        ],
    );
}

fn rejects_transfer_to_another_mint(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let other_mint = f.context.create_mint(&f.mint_authority, DECIMALS);
    let other_destination = f
        .context
        .create_token_account(Keypair::new().pubkey(), other_mint);
    let ix =
        f.context
            .transfer_instruction(f.owner.pubkey(), f.mint, f.source, other_destination, 1);
    f.context.assert_rejected(
        ix,
        &[&f.owner],
        ErrorCode::ConstraintTokenMint.into(),
        &[
            f.mint,
            f.source,
            f.destination,
            other_mint,
            other_destination,
        ],
    );
}

fn rejects_transfer_to_itself(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let ix = f
        .context
        .transfer_instruction(f.owner.pubkey(), f.mint, f.source, f.source, 1);
    // Anchor 1.1.2 проверяет дубликаты до пользовательского SourceEqualsDestination.
    f.context.assert_rejected(
        ix,
        &[&f.owner],
        ErrorCode::ConstraintDuplicateMutableAccount.into(),
        &f.addresses(),
    );
}

fn other_token_program(token_program: Pubkey) -> Pubkey {
    if token_program == anchor_spl::token_2022::ID {
        anchor_spl::token::ID
    } else {
        anchor_spl::token_2022::ID
    }
}

fn rejects_account_with_wrong_token_program(token_program: Pubkey) {
    let mut context = TestContext::new(token_program);
    let authority = Keypair::new();
    let mint = context.create_mint(&authority, DECIMALS);
    let (account, ix) = context.create_account_instruction(
        Keypair::new().pubkey(),
        mint,
        other_token_program(token_program),
    );
    assert!(context.svm.get_account(&account).is_none());
    // Init ATA выполняется до mint constraints: ошибку возвращает CPI в token-программу.
    context.assert_rejected_with_error(
        ix,
        &[],
        InstructionError::IncorrectProgramId,
        &[mint, account],
    );
}

fn rejects_mint_with_wrong_token_program(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let mut ix = f
        .context
        .mint_instruction(f.mint_authority.pubkey(), f.mint, f.source, 1);
    ix.accounts
        .iter_mut()
        .find(|meta| meta.pubkey == token_program)
        .unwrap()
        .pubkey = other_token_program(token_program);
    f.context.assert_rejected(
        ix,
        &[&f.mint_authority],
        ErrorCode::ConstraintMintTokenProgram.into(),
        &f.addresses(),
    );
}

fn rejects_transfer_with_wrong_token_program(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let mut ix =
        f.context
            .transfer_instruction(f.owner.pubkey(), f.mint, f.source, f.destination, 1);
    ix.accounts
        .iter_mut()
        .find(|meta| meta.pubkey == token_program)
        .unwrap()
        .pubkey = other_token_program(token_program);
    f.context.assert_rejected(
        ix,
        &[&f.owner],
        ErrorCode::ConstraintMintTokenProgram.into(),
        &f.addresses(),
    );
}

fn rejects_insufficient_balance(token_program: Pubkey) {
    let mut f = FundedToken::new(token_program);
    let ix = f.context.transfer_instruction(
        f.owner.pubkey(),
        f.mint,
        f.source,
        f.destination,
        INITIAL_SUPPLY + 1,
    );
    f.context.assert_rejected(
        ix,
        &[&f.owner],
        TokenError::InsufficientFunds as u32,
        &f.addresses(),
    );
}
