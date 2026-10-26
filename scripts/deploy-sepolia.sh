#!/usr/bin/env bash
# Deploy PresensiKTM ke Ethereum Sepolia lalu arahkan server di VPS ke kontrak itu,
# supaya setiap presensi bisa dicek di https://sepolia.etherscan.io.
#
# Butuh: ~/.config/smart-ktm/sepolia.key (private key wallet operator, berisi Sepolia ETH).
# Opsional: ETHERSCAN_API_KEY=... supaya kode sumber kontrak terverifikasi di Etherscan.
set -euo pipefail
cd "$(dirname "$0")/.."
HOST=${VPS_HOST:-vps}
RPC=${SEPOLIA_RPC:-https://ethereum-sepolia-rpc.publicnode.com}
EXPLORER=https://sepolia.etherscan.io
KUNCI_FILE=$HOME/.config/smart-ktm/sepolia.key
KUNCI=$(cat "$KUNCI_FILE")
ALAMAT=$(cast wallet address --private-key "$KUNCI")

SALDO=$(cast balance "$ALAMAT" --ether --rpc-url "$RPC")
echo "Wallet operator $ALAMAT, saldo $SALDO SepoliaETH"
if [ "$(cast to-wei "$SALDO")" = "0" ]; then
  echo "Saldo kosong. Isi dulu lewat faucet Sepolia, lalu jalankan lagi."; exit 1
fi

KONTRAK=${KONTRAK:-}
if [ -z "$KONTRAK" ]; then
  echo "Deploy PresensiKTM ke Sepolia..."
  HASIL=$(cd contracts && forge create src/PresensiKTM.sol:PresensiKTM --rpc-url "$RPC" --private-key "$KUNCI" --broadcast --json)
  KONTRAK=$(echo "$HASIL" | grep -o '"deployedTo": *"0x[0-9a-fA-F]*' | grep -o '0x[0-9a-fA-F]*')
  TX=$(echo "$HASIL" | grep -o '"transactionHash": *"0x[0-9a-fA-F]*' | grep -o '0x[0-9a-fA-F]*')
  echo "Kontrak: $EXPLORER/address/$KONTRAK"
  echo "Tx deploy: $EXPLORER/tx/$TX"
fi

if [ -n "${ETHERSCAN_API_KEY:-}" ]; then
  echo "Verifikasi kode sumber di Etherscan..."
  (cd contracts && forge verify-contract "$KONTRAK" src/PresensiKTM.sol:PresensiKTM --chain sepolia --etherscan-api-key "$ETHERSCAN_API_KEY" --watch) || echo "Verifikasi gagal (kontrak tetap jalan)."
fi

echo "Arahkan server VPS ke Sepolia..."
ssh "$HOST" "cd /opt/smart-ktm && cp .env .env.sebelum-sepolia-\$(date +%Y%m%d%H%M) && \
  sed -i -e 's|^CHAIN_RPC=.*|CHAIN_RPC=$RPC|' -e 's|^CHAIN_KONTRAK=.*|CHAIN_KONTRAK=$KONTRAK|' -e 's|^CHAIN_EXPLORER=.*|CHAIN_EXPLORER=$EXPLORER|' .env && \
  grep -q '^CHAIN_PRIVATE_KEY=' .env && sed -i 's|^CHAIN_PRIVATE_KEY=.*|CHAIN_PRIVATE_KEY=__KUNCI__|' .env"
# Kunci dikirim lewat stdin, tidak muncul di argumen proses.
echo "$KUNCI" | ssh "$HOST" 'read -r k; sed -i "s|^CHAIN_PRIVATE_KEY=__KUNCI__|CHAIN_PRIVATE_KEY=$k|" /opt/smart-ktm/.env; chmod 600 /opt/smart-ktm/.env; chown smartktm:smartktm /opt/smart-ktm/.env; systemctl restart smart-ktm; sleep 3; journalctl -u smart-ktm -n 3 --no-pager -o cat | grep -E "Blockchain|jalan"'
echo "Selesai. Kontrak PresensiKTM: $EXPLORER/address/$KONTRAK"
