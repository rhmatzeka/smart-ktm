#!/usr/bin/env bash
# Blockchain lokal (Anvil) untuk latihan dan demo tanpa faucet.
# Kunci di bawah adalah akun uji bawaan Anvil: publik, jangan dipakai di jaringan sungguhan.
set -euo pipefail
cd "$(dirname "$0")/.."
RPC=http://127.0.0.1:8545
KEY=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
ENV=server/.env
mkdir -p server/data
[ -f "$ENV" ] || cp server/.env.contoh "$ENV"

if ! cast chain-id --rpc-url $RPC >/dev/null 2>&1; then
  echo "Menyalakan Anvil (state disimpan di server/data/anvil-state.json)..."
  nohup anvil --port 8545 --state server/data/anvil-state.json --silent >server/data/anvil.log 2>&1 &
  for _ in $(seq 1 30); do cast chain-id --rpc-url $RPC >/dev/null 2>&1 && break; sleep 0.3; done
fi

atur() { # atur KUNCI NILAI di server/.env
  if grep -q "^$1=" "$ENV"; then sed -i "s|^$1=.*|$1=$2|" "$ENV"; else echo "$1=$2" >>"$ENV"; fi
}

KONTRAK=$(grep '^CHAIN_KONTRAK=' "$ENV" | cut -d= -f2 || true)
if [ -z "$KONTRAK" ] || [ "$(cast code "$KONTRAK" --rpc-url $RPC 2>/dev/null)" = "0x" ]; then
  echo "Deploy PresensiKTM ke Anvil..."
  KONTRAK=$(cd contracts && forge create src/PresensiKTM.sol:PresensiKTM --rpc-url $RPC --private-key $KEY --broadcast --json | grep -o '"deployedTo": *"0x[0-9a-fA-F]*' | grep -o '0x[0-9a-fA-F]*')
fi
atur CHAIN_RPC $RPC
atur CHAIN_PRIVATE_KEY $KEY
atur CHAIN_KONTRAK "$KONTRAK"
atur CHAIN_EXPLORER ""
echo "Kontrak PresensiKTM: $KONTRAK"
