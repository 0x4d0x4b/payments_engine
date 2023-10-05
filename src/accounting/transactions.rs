use crate::accounting::executable_tx::{ExecutableTransaction, TxError};
use crate::accounting::{make_tx, DepositState, Ledger, TxState, UserAccount};
use crate::core_types::{ClientId, TxId};
use enum_dispatch::enum_dispatch;
use rust_decimal::Decimal;
use serde::Deserialize;

const DEPOSIT_TAG: &str = "deposit";
const WITHDRAWAL_TAG: &str = "withdrawal";
const DISPUTE_TAG: &str = "dispute";
const RESOLVE_TAG: &str = "resolve";
const CHARGEBACK_TAG: &str = "chargeback";

#[derive(Deserialize, Debug, PartialEq)]
pub struct TransactionLog {
    #[serde(rename = "type")]
    tx_type: String,
    #[serde(rename = "client")]
    client_id: ClientId,
    #[serde(rename = "tx")]
    tx_id: TxId,
    #[serde(default, deserialize_with = "csv::invalid_option")]
    amount: Option<Decimal>,
}

#[enum_dispatch(ExecutableTransaction)]
#[derive(Debug, PartialEq)]
pub enum Transaction {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

#[derive(Debug, PartialEq)]
pub struct Deposit {
    client_id: ClientId,
    tx_id: TxId,
    amount: Decimal,
}

impl ExecutableTransaction for Deposit {
    fn execute_tx(&self, ledger: &mut Ledger) -> Result<(), TxError> {
        let client_account = ledger
            .accounts
            .entry(self.client_id)
            .or_insert(UserAccount::new(self.client_id));
        make_tx(
            &mut ledger.liabilities,
            &mut client_account.available,
            self.amount,
        );
        ledger.deposit_states.insert(
            self.tx_id,
            DepositState::new(self.client_id, self.tx_id, self.amount),
        );
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub struct Withdrawal {
    client_id: ClientId,
    tx_id: TxId,
    amount: Decimal,
}

impl ExecutableTransaction for Withdrawal {
    fn execute_tx(&self, ledger: &mut Ledger) -> Result<(), TxError> {
        if let Some(client_account) = ledger.accounts.get_mut(&self.client_id) {
            if client_account.locked {
                return Err(TxError::ClientAccountLocked);
            }
            if client_account.available.balance < self.amount {
                return Err(TxError::InsufficientFunds);
            }
            make_tx(
                &mut client_account.available,
                &mut ledger.liabilities,
                self.amount,
            );
            Ok(())
        } else {
            Err(TxError::ClientAccountNotFound)
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Dispute {
    client_id: ClientId,
    tx_id: TxId,
}

impl ExecutableTransaction for Dispute {
    fn execute_tx(&self, ledger: &mut Ledger) -> Result<(), TxError> {
        if let Some(client_account) = ledger.accounts.get_mut(&self.client_id) {
            if client_account.locked {
                return Err(TxError::ClientAccountLocked);
            }
            if let Some(deposit) = ledger.deposit_states.get_mut(&self.tx_id) {
                if deposit.tx_id != self.tx_id || deposit.client_id != self.client_id {
                    return Err(TxError::OriginTxNotFound);
                }
                if deposit.state != TxState::Resolved {
                    return Err(TxError::TxAlreadyDisputed);
                }
                deposit.state = TxState::Disputed;
                make_tx(
                    &mut client_account.available,
                    &mut client_account.held,
                    deposit.amount,
                );
                Ok(())
            } else {
                Err(TxError::OriginTxNotFound)
            }
        } else {
            Err(TxError::ClientAccountNotFound)
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Resolve {
    client_id: ClientId,
    tx_id: TxId,
}

impl ExecutableTransaction for Resolve {
    fn execute_tx(&self, ledger: &mut Ledger) -> Result<(), TxError> {
        if let Some(client_account) = ledger.accounts.get_mut(&self.client_id) {
            if client_account.locked {
                return Err(TxError::ClientAccountLocked);
            }
            if let Some(deposit) = ledger.deposit_states.get_mut(&self.tx_id) {
                if deposit.tx_id != self.tx_id || deposit.client_id != self.client_id {
                    return Err(TxError::OriginTxNotFound);
                }
                if deposit.state != TxState::Disputed {
                    return Err(TxError::TxNotDisputed);
                }
                deposit.state = TxState::Resolved;
                make_tx(
                    &mut client_account.held,
                    &mut client_account.available,
                    deposit.amount,
                );
                Ok(())
            } else {
                Err(TxError::OriginTxNotFound)
            }
        } else {
            Err(TxError::ClientAccountNotFound)
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Chargeback {
    client_id: ClientId,
    tx_id: TxId,
}

impl ExecutableTransaction for Chargeback {
    fn execute_tx(&self, ledger: &mut Ledger) -> Result<(), TxError> {
        if let Some(client_account) = ledger.accounts.get_mut(&self.client_id) {
            if client_account.locked {
                return Err(TxError::ClientAccountLocked);
            }
            if let Some(deposit) = ledger.deposit_states.get_mut(&self.tx_id) {
                if deposit.tx_id != self.tx_id || deposit.client_id != self.client_id {
                    return Err(TxError::OriginTxNotFound);
                }
                if deposit.state != TxState::Disputed {
                    return Err(TxError::TxNotDisputed);
                }
                deposit.state = TxState::ChargedBack;
                client_account.locked = true;
                make_tx(
                    &mut client_account.held,
                    &mut ledger.liabilities,
                    deposit.amount,
                );
                Ok(())
            } else {
                Err(TxError::OriginTxNotFound)
            }
        } else {
            Err(TxError::ClientAccountNotFound)
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum TransactionLogError {
    InvalidTxType,
    MissingAmount,
}

impl TryFrom<TransactionLog> for Transaction {
    type Error = TransactionLogError;

    fn try_from(log: TransactionLog) -> Result<Self, Self::Error> {
        let TransactionLog {
            tx_type,
            client_id,
            tx_id,
            amount,
        } = log;
        match tx_type.as_str() {
            DEPOSIT_TAG => {
                let amount = amount.ok_or(TransactionLogError::MissingAmount)?;
                Ok(Transaction::Deposit(Deposit {
                    client_id,
                    tx_id,
                    amount,
                }))
            }
            WITHDRAWAL_TAG => {
                let amount = log.amount.ok_or(TransactionLogError::MissingAmount)?;
                Ok(Transaction::Withdrawal(Withdrawal {
                    client_id,
                    tx_id,
                    amount,
                }))
            }
            DISPUTE_TAG => Ok(Transaction::Dispute(Dispute { client_id, tx_id })),
            RESOLVE_TAG => Ok(Transaction::Resolve(Resolve { client_id, tx_id })),
            CHARGEBACK_TAG => Ok(Transaction::Chargeback(Chargeback { client_id, tx_id })),
            _ => Err(TransactionLogError::InvalidTxType),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use csv::{ReaderBuilder, Trim};
    use rust_decimal_macros::dec;

    #[test]
    fn deserialize_transaction_log() {
        let data = r#"
            type, client, tx, amount
            deposit, 1, 1, 1.0
            deposit, 2, 2, 2.0
            deposit, 1, 3, 2.0
            withdrawal, 1, 4, 1.5
            withdrawal, 2, 5, 3.0
            dispute, 1, 3,
            resolve, 1, 3,
            chargeback, 1, 1,
        "#;

        let mut reader = ReaderBuilder::new()
            .trim(Trim::All)
            .from_reader(data.as_bytes());

        let mut reader_iter = reader.deserialize::<TransactionLog>();

        let deposit1 = reader_iter.next().unwrap().unwrap();
        assert_eq!(
            deposit1,
            TransactionLog {
                tx_type: DEPOSIT_TAG.to_string(),
                client_id: 1,
                tx_id: 1,
                amount: Some(dec!(1.0)),
            }
        );

        let deposit2 = reader_iter.next().unwrap().unwrap();
        assert_eq!(
            deposit2,
            TransactionLog {
                tx_type: "deposit".to_string(),
                client_id: 2,
                tx_id: 2,
                amount: Some(dec!(2.0)),
            }
        );

        let deposit3 = reader_iter.next().unwrap().unwrap();
        assert_eq!(
            deposit3,
            TransactionLog {
                tx_type: DEPOSIT_TAG.to_string(),
                client_id: 1,
                tx_id: 3,
                amount: Some(dec!(2.0)),
            }
        );

        let withdrawal1 = reader_iter.next().unwrap().unwrap();
        assert_eq!(
            withdrawal1,
            TransactionLog {
                tx_type: WITHDRAWAL_TAG.to_string(),
                client_id: 1,
                tx_id: 4,
                amount: Some(dec!(1.5)),
            }
        );

        let withdrawal2 = reader_iter.next().unwrap().unwrap();
        assert_eq!(
            withdrawal2,
            TransactionLog {
                tx_type: WITHDRAWAL_TAG.to_string(),
                client_id: 2,
                tx_id: 5,
                amount: Some(dec!(3.0)),
            }
        );

        let dispute = reader_iter.next().unwrap().unwrap();
        assert_eq!(
            dispute,
            TransactionLog {
                tx_type: DISPUTE_TAG.to_string(),
                client_id: 1,
                tx_id: 3,
                amount: None,
            }
        );

        let resolve = reader_iter.next().unwrap().unwrap();
        assert_eq!(
            resolve,
            TransactionLog {
                tx_type: RESOLVE_TAG.to_string(),
                client_id: 1,
                tx_id: 3,
                amount: None,
            }
        );

        let chargeback = reader_iter.next().unwrap().unwrap();
        assert_eq!(
            chargeback,
            TransactionLog {
                tx_type: CHARGEBACK_TAG.to_string(),
                client_id: 1,
                tx_id: 1,
                amount: None,
            }
        );
    }

    #[test]
    fn convert_into_transaction() {
        let deposit1 = Transaction::try_from(TransactionLog {
            tx_type: DEPOSIT_TAG.to_string(),
            client_id: 1,
            tx_id: 1,
            amount: Some(dec!(1.0)),
        });

        assert_eq!(
            deposit1,
            Ok(Transaction::Deposit(Deposit {
                client_id: 1,
                tx_id: 1,
                amount: dec!(1.0),
            }))
        );

        let deposit2 = Transaction::try_from(TransactionLog {
            tx_type: "deposit".to_string(),
            client_id: 2,
            tx_id: 2,
            amount: Some(dec!(2.0)),
        });

        assert_eq!(
            deposit2,
            Ok(Transaction::Deposit(Deposit {
                client_id: 2,
                tx_id: 2,
                amount: dec!(2.0),
            }))
        );

        let deposit3 = Transaction::try_from(TransactionLog {
            tx_type: DEPOSIT_TAG.to_string(),
            client_id: 1,
            tx_id: 3,
            amount: Some(dec!(2.0)),
        });

        assert_eq!(
            deposit3,
            Ok(Transaction::Deposit(Deposit {
                client_id: 1,
                tx_id: 3,
                amount: dec!(2.0),
            }))
        );

        let withdrawal1 = Transaction::try_from(TransactionLog {
            tx_type: WITHDRAWAL_TAG.to_string(),
            client_id: 1,
            tx_id: 4,
            amount: Some(dec!(1.5)),
        });

        assert_eq!(
            withdrawal1,
            Ok(Transaction::Withdrawal(Withdrawal {
                client_id: 1,
                tx_id: 4,
                amount: dec!(1.5),
            }))
        );

        let withdrawal2 = Transaction::try_from(TransactionLog {
            tx_type: WITHDRAWAL_TAG.to_string(),
            client_id: 2,
            tx_id: 5,
            amount: Some(dec!(3.0)),
        });

        assert_eq!(
            withdrawal2,
            Ok(Transaction::Withdrawal(Withdrawal {
                client_id: 2,
                tx_id: 5,
                amount: dec!(3.0),
            }))
        );

        let dispute = Transaction::try_from(TransactionLog {
            tx_type: DISPUTE_TAG.to_string(),
            client_id: 1,
            tx_id: 3,
            amount: None,
        });

        assert_eq!(
            dispute,
            Ok(Transaction::Dispute(Dispute {
                client_id: 1,
                tx_id: 3
            }))
        );

        let resolve = Transaction::try_from(TransactionLog {
            tx_type: RESOLVE_TAG.to_string(),
            client_id: 1,
            tx_id: 3,
            amount: None,
        });

        assert_eq!(
            resolve,
            Ok(Transaction::Resolve(Resolve {
                client_id: 1,
                tx_id: 3
            }))
        );

        let chargeback = Transaction::try_from(TransactionLog {
            tx_type: CHARGEBACK_TAG.to_string(),
            client_id: 1,
            tx_id: 1,
            amount: None,
        });

        assert_eq!(
            chargeback,
            Ok(Transaction::Chargeback(Chargeback {
                client_id: 1,
                tx_id: 1
            }))
        );

        let deposit_no_amount = Transaction::try_from(TransactionLog {
            tx_type: DEPOSIT_TAG.to_string(),
            client_id: 1,
            tx_id: 1,
            amount: None,
        });

        assert_eq!(deposit_no_amount, Err(TransactionLogError::MissingAmount));

        let withdrawal_no_amount = Transaction::try_from(TransactionLog {
            tx_type: WITHDRAWAL_TAG.to_string(),
            client_id: 2,
            tx_id: 5,
            amount: None,
        });

        assert_eq!(
            withdrawal_no_amount,
            Err(TransactionLogError::MissingAmount)
        );

        let invalid_log = Transaction::try_from(TransactionLog {
            tx_type: "Abcd".to_string(),
            client_id: 2,
            tx_id: 5,
            amount: Some(dec!(35.0)),
        });

        assert_eq!(invalid_log, Err(TransactionLogError::InvalidTxType));
    }
}
