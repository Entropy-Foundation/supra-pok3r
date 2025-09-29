use pok3r::aws::read_ip_map_s3;

#[tokio::main]
async fn main() {
    let ip_map = read_ip_map_s3("/home/ec2-user/pok3r/private-ips.json")
        .await
        .expect("failed to get ip map");

    for (id, ip) in ip_map {
        println!("ID = {id}\nIP = {:?}\n", ip);
    }
}
