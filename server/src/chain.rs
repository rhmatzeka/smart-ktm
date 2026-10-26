//! Pencatatan presensi ke smart contract PresensiKTM lewat `cast` (Foundry).
//! Yang dikirim ke chain hanya hash: sesi, mahasiswa (hash UID) dan bukti.

use anyhow::{bail, Context};
use tokio::{process::Command, sync::Mutex};

pub struct Chain {
    rpc: String,
    kunci: String,
    kontrak: String,
    pub explorer: Option<String>,
    cast: String,
    // Satu transaksi sekali jalan supaya nonce wallet tidak bentrok.
    antre: Mutex<()>,
}

impl Chain {
    pub fn dari_env() -> Option<Self> {
        let ambil = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Some(Self {
            rpc: ambil("CHAIN_RPC")?,
            kunci: ambil("CHAIN_PRIVATE_KEY")?,
            kontrak: ambil("CHAIN_KONTRAK")?,
            explorer: ambil("CHAIN_EXPLORER"),
            cast: ambil("CAST_BIN").unwrap_or_else(|| "cast".into()),
            antre: Mutex::new(()),
        })
    }

    pub fn link_tx(&self, tx: &str) -> Option<String> {
        self.explorer.as_ref().map(|e| format!("{}/tx/{tx}", e.trim_end_matches('/')))
    }

    pub async fn catat(&self, sesi: &str, mhs: &str, bukti: &str) -> anyhow::Result<String> {
        let _giliran = self.antre.lock().await;
        let out = Command::new(&self.cast)
            .args(["send", &self.kontrak, "catat(bytes32,bytes32,bytes32)"])
            .args([format!("0x{sesi}"), format!("0x{mhs}"), format!("0x{bukti}")])
            .args(["--rpc-url", &self.rpc, "--private-key", &self.kunci, "--json"])
            .output()
            .await
            .context("cast tidak bisa dijalankan")?;
        if !out.status.success() {
            bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
        }
        let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
        Ok(v["transactionHash"].as_str().context("tx hash tidak ada")?.to_string())
    }

    /// Timestamp blok saat bukti tercatat (0 = belum tercatat).
    pub async fn cek(&self, bukti: &str) -> anyhow::Result<u64> {
        let out = Command::new(&self.cast)
            .args(["call", &self.kontrak, "tercatat(bytes32)(uint64)", &format!("0x{bukti}")])
            .args(["--rpc-url", &self.rpc])
            .output()
            .await?;
        if !out.status.success() {
            bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
        }
        let s = String::from_utf8_lossy(&out.stdout);
        Ok(s.split_whitespace().next().unwrap_or("0").parse()?)
    }
}
