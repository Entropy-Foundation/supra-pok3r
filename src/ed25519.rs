use libp2p::identity::{
    self,
    ed25519::{Keypair as KeypairEd25519, SecretKey as SkEd25519},
};
use std::path::Path;

use tokio::{fs::File, io::AsyncWriteExt};

pub static SK_FILE_NAME: &str = "sk_ed25519.bin";
pub static PK_FILE_NAME: &str = "pk_ed25519.bin";

pub async fn init_ed25519_keypair(dir: String) -> Result<identity::Keypair, String> {
    let keypair = KeypairEd25519::generate();
    let sk = keypair.secret();
    let pk = keypair.public();

    let sk_bytes = sk.as_ref();
    let pk_bytes = &pk.to_bytes();

    // 1) Ensure parent directory exists
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("failed to create directory [{dir}] for keypair: {:?}", e))?;

    let path = Path::new(&dir);

    let mut f_sk = File::create(path.join(SK_FILE_NAME))
        .await
        .map_err(|e| format!("failed to make sk file: {:?}", e))?;
    let mut f_pk = File::create(path.join(PK_FILE_NAME))
        .await
        .map_err(|e| format!("failed to make pk file: {:?}", e))?;

    f_sk.write(sk_bytes)
        .await
        .map_err(|e| format!("failed to write to sk file: {:?}", e))?;
    f_pk.write(pk_bytes)
        .await
        .map_err(|e| format!("failed to write to pk file: {:?}", e))?;

    println!(
        "Keypair generated and saved in {dir}\nPK = 0x{}",
        hex::encode(pk_bytes)
    );
    Ok(keypair.into())
}

pub async fn read_ed25519_keypair(dir: String) -> Result<identity::Keypair, String> {
    let sk_bytes = tokio::fs::read(format!("{dir}/{SK_FILE_NAME}"))
        .await
        .map_err(|e| format!("failed to read sk from file: {:?}", e))?;
    let sk = SkEd25519::try_from_bytes(sk_bytes)
        .map_err(|e| format!("failed to deserialize keypair: {:?}", e))?;

    let keypair: KeypairEd25519 = sk.into();

    println!(
        "Keypair read from {dir}\nPK = 0x{}",
        hex::encode(keypair.public().to_bytes())
    );
    Ok(keypair.into())
}

pub fn generate_ed25519_from_seed(secret_key_seed: u8) -> identity::Keypair {
    // for now we are using a single byte as the seed
    // this is not secure obviously,
    // but we are not using it to make life easy
    let mut bytes = [0u8; 32];
    bytes[0] = secret_key_seed;

    identity::Keypair::ed25519_from_bytes(bytes).expect("only errors on wrong length")
}
