use crate::errors::AppError;
use crate::models::{Account, Transaction, TransactionType};

#[derive(Default)]
pub struct Ledger {
    accounts: Vec<Account>,
    transactions: Vec<Transaction>,
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_account(&mut self, name: &str, balance: u64) -> Result<u64, AppError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::EmptyName);
        }

        let id = self.accounts.len() as u64 + 1;
        self.accounts.push(Account {
            id,
            name: name.to_owned(),
            balance,
        });

        if balance > 0 {
            self.transactions.push(Transaction {
                id: self.transactions.len() as u64 + 1,
                from: 0,
                to: id,
                amount: balance,
                transaction_type: TransactionType::InitialFunding,
            });
        }

        Ok(id)
    }

    pub fn get_account(&self, id: u64) -> Option<&Account> {
        self.accounts.iter().find(|account| account.id == id)
    }

    pub fn transfer(&mut self, from: u64, to: u64, amount: u64) -> Result<(), AppError> {
        if amount == 0 {
            return Err(AppError::ZeroAmount);
        }

        let sender_balance = self
            .get_account(from)
            .ok_or(AppError::AccountNotFound(from))?
            .balance;
        let receiver_index = self
            .accounts
            .iter()
            .position(|account| account.id == to)
            .ok_or(AppError::AccountNotFound(to))?;

        if from == to {
            return Err(AppError::SameAccount);
        }

        if sender_balance < amount {
            return Err(AppError::InsufficientBalance {
                available: sender_balance,
                required: amount,
            });
        }

        let receiver_balance = self.accounts[receiver_index]
            .balance
            .checked_add(amount)
            .ok_or(AppError::BalanceOverflow)?;

        let sender_index = self
            .accounts
            .iter()
            .position(|account| account.id == from)
            .ok_or(AppError::AccountNotFound(from))?;
        self.accounts[sender_index].balance = sender_balance - amount;
        self.accounts[receiver_index].balance = receiver_balance;
        self.transactions.push(Transaction {
            id: self.transactions.len() as u64 + 1,
            from,
            to,
            amount,
            transaction_type: TransactionType::Transfer,
        });

        Ok(())
    }

    pub fn print_accounts(&self) {
        if self.accounts.is_empty() {
            println!("No accounts yet.");
            return;
        }

        println!("Accounts:");
        for account in &self.accounts {
            println!(
                "  #{}: {} (balance: {})",
                account.id, account.name, account.balance
            );
        }
    }

    pub fn print_transactions(&self) {
        if self.transactions.is_empty() {
            println!("No transactions yet.");
            return;
        }

        println!("Transactions:");
        for transaction in &self.transactions {
            if transaction.transaction_type == TransactionType::InitialFunding {
                println!(
                    "  #{}: {} -> account #{}: {}",
                    transaction.id,
                    transaction.transaction_type,
                    transaction.to,
                    transaction.amount
                );
            } else {
                println!(
                    "  #{}: {}: account #{} -> account #{}: {}",
                    transaction.id,
                    transaction.transaction_type,
                    transaction.from,
                    transaction.to,
                    transaction.amount
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger_with_two_accounts() -> Ledger {
        let mut ledger = Ledger::new();
        ledger.create_account("Alice", 100).unwrap();
        ledger.create_account("Bob", 10).unwrap();
        ledger
    }

    #[test]
    fn successful_transfer() {
        let mut ledger = ledger_with_two_accounts();

        assert_eq!(ledger.transfer(1, 2, 40), Ok(()));
        assert_eq!(ledger.get_account(1).unwrap().balance, 60);
        assert_eq!(ledger.get_account(2).unwrap().balance, 50);
        assert_eq!(ledger.transactions.len(), 3);
        assert_eq!(
            ledger.transactions.last(),
            Some(&Transaction {
                id: 3,
                from: 1,
                to: 2,
                amount: 40,
                transaction_type: TransactionType::Transfer,
            })
        );
    }

    #[test]
    fn insufficient_balance() {
        let mut ledger = ledger_with_two_accounts();

        assert_eq!(
            ledger.transfer(1, 2, 101),
            Err(AppError::InsufficientBalance {
                available: 100,
                required: 101,
            })
        );
        assert_eq!(ledger.get_account(1).unwrap().balance, 100);
        assert_eq!(ledger.get_account(2).unwrap().balance, 10);
        assert_eq!(ledger.transactions.len(), 2);
    }

    #[test]
    fn sender_not_found() {
        let mut ledger = ledger_with_two_accounts();

        assert_eq!(
            ledger.transfer(99, 2, 10),
            Err(AppError::AccountNotFound(99))
        );
        assert_eq!(ledger.transactions.len(), 2);
    }

    #[test]
    fn receiver_not_found() {
        let mut ledger = ledger_with_two_accounts();

        assert_eq!(
            ledger.transfer(1, 99, 10),
            Err(AppError::AccountNotFound(99))
        );
        assert_eq!(ledger.transactions.len(), 2);
    }

    #[test]
    fn zero_amount_transfer() {
        let mut ledger = ledger_with_two_accounts();

        assert_eq!(ledger.transfer(1, 2, 0), Err(AppError::ZeroAmount));
        assert_eq!(ledger.transactions.len(), 2);
    }

    #[test]
    fn transfer_to_same_account() {
        let mut ledger = ledger_with_two_accounts();

        assert_eq!(ledger.transfer(1, 1, 10), Err(AppError::SameAccount));
        assert_eq!(ledger.get_account(1).unwrap().balance, 100);
    }

    #[test]
    fn receiver_balance_overflow() {
        let mut ledger = Ledger::new();
        ledger.create_account("Alice", 1).unwrap();
        ledger.create_account("Bob", u64::MAX).unwrap();

        assert_eq!(ledger.transfer(1, 2, 1), Err(AppError::BalanceOverflow));
        assert_eq!(ledger.get_account(1).unwrap().balance, 1);
        assert_eq!(ledger.get_account(2).unwrap().balance, u64::MAX);
        assert_eq!(ledger.transactions.len(), 2);
    }

    #[test]
    fn empty_name_is_rejected() {
        let mut ledger = Ledger::new();

        assert_eq!(ledger.create_account("  ", 10), Err(AppError::EmptyName));
        assert_eq!(ledger.get_account(1), None);
    }
}
