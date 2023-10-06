use crate::accounting::executable_tx::{ExecutableTransaction, TxError};
use crate::core_types::{ClientId, TxId};
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::hash_map::Iter;
use std::collections::HashMap;

mod executable_tx;
pub mod transactions;

struct SubAccount {
    balance: Decimal,
}

impl SubAccount {
    pub fn new() -> Self {
        Self {
            balance: Decimal::ZERO,
        }
    }
}

pub struct UserAccount {
    client_id: ClientId,
    available: SubAccount,
    held: SubAccount,
    locked: bool,
}

impl UserAccount {
    pub fn new(client_id: ClientId) -> Self {
        Self {
            client_id,
            available: SubAccount::new(),
            held: SubAccount::new(),
            locked: false,
        }
    }

    pub fn total(&self) -> Decimal {
        self.available.balance + self.held.balance
    }
}

#[derive(Serialize)]
pub struct AccountLog {
    #[serde(rename = "client")]
    client_id: ClientId,
    available: Decimal,
    held: Decimal,
    total: Decimal,
    locked: bool,
}

impl From<&UserAccount> for AccountLog {
    fn from(user_account: &UserAccount) -> Self {
        let total = user_account.total();
        AccountLog {
            client_id: user_account.client_id,
            available: user_account.available.balance,
            held: user_account.held.balance,
            total,
            locked: user_account.locked,
        }
    }
}

#[derive(PartialEq)]
enum TxState {
    Resolved,
    Disputed,
    ChargedBack,
}

struct DepositState {
    client_id: ClientId,
    tx_id: TxId,
    amount: Decimal,
    state: TxState,
}

impl DepositState {
    fn new(client_id: ClientId, tx_id: TxId, amount: Decimal) -> Self {
        Self {
            client_id,
            tx_id,
            amount,
            state: TxState::Resolved,
        }
    }
}

pub struct Ledger {
    liabilities: SubAccount,
    accounts: HashMap<ClientId, UserAccount>,
    deposit_states: HashMap<TxId, DepositState>,
}

impl Ledger {
    pub fn new() -> Self {
        Self {
            liabilities: SubAccount::new(),
            accounts: HashMap::new(),
            deposit_states: HashMap::new(),
        }
    }

    pub fn execute(&mut self, tx: &impl ExecutableTransaction) -> Result<(), TxError> {
        tx.execute_tx(self)
    }

    pub fn accounts_iter(&self) -> Iter<ClientId, UserAccount> {
        self.accounts.iter()
    }
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

fn make_tx(source: &mut SubAccount, destination: &mut SubAccount, amount: Decimal) {
    source.balance -= amount;
    destination.balance += amount;
}

#[cfg(test)]
mod tests {
    use crate::accounting::executable_tx::TxError;
    use crate::accounting::transactions::{
        Chargeback, Deposit, Dispute, Resolve, Transaction, Withdrawal,
    };
    use crate::accounting::Ledger;
    use crate::core_types::ClientId;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    fn verify_balances(ledger: &Ledger, client_id: ClientId, available: Decimal, held: Decimal) {
        let user_account = ledger.accounts.get(&client_id).unwrap();
        assert_eq!(user_account.available.balance, available);
        assert_eq!(user_account.held.balance, held);
        assert_eq!(user_account.available.balance, available);
    }

    fn verify_liabilities(ledger: &Ledger, liabilities: Decimal) {
        assert_eq!(ledger.liabilities.balance, liabilities);
    }

    fn verify_account_locked(ledger: &Ledger, client_id: ClientId) {
        let locked = ledger.accounts.get(&client_id).unwrap().locked;
        assert!(locked);
    }

    fn verify_account_not_locked(ledger: &Ledger, client_id: ClientId) {
        let locked = ledger.accounts.get(&client_id).unwrap().locked;
        assert!(!locked);
    }

    #[test]
    fn deposits_and_withdrawals_only() {
        let mut ledger = Ledger::new();
        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 1, dec!(1.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(1.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-1.0));

        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(2, 2, dec!(2.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(1.0), dec!(0.0));
        verify_balances(&ledger, 2, dec!(2.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-3.0));

        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 3, dec!(2.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(3.0), dec!(0.0));
        verify_balances(&ledger, 2, dec!(2.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-5.0));

        assert!(ledger
            .execute(&Transaction::Withdrawal(Withdrawal::new(1, 4, dec!(1.5))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(1.5), dec!(0.0));
        verify_balances(&ledger, 2, dec!(2.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-3.5));

        assert_eq!(
            ledger.execute(&Transaction::Withdrawal(Withdrawal::new(2, 5, dec!(3.0)))),
            Err(TxError::InsufficientFunds)
        );
        verify_balances(&ledger, 1, dec!(1.5), dec!(0.0));
        verify_balances(&ledger, 2, dec!(2.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-3.5));

        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(3, 6, dec!(100.0001))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(1.5), dec!(0.0));
        verify_balances(&ledger, 2, dec!(2.0), dec!(0.0));
        verify_balances(&ledger, 3, dec!(100.0001), dec!(0.0));
        verify_liabilities(&ledger, dec!(-103.5001));

        assert!(ledger
            .execute(&Transaction::Withdrawal(Withdrawal::new(
                3,
                7,
                dec!(50.0002)
            )))
            .is_ok());
        verify_balances(&ledger, 1, dec!(1.5), dec!(0.0));
        verify_balances(&ledger, 2, dec!(2.0), dec!(0.0));
        verify_balances(&ledger, 3, dec!(49.9999), dec!(0.0));
        verify_liabilities(&ledger, dec!(-53.4999));
    }

    #[test]
    fn dispute_resolved() {
        let mut ledger = Ledger::new();
        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 1, dec!(50.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-50.0));

        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 2, dec!(30.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(80.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-80.0));

        assert!(ledger
            .execute(&Transaction::Withdrawal(Withdrawal::new(1, 3, dec!(20.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(60.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-60.0));

        assert!(ledger
            .execute(&Transaction::Dispute(Dispute::new(1, 2)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(30.0), dec!(30.0));
        verify_liabilities(&ledger, dec!(-60.0));

        assert!(ledger
            .execute(&Transaction::Resolve(Resolve::new(1, 2)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(60.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-60.0));
    }

    #[test]
    fn already_disputed() {
        let mut ledger = Ledger::new();
        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 1, dec!(50.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-50.0));

        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 2, dec!(30.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(80.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-80.0));

        assert!(ledger
            .execute(&Transaction::Withdrawal(Withdrawal::new(1, 3, dec!(20.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(60.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-60.0));

        assert!(ledger
            .execute(&Transaction::Dispute(Dispute::new(1, 2)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(30.0), dec!(30.0));
        verify_liabilities(&ledger, dec!(-60.0));

        assert_eq!(
            ledger.execute(&Transaction::Dispute(Dispute::new(1, 2))),
            Err(TxError::TxAlreadyDisputed)
        );
        verify_balances(&ledger, 1, dec!(30.0), dec!(30.0));
        verify_liabilities(&ledger, dec!(-60.0));

        assert!(ledger
            .execute(&Transaction::Resolve(Resolve::new(1, 2)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(60.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-60.0));
    }

    #[test]
    fn insufficient_funds_while_disputed() {
        let mut ledger = Ledger::new();
        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 1, dec!(50.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-50.0));

        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 2, dec!(30.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(80.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-80.0));

        assert!(ledger
            .execute(&Transaction::Dispute(Dispute::new(1, 2)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(50.0), dec!(30.0));
        verify_liabilities(&ledger, dec!(-80.0));

        assert_eq!(
            ledger.execute(&Transaction::Withdrawal(Withdrawal::new(1, 3, dec!(60.0)))),
            Err(TxError::InsufficientFunds)
        );
        verify_balances(&ledger, 1, dec!(50.0), dec!(30.0));
        verify_liabilities(&ledger, dec!(-80.0));
    }

    #[test]
    fn client_account_not_found() {
        let mut ledger = Ledger::new();
        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 1, dec!(50.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-50.0));

        assert_eq!(
            ledger.execute(&Transaction::Withdrawal(Withdrawal::new(2, 2, dec!(60.0)))),
            Err(TxError::ClientAccountNotFound)
        );
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-50.0));

        assert_eq!(
            ledger.execute(&Transaction::Dispute(Dispute::new(2, 2))),
            Err(TxError::ClientAccountNotFound)
        );
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-50.0));

        assert_eq!(
            ledger.execute(&Transaction::Resolve(Resolve::new(2, 2))),
            Err(TxError::ClientAccountNotFound)
        );
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-50.0));

        assert_eq!(
            ledger.execute(&Transaction::Chargeback(Chargeback::new(2, 2))),
            Err(TxError::ClientAccountNotFound)
        );
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-50.0));
    }

    #[test]
    fn origin_tx_not_found() {
        let mut ledger = Ledger::new();
        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 1, dec!(50.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-50.0));

        assert!(ledger
            .execute(&Transaction::Withdrawal(Withdrawal::new(1, 2, dec!(30.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(20.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-20.0));

        assert_eq!(
            ledger.execute(&Transaction::Dispute(Dispute::new(1, 2))),
            Err(TxError::OriginTxNotFound)
        );
        verify_balances(&ledger, 1, dec!(20.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-20.0));

        assert_eq!(
            ledger.execute(&Transaction::Resolve(Resolve::new(1, 2))),
            Err(TxError::OriginTxNotFound)
        );
        verify_balances(&ledger, 1, dec!(20.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-20.0));

        assert_eq!(
            ledger.execute(&Transaction::Chargeback(Chargeback::new(1, 2))),
            Err(TxError::OriginTxNotFound)
        );
        verify_balances(&ledger, 1, dec!(20.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-20.0));
    }

    #[test]
    fn tx_not_disputed() {
        let mut ledger = Ledger::new();
        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 1, dec!(50.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-50.0));

        assert!(ledger
            .execute(&Transaction::Withdrawal(Withdrawal::new(1, 2, dec!(30.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(20.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-20.0));

        assert_eq!(
            ledger.execute(&Transaction::Resolve(Resolve::new(1, 1))),
            Err(TxError::TxNotDisputed)
        );
        verify_balances(&ledger, 1, dec!(20.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-20.0));

        assert_eq!(
            ledger.execute(&Transaction::Chargeback(Chargeback::new(1, 1))),
            Err(TxError::TxNotDisputed)
        );
        verify_balances(&ledger, 1, dec!(20.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-20.0));

        assert!(ledger
            .execute(&Transaction::Dispute(Dispute::new(1, 1)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(-30.0), dec!(50.0));
        verify_liabilities(&ledger, dec!(-20.0));

        assert!(ledger
            .execute(&Transaction::Resolve(Resolve::new(1, 1)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(20.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-20.0));

        assert_eq!(
            ledger.execute(&Transaction::Chargeback(Chargeback::new(1, 1))),
            Err(TxError::TxNotDisputed)
        );
        verify_balances(&ledger, 1, dec!(20.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-20.0));

        assert_eq!(
            ledger.execute(&Transaction::Resolve(Resolve::new(1, 1))),
            Err(TxError::TxNotDisputed)
        );
        verify_balances(&ledger, 1, dec!(20.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-20.0));
    }

    #[test]
    fn chargeback() {
        let mut ledger = Ledger::new();
        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 1, dec!(50.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-50.0));

        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 2, dec!(30.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(80.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-80.0));

        assert!(ledger
            .execute(&Transaction::Withdrawal(Withdrawal::new(1, 3, dec!(20.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(60.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-60.0));

        assert!(ledger
            .execute(&Transaction::Dispute(Dispute::new(1, 2)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(30.0), dec!(30.0));
        verify_liabilities(&ledger, dec!(-60.0));

        assert!(ledger
            .execute(&Transaction::Chargeback(Chargeback::new(1, 2)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(30.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-30.0));
        verify_account_locked(&ledger, 1);

        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(2, 4, dec!(60.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(30.0), dec!(0.0));
        verify_balances(&ledger, 2, dec!(60.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-90.0));

        assert!(ledger
            .execute(&Transaction::Withdrawal(Withdrawal::new(2, 5, dec!(20.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(30.0), dec!(0.0));
        verify_balances(&ledger, 2, dec!(40.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-70.0));

        assert!(ledger
            .execute(&Transaction::Dispute(Dispute::new(2, 4)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(30.0), dec!(0.0));
        verify_balances(&ledger, 2, dec!(-20.0), dec!(60.0));
        verify_liabilities(&ledger, dec!(-70.0));

        assert!(ledger
            .execute(&Transaction::Chargeback(Chargeback::new(2, 4)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(30.0), dec!(0.0));
        verify_balances(&ledger, 2, dec!(-20.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-10.0));
        verify_account_locked(&ledger, 1);
        verify_account_locked(&ledger, 2);
    }

    #[test]
    fn ops_after_chargeback() {
        let mut ledger = Ledger::new();
        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 1, dec!(50.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-50.0));

        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 2, dec!(30.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(80.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-80.0));

        assert!(ledger
            .execute(&Transaction::Withdrawal(Withdrawal::new(1, 3, dec!(20.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(60.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-60.0));

        assert!(ledger
            .execute(&Transaction::Dispute(Dispute::new(1, 2)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(30.0), dec!(30.0));
        verify_liabilities(&ledger, dec!(-60.0));

        assert!(ledger
            .execute(&Transaction::Chargeback(Chargeback::new(1, 2)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(30.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-30.0));
        verify_account_locked(&ledger, 1);

        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 4, dec!(40.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(70.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-70.0));
        verify_account_locked(&ledger, 1);

        assert_eq!(
            ledger.execute(&Transaction::Withdrawal(Withdrawal::new(1, 5, dec!(40.0)))),
            Err(TxError::ClientAccountLocked)
        );
        verify_balances(&ledger, 1, dec!(70.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-70.0));
        verify_account_locked(&ledger, 1);

        assert_eq!(
            ledger.execute(&Transaction::Dispute(Dispute::new(1, 2))),
            Err(TxError::TxAlreadyDisputed)
        );
        verify_balances(&ledger, 1, dec!(70.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-70.0));
        verify_account_locked(&ledger, 1);

        assert_eq!(
            ledger.execute(&Transaction::Resolve(Resolve::new(1, 2))),
            Err(TxError::TxNotDisputed)
        );
        verify_balances(&ledger, 1, dec!(70.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-70.0));
        verify_account_locked(&ledger, 1);

        assert_eq!(
            ledger.execute(&Transaction::Chargeback(Chargeback::new(1, 2))),
            Err(TxError::TxNotDisputed)
        );
        verify_balances(&ledger, 1, dec!(70.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-70.0));
        verify_account_locked(&ledger, 1);

        assert!(ledger
            .execute(&Transaction::Dispute(Dispute::new(1, 1)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(20.0), dec!(50.0));
        verify_liabilities(&ledger, dec!(-70.0));
        verify_account_locked(&ledger, 1);

        assert!(ledger
            .execute(&Transaction::Resolve(Resolve::new(1, 1)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(70.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-70.0));
        verify_account_locked(&ledger, 1);

        assert!(ledger
            .execute(&Transaction::Dispute(Dispute::new(1, 1)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(20.0), dec!(50.0));
        verify_liabilities(&ledger, dec!(-70.0));
        verify_account_locked(&ledger, 1);

        assert!(ledger
            .execute(&Transaction::Chargeback(Chargeback::new(1, 1)))
            .is_ok());
        verify_balances(&ledger, 1, dec!(20.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-20.0));
        verify_account_locked(&ledger, 1);
    }

    #[test]
    fn mismatch_client_id_and_tx_id() {
        let mut ledger = Ledger::new();
        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(1, 1, dec!(50.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-50.0));

        assert!(ledger
            .execute(&Transaction::Deposit(Deposit::new(2, 2, dec!(30.0))))
            .is_ok());
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_balances(&ledger, 2, dec!(30.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-80.0));

        assert_eq!(
            ledger.execute(&Transaction::Dispute(Dispute::new(1, 2))),
            Err(TxError::OriginTxNotFound)
        );
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_balances(&ledger, 2, dec!(30.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-80.0));

        assert_eq!(
            ledger.execute(&Transaction::Resolve(Resolve::new(1, 2))),
            Err(TxError::OriginTxNotFound)
        );
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_balances(&ledger, 2, dec!(30.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-80.0));

        assert_eq!(
            ledger.execute(&Transaction::Chargeback(Chargeback::new(1, 2))),
            Err(TxError::OriginTxNotFound)
        );
        verify_balances(&ledger, 1, dec!(50.0), dec!(0.0));
        verify_balances(&ledger, 2, dec!(30.0), dec!(0.0));
        verify_liabilities(&ledger, dec!(-80.0));

        verify_account_not_locked(&ledger, 1);
        verify_account_not_locked(&ledger, 2);
    }
}
