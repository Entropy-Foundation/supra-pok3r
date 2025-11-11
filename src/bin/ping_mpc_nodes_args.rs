#![cfg(feature = "aws")]

use clap::Parser;

use aws_sdk_ec2::{types::Filter as Ec2Filter, Client as ClientEc2};
use aws_sdk_ssm::types::InstanceInformationStringFilter;
use aws_sdk_ssm::{
    types::{CommandInvocationStatus, Target},
    Client as SsmClient,
};
use pok3r::{
    aws::{aws_config, init_ec2_client, init_s3_client, init_ssm_client, read_deck_s3_node},
    ed25519::init_ed25519_keypair,
    shuffler::verify_deck_proof,
};
use tokio::time::{sleep, Duration};

const QUERY_INTERVAL_SECS: u64 = 300;
const MIN_STORED: u64 = 100;
const MAX_STORED: u64 = 150;

/// Simple program to greet a person
#[derive(Parser, Debug)]
//#[command(author, version, about, long_about = None)]
struct Args {
    /// starting deck_no
    #[clap(long)]
    starting_deck: u64,

    /// number of decks to compute
    #[clap(long)]
    num_decks: u64,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let cfg = aws_config().await;
    let ssm_client = init_ssm_client(&cfg);
    let ec2_client = init_ec2_client(&cfg);
    let s3_client = init_s3_client(&cfg);

    //let tag_key = "role";
    //let tag_val = "mpc-node";

    let starting_deck = args.starting_deck;
    let num_decks = args.num_decks;
    /*
        let ec2_ids = list_ec2_instances_by_tag(&ec2_client, tag_key, tag_val)
            .await
            .expect("ec2 describe failed");
        println!("EC2 tag-matched instances: {}", ec2_ids.len());

        let ssm_ids = list_ssm_online_by_tag(&ssm_client, tag_key, tag_val)
            .await
            .expect("ssm describe failed");
        println!("SSM Online instances with tag: {}", ssm_ids.len());
    */

    //let cmd: String = format!("/home/ec2-user/pok3r/compute_n_decks --starting-deck {starting_deck} --num-decks {num_decks}");
    //let cmd: String = format!("/home/ec2-user/pok3r/compute_n_decks_dvrf --starting-deck {starting_deck} --num-decks {num_decks}");
    //pok3r::aws::kick_workers_multi(&ssm_client, tag_key, tag_val, starting_deck, num_decks, cmd)
    //    .await
    //    .expect("failed to kick workers");
    pok3r::aws::kick_workers_multi_range(&ssm_client, starting_deck, num_decks).await.expect("failed to kick workers");

    println!("done pushing workers, attempting to read decks from db");

    let id = "i-0e8306fb1260fdb8a";
    let (deck_i, proof_i) = read_deck_s3_node(&s3_client, starting_deck, id)
        .await
        .expect("failed to read deck from s3");
    let (deck_f, proof_f) = read_deck_s3_node(&s3_client, starting_deck + num_decks - 1, id)
        .await
        .expect("failed to read deck from s3");

    println!("read deck from aws");

    /*
    loop {
        let last_pub = 6;
        let last_used = 0;

        let num_stored = last_pub - last_used;

        if num_stored < MIN_STORED {
            call_mpc_nodes(last_pub + 1, last_pub + MAX_STORED, ()).await;
        }

        sleep(Duration::from_secs(QUERY_INTERVAL_SECS)).await;
        //break;
    }
    */
}

/*
pub async fn kick_workers_multi_range(
    ssm: &SsmClient,
    tag_key: &str,
    tag_val: &str,
    starting_deck: u64,
    num_decks: u64,
) -> anyhow::Result<()> {
    let cmd: String = format!("/home/ec2-user/pok3r/compute_n_decks_dvrf --starting-deck {starting_deck} --num-decks {num_decks}");
    kick_workers_multi(ssm, tag_key, tag_val, cmd)
}

pub async fn kick_workers_multi(
    ssm: &SsmClient,
    tag_key: &str,
    tag_val: &str,
    cmd: String,
) -> anyhow::Result<()> {
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

    println!("Sent command. Command ID: {}", command_id);

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
            println!("Command status: {status}, target_count={target_count}, completed={completed}, errors={errors}");
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
        sleep(Duration::from_secs(tick)).await;
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
            sleep(Duration::from_secs(2)).await;
            continue;
        }

        let mut all_done = true;
        for inv in invs {
            let iid = inv.instance_id().expect("unknown");
            let status = inv.status().expect("unknown");
            println!("{} -> {:?}", iid, status);

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
        sleep(Duration::from_secs(2)).await;
    }

    println!("All SSM command invocations finished.");

    Ok(())
}
*/
pub async fn kick_workers(
    ssm: &SsmClient,
    tag_key: &str,
    tag_val: &str,
    _deck_no: u64,
) -> anyhow::Result<()> {
    let target = Target::builder()
        .key(format!("tag:{}", tag_key))
        .values(tag_val)
        //.values("EC2-S3-Access")
        .build();
    println!("{:?}", target);
    let cmd = format!("bash /home/ec2-user/test_script.sh");
    //let cmd = format!("echo \"Hello World\" >> \"/home/ec2-user/hw.txt\"");
    //let cmd = format!("/home/ec2-user/test_script.sh", deck_no);
    let resp = ssm
        .send_command()
        .document_name("AWS-RunShellScript")
        .targets(target)
        .parameters("commands", vec![cmd])
        .send()
        .await?;

    let command_id = resp
        .command()
        .and_then(|c| c.command_id())
        .ok_or_else(|| anyhow::anyhow!("No command_id returned"))?;

    println!("Sent command. Command ID: {}", command_id);

    loop {
        // Get list of invocations for this command
        let invocations = ssm
            .list_command_invocations()
            .command_id(command_id)
            .details(true)
            .send()
            .await?;

        let mut all_done = true;

        let command_invocations = invocations.command_invocations();
        if command_invocations.is_empty() {
            anyhow::bail!("SSM sent the command, but 0 instance invocations were created. Check your tag filter (tag:{}={}).", tag_key, tag_val);
        }
        for inv in command_invocations {
            let instance_id = inv.instance_id().unwrap_or("unknown");
            let status = inv.status().expect("failed to make connection");

            println!("Instance {} → status: {:?}", instance_id, status);

            for plugin in inv.command_plugins() {
                if let Some(output) = plugin.output() {
                    println!("--- STDOUT (instance: {}) ---\n{}", instance_id, output);
                }
                if let Some(stderr) = plugin.standard_error_url() {
                    println!("--- STDERR URL (instance: {}) ---\n{}", instance_id, stderr);
                }
            }

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
                    anyhow::bail!(
                        "SSM command failed on {} with status {:?}",
                        instance_id,
                        status
                    );
                }
                CommandInvocationStatus::Success => {}
                _ => all_done = false,
            }
        }

        if all_done {
            break;
        }

        println!("Waiting 5s before next status check...");
        sleep(Duration::from_secs(5)).await;
    }

    Ok(())
}

pub async fn list_ec2_instances_by_tag(
    ec2: &ClientEc2,
    tag_key: &str,
    tag_val: &str,
) -> anyhow::Result<Vec<String>> {
    let resp = ec2
        .describe_instances()
        .filters(
            Ec2Filter::builder()
                .name(format!("tag:{tag_key}"))
                .values(tag_val)
                .build(),
        )
        .filters(
            Ec2Filter::builder()
                .name("instance-state-name")
                .values("running")
                .build(),
        )
        .send()
        .await?;

    let mut ids = Vec::new();
    for r in resp.reservations() {
        for i in r.instances() {
            if let Some(id) = i.instance_id() {
                let name = i
                    .tags()
                    .iter()
                    .find(|t| t.key() == Some("Name"))
                    .and_then(|t| t.value())
                    .unwrap_or("Unnamed");
                let ip = i.private_ip_address().unwrap_or("?");
                println!("EC2 tag match -> {id}  Name={name}  PrivateIP={ip}");
                ids.push(id.to_string());
            }
        }
    }
    Ok(ids)
}

/// List SSM-managed instances that are Online and have the same tag
pub async fn list_ssm_online_by_tag(
    ssm: &SsmClient,
    tag_key: &str,
    tag_val: &str,
) -> anyhow::Result<Vec<String>> {
    let filter = InstanceInformationStringFilter::builder()
        .key(format!("tag:{tag_key}"))
        .values(tag_val)
        .build();

    let resp = ssm
        .describe_instance_information()
        .filters(filter?)
        .send()
        .await?;

    let mut ids = Vec::new();
    for ii in resp.instance_information_list() {
        let id = ii.instance_id().expect("unknown");
        let ping = ii.ping_status().as_deref().expect("Unknown").clone();
        println!(
            "SSM Online match -> {id}  Ping={ping}  Platform={}",
            ii.platform_name().expect("Unknown")
        );
        ids.push(id.to_string());
    }
    Ok(ids)
}

async fn call_mpc_nodes(lb: u64, ub: u64, stream: ()) {}

async fn deck_listener() {}
