use anyhow::Context;
use ark_serialize::CanonicalDeserialize;
use aws_config::{Region, SdkConfig};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client as ClientDb;
use aws_sdk_ec2::Client as ClientEc2;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client as ClientS3;
use aws_sdk_ssm::types::CommandInvocationStatus;
use aws_sdk_ssm::Client as ClientSsm;
use libp2p::identity::ed25519::PublicKey;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::IpAddr;
use std::str::FromStr;

use reqwest::{Client, StatusCode};

use crate::address_book::InstanceId;
use crate::common::{Ciphertext, DeckProof};
use crate::encoding::serialize_deck_and_proof;

static S3_REGION: Region = Region::from_static("us-east-2");
static EC2_REGION: Region = Region::from_static("us-east-2");
static DB_REGION: Region = Region::from_static("us-east-2");
static BUCKET: &str = "poker-storage-bucket";
const IMDS: &str = "http://169.254.169.254/latest";

pub async fn read_deck_counter(client: &ClientDb) -> anyhow::Result<u64> {
    read_dynamo_db(client, "deck_alloc", "pk", "counter", "next").await
}

pub async fn read_dynamo_db(
    client: &ClientDb,
    table_name: &str,
    key: &str,
    val: &str,
    item_: &str,
) -> anyhow::Result<u64> {
    let resp = client
        .get_item()
        .table_name(table_name) // your table name
        .key(key, AttributeValue::S(val.to_string()))
        .send()
        .await?;

    if let Some(item) = resp.item {
        if let Some(attr) = item.get(item_) {
            if let Some(n_str) = attr.as_n().ok() {
                let value = n_str.parse::<u64>()?;
                return Ok(value);
            }
        }
    }

    anyhow::bail!("Counter item not found or missing 'next'")
}

pub async fn aws_config() -> SdkConfig {
    aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await
}

pub fn init_s3_client(cfg: &SdkConfig) -> ClientS3 {
    let s3_cfg = aws_sdk_s3::config::Builder::from(cfg)
        .region(S3_REGION.clone())
        .build();
    ClientS3::from_conf(s3_cfg)
}

pub fn init_ec2_client(cfg: &SdkConfig) -> ClientEc2 {
    let ec2_cfg = aws_sdk_ec2::config::Builder::from(cfg)
        .region(EC2_REGION.clone())
        .build();
    ClientEc2::from_conf(ec2_cfg)
}

pub fn init_db_client(cfg: &SdkConfig) -> ClientDb {
    let db_cfg = aws_sdk_dynamodb::config::Builder::from(cfg)
        .region(DB_REGION.clone())
        .build();
    ClientDb::from_conf(db_cfg)
}

pub fn init_ssm_client(cfg: &SdkConfig) -> ClientSsm {
    let ssm_cfg = aws_sdk_ssm::config::Builder::from(cfg)
        .region(DB_REGION.clone())
        .build();
    ClientSsm::from_conf(ssm_cfg)
}

fn deck_file_name_aws(deck_no: u64) -> String {
    format!("test_key/deck_{deck_no}.bin")
}
async fn deck_file_name_aws_as_mpc(deck_no: u64) -> String {
    let id = get_instance_id().await.expect("failed to get instance id");
    format!("{}/test_key/deck_{}.bin", id, deck_no)
}
async fn deck_file_name_aws_as_mpc_dvrf(deck_no: u64) -> String {
    let id = get_instance_id().await.expect("failed to get instance id");
    format!("{}/dvrf_key/deck_{}.bin", id, deck_no)
}

pub async fn write_s3(client: &ClientS3, dir: &str, file: &str, contents: &[u8]) {
    let key = format!("{}/{file}", dir.trim_end_matches("/"));
    // SAVE DECK TO S3 BUCKET
    match client
        .put_object()
        .bucket(BUCKET)
        .key(&key)
        .body(ByteStream::from(contents.to_vec()))
        // Later: Setup KMS encryption
        // .server_side_encryption(aws_sdk_s3::types::ServerSideEncryption::AwsKms)
        // .ssekms_key_id("arn:aws:kms:REGION:ACCOUNT:key/KEY_ID")
        .content_type("application/octet-stream")
        .send()
        .await
    {
        Ok(_) => println!("Wrote s3://{BUCKET}/{key}"),
        Err(_) => println!("failed to write to bucket"),
    }
}

pub async fn read_s3(client: &ClientS3, dir: &str, file: &str) -> Result<Vec<u8>, String> {
    let key = format!("{}/{file}", dir.trim_end_matches("/"));

    client
        .get_object()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .map_err(|e| format!("failed to get response: {:?}", e))?
        .body
        .collect()
        .await
        .map_err(|e| format!("failed to collect body: {:?}", e))
        .map(|stream| stream.into_bytes().to_vec())
}

/// List files directly under `dir/` (not recursive).
pub async fn list_files_shallow_s3(client: &ClientS3, dir: &str) -> Result<Vec<String>, String> {
    let prefix = if dir.ends_with('/') {
        dir.to_string()
    } else {
        format!("{dir}/")
    };

    let resp = client
        .list_objects_v2()
        .bucket(BUCKET)
        .prefix(&prefix)
        .delimiter("/") // group subfolders; files at this level go to Contents
        .send()
        .await
        .map_err(|e| format!("failed to list {dir}: {:?}", e))?;

    let mut files = Vec::new();

    for obj in resp.contents() {
        if let Some(k) = obj.key() {
            // Keep only files at this level (no extra '/')
            if let Some(name) = k.strip_prefix(&prefix) {
                if !name.is_empty() && !name.contains('/') {
                    files.push(name.to_string());
                }
            }
        }
    }

    Ok(files)
}

/// List immediate "folders" (common prefixes) under `dir` (S3 prefix).
/// Returns names relative to `dir` (no trailing slash).
pub async fn list_dir_s3(client: &ClientS3, dir: &str) -> Result<Vec<String>, String> {
    // Normalize: strip leading/trailing '/', then add trailing '/' if non-empty
    let trimmed = dir.trim_matches('/');
    let prefix = if trimmed.is_empty() {
        String::new()
    } else {
        format!("{}/", trimmed)
    };

    let mut out = BTreeSet::new(); // dedup + sorted
    let mut token: Option<String> = None;

    loop {
        let mut req = client.list_objects_v2().bucket(BUCKET).delimiter("/");

        if !prefix.is_empty() {
            req = req.prefix(&prefix);
        }
        if let Some(t) = &token {
            req = req.continuation_token(t);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("list failed for '{dir}': {:?}", e))?;

        // Folders are in CommonPrefixes
        for cp in resp.common_prefixes() {
            if let Some(full) = cp.prefix() {
                // Strip the parent prefix and trailing '/'
                let name = full
                    .strip_prefix(&prefix)
                    .unwrap_or(full)
                    .trim_end_matches('/');
                if !name.is_empty() {
                    out.insert(name.to_string());
                }
            }
        }

        // Continue if there's another page
        if let Some(next) = resp.next_continuation_token() {
            token = Some(next.to_string());
        } else {
            break;
        }
    }

    Ok(out.into_iter().collect())
}

pub async fn save_deck_s3(client: &ClientS3, deck_no: u64, deck: Ciphertext, proof: DeckProof) {
    let key = deck_file_name_aws(deck_no);
    let bytes = serialize_deck_and_proof(deck, proof);

    let dir = "test_key";
    let file = format!("deck_{deck_no}.bin");
    assert!(key.eq(&format!("{dir}/{file}")));

    write_s3(client, dir, file.as_str(), &bytes).await;
}

pub async fn save_deck_s3_node(
    client: &ClientS3,
    deck_no: u64,
    deck: Ciphertext,
    proof: DeckProof,
) {
    let key = deck_file_name_aws_as_mpc(deck_no).await;
    let bytes = serialize_deck_and_proof(deck, proof);

    let id = get_instance_id().await.expect("failed to get instance id");
    let dir = format!("{id}/test_key");
    let file = format!("deck_{deck_no}.bin");
    assert!(key.eq(&format!("{dir}/{file}")));

    write_s3(client, &dir, &file, &bytes).await;
}

pub async fn save_deck_s3_node_dvrf(
    client: &ClientS3,
    deck_no: u64,
    deck: Ciphertext,
    proof: DeckProof,
) {
    let key = deck_file_name_aws_as_mpc_dvrf(deck_no).await;
    let bytes = serialize_deck_and_proof(deck, proof);

    let id = get_instance_id().await.expect("failed to get instance id");
    let dir = format!("{id}/dvrf_key");
    let file = format!("deck_{deck_no}.bin");
    assert!(key.eq(&format!("{dir}/{file}")));

    write_s3(client, &dir, &file, &bytes).await;
}

pub async fn read_deck_s3(
    client: &ClientS3,
    deck_no: u64,
) -> Result<(Ciphertext, DeckProof), String> {
    let key = deck_file_name_aws(deck_no);

    let dir = "test_key";
    let file = format!("deck_{deck_no}.bin");
    assert!(key.eq(&format!("{dir}/{file}")));
    let bytes = read_s3(client, dir, file.as_str()).await?;

    CanonicalDeserialize::deserialize_compressed(&*bytes)
        .map_err(|e| format!("failed to deserialize deck and proof: {:?}", e))
}

pub async fn read_deck_s3_node(
    client: &ClientS3,
    deck_no: u64,
    id: &str,
) -> Result<(Ciphertext, DeckProof), String> {
    let file = format!("deck_{deck_no}.bin");
    let dir = format!("{}/test_key", id);

    let bytes = read_s3(client, &dir, &file).await?;

    CanonicalDeserialize::deserialize_compressed(&*bytes)
        .map_err(|e| format!("failed to deserialize deck and proof: {:?}", e))
}
pub async fn read_deck_s3_node_dvrf(
    client: &ClientS3,
    deck_no: u64,
    id: &str,
) -> Result<(Ciphertext, DeckProof), String> {
    let file = format!("deck_{deck_no}.bin");
    let dir = format!("{}/dvrf_key", id);

    let bytes = read_s3(client, &dir, &file).await?;

    CanonicalDeserialize::deserialize_compressed(&*bytes)
        .map_err(|e| format!("failed to deserialize deck and proof: {:?}", e))
}

async fn imds_token(http: &Client) -> anyhow::Result<String> {
    let r = http
        .put(format!("{IMDS}/api/token"))
        .header("X-aws-ec2-metadata-token-ttl-seconds", "21600")
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await?;
    if r.status() != StatusCode::OK {
        anyhow::bail!("imds token {}", r.status());
    }
    Ok(r.text().await?)
}

pub async fn get_instance_id() -> anyhow::Result<String> {
    let http = Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()?;
    let token = imds_token(&http).await?;
    let r = http
        .get(format!("{IMDS}/meta-data/instance-id"))
        .header("X-aws-ec2-metadata-token", token)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await?;
    if r.status() != StatusCode::OK {
        anyhow::bail!("imds id {}", r.status());
    }
    Ok(r.text().await?)
}

pub async fn read_pk_map_s3(client: &ClientS3) -> BTreeMap<InstanceId, PublicKey> {
    let dir = "address_book/ed25519";
    let keys = list_files_shallow_s3(client, dir)
        .await
        .expect("failed to get list of pks");

    let mut out = BTreeMap::new();

    for key in keys {
        println!("reading file {dir}/{key}");
        let pk_bytes = read_s3(client, dir, key.as_str())
            .await
            .expect(&format!("failed to load file contents for {key}"));

        let pk = PublicKey::try_from_bytes(&pk_bytes).expect("failed to convert pk to bytes");

        out.insert(key.trim_end_matches(".bin").to_owned(), pk);
    }

    out
}

pub async fn read_ip_map_s3(path: &str) -> anyhow::Result<BTreeMap<InstanceId, IpAddr>> {
    let txt = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("read {}", path))?;
    ip_map_from_str(&txt)
}

fn ip_map_from_str(txt: &str) -> anyhow::Result<BTreeMap<String, IpAddr>> {
    let raw: HashMap<String, String> =
        serde_json::from_str(txt).with_context(|| "parse instance map JSON")?;
    let mut out = BTreeMap::new();
    for (iid, ip_s) in raw {
        let ip =
            IpAddr::from_str(&ip_s).with_context(|| format!("bad IP for {}: {}", iid, ip_s))?;
        out.insert(iid, ip);
    }
    Ok(out)
}

pub async fn kick_workers_multi_range(
    ssm: &ClientSsm,
    starting_deck: u64,
    num_decks: u64,
) -> anyhow::Result<()> {
    let cmd: String = format!("/home/ec2-user/pok3r/compute_n_decks_dvrf --starting-deck {starting_deck} --num-decks {num_decks}");
    kick_workers_multi(ssm, cmd).await
}

pub async fn kick_workers_multi(ssm: &ClientSsm, cmd: String) -> anyhow::Result<()> {
    let tag_key = "role";
    let tag_val = "mpc-node";

    let target = aws_sdk_ssm::types::Target::builder()
        .key(format!("tag:{tag_key}"))
        .values(tag_val)
        .build();

    // Self-verifying command: append + show file state
    //let cmd = format!("bash /home/ec2-user/pok3r/compute_n_decks.sh {starting_deck} {num_decks}");
    //let cmd: String = format!("/home/ec2-user/pok3r/compute_n_decks --starting-deck {starting_deck} --num-decks {num_decks}");

    let resp = ssm
        .send_command()
        .document_name("AWS-RunShellScript")
        .targets(target)
        .parameters("commands", vec![cmd])
        .parameters("executionTimeout", vec!["600".into()])
        .send()
        .await?;

    let command_id = resp
        .command()
        .and_then(|c| c.command_id())
        .ok_or_else(|| anyhow::anyhow!("No command_id returned"))?
        .to_string();

    //println!("Sent command. Command ID: {}", command_id);

    // --- Wait for fan-out: TargetCount > 0 ---
    let mut waited = 0u64;
    let max_wait = 60u64;
    let tick = 2u64;
    let (mut target_count, mut completed, mut errors) = (0i32, 0i32, 0i32);

    loop {
        let lc = ssm.list_commands().command_id(&command_id).send().await?;

        if let Some(cmd) = lc.commands().first() {
            target_count = cmd.target_count();
            completed = cmd.completed_count();
            errors = cmd.error_count();
            let status = cmd.status().map(|s| s.as_str()).expect("Unknown");
            //println!("Command status: {status}, target_count={target_count}, completed={completed}, errors={errors}");
        }

        if target_count > 0 {
            break;
        }
        if waited >= max_wait {
            anyhow::bail!(
                "SSM created command {}, but TargetCount stayed 0 for {}s (tag {}={}).",
                command_id,
                max_wait,
                tag_key,
                tag_val
            );
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(tick)).await;
        waited += tick;
    }

    // --- Now poll invocations until all complete ---
    loop {
        let invs = ssm
            .list_command_invocations()
            .command_id(&command_id)
            .details(true)
            .send()
            .await?
            .command_invocations()
            .to_vec();

        if invs.is_empty() {
            // We know target_count > 0, so give SSM a moment to surface per-instance records.
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            continue;
        }

        let mut all_done = true;
        for inv in invs {
            let iid = inv.instance_id().expect("unknown");
            let status = inv.status().expect("unknown");
            //println!("{} -> {:?}", iid, status);

            /*
            for p in inv.command_plugins() {
                if let Some(out) = p.output() {
                    if !out.is_empty() {
                        println!("--- STDOUT [{}] ---\n{}", iid, out);
                    }
                }
                // Note: standard_error_url is just a URL, not content.
                if let Some(url) = p.standard_error_url() {
                    if !url.is_empty() {
                        println!("--- STDERR URL [{}] ---\n{}", iid, url);
                    }
                }
            }
            */
            match status {
                CommandInvocationStatus::Pending
                | CommandInvocationStatus::InProgress
                | CommandInvocationStatus::Delayed => {
                    all_done = false;
                }
                CommandInvocationStatus::Cancelled
                | CommandInvocationStatus::TimedOut
                | CommandInvocationStatus::Failed
                | CommandInvocationStatus::Cancelling => {
                    anyhow::bail!("SSM command failed on {} with status {:?}", iid, status);
                }
                CommandInvocationStatus::Success => { /* ok */ }
                _ => {
                    all_done = false;
                }
            }
        }

        if all_done {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }

    //println!("All SSM command invocations finished.");

    Ok(())
}
