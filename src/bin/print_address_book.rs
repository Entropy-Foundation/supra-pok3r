use pok3r::aws::{aws_config, get_instance_id, init_s3_client, read_pk_map_s3};

#[tokio::main]
async fn main() {
    let cfg = aws_config().await;
    let client = init_s3_client(&cfg);

    let pks = read_pk_map_s3(&client).await;
    let my_id = get_instance_id().await.expect("failed to get instance id");
    for (id, pk) in pks {
        if id.contains(&my_id) {
            print!("MY ID: ")
        } else {
            print!("       ")
        }
        println!("({id}, {:?})", pk);
    }
}
