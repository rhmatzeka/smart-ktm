#!/usr/bin/env bash
# Deploy Smart KTM ke VPS (default host ssh "vps") di /opt/smart-ktm, tetap jalan walau laptop mati.
#   ./scripts/deploy-vps.sh           update kode + web (data di VPS tidak disentuh)
#   ./scripts/deploy-vps.sh --pindah  sekali saja: pindahkan data + tunnel dari laptop ke VPS
set -euo pipefail
cd "$(dirname "$0")/.."
HOST=${VPS_HOST:-vps}
CF_ID=$(~/.local/bin/cloudflared tunnel list 2>/dev/null | awk '$2=="smart-ktm"{print $1}')
PINDAH=${1:-}

echo "1. Build web + server"
npm --prefix web run build >/dev/null
(cd server && CARGO_BUILD_JOBS=2 cargo build -q)
SEMENTARA=$(mktemp -d)
strip -o "$SEMENTARA/smart-ktm-server" server/target/debug/smart-ktm-server

echo "2. Kirim berkas"
ssh "$HOST" 'id smartktm >/dev/null 2>&1 || useradd --system --home /opt/smart-ktm --shell /usr/sbin/nologin smartktm; mkdir -p /opt/smart-ktm/{bin,web,data}'
rsync -az "$SEMENTARA/smart-ktm-server" "$(readlink -f ~/.foundry/bin/anvil)" "$(readlink -f ~/.foundry/bin/cast)" "$HOST:/opt/smart-ktm/bin/"
rsync -az --delete web/dist/ "$HOST:/opt/smart-ktm/web/"
rm -rf "$SEMENTARA"

if [ "$PINDAH" = "--pindah" ]; then
  echo "3. Pindahkan data dari laptop (layanan laptop dihentikan dulu supaya data utuh)"
  systemctl --user stop smart-ktm-tunnel smart-ktm smart-ktm-anvil 2>/dev/null || true
  rsync -az server/data/ "$HOST:/opt/smart-ktm/data/"
  rsync -az server/.env "$HOST:/opt/smart-ktm/.env"
  ssh "$HOST" 'mkdir -p /etc/cloudflared'
  rsync -az "$HOME/.cloudflared/$CF_ID.json" "$HOST:/etc/cloudflared/smart-ktm.json"
  ssh "$HOST" "cat > /etc/cloudflared/smart-ktm.yml" <<EOF
tunnel: $CF_ID
credentials-file: /etc/cloudflared/smart-ktm.json
ingress:
  - hostname: ktm.rahmateka.my.id
    service: http://localhost:8080
  - service: http_status:404
EOF
fi

echo "4. Layanan systemd di VPS"
ssh "$HOST" 'bash -s' <<'EOF'
set -e
cat > /etc/systemd/system/smart-ktm-anvil.service <<UNIT
[Unit]
Description=Smart KTM - blockchain lokal (Anvil)
[Service]
User=smartktm
ExecStart=/opt/smart-ktm/bin/anvil --port 8545 --state /opt/smart-ktm/data/anvil-state.json --state-interval 30 --silent
Restart=always
[Install]
WantedBy=multi-user.target
UNIT
cat > /etc/systemd/system/smart-ktm.service <<UNIT
[Unit]
Description=Smart KTM - server web + API (:8080)
After=smart-ktm-anvil.service
Wants=smart-ktm-anvil.service
[Service]
User=smartktm
WorkingDirectory=/opt/smart-ktm
Environment=DATA_DIR=/opt/smart-ktm/data WEB_DIR=/opt/smart-ktm/web CAST_BIN=/opt/smart-ktm/bin/cast
ExecStart=/opt/smart-ktm/bin/smart-ktm-server
Restart=always
RestartSec=3
[Install]
WantedBy=multi-user.target
UNIT
cat > /etc/systemd/system/smart-ktm-tunnel.service <<UNIT
[Unit]
Description=Cloudflare Tunnel ktm.rahmateka.my.id -> localhost:8080
After=network-online.target smart-ktm.service
Wants=network-online.target
[Service]
ExecStart=/usr/local/bin/cloudflared tunnel --no-autoupdate --config /etc/cloudflared/smart-ktm.yml run
Restart=always
RestartSec=5
[Install]
WantedBy=multi-user.target
UNIT
chown -R smartktm:smartktm /opt/smart-ktm
chmod 600 /opt/smart-ktm/.env /etc/cloudflared/smart-ktm.json 2>/dev/null || true
systemctl daemon-reload
systemctl enable -q smart-ktm-anvil smart-ktm smart-ktm-tunnel
systemctl restart smart-ktm-anvil smart-ktm smart-ktm-tunnel
sleep 4
systemctl is-active smart-ktm-anvil smart-ktm smart-ktm-tunnel | tr '\n' ' '; echo
curl -s -o /dev/null -w "VPS lokal :8080 -> %{http_code}\n" http://127.0.0.1:8080/
EOF

if [ "$PINDAH" = "--pindah" ]; then
  systemctl --user disable -q smart-ktm-tunnel smart-ktm smart-ktm-anvil 2>/dev/null || true
  echo "Layanan laptop dimatikan; Smart KTM sekarang jalan dari VPS."
fi
