use clap::Parser;
use pok3r::ed25519::init_ed25519_keypair;

#[derive(Parser, Debug)]
//#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    path: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    init_ed25519_keypair(args.path)
        .await
        .expect("failed to initialize keypair");
}
