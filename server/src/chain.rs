//! Pencatatan presensi ke smart contract PresensiKTM lewat `cast` (Foundry).
//! Yang dikirim ke chain hanya hash: sesi, mahasiswa (hash UID) dan bukti.

use anyhow::{bail, Context};
use tokio::{process::Command, sync::Mutex};

pub struct Chain {
    rpc: String,
    kunci: String,
    kontrak: String,
    /// Kontrak SertifikatKTM (opsional).
    kontrak_sertifikat: Option<String>,
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
            kontrak_sertifikat: ambil("CHAIN_KONTRAK_SERTIFIKAT"),
            explorer: ambil("CHAIN_EXPLORER"),
            cast: ambil("CAST_BIN").unwrap_or_else(|| "cast".into()),
            antre: Mutex::new(()),
        })
    }

    /// Halaman kontrak di explorer (atau alamat mentah kalau explorer tidak diatur).
    pub fn link_kontrak(&self) -> String {
        match &self.explorer {
            Some(e) => format!("{}/address/{}", e.trim_end_matches('/'), self.kontrak),
            None => self.kontrak.clone(),
        }
    }

    pub fn link_tx(&self, tx: &str) -> Option<String> {
        self.explorer.as_ref().map(|e| format!("{}/tx/{tx}", e.trim_end_matches('/')))
    }

    pub fn ada_sertifikat(&self) -> bool {
        self.kontrak_sertifikat.is_some()
    }

    pub fn link_kontrak_sertifikat(&self) -> Option<String> {
        let k = self.kontrak_sertifikat.as_ref()?;
        Some(match &self.explorer {
            Some(e) => format!("{}/address/{k}", e.trim_end_matches('/')),
            None => k.clone(),
        })
    }

    /// Kirim satu transaksi `fungsi(bytes32,bytes32,bytes32)` ke kontrak; hasilnya tx hash.
    async fn kirim(&self, kontrak: &str, fungsi: &str, a: &str, b: &str, c: &str) -> anyhow::Result<String> {
        let _giliran = self.antre.lock().await;
        let out = Command::new(&self.cast)
            .args(["send", kontrak, fungsi])
            .args([format!("0x{a}"), format!("0x{b}"), format!("0x{c}")])
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

    /// Timestamp blok dari mapping `getter(bytes32)(uint64)` (0 = belum ada).
    async fn baca(&self, kontrak: &str, getter: &str, kunci: &str) -> anyhow::Result<u64> {
        let out = Command::new(&self.cast)
            .args(["call", kontrak, getter, &format!("0x{kunci}")])
            .args(["--rpc-url", &self.rpc])
            .output()
            .await?;
        if !out.status.success() {
            bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
        }
        let s = String::from_utf8_lossy(&out.stdout);
        Ok(s.split_whitespace().next().unwrap_or("0").parse()?)
    }

    pub async fn catat(&self, sesi: &str, mhs: &str, bukti: &str) -> anyhow::Result<String> {
        self.kirim(&self.kontrak, "catat(bytes32,bytes32,bytes32)", sesi, mhs, bukti).await
    }

    /// Timestamp blok saat bukti presensi tercatat (0 = belum tercatat).
    pub async fn cek(&self, bukti: &str) -> anyhow::Result<u64> {
        self.baca(&self.kontrak, "tercatat(bytes32)(uint64)", bukti).await
    }

    pub async fn terbitkan(&self, acara: &str, penerima: &str, kode: &str) -> anyhow::Result<String> {
        let k = self.kontrak_sertifikat.as_deref().context("CHAIN_KONTRAK_SERTIFIKAT belum diatur")?;
        self.kirim(k, "terbitkan(bytes32,bytes32,bytes32)", acara, penerima, kode).await
    }

    pub async fn cek_sertifikat(&self, kode: &str) -> anyhow::Result<u64> {
        let k = self.kontrak_sertifikat.as_deref().context("CHAIN_KONTRAK_SERTIFIKAT belum diatur")?;
        self.baca(k, "terbit(bytes32)(uint64)", kode).await
    }
}
