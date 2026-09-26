use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum AppError {
    EmptyName,
    AccountNotFound(u64),
    ZeroAmount,
    SameAccount,
    InsufficientBalance { available: u64, required: u64 },
    BalanceOverflow,
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => write!(f, "Account name cannot be empty"),
            Self::AccountNotFound(id) => write!(f, "Account {id} not found"),
            Self::ZeroAmount => write!(f, "Transfer amount must be greater than zero"),
            Self::SameAccount => write!(f, "Sender and receiver must be different accounts"),
            Self::InsufficientBalance {
                available,
                required,
            } => write!(
                f,
                "Insufficient balance: available {available}, required {required}"
            ),
            Self::BalanceOverflow => write!(f, "Receiver balance would overflow"),
        }
    }
}

impl std::error::Error for AppError {}
