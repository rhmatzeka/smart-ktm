mod ai;
mod api;
mod chain;
mod data;
mod db;
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
    let conn = rusqlite::Connection::open(&db_path)?;
    db::siapkan(&conn)?;

    let ai = ai::Konfig::dari_env();
    let chain = chain::Chain::dari_env().map(Arc::new);
    let app = App {
        db: Arc::new(Mutex::new(conn)),
        rahasia: Arc::new(db::rahasia(&data_dir)?),
        http: reqwest::Client::new(),
        cache: Arc::default(),
        chain,
        ai: Arc::new(ai),
    };
    println!(
        "AI: {} | Blockchain: {}",
        if app.ai.kunci.is_some() { app.ai.model.as_str() } else { "offline (GEMINI_API_KEY kosong)" },
        if app.chain.is_some() { "aktif" } else { "mati (presensi hanya lokal)" }
    );

    let web = env("WEB_DIR", "../web/dist");
    let router = Router::new()
        .nest("/api", api::router())
        .fallback_service(ServeDir::new(&web).not_found_service(ServeFile::new(format!("{web}/404.html"))))
        .with_state(app);

    let alamat = format!("{}:{}", env("HOST", "127.0.0.1"), env("PORT", "8080"));
    let listener = tokio::net::TcpListener::bind(&alamat).await?;
    println!("Smart KTM jalan di http://{alamat}");
    axum::serve(listener, router).await?;
    Ok(())
}
