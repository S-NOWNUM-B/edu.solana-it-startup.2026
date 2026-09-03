use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct EscrowState {
    pub sender: Pubkey,
    pub receiver: Pubkey,
    pub mint: Pubkey,
    pub amount: u64,
    pub deal_id: u64,
    pub bump: u8,
    pub status: EscrowStatus,
}

#[derive(AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, PartialEq, Eq, Debug)]
pub enum EscrowStatus {
    Created,
    Funded,
    Released,
    Cancelled,
}

// Постоянная отметка запрещает повторное использование ID после закрытия state.
// PDA содержит sender/deal_id, а статус сохраняет результат завершённой сделки.
#[account]
#[derive(InitSpace)]
pub struct DealReceipt {
    pub status: EscrowStatus,
}
