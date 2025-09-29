use pok3r::aws::get_instance_id;

#[tokio::main]
async fn main() {
    let id = get_instance_id().await.expect("failed to get instance id");
    println!("INSTANCE ID = {id}");
}
