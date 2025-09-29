use pok3r::aws::{aws_config, init_s3_client, list_files_shallow_s3};

#[tokio::main]
async fn main() {
    let cfg = aws_config().await;
    let client = init_s3_client(&cfg);
    let files = list_files_shallow_s3(&client, "address_book/ed25519/")
        .await
        .expect("failed to read files");

    for f in files {
        println!("FILENAME = {:?}", f);
    }

    println!("done printing files");
}
