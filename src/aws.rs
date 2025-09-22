use ark_serialize::CanonicalDeserialize;
use aws_config::Region;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;

use crate::common::{Ciphertext, DeckProof};
use crate::encoding::serialize_deck_and_proof;

static REGION: Region = Region::from_static("us-east-2");
static BUCKET: &str = "poker-storage-bucket";

pub async fn init_aws_client() -> Client {
    let cfg = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let s3_cfg = aws_sdk_s3::config::Builder::from(&cfg)
        .region(REGION.clone())
        .build();
    Client::from_conf(s3_cfg)
}

fn deck_file_name_aws(deck_no: u64) -> String {
    format!("test_key/deck_{deck_no}.bin")
}

pub async fn save_deck_to_aws(client: &Client, deck_no: u64, deck: Ciphertext, proof: DeckProof) {
    let key = deck_file_name_aws(deck_no);
    let bytes = serialize_deck_and_proof(deck, proof);
    // SAVE DECK TO S3 BUCKET
    match client
        .put_object()
        .bucket(BUCKET)
        .key(&key)
        .body(ByteStream::from(bytes))
        // Later: Setup KMS encryption
        // .server_side_encryption(aws_sdk_s3::types::ServerSideEncryption::AwsKms)
        // .ssekms_key_id("arn:aws:kms:REGION:ACCOUNT:key/KEY_ID")
        .content_type("application/octet-stream")
        .send()
        .await
    {
        Ok(_) => println!("Wrote s3://{BUCKET}/test_key/deck_{deck_no}.bin"),
        Err(_) => println!("failed to write to bucket"),
    }
}

pub async fn read_deck_from_aws(
    client: &Client,
    deck_no: u64,
) -> Result<(Ciphertext, DeckProof), ()> {
    let key = deck_file_name_aws(deck_no);

    let resp = client
        .get_object()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("failed to get response");
    let bytes = resp
        .body
        .collect()
        .await
        .expect("failed to collect body")
        .into_bytes()
        .to_vec();

    CanonicalDeserialize::deserialize_compressed(&*bytes).map_err(|_| ())
}
