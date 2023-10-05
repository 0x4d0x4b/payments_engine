use crate::accounting::transactions::{Transaction, TransactionLog, TransactionLogError};
use crate::accounting::{AccountLog, Ledger};
use csv_async::Trim;
use tokio::sync::mpsc::Sender;
use tokio_stream::StreamExt;

pub mod accounting;
mod core_types;

pub async fn read_data(file_path: String, sender: Sender<Transaction>) {
    let mut file = tokio::fs::File::open(&file_path)
        .await
        .expect("Input file does not exist or no permissions to read");
    let mut reader = csv_async::AsyncReaderBuilder::new()
        .trim(Trim::All)
        .create_deserializer(&mut file);
    let mut records = reader.deserialize::<TransactionLog>();
    while let Some(fetched_tx) = records.next().await {
        if let Ok(tx) = fetched_tx
            .map_err(|_err| TransactionLogError::InvalidTxType)
            .and_then(Transaction::try_from)
        {
            sender.send(tx).await.ok();
        }
    }
}

pub async fn output_data(ledger: &Ledger) {
    let account_logs = ledger
        .accounts_iter()
        .map(|(_client_id, user_account)| AccountLog::from(user_account))
        .collect::<Vec<AccountLog>>();

    let mut writer = csv_async::AsyncWriterBuilder::new().create_serializer(tokio::io::stdout());
    for log in account_logs {
        writer.serialize(log).await.ok();
    }
}
