use pok3r::{
    aws::{aws_config, get_instance_id, init_s3_client, read_s3},
    ed25519::read_ed25519_keypair,
};

#[tokio::main]
async fn main() {
    let instance_id = get_instance_id().await.expect("failed to get instance id");
    let cfg = aws_config().await;
    let client = init_s3_client(&cfg);
    let pk_s3 = read_s3(
        &client,
        "address_book/ed25519",
        format!("{instance_id}.bin").as_str(),
    )
    .await
    .expect("failed to get pk from s3");
    let pk_local = read_ed25519_keypair("/home/ec2-user/pok3r/keypair".to_owned())
        .await
        .expect("foo")
        .try_into_ed25519()
        .expect("bar")
        .public()
        .to_bytes()
        .to_vec();

    assert_eq!(pk_local, pk_s3, "stored pk not same as s3 pk");

    println!("test done");
}
