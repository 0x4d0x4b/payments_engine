use payments_engine::accounting::Ledger;

const CHANNEL_SIZE: usize = 4096;

#[tokio::main]
async fn main() {
    let mut args = std::env::args();
    let exec_name = args.next().expect("Exec name should always exist");
    let file_path = match args.next() {
        Some(path) => path,
        None => {
            eprintln!("Usage: {} <input_file_path>", exec_name);
            return;
        }
    };

    let (sender, mut receiver) = tokio::sync::mpsc::channel(CHANNEL_SIZE);

    tokio::spawn(payments_engine::read_data(file_path, sender));

    let mut ledger = Ledger::new();
    while let Some(tx) = receiver.recv().await {
        ledger.execute(&tx).ok();
    }

    payments_engine::output_data(&ledger).await;
}
