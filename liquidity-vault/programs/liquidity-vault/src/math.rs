use anchor_lang::prelude::*;

use crate::error::VaultError;

/// a × b / c в u128; округление вниз — в пользу vault.
pub fn mul_div(a: u64, b: u64, c: u64) -> Result<u64> {
    require!(c > 0, VaultError::MathOverflow);
    let result = (a as u128)
        .checked_mul(b as u128)
        .ok_or(VaultError::MathOverflow)?
        / c as u128;
    u64::try_from(result).map_err(|_| error!(VaultError::MathOverflow))
}

/// Первый вкладчик получает shares 1:1, дальше — вклад ÷ токены vault × все shares.
pub fn shares_for_deposit(amount: u64, total_shares: u64, vault_balance: u64) -> Result<u64> {
    if total_shares == 0 || vault_balance == 0 {
        return Ok(amount);
    }
    mul_div(amount, total_shares, vault_balance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_depositor_gets_shares_one_to_one() {
        assert_eq!(shares_for_deposit(1_000, 0, 0).unwrap(), 1_000);
    }

    #[test]
    fn late_depositor_does_not_take_earned_income() {
        // Пример методички: 1100 токенов на 1000 shares, вклад 110 даёт 100 shares
        assert_eq!(shares_for_deposit(110, 1_000, 1_100).unwrap(), 100);
    }

    #[test]
    fn rounds_down_and_rejects_zero_divisor() {
        assert_eq!(shares_for_deposit(1, 1_000, 1_100).unwrap(), 0);
        assert!(mul_div(1, 1, 0).is_err());
        assert!(mul_div(u64::MAX, u64::MAX, 1).is_err());
    }
}
