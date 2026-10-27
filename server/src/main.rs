mod ai;
mod api;
mod batas;
mod chain;
mod data;
mod db;
mod sinkron;
mod waktu;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
pub struct App {
    pub db: Arc<Mutex<rusqlite::Connection>>,
    pub rahasia: Arc<String>,
    pub ai: Arc<ai::Konfig>,
    pub chain: Option<Arc<chain::Chain>>,
    pub http: reqwest::Client,
    pub cache: Arc<Mutex<HashMap<String, String>>>,
    pub batas: Arc<batas::Pembatas>,
}

fn env(k: &str, d: &str) -> String {
    std::env::var(k).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| d.to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let data_dir = env("DATA_DIR", "data");
    std::fs::create_dir_all(&data_dir)?;
    let db_path = format!("{data_dir}/smart-ktm.db");
    if std::env::args().any(|a| a == "--reset") {
        for akhiran in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{db_path}{akhiran}"));
        }
        println!("Database direset, data contoh dibuat ulang.");
    }
    let mut conn = rusqlite::Connection::open(&db_path)?;
    db::siapkan(&conn)?;

    // `--sinkron data.json`: impor jadwal MyUnpam untuk mahasiswa yang KTM-nya tertaut, lalu keluar.
    let argumen: Vec<String> = std::env::args().collect();
    if let Some(i) = argumen.iter().position(|a| a == "--sinkron") {
        let berkas = argumen.get(i + 1).ok_or_else(|| anyhow::anyhow!("pakai: --sinkron data.json"))?;
        let d: sinkron::DataUnpam = serde_json::from_str(&std::fs::read_to_string(berkas)?)?;
        let user: i64 = conn.query_row(
            "SELECT id FROM users WHERE peran = 'mhs' AND (nim = ?1 OR uid_hash IS NOT NULL) ORDER BY nim = ?1 DESC, id LIMIT 1",
            [&d.nim],
            |r| r.get(0),
        )?;
        sinkron::bersihkan_contoh(&mut conn, user)?;
        let n = sinkron::sinkron(&mut conn, user, &d)?;
        println!("{n} kelas MyUnpam disinkronkan untuk NIM {}", d.nim);
        return Ok(());
    }

    let ai = ai::Konfig::dari_env();
    let chain = chain::Chain::dari_env().map(Arc::new);
    let app = App {
        db: Arc::new(Mutex::new(conn)),
        rahasia: Arc::new(db::rahasia(&data_dir)?),
        http: reqwest::Client::new(),
        cache: Arc::default(),
        batas: Arc::default(),
        chain,
        ai: Arc::new(ai),
    };
    println!(
        "AI: {} | Blockchain: {}",
        app.ai.label(),
        if app.chain.is_some() { "aktif" } else { "mati (presensi hanya lokal)" }
    );

    // Transaksi blockchain yang gagal/macet dicoba ulang tiap 2 menit.
    let app_ulang = app.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(120)).await;
            api::coba_ulang_chain(&app_ulang).await;
        }
    });

    let web = env("WEB_DIR", "../web/dist");
    // APK Android disajikan dari data/unduh (diunggah scripts/deploy-vps.sh, tidak ikut repo).
    let router = Router::new()
        .nest("/api", api::router())
        .nest_service("/unduh", ServeDir::new(format!("{data_dir}/unduh")))
        .fallback_service(ServeDir::new(&web).not_found_service(ServeFile::new(format!("{web}/404.html"))))
        .with_state(app);

    let alamat = format!("{}:{}", env("HOST", "127.0.0.1"), env("PORT", "8080"));
    let listener = tokio::net::TcpListener::bind(&alamat).await?;
    println!("Smart KTM jalan di http://{alamat}");
    axum::serve(listener, router).await?;
    Ok(())
}
