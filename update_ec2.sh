KEY="${KEY:-$HOME/.ssh/MPC-ed25519.pem}"
TARGET_OS="x86_64-unknown-linux-musl"
BIN_NAME="${1:-pok3r}"
LOCAL_BIN="target/${TARGET_OS}/release/${BIN_NAME}"
REMOTE_DIR="/home/ec2-user/pok3r"
REMOTE_BIN="${REMOTE_DIR}/${BIN_NAME}"
USER="ec2-user"

ip0="18.221.251.21"
ip1="18.118.86.228"
ip2="3.16.43.247"
ip3="3.16.183.255"

HOSTS=("$ip0" "$ip1" "$ip2" "$ip3")

echo "==> Building pok3r for ec2"
cargo build-ec2

# ensure binary exists
if [[ ! -f "${LOCAL_BIN}" ]]; then
  echo "Build output not found at ${LOCAL_BIN}" >&2
  exit 1
fi

echo "==> Shipping ${LOCAL_BIN} to ${#HOSTS[@]} hosts…"
for host in "${HOSTS[@]}"; do
  (
    echo "-- ${host}: copying binary"
    scp -o StrictHostKeyChecking=accept-new -i "${KEY}" "${LOCAL_BIN}" "${USER}@${host}":"${REMOTE_BIN}"
    echo "-- ${host}: done"
  ) &
done

wait
echo "==> All copies complete."