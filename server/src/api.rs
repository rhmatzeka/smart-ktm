//! Endpoint HTTP di bawah /api.

use std::time::Duration;

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
        .route("/kehadiran/{id}", get(kehadiran))
        .route("/jadwal", get(jadwal_minggu))
        .route("/ktm/hilang", post(lapor_hilang))
        .route("/keamanan", post(atur_keamanan))
        .route("/pin", post(ganti_pin))
        .route("/acara", get(daftar_acara).post(buat_acara))
        .route("/acara/{id}", get(lihat_acara))
        .route("/acara/{id}/tap", post(tap_acara))
        .route("/acara/{id}/tutup", post(tutup_acara))
        .route("/sertifikat", get(sertifikat_saya))
        .route("/sertifikat/{kode}", get(lihat_sertifikat))
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

/// Sesi login berlaku 30 hari.
const SESI_HARI: i64 = 30;

fn batas_sesi() -> String {
    waktu::teks(&(waktu::sekarang() - chrono::Duration::days(SESI_HARI)))
}

fn pengguna(c: &Connection, h: &HeaderMap) -> Result<Pengguna, Gagal> {
    let t = token(h).ok_or_else(|| galat(StatusCode::UNAUTHORIZED, "Belum masuk"))?;
    c.query_row(
        "SELECT u.id, u.peran, u.nim, u.nama, u.prodi, u.dibuat FROM sesi_login s JOIN users u ON u.id = s.user_id
         WHERE s.token = ?1 AND s.dibuat >= ?2",
        params![t, batas_sesi()],
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
    c.execute("DELETE FROM sesi_login WHERE dibuat < ?1", [batas_sesi()])?;
    let t = hex::encode(rand::random::<[u8; 24]>());
    c.execute("INSERT INTO sesi_login(token, user_id, dibuat) VALUES(?1, ?2, ?3)", params![t, user, waktu::teks(&waktu::sekarang())])?;
    Ok(t)
}

/// HP tepercaya: pernah masuk pakai PIN. Hanya HP ini yang boleh masuk dengan tap KTM tanpa PIN,
/// jadi orang yang sekadar tahu UID (UID MIFARE Classic tidak rahasia) tetap harus tahu PIN.
fn perangkat_baru(c: &Connection, user: i64) -> rusqlite::Result<String> {
    let t = hex::encode(rand::random::<[u8; 24]>());
    c.execute(
        "INSERT INTO perangkat(kode_hash, user_id, dibuat) VALUES(?1, ?2, ?3)",
        params![db::sha(&t), user, waktu::teks(&waktu::sekarang())],
    )?;
    Ok(t)
}

/// Token HP tepercaya yang dikirim HP: satu per akun yang pernah masuk di HP itu,
/// supaya HP yang dipakai bergantian (pemilik lalu teman) tidak saling menimpa. Versi lama mengirim satu teks.
#[derive(Deserialize, Default)]
#[serde(untagged)]
enum TokenHp {
    #[default]
    Kosong,
    Satu(String),
    Banyak(Vec<String>),
}

impl TokenHp {
    fn daftar(&self) -> Vec<&str> {
        match self {
            TokenHp::Kosong => vec![],
            TokenHp::Satu(t) => vec![t.as_str()],
            TokenHp::Banyak(v) => v.iter().take(MAKS_TOKEN_HP).map(String::as_str).collect(),
        }
    }
}

const MAKS_TOKEN_HP: usize = 5;

fn perangkat_sah(c: &Connection, user: i64, token: &[&str]) -> rusqlite::Result<bool> {
    for t in token.iter().filter(|t| !t.is_empty()) {
        let ada: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM perangkat WHERE kode_hash = ?1 AND user_id = ?2)", params![db::sha(t), user], |r| r.get(0))?;
        if ada {
            return Ok(true);
        }
    }
    Ok(false)
}

/// IP asli pengunjung (lewat Cloudflare Tunnel).
fn ip(h: &HeaderMap) -> String {
    h.get("cf-connecting-ip")
        .or_else(|| h.get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').next().unwrap_or("").trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "lokal".into())
}

/// Batas permintaan: `maks` kali per `detik` untuk satu kunci.
fn batasi(app: &App, kunci: &str, maks: u32, detik: u64) -> Result<(), Gagal> {
    if app.batas.izin(kunci, maks, Duration::from_secs(detik)) {
        Ok(())
    } else {
        Err(galat(StatusCode::TOO_MANY_REQUESTS, "Terlalu banyak percobaan. Tunggu sebentar lalu coba lagi."))
    }
}

const PIN_MAKS_GAGAL: u32 = 5;
const PIN_KUNCI: Duration = Duration::from_secs(15 * 60);

/// Cek PIN dengan penguncian akun setelah 5 kali salah dalam 15 menit.
fn cek_pin_akun(app: &App, user: i64, pin: &str, pin_hash: &str) -> Result<(), Gagal> {
    let kunci = format!("pin:{user}");
    if app.batas.terkunci(&kunci, PIN_MAKS_GAGAL, PIN_KUNCI) {
        return Err(galat(StatusCode::TOO_MANY_REQUESTS, "PIN salah 5 kali. Akun dikunci 15 menit."));
    }
    if db::cek_pin(pin, pin_hash) {
        app.batas.hapus(&kunci);
        Ok(())
    } else {
        app.batas.gagal(&kunci, PIN_KUNCI);
        Err(galat(StatusCode::UNAUTHORIZED, "PIN salah"))
    }
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

#[derive(Deserialize)]
struct CekKtm {
    uid: String,
    #[serde(default)]
    perangkat: TokenHp,
}

/// `milik_sesi` = KTM ini milik akun yang sudah masuk di perangkat ini,
/// jadi tap cukup untuk langsung membuka beranda tanpa PIN.
async fn ktm_cek(State(app): State<App>, h: HeaderMap, Json(b): Json<CekKtm>) -> Hasil {
    batasi(&app, &format!("cek:{}", ip(&h)), 60, 60)?;
    let hash = uid_hash(&app, &b.uid)?;
    let c = app.db.lock().unwrap();
    let r: Option<(i64, String, String, bool, Option<String>, bool)> = c
        .query_row("SELECT id, nama, peran, hilang, hilang_pesan, pin_tap FROM users WHERE uid_hash = ?1", [&hash], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })
        .optional()?;
    let sesi = pengguna(&c, &h).ok();
    Ok(Json(match r {
        // KTM yang dilaporkan hilang tidak bisa dipakai masuk; penemu melihat cara menghubungi pemilik.
        Some((_, n, _, true, pesan, _)) => json!({ "terdaftar": true, "hilang": true, "nama": nama_depan(&n), "pesan": pesan.unwrap_or_default(), "milik_sesi": false }),
        Some((id, n, p, false, _, pin_tap)) => json!({
            "terdaftar": true, "hilang": false, "nama": nama_depan(&n), "peran": p,
            "milik_sesi": sesi.is_some_and(|s| s.id == id),
            "perlu_pin": pin_tap || !perangkat_sah(&c, id, &b.perangkat.daftar())?,
        }),
        None => json!({ "terdaftar": false, "hilang": false, "milik_sesi": false }),
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

async fn daftar(State(app): State<App>, hd: HeaderMap, Json(b): Json<Daftar>) -> Hasil {
    batasi(&app, &format!("daftar:{}", ip(&hd)), 10, 600)?;
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
    let ada: Option<(i64, String, String, Option<String>, String, bool)> = c
        .query_row("SELECT id, peran, pin_hash, uid_hash, nama, hilang FROM users WHERE nim = ?1", [nim], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })
        .optional()?;
    let (id, nama) = match ada {
        // NIM sudah ada (mis. akun demo): KTM ditautkan setelah PIN cocok.
        // Kalau KTM lama dilaporkan hilang, KTM baru menggantikannya.
        Some((id, peran, pin_hash, uid, nama, hilang)) => {
            if peran != "mhs" {
                return Err(galat(StatusCode::CONFLICT, "NIM ini bukan akun mahasiswa"));
            }
            if uid.is_some() && !hilang {
                return Err(galat(StatusCode::CONFLICT, "NIM ini sudah punya KTM terdaftar"));
            }
            cek_pin_akun(&app, id, &b.pin, &pin_hash)?;
            c.execute("UPDATE users SET uid_hash = ?1, hilang = 0, hilang_pesan = NULL WHERE id = ?2", params![h, id])?;
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
    let p = perangkat_baru(&c, id)?;
    Ok(Json(json!({ "token": t, "peran": "mhs", "nama": nama, "perangkat": p })))
}

#[derive(Deserialize)]
struct Masuk {
    uid: Option<String>,
    nim: Option<String>,
    #[serde(default)]
    pin: String,
    #[serde(default)]
    perangkat: TokenHp,
}

async fn masuk(State(app): State<App>, h: HeaderMap, Json(b): Json<Masuk>) -> Hasil {
    batasi(&app, &format!("masuk:{}", ip(&h)), 30, 60)?;
    let c = app.db.lock().unwrap();
    let cari = "SELECT id, peran, nama, pin_hash, hilang, pin_tap FROM users WHERE ";
    type Akun = (i64, String, String, String, bool, bool);
    let baris = |r: &rusqlite::Row| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?));
    let akun: Option<Akun> = match (&b.uid, &b.nim) {
        (Some(uid), _) => c.query_row(&format!("{cari}uid_hash = ?1"), [uid_hash(&app, uid)?], baris).optional()?,
        (None, Some(nim)) => c.query_row(&format!("{cari}nim = ?1"), [nim.trim()], baris).optional()?,
        _ => return Err(galat(StatusCode::BAD_REQUEST, "Tap KTM atau isi NIM")),
    };
    let Some((id, peran, nama, pin_hash, hilang, pin_tap)) = akun else {
        return Err(galat(StatusCode::NOT_FOUND, "Akun tidak ditemukan"));
    };
    if b.uid.is_some() && hilang {
        return Err(galat(StatusCode::FORBIDDEN, "KTM ini dilaporkan hilang dan sudah diblokir"));
    }
    // Tap KTM tanpa PIN: hanya di HP tepercaya dan kalau pemilik tidak menyalakan "minta PIN".
    if b.uid.is_some() && b.pin.is_empty() {
        if !pin_tap && perangkat_sah(&c, id, &b.perangkat.daftar())? {
            let t = token_baru(&c, id)?;
            return Ok(Json(json!({ "token": t, "peran": peran, "nama": nama })));
        }
        return Err(galat(StatusCode::UNAUTHORIZED, "Masukkan PIN sekali untuk HP ini"));
    }
    cek_pin_akun(&app, id, &b.pin, &pin_hash)?;
    let t = token_baru(&c, id)?;
    let p = perangkat_baru(&c, id)?;
    Ok(Json(json!({ "token": t, "peran": peran, "nama": nama, "perangkat": p })))
}

#[derive(Deserialize)]
struct GantiPin {
    lama: String,
    baru: String,
}

/// Ganti PIN: semua HP tepercaya lain dicabut, HP ini tetap tepercaya.
async fn ganti_pin(State(app): State<App>, h: HeaderMap, Json(b): Json<GantiPin>) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    if !pin_valid(&b.baru) {
        return Err(galat(StatusCode::BAD_REQUEST, "PIN baru harus 6 angka"));
    }
    let pin_hash: String = c.query_row("SELECT pin_hash FROM users WHERE id = ?1", [u.id], |r| r.get(0))?;
    cek_pin_akun(&app, u.id, &b.lama, &pin_hash)?;
    c.execute("UPDATE users SET pin_hash = ?1 WHERE id = ?2", params![db::hash_pin(&b.baru), u.id])?;
    c.execute("DELETE FROM perangkat WHERE user_id = ?1", [u.id])?;
    let p = perangkat_baru(&c, u.id)?;
    Ok(Json(json!({ "ok": true, "perangkat": p })))
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
    let (bio, github, portofolio, publik, ktm, foto, semester): (String, String, String, bool, bool, Option<String>, Option<String>) = c.query_row(
        "SELECT bio, github, portofolio, publik, uid_hash IS NOT NULL, foto, semester FROM users WHERE id = ?1",
        [u.id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
    )?;
    Ok(Json(json!({
        "nama": u.nama, "nim": u.nim, "prodi": u.prodi, "peran": u.peran,
        "bio": bio, "github": github, "portofolio": portofolio, "publik": publik, "ktm": ktm,
        "foto": foto, "semester": semester,
        "hilang": c.query_row("SELECT hilang FROM users WHERE id = ?1", [u.id], |r| r.get::<_, bool>(0))?,
        "pin_tap": c.query_row("SELECT pin_tap FROM users WHERE id = ?1", [u.id], |r| r.get::<_, bool>(0))?,
        "hilang_pesan": c.query_row("SELECT hilang_pesan FROM users WHERE id = ?1", [u.id], |r| r.get::<_, Option<String>>(0))?,
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
    batasi(&app, &format!("chat:{}", u.id), 40, 3600)?;
    let pesan: String = b.pesan.trim().chars().take(1000).collect();
    if pesan.is_empty() {
        return Err(galat(StatusCode::BAD_REQUEST, "Pesan kosong"));
    }
    Ok(Json(ai::chat(&app, &u, &pesan, &b.riwayat).await))
}

/// Jadwal satu minggu (Senin..Minggu) milik pengguna.
async fn jadwal_minggu(State(app): State<App>, h: HeaderMap) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    let kelas: Vec<Value> = data::jadwal(&c, u.id, None)?.iter().map(|k| k.json()).collect();
    Ok(Json(json!({ "hari_ini": waktu::hari_ke(&waktu::sekarang()), "kelas": kelas })))
}

async fn kehadiran(State(app): State<App>, h: HeaderMap, Path(id): Path<i64>) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    let mut v = data::kehadiran(&c, u.id, id)?.ok_or_else(|| galat(StatusCode::NOT_FOUND, "Mata kuliah tidak ada di jadwalmu"))?;
    if let (Some(ch), Some(tap)) = (&app.chain, v["tap"].as_array_mut()) {
        for t in tap {
            if let Some(tx) = t["tx"].as_str().map(str::to_string) {
                t["link_tx"] = json!(ch.link_tx(&tx));
            }
        }
    }
    Ok(Json(v))
}

async fn sinkron_unpam(State(app): State<App>, h: HeaderMap, Json(d): Json<sinkron::DataUnpam>) -> Hasil {
    let mut c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    wajib(&u, "mhs")?;
    batasi(&app, &format!("sinkron:{}", u.id), 10, 600)?;
    let n = sinkron::sinkron(&mut c, u.id, &d).map_err(|e| galat(StatusCode::BAD_REQUEST, e.to_string()))?;
    app.cache.lock().unwrap().retain(|k, _| !k.starts_with(&format!("{}:", u.id)));
    Ok(Json(json!({ "jumlah": n })))
}

/// KTM orang lain ditap: hanya profil publik, tanpa jadwal/nilai.
async fn kenalan(State(app): State<App>, hd: HeaderMap, Json(b): Json<Uid>) -> Hasil {
    batasi(&app, &format!("kenalan:{}", ip(&hd)), 30, 60)?;
    let h = uid_hash(&app, &b.uid)?;
    let c = app.db.lock().unwrap();
    let r: Option<(i64, String, String, String, String, String, bool, String, Option<String>)> = c
        .query_row(
            "SELECT id, nama, prodi, bio, github, portofolio, publik, dibuat, foto FROM users WHERE uid_hash = ?1 AND peran = 'mhs'",
            [h],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)),
        )
        .optional()?;
    let Some((id, nama, prodi, bio, github, portofolio, publik, dibuat, foto)) = r else {
        return Err(galat(StatusCode::NOT_FOUND, "KTM ini belum terdaftar di Smart KTM"));
    };
    let (hilang, pesan): (bool, Option<String>) = c.query_row("SELECT hilang, hilang_pesan FROM users WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    if hilang {
        return Ok(Json(json!({ "hilang": true, "nama": nama_depan(&nama), "pesan": pesan.unwrap_or_default() })));
    }
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
    let n_sertifikat: i64 = c.query_row("SELECT COUNT(*) FROM sertifikat WHERE user_id = ?1", [id], |r| r.get(0))?;
    if n_sertifikat > 0 {
        lencana.insert(0, format!("{n_sertifikat} sertifikat on-chain"));
    }
    Ok(Json(json!({
        "privat": false, "nama": nama, "prodi": prodi, "kampus": "Universitas Pamulang", "foto": foto,
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
        batasi(&app, &format!("tap:{}", u.id), 120, 60)?;
        let (matkul_id, terbuka) = sesi_milik(&c, id, u.id)?;
        if !terbuka {
            return Err(galat(StatusCode::CONFLICT, "Sesi sudah ditutup"));
        }
        let (mid, nama, nim, foto) = mahasiswa_dari_ktm(&c, &mhs_hash)?;
        let ikut: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM ambil WHERE user_id = ?1 AND matkul_id = ?2)", params![mid, matkul_id], |r| r.get(0))?;
        if !ikut {
            // Belum sinkron MyUnpam tapi hadir di kelas: dosen yang melihat langsung, jadi dimasukkan ke kelas.
            c.execute("INSERT INTO ambil(user_id, matkul_id) VALUES(?1, ?2)", params![mid, matkul_id])?;
        }
        let lama: Option<(String, String, Option<String>)> = c
            .query_row("SELECT waktu, bukti, tx FROM presensi WHERE sesi_id = ?1 AND user_id = ?2", params![id, mid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        let dasar = json!({ "nama": nama, "nim": samarkan_nim(&nim), "foto": foto });
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

/// Mahasiswa pemilik KTM untuk presensi/sertifikat; KTM hilang ditolak.
fn mahasiswa_dari_ktm(c: &Connection, hash: &str) -> Result<(i64, String, String, Option<String>), Gagal> {
    let mhs: Option<(i64, String, String, Option<String>, bool)> = c
        .query_row("SELECT id, nama, nim, foto, hilang FROM users WHERE uid_hash = ?1 AND peran = 'mhs'", [hash], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .optional()?;
    match mhs {
        None => Err(galat(StatusCode::NOT_FOUND, "KTM belum terdaftar. Minta mahasiswa daftar dulu di Smart KTM.")),
        Some((.., true)) => Err(galat(StatusCode::FORBIDDEN, "KTM ini dilaporkan hilang. Jangan dipakai, kembalikan ke pemiliknya.")),
        Some((id, nama, nim, foto, false)) => Ok((id, nama, nim, foto)),
    }
}

// ---------- KTM hilang ----------

#[derive(Deserialize)]
struct LaporHilang {
    hilang: bool,
    #[serde(default)]
    pesan: String,
}

async fn lapor_hilang(State(app): State<App>, h: HeaderMap, Json(b): Json<LaporHilang>) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    let pesan: String = b.pesan.trim().chars().take(160).collect();
    c.execute(
        "UPDATE users SET hilang = ?1, hilang_pesan = ?2 WHERE id = ?3",
        params![b.hilang, if b.hilang && !pesan.is_empty() { Some(pesan) } else { None }, u.id],
    )?;
    Ok(Json(json!({ "hilang": b.hilang })))
}

#[derive(Deserialize)]
struct Keamanan {
    pin_tap: bool,
}

async fn atur_keamanan(State(app): State<App>, h: HeaderMap, Json(b): Json<Keamanan>) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    c.execute("UPDATE users SET pin_tap = ?1 WHERE id = ?2", params![b.pin_tap, u.id])?;
    Ok(Json(json!({ "pin_tap": b.pin_tap })))
}

// ---------- acara & sertifikat ----------

#[derive(Deserialize)]
struct AcaraBaru {
    nama: String,
    penyelenggara: String,
    #[serde(default)]
    tempat: String,
    tanggal: String,
}

async fn buat_acara(State(app): State<App>, h: HeaderMap, Json(b): Json<AcaraBaru>) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    wajib(&u, "dosen")?;
    let potong = |s: &str, n: usize| s.trim().chars().take(n).collect::<String>();
    let (nama, penyelenggara) = (potong(&b.nama, 100), potong(&b.penyelenggara, 80));
    if nama.is_empty() || penyelenggara.is_empty() {
        return Err(galat(StatusCode::BAD_REQUEST, "Nama acara dan penyelenggara wajib diisi"));
    }
    if chrono::NaiveDate::parse_from_str(b.tanggal.trim(), "%Y-%m-%d").is_err() {
        return Err(galat(StatusCode::BAD_REQUEST, "Tanggal harus YYYY-MM-DD"));
    }
    c.execute(
        "INSERT INTO acara(nama, penyelenggara, tempat, tanggal, dibuat_oleh, dibuat) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        params![nama, penyelenggara, potong(&b.tempat, 80), b.tanggal.trim(), u.id, waktu::teks(&waktu::sekarang())],
    )?;
    Ok(Json(json!({ "id": c.last_insert_rowid() })))
}

async fn daftar_acara(State(app): State<App>, h: HeaderMap) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    wajib(&u, "dosen")?;
    let mut q = c.prepare(
        "SELECT a.id, a.nama, a.penyelenggara, a.tempat, a.tanggal, a.dibuka, (SELECT COUNT(*) FROM sertifikat s WHERE s.acara_id = a.id)
         FROM acara a WHERE a.dibuat_oleh = ?1 ORDER BY a.id DESC",
    )?;
    let acara = q
        .query_map([u.id], |r| {
            Ok(json!({ "id": r.get::<_, i64>(0)?, "nama": r.get::<_, String>(1)?, "penyelenggara": r.get::<_, String>(2)?,
                       "tempat": r.get::<_, String>(3)?, "tanggal": r.get::<_, String>(4)?, "dibuka": r.get::<_, bool>(5)?,
                       "jumlah": r.get::<_, i64>(6)? }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Json(json!({ "acara": acara })))
}

fn acara_milik(c: &Connection, id: i64, user: i64) -> Result<(String, String, String, String, bool), Gagal> {
    c.query_row(
        "SELECT nama, penyelenggara, tempat, tanggal, dibuka FROM acara WHERE id = ?1 AND dibuat_oleh = ?2",
        params![id, user],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )
    .optional()?
    .ok_or_else(|| galat(StatusCode::NOT_FOUND, "Acara tidak ditemukan"))
}

async fn lihat_acara(State(app): State<App>, h: HeaderMap, Path(id): Path<i64>) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    wajib(&u, "dosen")?;
    let (nama, penyelenggara, tempat, tanggal, dibuka) = acara_milik(&c, id, u.id)?;
    let mut q = c.prepare(
        "SELECT u.nama, u.nim, s.waktu, s.status, s.tx, s.kode FROM sertifikat s JOIN users u ON u.id = s.user_id
         WHERE s.acara_id = ?1 ORDER BY s.waktu DESC",
    )?;
    let peserta = q
        .query_map([id], |r| {
            let tx: Option<String> = r.get(4)?;
            Ok(json!({ "nama": r.get::<_, String>(0)?, "nim": samarkan_nim(&r.get::<_, String>(1)?), "waktu": r.get::<_, String>(2)?,
                       "status": r.get::<_, String>(3)?, "link_tx": tx.as_deref().and_then(|t| app.chain.as_ref().and_then(|c| c.link_tx(t))),
                       "tx": tx, "kode": r.get::<_, String>(5)? }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Json(json!({
        "id": id, "nama": nama, "penyelenggara": penyelenggara, "tempat": tempat, "tanggal": tanggal, "dibuka": dibuka,
        "peserta": peserta, "kontrak": app.chain.as_ref().and_then(|c| c.link_kontrak_sertifikat()),
    })))
}

async fn tap_acara(State(app): State<App>, h: HeaderMap, Path(id): Path<i64>, Json(b): Json<Uid>) -> Hasil {
    let hash = uid_hash(&app, &b.uid)?;
    let (balasan, kirim) = {
        let c = app.db.lock().unwrap();
        let u = pengguna(&c, &h)?;
        wajib(&u, "dosen")?;
        batasi(&app, &format!("tap:{}", u.id), 120, 60)?;
        let (.., dibuka) = acara_milik(&c, id, u.id)?;
        if !dibuka {
            return Err(galat(StatusCode::CONFLICT, "Check-in acara sudah ditutup"));
        }
        let (mid, nama, nim, foto) = mahasiswa_dari_ktm(&c, &hash)?;
        let dasar = json!({ "nama": nama, "nim": samarkan_nim(&nim), "foto": foto });
        let lama: Option<(String, String)> = c
            .query_row("SELECT waktu, kode FROM sertifikat WHERE acara_id = ?1 AND user_id = ?2", params![id, mid], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        if let Some((saat, kode)) = lama {
            let mut v = dasar;
            v["status"] = json!("sudah");
            v["waktu"] = json!(saat);
            v["kode"] = json!(kode);
            return Ok(Json(v));
        }
        let saat = waktu::teks(&waktu::sekarang());
        let kode = db::sha(&format!("{}:sertifikat:{id}:{mid}:{saat}", app.rahasia));
        let ke_chain = app.chain.as_ref().is_some_and(|c| c.ada_sertifikat());
        c.execute(
            "INSERT INTO sertifikat(acara_id, user_id, waktu, kode, status) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![id, mid, saat, kode, if ke_chain { "menunggu" } else { "lokal" }],
        )?;
        let sid = c.last_insert_rowid();
        let mut v = dasar;
        v["status"] = json!("terbit");
        v["waktu"] = json!(saat);
        v["kode"] = json!(kode);
        (v, ke_chain.then(|| (sid, db::sha(&format!("{}:acara:{id}", app.rahasia)), kode)))
    };
    if let (Some(ch), Some((sid, acara_hash, kode))) = (app.chain.clone(), kirim) {
        let db = app.db.clone();
        tokio::spawn(async move {
            let (tx, status) = match ch.terbitkan(&acara_hash, &hash, &kode).await {
                Ok(tx) => (Some(tx), "onchain"),
                Err(e) => {
                    eprintln!("sertifikat gagal: {e:#}");
                    (None, "gagal")
                }
            };
            let _ = db.lock().unwrap().execute("UPDATE sertifikat SET tx = ?1, status = ?2 WHERE id = ?3", params![tx, status, sid]);
        });
    }
    Ok(Json(balasan))
}

async fn tutup_acara(State(app): State<App>, h: HeaderMap, Path(id): Path<i64>) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    wajib(&u, "dosen")?;
    acara_milik(&c, id, u.id)?;
    c.execute("UPDATE acara SET dibuka = 0 WHERE id = ?1", [id])?;
    Ok(Json(json!({ "ok": true })))
}

async fn sertifikat_saya(State(app): State<App>, h: HeaderMap) -> Hasil {
    let c = app.db.lock().unwrap();
    let u = pengguna(&c, &h)?;
    let mut q = c.prepare(
        "SELECT s.kode, a.nama, a.penyelenggara, a.tanggal, s.status, s.tx FROM sertifikat s JOIN acara a ON a.id = s.acara_id
         WHERE s.user_id = ?1 ORDER BY a.tanggal DESC, s.id DESC",
    )?;
    let daftar = q
        .query_map([u.id], |r| {
            let tx: Option<String> = r.get(5)?;
            Ok(json!({ "kode": r.get::<_, String>(0)?, "acara": r.get::<_, String>(1)?, "penyelenggara": r.get::<_, String>(2)?,
                       "tanggal": r.get::<_, String>(3)?, "status": r.get::<_, String>(4)?,
                       "link_tx": tx.as_deref().and_then(|t| app.chain.as_ref().and_then(|c| c.link_tx(t))) }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Json(json!({ "sertifikat": daftar })))
}

/// Halaman publik sertifikat: siapa pun dengan tautannya bisa memverifikasi ke blockchain.
async fn lihat_sertifikat(State(app): State<App>, Path(kode): Path<String>) -> Hasil {
    let kode = kode.trim_start_matches("0x").to_lowercase();
    if kode.len() != 64 || !kode.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(galat(StatusCode::BAD_REQUEST, "Kode sertifikat tidak valid"));
    }
    let rec: Option<(String, String, Option<String>, String, String, String, String, String, Option<String>, String)> = {
        let c = app.db.lock().unwrap();
        c.query_row(
            "SELECT u.nama, u.prodi, u.foto, a.nama, a.penyelenggara, a.tempat, a.tanggal, s.waktu, s.tx, s.status
             FROM sertifikat s JOIN users u ON u.id = s.user_id JOIN acara a ON a.id = s.acara_id WHERE s.kode = ?1",
            [&kode],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?)),
        )
        .optional()?
    };
    let Some((nama, prodi, foto, acara, penyelenggara, tempat, tanggal, saat, tx, status)) = rec else {
        return Err(galat(StatusCode::NOT_FOUND, "Sertifikat tidak ditemukan"));
    };
    let mut chain = Value::Null;
    if let Some(ch) = app.chain.as_ref().filter(|c| c.ada_sertifikat()) {
        chain = match ch.cek_sertifikat(&kode).await {
            Ok(ts) => json!({ "tercatat": ts > 0, "timestamp": ts }),
            Err(e) => json!({ "galat": e.to_string() }),
        };
    }
    Ok(Json(json!({
        "kode": kode, "nama": nama, "prodi": prodi, "foto": foto, "acara": acara, "penyelenggara": penyelenggara,
        "tempat": tempat, "tanggal": tanggal, "waktu": saat, "status": status, "tx": tx,
        "link_tx": tx.as_deref().and_then(|t| app.chain.as_ref().and_then(|c| c.link_tx(t))),
        "kontrak": app.chain.as_ref().and_then(|c| c.link_kontrak_sertifikat()), "chain": chain,
    })))
}

#[cfg(test)]
mod tes {
    use super::*;

    #[test]
    fn ktm_hilang_ditolak_untuk_presensi_dan_sertifikat() {
        let c = Connection::open_in_memory().unwrap();
        db::siapkan(&c).unwrap();
        c.execute("INSERT INTO users(peran, nim, nama, pin_hash, uid_hash, dibuat) VALUES('mhs', '231011409998', 'Uji Hilang', 'x', 'abc', '2026-10-01 00:00')", []).unwrap();
        assert!(mahasiswa_dari_ktm(&c, "abc").is_ok());
        c.execute("UPDATE users SET hilang = 1 WHERE uid_hash = 'abc'", []).unwrap();
        let Err(Gagal(kode, pesan)) = mahasiswa_dari_ktm(&c, "abc") else { panic!("KTM hilang harus ditolak") };
        assert_eq!(kode, StatusCode::FORBIDDEN);
        assert!(pesan.contains("hilang"));
        assert!(mahasiswa_dari_ktm(&c, "tidak-ada").is_err());
    }

    #[test]
    fn tap_tanpa_pin_hanya_di_hp_tepercaya() {
        let c = Connection::open_in_memory().unwrap();
        db::siapkan(&c).unwrap();
        let (a, b): (i64, i64) = c.query_row("SELECT MIN(id), MAX(id) FROM users WHERE peran = 'mhs'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        let hp = perangkat_baru(&c, a).unwrap();
        assert!(perangkat_sah(&c, a, &[hp.as_str()]).unwrap());
        assert!(!perangkat_sah(&c, b, &[hp.as_str()]).unwrap(), "token HP milik akun lain tidak berlaku");
        assert!(!perangkat_sah(&c, a, &[]).unwrap());
        assert!(!perangkat_sah(&c, a, &["tebakan"]).unwrap());
    }

    #[test]
    fn satu_hp_dipakai_bergantian_dua_akun() {
        let c = Connection::open_in_memory().unwrap();
        db::siapkan(&c).unwrap();
        let (a, b): (i64, i64) = c.query_row("SELECT MIN(id), MAX(id) FROM users WHERE peran = 'mhs'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        let hp_a = perangkat_baru(&c, a).unwrap();
        let hp_b = perangkat_baru(&c, b).unwrap();
        // Teman daftar di HP ini sesudah pemilik: token pemilik tidak boleh hilang.
        let disimpan: TokenHp = serde_json::from_value(json!([hp_b, hp_a])).unwrap();
        assert!(perangkat_sah(&c, a, &disimpan.daftar()).unwrap());
        assert!(perangkat_sah(&c, b, &disimpan.daftar()).unwrap());
        // Versi lama mengirim satu teks.
        let lama: TokenHp = serde_json::from_value(json!(hp_a)).unwrap();
        assert!(perangkat_sah(&c, a, &lama.daftar()).unwrap());
    }
}

/// Kirim ulang presensi & sertifikat yang gagal atau macet ke blockchain.
/// Dicek dulu ke kontrak supaya yang ternyata sudah tercatat tidak dikirim dua kali.
pub async fn coba_ulang_chain(app: &App) {
    let Some(ch) = app.chain.clone() else { return };
    let macet = waktu::teks(&(waktu::sekarang() - chrono::Duration::minutes(3)));
    let (presensi, sertifikat) = {
        let c = app.db.lock().unwrap();
        let ambil = |sql: &str| -> Vec<(i64, i64, String, String)> {
            c.prepare(sql)
                .and_then(|mut q| q.query_map([&macet], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect())
                .unwrap_or_default()
        };
        (
            ambil("SELECT p.id, p.sesi_id, u.uid_hash, p.bukti FROM presensi p JOIN users u ON u.id = p.user_id
                   WHERE (p.status = 'gagal' OR (p.status = 'menunggu' AND p.waktu < ?1)) AND u.uid_hash IS NOT NULL LIMIT 20"),
            if ch.ada_sertifikat() {
                ambil("SELECT s.id, s.acara_id, u.uid_hash, s.kode FROM sertifikat s JOIN users u ON u.id = s.user_id
                       WHERE (s.status = 'gagal' OR (s.status = 'menunggu' AND s.waktu < ?1)) AND u.uid_hash IS NOT NULL LIMIT 20")
            } else {
                vec![]
            },
        )
    };
    for (id, sesi, mhs, bukti) in presensi {
        let hasil = match ch.cek(&bukti).await {
            Ok(ts) if ts > 0 => Ok(None),
            _ => ch.catat(&db::sha(&format!("{}:sesi:{sesi}", app.rahasia)), &mhs, &bukti).await.map(Some),
        };
        simpan_hasil(app, "presensi", id, hasil);
    }
    for (id, acara, mhs, kode) in sertifikat {
        let hasil = match ch.cek_sertifikat(&kode).await {
            Ok(ts) if ts > 0 => Ok(None),
            _ => ch.terbitkan(&db::sha(&format!("{}:acara:{acara}", app.rahasia)), &mhs, &kode).await.map(Some),
        };
        simpan_hasil(app, "sertifikat", id, hasil);
    }
}

fn simpan_hasil(app: &App, tabel: &str, id: i64, hasil: anyhow::Result<Option<String>>) {
    let c = app.db.lock().unwrap();
    let _ = match hasil {
        Ok(Some(tx)) => c.execute(&format!("UPDATE {tabel} SET tx = ?1, status = 'onchain' WHERE id = ?2"), params![tx, id]),
        Ok(None) => c.execute(&format!("UPDATE {tabel} SET status = 'onchain' WHERE id = ?1"), [id]),
        Err(e) => {
            eprintln!("coba ulang {tabel} {id} gagal: {e:#}");
            c.execute(&format!("UPDATE {tabel} SET status = 'gagal' WHERE id = ?1"), [id])
        }
    };
}
