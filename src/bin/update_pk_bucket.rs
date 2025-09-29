use clap::Parser;
use pok3r::aws::{aws_config, get_instance_id, init_s3_client, write_s3};
use pok3r::ed25519::read_ed25519_keypair;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    path: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let keypair = read_ed25519_keypair(args.path)
        .await
        .expect("failed to read keypair");
    let pk_bytes = keypair
        .public()
        .try_into_ed25519()
        .expect("failed to convert pk to ed25519")
        .to_bytes();

    let cfg = aws_config().await;

    let instance_id = get_instance_id().await.expect("failed to get instance id");

    let s3_client = init_s3_client(&cfg);

    write_s3(
        &s3_client,
        "address_book/ed25519",
        format!("{instance_id}.bin").as_str(),
        &pk_bytes,
    )
    .await;
}
