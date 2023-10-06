# Assumptions
* Input file is formatted correctly. Any ill-formatted line will result in
deserialization error and will be ignored.
* There is at least one transaction in the input file.
  If not, output will be empty, i.e. it **will not** contain a csv header
* Client IDs and Transaction IDs are globally unique
* New client accounts are created only upon `Deposit` 
to avoid creation of empty records
* The only transaction subject to dispute is `Deposit`.
* `Withdrawal`s are not disputable because raising a dispute on
`Withdrawal` would increase available balance and create vulnerability
allowing double spending
* `Withdrawal`s are not allowed from locked (frozen) account,
but other operations are permitted
* Reasoning on above point is that `Deposit`, `Dispute`, `Resolve`
and `Chargeback` transactions are treated as authoritative transactions
coming from a trusted 3rd-party vendors and should be applied to the
client's account if relevant client ID exists, transaction is in a relevant
state and was not subject to `Chargeback` previously. However, `Withdrawal`s are
initiated on our platform, and we can decide whether to process them or not.
They are not processed when account is locked or there are insufficient
funds on the account.
* `Dispute` can be raised on `Resolve`d transactions, which means
multiple `Dispute-Resolve` cycles are possible on the same transaction, but
`Dispute-Chargeback` is final and no further `Dispute`s are possible on
the transaction

# Implementation
This simple payment engine utilizes type system to ensure correctness.
Each transaction is parsed into an `enum Transaction` and nested, concrete
data types corresponding to each of the variants: `Deposit`, `Withdrawal`,
`Dispute`, `Resolve` and `Chargeback`. Each of those data types implements
`trait ExecutableTransaction` adding polymorphic behaviour to the type
system through `fn execute_tx(ledger: &Ledger) -> Result<(), TxError>`
allowing to capture detailed errors when transaction is
rejected by a `Ledger`.

Numeric values representing account balances are of `Decimal` type
from `rust_decimal` crate.

The engine features double entry accounting through internal `liabilities`
account. That means that at each point the sum of client's total balances
and liabilities is equal to zero. Or in the other words the sum of client's
funds on the platform is equal to liabilities with opposite sign.
This property is being verified throughout all unit tests.

Input data is read asynchronously with the help of `tokio` and
`csv_async` crates. Reading takes place on a separate task.
Parsed transactions are passed into `main` task through a channel.

In the main task each of the received transactions is applied to the `Ledger`.
When channel is closed, that is, entire file is read, the output is generated
and published on `stdout`

# Testing

A set of unit tests to verify parsing and operation have been implemented.
Please run
```shell
cargo test
```