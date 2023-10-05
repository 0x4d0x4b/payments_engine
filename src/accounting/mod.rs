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
