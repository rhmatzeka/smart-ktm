#!/usr/bin/env bash
# Online-kan Smart KTM di https://ktm.rahmateka.my.id lewat Cloudflare Tunnel dari laptop ini.
# Memasang 3 layanan systemd user: Anvil (blockchain lokal), server Smart KTM (:8080), dan tunnel.
# Jalankan sekali: ./scripts/deploy-tunnel.sh   (cek: systemctl --user status smart-ktm smart-ktm-tunnel)
set -euo pipefail
cd "$(dirname "$0")/.."
AKAR=$(pwd)
CF=${CLOUDFLARED:-$HOME/.local/bin/cloudflared}
HOST=ktm.rahmateka.my.id
UNIT=$HOME/.config/systemd/user

# 1. Build web + server (binary debug, dependency sudah dioptimasi)
npm --prefix web run build >/dev/null
(cd server && CARGO_BUILD_JOBS=2 cargo build -q)
./scripts/chain-lokal.sh >/dev/null

# 2. Tunnel + DNS
$CF tunnel list 2>/dev/null | awk '{print $2}' | grep -qx smart-ktm || $CF tunnel create smart-ktm
ID=$($CF tunnel list 2>/dev/null | awk '$2=="smart-ktm"{print $1}')
cat > "$HOME/.cloudflared/smart-ktm.yml" <<EOF
tunnel: $ID
credentials-file: $HOME/.cloudflared/$ID.json
ingress:
  - hostname: $HOST
    service: http://localhost:8080
  - service: http_status:404
EOF
$CF tunnel route dns smart-ktm "$HOST" || true

# 3. Layanan systemd user
mkdir -p "$UNIT"
pkill -f "anvil --port 8545" 2>/dev/null || true
cat > "$UNIT/smart-ktm-anvil.service" <<EOF
[Unit]
Description=Smart KTM - blockchain lokal (Anvil)
[Service]
ExecStart=$HOME/.foundry/bin/anvil --port 8545 --state $AKAR/server/data/anvil-state.json --silent
Restart=always
[Install]
WantedBy=default.target
EOF
cat > "$UNIT/smart-ktm.service" <<EOF
[Unit]
Description=Smart KTM - server web + API (:8080)
After=smart-ktm-anvil.service
Wants=smart-ktm-anvil.service
[Service]
WorkingDirectory=$AKAR/server
Environment=CAST_BIN=$HOME/.foundry/bin/cast
ExecStart=$AKAR/server/target/debug/smart-ktm-server
Restart=always
RestartSec=3
[Install]
WantedBy=default.target
EOF
cat > "$UNIT/smart-ktm-tunnel.service" <<EOF
[Unit]
Description=Cloudflare Tunnel $HOST -> localhost:8080
After=network-online.target smart-ktm.service
Wants=network-online.target
[Service]
ExecStart=$CF tunnel --no-autoupdate --config $HOME/.cloudflared/smart-ktm.yml run smart-ktm
Restart=always
RestartSec=5
[Install]
WantedBy=default.target
EOF
pkill -f "target/debug/smart-ktm-server" 2>/dev/null || true
systemctl --user daemon-reload
systemctl --user enable --now smart-ktm-anvil smart-ktm smart-ktm-tunnel
sleep 4
curl -s -o /dev/null -w "lokal :8080 -> %{http_code}\n" http://127.0.0.1:8080/ || true
echo "Selesai. Buka https://$HOST di Chrome HP (DNS baru bisa butuh 1-2 menit)."
