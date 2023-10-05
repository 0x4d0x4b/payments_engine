use crate::accounting::Ledger;
use enum_dispatch::enum_dispatch;

pub enum TxError {
    ClientAccountLocked,
    InsufficientFunds,
    ClientAccountNotFound,
    OriginTxNotFound,
    TxAlreadyDisputed,
    TxNotDisputed,
}

#[enum_dispatch]
pub trait ExecutableTransaction {
    fn execute_tx(&self, ledger: &mut Ledger) -> Result<(), TxError>;
}
