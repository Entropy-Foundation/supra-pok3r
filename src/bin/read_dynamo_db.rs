use pok3r::aws::{aws_config, init_db_client, read_deck_counter};

#[tokio::main]
async fn main() {
    let cfg = aws_config().await;
    let client = init_db_client(&cfg);

    let counter = read_deck_counter(&client)
        .await
        .expect("failed to read counter");

    println!("counter: {counter}");
}
