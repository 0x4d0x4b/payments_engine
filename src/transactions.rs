use crate::core_types;
use crate::core_types::{ClientId, TxId};
use rust_decimal::Decimal;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct TransactionLog {
    #[serde(rename = "type")]
    tx_type: String,
    #[serde(rename = "client")]
    client_id: ClientId,
    #[serde(rename = "tx")]
    tx_id: TxId,
    amount: Option<Decimal>,
}

pub enum Transaction {
    Deposit(Deposit),
    Withdrawal(Withdrawal),
    Dispute(Dispute),
    Resolve(Resolve),
    Chargeback(Chargeback),
}

pub struct Deposit {
    client_id: ClientId,
    tx_id: TxId,
    amount: Decimal,
}

pub struct Withdrawal {
    client_id: ClientId,
    tx_id: TxId,
    amount: Decimal,
}

pub struct Dispute {
    client_id: ClientId,
    tx_id: TxId,
}

pub struct Resolve {
    client_id: ClientId,
    tx_id: TxId,
}

pub struct Chargeback {
    client_id: ClientId,
    tx_id: TxId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(4, 4);
    }
}
