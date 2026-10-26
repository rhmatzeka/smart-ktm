//! Endpoint HTTP di bawah /api.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{ai, data, db, sinkron, waktu, App};

pub struct Gagal(StatusCode, String);

impl IntoResponse for Gagal {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "galat": self.1 }))).into_response()
    }
}

impl From<rusqlite::Error> for Gagal {
    fn from(e: rusqlite::Error) -> Self {
        eprintln!("db: {e}");
        Gagal(StatusCode::INTERNAL_SERVER_ERROR, "Galat database".into())
    }
}

fn galat(kode: StatusCode, pesan: impl Into<String>) -> Gagal {
    Gagal(kode, pesan.into())
}

type Hasil = Result<Json<Value>, Gagal>;

#[derive(Clone)]
pub struct Pengguna {
    pub id: i64,
    pub peran: String,
    pub nim: String,
    pub nama: String,
    pub prodi: String,
    pub dibuat: String,
}

pub fn router() -> Router<App> {
    Router::new()
        .route("/ktm/cek", post(ktm_cek))
        .route("/daftar", post(daftar))
        .route("/masuk", post(masuk))
        .route("/keluar", post(keluar))
        .route("/saya", get(saya))
        .route("/profil", post(ubah_profil))
        .route("/beranda", get(beranda))
        .route("/tugas/{id}/selesai", post(tugas_selesai))
        .route("/chat", post(chat))
        .route("/sinkron", post(sinkron_unpam))
        .route("/kenalan", post(kenalan))
        .route("/verifikasi/{bukti}", get(verifikasi))
        .route("/dosen/kelas", get(dosen_kelas))
        .route("/dosen/sesi", post(buka_sesi))
        .route("/dosen/sesi/{id}", get(lihat_sesi))
        .route("/dosen/sesi/{id}/tap", post(tap_presensi))
        .route("/dosen/sesi/{id}/tutup", post(tutup_sesi))
}

// ---------- bantuan ----------

fn token(h: &HeaderMap) -> Option<String> {
    h.get("authorization")?.to_str().ok()?.strip_prefix("Bearer ").map(str::to_string)
}

fn pengguna(c: &Connection, h: &HeaderMap) -> Result<Pengguna, Gagal> {
    let t = token(h).ok_or_else(|| galat(StatusCode::UNAUTHORIZED, "Belum masuk"))?;
    c.query_row(
        "SELECT u.id, u.peran, u.nim, u.nama, u.prodi, u.dibuat FROM sesi_login s JOIN users u ON u.id = s.user_id WHERE s.token = ?1",
        [t],
        |r| Ok(Pengguna { id: r.get(0)?, peran: r.get(1)?, nim: r.get(2)?, nama: r.get(3)?, prodi: r.get(4)?, dibuat: r.get(5)? }),
    )
    .optional()?
    .ok_or_else(|| galat(StatusCode::UNAUTHORIZED, "Sesi habis, silakan masuk lagi"))
}

fn wajib(u: &Pengguna, peran: &str) -> Result<(), Gagal> {
    if u.peran == peran {
        Ok(())
    } else {
        Err(galat(StatusCode::FORBIDDEN, "Halaman ini bukan untuk akunmu"))
    }
}

fn token_baru(c: &Connection, user: i64) -> rusqlite::Result<String> {
    let t = hex::encode(rand::random::<[u8; 24]>());
    c.execute("INSERT INTO sesi_login(token, user_id, dibuat) VALUES(?1, ?2, ?3)", params![t, user, waktu::teks(&waktu::sekarang())])?;
    Ok(t)
}

fn uid_hash(app: &App, uid: &str) -> Result<String, Gagal> {
    db::hash_uid(&app.rahasia, uid).ok_or_else(|| galat(StatusCode::BAD_REQUEST, "UID kartu tidak valid"))
}

fn nama_depan(n: &str) -> String {
    n.split_whitespace().next().unwrap_or(n).to_string()
}

fn samarkan_nim(n: &str) -> String {
    if n.len() <= 6 {
        return n.into();
    }
    format!("{}{}{}", &n[..4], "•".repeat(n.len() - 6), &n[n.len() - 2..])
}

fn pin_valid(p: &str) -> bool {
    p.len() == 6 && p.chars().all(|c| c.is_ascii_digit())
}

// ---------- akun ----------

#[derive(Deserialize)]
struct Uid {
    uid: String,
}

/// `milik_sesi` = KTM ini milik akun yang sudah masuk di perangkat ini,
/// jadi tap cukup untuk langsung membuka beranda tanpa PIN.
async fn ktm_cek(State(app): State<App>, h: HeaderMap, Json(b): Json<Uid>) -> Hasil {
    let hash = uid_hash(&app, &b.uid)?;
    let c = app.db.lock().unwrap();
    let r: Option<(i64, String, String)> =
        c.query_row("SELECT id, nama, peran FROM users WHERE uid_hash = ?1", [&hash], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
    let sesi = pengguna(&c, &h).ok();
    Ok(Json(match r {
        Some((id, n, p)) => json!({ "terdaftar": true, "nama": nama_depan(&n), "peran": p, "milik_sesi": sesi.is_some_and(|s| s.id == id) }),
        None => json!({ "terdaftar": false, "milik_sesi": false }),
    }))
}

#[derive(Deserialize)]
struct Daftar {
    uid: String,
    nim: String,
    nama: Option<String>,
    prodi: Option<String>,
    pin: String,
}

async fn daftar(State(app): State<App>, Json(b): Json<Daftar>) -> Hasil {
    let h = uid_hash(&app, &b.uid)?;
    let nim = b.nim.trim();
    if !(6..=15).contains(&nim.len()) || !nim.chars().all(|c| c.is_ascii_digit()) {
        return Err(galat(StatusCode::BAD_REQUEST, "NIM harus 6-15 angka"));
    }
    if !pin_valid(&b.pin) {
        return Err(galat(StatusCode::BAD_REQUEST, "PIN harus 6 angka"));
    }
    let c = app.db.lock().unwrap();
    let dipakai: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM users WHERE uid_hash = ?1)", [&h], |r| r.get(0))?;
    if dipakai {
        return Err(galat(StatusCode::CONFLICT, "KTM ini sudah terdaftar"));
    }
    let ada: Option<(i64, String, String, Option<String>, String)> = c
        .query_row("SELECT id, peran, pin_hash, uid_hash, nama FROM users WHERE nim = ?1", [nim], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .optional()?;
    let (id, nama) = match ada {
        // NIM sudah ada (mis. akun demo): KTM ditautkan setelah PIN cocok.
        Some((id, peran, pin_hash, uid, nama)) => {
            if peran != "mhs" {
                return Err(galat(StatusCode::CONFLICT, "NIM ini bukan akun mahasiswa"));
            }
            if uid.is_some() {
                return Err(galat(StatusCode::CONFLICT, "NIM ini sudah punya KTM terdaftar"));
            }
            if !db::cek_pin(&b.pin, &pin_hash) {
                return Err(galat(StatusCode::UNAUTHORIZED, "PIN salah untuk NIM ini"));
            }
            c.execute("UPDATE users SET uid_hash = ?1 WHERE id = ?2", params![h, id])?;
            (id, nama)
        }
        None => {
            let nama = b.nama.as_deref().unwrap_or("").trim().chars().take(60).collect::<String>();
            if nama.is_empty() {
                return Err(galat(StatusCode::BAD_REQUEST, "Nama wajib diisi"));
            }
            let prodi = b.prodi.as_deref().map(str::trim).filter(|p| !p.is_empty()).unwrap_or("Teknik Informatika");
            c.execute(
                "INSERT INTO users(peran, nim, nama, prodi, pin_hash, uid_hash, dibuat) VALUES('mhs', ?1, ?2, ?3, ?4, ?5, ?6)",
                params![nim, nama, prodi, db::hash_pin(&b.pin), h, waktu::teks(&waktu::sekarang())],
            )?;
            // Jadwal diisi lewat sinkron MyUnpam, bukan otomatis semua matkul.
            (c.last_insert_rowid(), nama)
        }
    };
    let t = token_baru(&c, id)?;
    Ok(Json(json!({ "token": t, "peran": "mhs", "nama": nama })))
}

#[derive(Deserialize)]
struct Masuk {
    uid: Option<String>,
    nim: Option<String>,
    pin: String,
}

async fn masuk(State(app): State<App>, Json(b): Json<Masuk>) -> Hasil {
    let c = app.db.lock().unwrap();
    let cari = "SELECT id, peran, nama, pin_hash FROM users WHERE ";
    let akun: Option<(i64, String, String, String)> = match (&b.uid, &b.nim) {
        (Some(uid), _) => {
            let h = uid_hash(&app, uid)?;
            c.query_row(&format!("{cari}uid_hash = ?1"), [h], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).optional()?
        }
        (None, Some(nim)) => c.query_row(&format!("{cari}nim = ?1"), [nim.trim()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).optional()?,
        _ => return Err(galat(StatusCode::BAD_REQUEST, "Tap KTM atau isi NIM")),
    };
    let Some((id, peran, nama, pin_hash)) = akun else {
        return Err(galat(StatusCode::NOT_FOUND, "Akun tidak ditemukan"));
    };
    if !db::cek_pin(&b.pin, &pin_hash) {
        return Err(galat(StatusCode::UNAUTHORIZED, "PIN salah"));
    }
    let t = token_baru(&c, id)?;
    Ok(Json(json!({ "token": t, "peran": peran, "nama": nama })))
}

async fn keluar(State(app): State<App>, h: HeaderMap) -> Hasil {
    if let Some(t) = token(&h) {
        app.db.lock().unwrap().execute("DELETE FROM sesi_login WHERE token = ?1", [t])?;
    }
    Ok(Json(json!({ "ok": true })))
}

async fn saya(State(app): State<App>, h: HeaderMap) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    let (bio, github, portofolio, publik, ktm): (String, String, String, bool, bool) = c.query_row(
        "SELECT bio, github, portofolio, publik, uid_hash IS NOT NULL FROM users WHERE id = ?1",
        [u.id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    Ok(Json(json!({
        "nama": u.nama, "nim": u.nim, "prodi": u.prodi, "peran": u.peran,
        "bio": bio, "github": github, "portofolio": portofolio, "publik": publik, "ktm": ktm,
    })))
}

#[derive(Deserialize)]
struct Profil {
    bio: String,
    github: String,
    portofolio: String,
    publik: bool,
}

async fn ubah_profil(State(app): State<App>, h: HeaderMap, Json(b): Json<Profil>) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    let potong = |s: &str, n: usize| s.trim().chars().take(n).collect::<String>();
    c.execute(
        "UPDATE users SET bio = ?1, github = ?2, portofolio = ?3, publik = ?4 WHERE id = ?5",
        params![potong(&b.bio, 160), potong(&b.github, 60), potong(&b.portofolio, 120), b.publik, u.id],
    )?;
    Ok(Json(json!({ "ok": true })))
}

// ---------- mahasiswa ----------

async fn beranda(State(app): State<App>, h: HeaderMap) -> Hasil {
    let (u, mut isi) = {
        let c = app.db.lock().unwrap();
        let u = pengguna(&c, &h)?;
        wajib(&u, "mhs")?;
        let isi = data::beranda(&c, u.id, &nama_depan(&u.nama), &u.dibuat, &waktu::sekarang())?;
        (u, isi)
    };
    let ai = ai::briefing(&app, u.id, &isi).await;
    isi["ringkasan_ai"] = json!(ai.is_some());
    if let Some(t) = ai {
        isi["ringkasan"] = json!(t);
    }
    Ok(Json(isi))
}

async fn tugas_selesai(State(app): State<App>, h: HeaderMap, Path(id): Path<i64>) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    c.execute(
        "INSERT OR IGNORE INTO tugas_selesai(user_id, tugas_id, waktu)
         SELECT ?1, t.id, ?3 FROM tugas t JOIN ambil a ON a.matkul_id = t.matkul_id AND a.user_id = ?1 WHERE t.id = ?2",
        params![u.id, id, waktu::teks(&waktu::sekarang())],
    )?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct Chat {
    pesan: String,
    #[serde(default)]
    riwayat: Vec<ai::Pesan>,
}

async fn chat(State(app): State<App>, h: HeaderMap, Json(b): Json<Chat>) -> Hasil {
    let u = {
        let c = app.db.lock().unwrap();
        let u = pengguna(&c, &h)?;
        wajib(&u, "mhs")?;
        u
    };
    let pesan: String = b.pesan.trim().chars().take(1000).collect();
    if pesan.is_empty() {
        return Err(galat(StatusCode::BAD_REQUEST, "Pesan kosong"));
    }
    Ok(Json(ai::chat(&app, &u, &pesan, &b.riwayat).await))
}

async fn sinkron_unpam(State(app): State<App>, h: HeaderMap, Json(d): Json<sinkron::DataUnpam>) -> Hasil {
    let mut c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    wajib(&u, "mhs")?;
    let n = sinkron::sinkron(&mut c, u.id, &d).map_err(|e| galat(StatusCode::BAD_REQUEST, e.to_string()))?;
    app.cache.lock().unwrap().retain(|k, _| !k.starts_with(&format!("{}:", u.id)));
    Ok(Json(json!({ "jumlah": n })))
}

/// KTM orang lain ditap: hanya profil publik, tanpa jadwal/nilai.
async fn kenalan(State(app): State<App>, Json(b): Json<Uid>) -> Hasil {
    let h = uid_hash(&app, &b.uid)?;
    let c = app.db.lock().unwrap();
    let r: Option<(i64, String, String, String, String, String, bool, String)> = c
        .query_row(
            "SELECT id, nama, prodi, bio, github, portofolio, publik, dibuat FROM users WHERE uid_hash = ?1 AND peran = 'mhs'",
            [h],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
        )
        .optional()?;
    let Some((id, nama, prodi, bio, github, portofolio, publik, dibuat)) = r else {
        return Err(galat(StatusCode::NOT_FOUND, "KTM ini belum terdaftar di Smart KTM"));
    };
    if !publik {
        return Ok(Json(json!({ "privat": true, "nama": nama_depan(&nama) })));
    }
    let rekap = data::presensi(&c, id, &dibuat)?;
    let mut lencana: Vec<String> = rekap
        .iter()
        .filter(|p| p["absen"] == 0 && p["total"].as_i64().unwrap_or(0) >= 3)
        .map(|p| format!("Hadir penuh · {}", p["matkul"].as_str().unwrap()))
        .collect();
    let onchain: i64 = rekap.iter().map(|p| p["onchain"].as_i64().unwrap_or(0)).sum();
    if onchain > 0 {
        lencana.insert(0, format!("{onchain} presensi tercatat on-chain"));
    }
    Ok(Json(json!({
        "privat": false, "nama": nama, "prodi": prodi, "kampus": "Universitas Pamulang",
        "bio": bio, "github": github, "portofolio": portofolio, "lencana": lencana,
    })))
}

/// Halaman publik: siapa pun bisa mengecek sebuah bukti presensi.
async fn verifikasi(State(app): State<App>, Path(bukti): Path<String>) -> Hasil {
    let bukti = bukti.trim_start_matches("0x").to_lowercase();
    if bukti.len() != 64 || !bukti.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(galat(StatusCode::BAD_REQUEST, "Kode bukti harus 64 karakter hex"));
    }
    let rec: Option<(String, String, String, i64, String, Option<String>, String)> = {
        let c = app.db.lock().unwrap();
        c.query_row(
            "SELECT u.nama, u.nim, m.nama, s.pertemuan, p.waktu, p.tx, p.status FROM presensi p
             JOIN sesi s ON s.id = p.sesi_id JOIN matkul m ON m.id = s.matkul_id JOIN users u ON u.id = p.user_id
             WHERE p.bukti = ?1",
            [&bukti],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .optional()?
    };
    let Some((nama, nim, matkul, pertemuan, saat, tx, status)) = rec else {
        return Err(galat(StatusCode::NOT_FOUND, "Bukti tidak ditemukan"));
    };
    let mut chain = Value::Null;
    if let Some(ch) = &app.chain {
        chain = match ch.cek(&bukti).await {
            Ok(ts) => json!({ "tercatat": ts > 0, "timestamp": ts }),
            Err(e) => json!({ "galat": e.to_string() }),
        };
    }
    let link = tx.as_deref().and_then(|t| app.chain.as_ref().and_then(|c| c.link_tx(t)));
    Ok(Json(json!({
        "mahasiswa": nama_depan(&nama), "nim": samarkan_nim(&nim), "matkul": matkul, "pertemuan": pertemuan,
        "waktu": saat, "status": status, "tx": tx, "link_tx": link, "chain": chain, "bukti": bukti,
        "kontrak": app.chain.as_ref().map(|c| c.link_kontrak()),
    })))
}

// ---------- dosen ----------

async fn dosen_kelas(State(app): State<App>, h: HeaderMap) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    wajib(&u, "dosen")?;
    let mut q = c.prepare(
        "SELECT m.id, m.kode, m.nama, m.hari, m.mulai, m.selesai, m.ruang,
           (SELECT COUNT(*) FROM ambil a WHERE a.matkul_id = m.id),
           (SELECT COUNT(*) FROM sesi s WHERE s.matkul_id = m.id),
           (SELECT id FROM sesi s WHERE s.matkul_id = m.id AND s.ditutup IS NULL ORDER BY id DESC LIMIT 1)
         FROM matkul m WHERE m.dosen_id = ?1 ORDER BY m.hari, m.mulai",
    )?;
    let kelas = q
        .query_map([u.id], |r| {
            Ok(json!({
                "id": r.get::<_, i64>(0)?, "kode": r.get::<_, String>(1)?, "nama": r.get::<_, String>(2)?,
                "hari": waktu::nama_hari(r.get(3)?), "mulai": r.get::<_, String>(4)?, "selesai": r.get::<_, String>(5)?,
                "ruang": r.get::<_, String>(6)?, "peserta": r.get::<_, i64>(7)?, "pertemuan": r.get::<_, i64>(8)?,
                "sesi_aktif": r.get::<_, Option<i64>>(9)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Json(json!({ "nama": u.nama, "kelas": kelas })))
}

#[derive(Deserialize)]
struct BukaSesi {
    matkul_id: i64,
}

async fn buka_sesi(State(app): State<App>, h: HeaderMap, Json(b): Json<BukaSesi>) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    wajib(&u, "dosen")?;
    let milik: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM matkul WHERE id = ?1 AND dosen_id = ?2)", params![b.matkul_id, u.id], |r| r.get(0))?;
    if !milik {
        return Err(galat(StatusCode::FORBIDDEN, "Ini bukan kelas Anda"));
    }
    let aktif: Option<i64> = c
        .query_row("SELECT id FROM sesi WHERE matkul_id = ?1 AND ditutup IS NULL ORDER BY id DESC LIMIT 1", [b.matkul_id], |r| r.get(0))
        .optional()?;
    let id = match aktif {
        Some(id) => id,
        None => {
            let ke: i64 = c.query_row("SELECT COALESCE(MAX(pertemuan), 0) + 1 FROM sesi WHERE matkul_id = ?1", [b.matkul_id], |r| r.get(0))?;
            c.execute(
                "INSERT INTO sesi(matkul_id, pertemuan, dibuka) VALUES(?1, ?2, ?3)",
                params![b.matkul_id, ke, waktu::teks(&waktu::sekarang())],
            )?;
            c.last_insert_rowid()
        }
    };
    Ok(Json(json!({ "id": id })))
}

/// Sesi milik dosen ini: (matkul_id, masih_terbuka)
fn sesi_milik(c: &Connection, sesi: i64, dosen: i64) -> Result<(i64, bool), Gagal> {
    c.query_row(
        "SELECT s.matkul_id, s.ditutup IS NULL FROM sesi s JOIN matkul m ON m.id = s.matkul_id WHERE s.id = ?1 AND m.dosen_id = ?2",
        params![sesi, dosen],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()?
    .ok_or_else(|| galat(StatusCode::NOT_FOUND, "Sesi tidak ditemukan"))
}

async fn lihat_sesi(State(app): State<App>, h: HeaderMap, Path(id): Path<i64>) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    wajib(&u, "dosen")?;
    let (matkul_id, terbuka) = sesi_milik(&c, id, u.id)?;
    let (nama, ruang, pertemuan, dibuka): (String, String, i64, String) = c.query_row(
        "SELECT m.nama, m.ruang, s.pertemuan, s.dibuka FROM sesi s JOIN matkul m ON m.id = s.matkul_id WHERE s.id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let mut q = c.prepare(
        "SELECT u.nama, u.nim, p.waktu, p.status, p.tx, p.bukti FROM ambil a JOIN users u ON u.id = a.user_id
         LEFT JOIN presensi p ON p.user_id = u.id AND p.sesi_id = ?1
         WHERE a.matkul_id = ?2 ORDER BY p.waktu IS NULL, p.waktu DESC, u.nama",
    )?;
    let peserta = q
        .query_map(params![id, matkul_id], |r| {
            let tx: Option<String> = r.get(4)?;
            let link = tx.as_deref().and_then(|t| app.chain.as_ref().and_then(|c| c.link_tx(t)));
            Ok(json!({
                "nama": r.get::<_, String>(0)?, "nim": samarkan_nim(&r.get::<_, String>(1)?),
                "waktu": r.get::<_, Option<String>>(2)?, "status": r.get::<_, Option<String>>(3)?,
                "tx": tx, "link_tx": link, "bukti": r.get::<_, Option<String>>(5)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let hadir = peserta.iter().filter(|p| !p["waktu"].is_null()).count();
    Ok(Json(json!({
        "id": id, "matkul": nama, "ruang": ruang, "pertemuan": pertemuan, "dibuka": dibuka, "terbuka": terbuka,
        "hadir": hadir, "total": peserta.len(), "peserta": peserta, "chain": app.chain.is_some(),
        "kontrak": app.chain.as_ref().map(|c| c.link_kontrak()),
    })))
}

async fn tap_presensi(State(app): State<App>, h: HeaderMap, Path(id): Path<i64>, Json(b): Json<Uid>) -> Hasil {
    let mhs_hash = uid_hash(&app, &b.uid)?;
    let (balasan, kirim) = {
        let c = app.db.lock().unwrap();
        let u = pengguna(&c, &h)?;
        wajib(&u, "dosen")?;
        let (matkul_id, terbuka) = sesi_milik(&c, id, u.id)?;
        if !terbuka {
            return Err(galat(StatusCode::CONFLICT, "Sesi sudah ditutup"));
        }
        let mhs: Option<(i64, String, String)> = c
            .query_row("SELECT id, nama, nim FROM users WHERE uid_hash = ?1 AND peran = 'mhs'", [&mhs_hash], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        let Some((mid, nama, nim)) = mhs else {
            return Err(galat(StatusCode::NOT_FOUND, "KTM belum terdaftar. Minta mahasiswa daftar dulu di Smart KTM."));
        };
        let ikut: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM ambil WHERE user_id = ?1 AND matkul_id = ?2)", params![mid, matkul_id], |r| r.get(0))?;
        if !ikut {
            // Belum sinkron MyUnpam tapi hadir di kelas: dosen yang melihat langsung, jadi dimasukkan ke kelas.
            c.execute("INSERT INTO ambil(user_id, matkul_id) VALUES(?1, ?2)", params![mid, matkul_id])?;
        }
        let lama: Option<(String, String, Option<String>)> = c
            .query_row("SELECT waktu, bukti, tx FROM presensi WHERE sesi_id = ?1 AND user_id = ?2", params![id, mid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        let dasar = json!({ "nama": nama, "nim": samarkan_nim(&nim) });
        if let Some((saat, bukti, tx)) = lama {
            let mut v = dasar;
            v["status"] = json!("sudah");
            v["waktu"] = json!(saat);
            v["bukti"] = json!(bukti);
            v["tx"] = json!(tx);
            return Ok(Json(v));
        }
        let saat = waktu::teks(&waktu::sekarang());
        let bukti = db::sha(&format!("{}:presensi:{id}:{mid}:{saat}", app.rahasia));
        let status = if app.chain.is_some() { "menunggu" } else { "lokal" };
        c.execute(
            "INSERT INTO presensi(sesi_id, user_id, waktu, bukti, status) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![id, mid, saat, bukti, status],
        )?;
        let pid = c.last_insert_rowid();
        let mut v = dasar;
        v["status"] = json!("hadir");
        v["waktu"] = json!(saat);
        v["bukti"] = json!(bukti);
        v["chain"] = json!(status);
        let sesi_hash = db::sha(&format!("{}:sesi:{id}", app.rahasia));
        (v, (pid, sesi_hash, bukti))
    };

    if let Some(ch) = app.chain.clone() {
        let db = app.db.clone();
        let (pid, sesi_hash, bukti) = kirim;
        tokio::spawn(async move {
            let (tx, status) = match ch.catat(&sesi_hash, &mhs_hash, &bukti).await {
                Ok(tx) => (Some(tx), "onchain"),
                Err(e) => {
                    eprintln!("chain gagal: {e:#}");
                    (None, "gagal")
                }
            };
            let _ = db.lock().unwrap().execute("UPDATE presensi SET tx = ?1, status = ?2 WHERE id = ?3", params![tx, status, pid]);
        });
    }
    Ok(Json(balasan))
}

async fn tutup_sesi(State(app): State<App>, h: HeaderMap, Path(id): Path<i64>) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    wajib(&u, "dosen")?;
    sesi_milik(&c, id, u.id)?;
    c.execute("UPDATE sesi SET ditutup = ?1 WHERE id = ?2 AND ditutup IS NULL", params![waktu::teks(&waktu::sekarang()), id])?;
    Ok(Json(json!({ "ok": true })))
}
