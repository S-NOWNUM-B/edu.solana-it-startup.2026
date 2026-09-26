use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub struct Account {
    pub id: u64,
    pub name: String,
    pub balance: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TransactionType {
    InitialFunding,
    Transfer,
}

impl fmt::Display for TransactionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InitialFunding => write!(f, "Initial funding"),
            Self::Transfer => write!(f, "Transfer"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Transaction {
    pub id: u64,
    pub from: u64,
    pub to: u64,
    pub amount: u64,
    pub transaction_type: TransactionType,
}
