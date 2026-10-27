//! Skema SQLite + data contoh (prototipe, tidak terhubung ke sistem kampus).

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::Duration;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use crate::waktu;

const SKEMA: &str = "
PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;
CREATE TABLE IF NOT EXISTS users(
  id INTEGER PRIMARY KEY, peran TEXT NOT NULL, nim TEXT NOT NULL UNIQUE, nama TEXT NOT NULL,
  prodi TEXT NOT NULL DEFAULT '', pin_hash TEXT NOT NULL, uid_hash TEXT UNIQUE,
  bio TEXT NOT NULL DEFAULT '', github TEXT NOT NULL DEFAULT '', portofolio TEXT NOT NULL DEFAULT '',
  publik INTEGER NOT NULL DEFAULT 1, dibuat TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sesi_login(token TEXT PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id), dibuat TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS matkul(
  id INTEGER PRIMARY KEY, kode TEXT NOT NULL UNIQUE, nama TEXT NOT NULL, sks INTEGER NOT NULL,
  dosen_id INTEGER NOT NULL REFERENCES users(id), hari INTEGER NOT NULL, mulai TEXT NOT NULL,
  selesai TEXT NOT NULL, ruang TEXT NOT NULL, batas_absen INTEGER NOT NULL DEFAULT 4, dosen_nama TEXT);
CREATE TABLE IF NOT EXISTS ambil(user_id INTEGER NOT NULL REFERENCES users(id), matkul_id INTEGER NOT NULL REFERENCES matkul(id), PRIMARY KEY(user_id, matkul_id));
CREATE TABLE IF NOT EXISTS tugas(id INTEGER PRIMARY KEY, matkul_id INTEGER NOT NULL REFERENCES matkul(id), judul TEXT NOT NULL, deskripsi TEXT NOT NULL, deadline TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS tugas_selesai(user_id INTEGER NOT NULL, tugas_id INTEGER NOT NULL, waktu TEXT NOT NULL, PRIMARY KEY(user_id, tugas_id));
CREATE TABLE IF NOT EXISTS sesi(id INTEGER PRIMARY KEY, matkul_id INTEGER NOT NULL REFERENCES matkul(id), pertemuan INTEGER NOT NULL, dibuka TEXT NOT NULL, ditutup TEXT);
CREATE TABLE IF NOT EXISTS presensi(
  id INTEGER PRIMARY KEY, sesi_id INTEGER NOT NULL REFERENCES sesi(id), user_id INTEGER NOT NULL REFERENCES users(id),
  waktu TEXT NOT NULL, bukti TEXT NOT NULL UNIQUE, tx TEXT, status TEXT NOT NULL, UNIQUE(sesi_id, user_id));
CREATE TABLE IF NOT EXISTS pertemuan_unpam(
  user_id INTEGER NOT NULL REFERENCES users(id), matkul_id INTEGER NOT NULL REFERENCES matkul(id), ke INTEGER NOT NULL,
  jenis TEXT NOT NULL DEFAULT '', mulai TEXT NOT NULL, akhir TEXT NOT NULL, status TEXT, waktu TEXT,
  PRIMARY KEY(user_id, matkul_id, ke));
CREATE TABLE IF NOT EXISTS acara(
  id INTEGER PRIMARY KEY, nama TEXT NOT NULL, penyelenggara TEXT NOT NULL, tempat TEXT NOT NULL DEFAULT '',
  tanggal TEXT NOT NULL, dibuat_oleh INTEGER NOT NULL REFERENCES users(id), dibuka INTEGER NOT NULL DEFAULT 1, dibuat TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sertifikat(
  id INTEGER PRIMARY KEY, acara_id INTEGER NOT NULL REFERENCES acara(id), user_id INTEGER NOT NULL REFERENCES users(id),
  waktu TEXT NOT NULL, kode TEXT NOT NULL UNIQUE, tx TEXT, status TEXT NOT NULL, UNIQUE(acara_id, user_id));
CREATE TABLE IF NOT EXISTS perangkat(kode_hash TEXT PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id), dibuat TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS pengingat(id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id), teks TEXT NOT NULL, waktu TEXT NOT NULL, dibuat TEXT NOT NULL);
";

pub fn sha(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// UID KTM tidak pernah disimpan mentah: hanya hash bergaram rahasia server.
pub fn hash_uid(rahasia: &str, uid: &str) -> Option<String> {
    let bersih: String = uid.chars().filter(|c| c.is_ascii_hexdigit()).collect::<String>().to_lowercase();
    if bersih.len() < 8 || bersih.len() > 20 {
        return None;
    }
    Some(sha(&format!("{rahasia}:ktm:{bersih}")))
}

pub fn hash_pin(pin: &str) -> String {
    let garam = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(pin.as_bytes(), &garam).unwrap().to_string()
}

pub fn cek_pin(pin: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| Argon2::default().verify_password(pin.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

pub fn rahasia(data_dir: &str) -> anyhow::Result<String> {
    if let Ok(r) = std::env::var("KTM_RAHASIA") {
        if !r.is_empty() {
            return Ok(r);
        }
    }
    let path = format!("{data_dir}/rahasia.txt");
    if let Ok(r) = std::fs::read_to_string(&path) {
        return Ok(r.trim().to_string());
    }
    let r = hex::encode(rand::random::<[u8; 32]>());
    std::fs::write(&path, &r)?;
    Ok(r)
}

pub fn siapkan(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch(SKEMA)?;
    // Database lama belum punya kolom dosen_nama (nama dosen asli hasil impor).
    let punya: bool = c.query_row("SELECT COUNT(*) > 0 FROM pragma_table_info('matkul') WHERE name = 'dosen_nama'", [], |r| r.get(0))?;
    if !punya {
        c.execute("ALTER TABLE matkul ADD COLUMN dosen_nama TEXT", [])?;
    }
    // Foto profil & semester dari MyUnpam.
    for (kolom, tipe) in [("foto", "TEXT"), ("semester", "TEXT"), ("hilang", "INTEGER NOT NULL DEFAULT 0"), ("hilang_pesan", "TEXT"), ("pin_tap", "INTEGER NOT NULL DEFAULT 0")] {
        let ada: bool = c.query_row("SELECT COUNT(*) > 0 FROM pragma_table_info('users') WHERE name = ?1", [kolom], |r| r.get(0))?;
        if !ada {
            c.execute(&format!("ALTER TABLE users ADD COLUMN {kolom} {tipe}"), [])?;
        }
    }
    c.busy_timeout(std::time::Duration::from_secs(5))?;
    let ada: i64 = c.query_row("SELECT COUNT(*) FROM matkul", [], |r| r.get(0))?;
    if ada == 0 {
        isi_contoh(c)?;
    }
    Ok(())
}

fn env(k: &str, d: &str) -> String {
    std::env::var(k).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| d.to_string())
}

fn isi_contoh(c: &Connection) -> rusqlite::Result<()> {
    let kini = waktu::sekarang();
    let lama = waktu::teks(&(kini - Duration::days(365)));
    let tambah_user = |peran: &str, nim: &str, nama: &str, prodi: &str, pin: &str, uid: Option<String>| {
        c.execute(
            "INSERT INTO users(peran,nim,nama,prodi,pin_hash,uid_hash,dibuat) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![peran, nim, nama, prodi, hash_pin(pin), uid, lama],
        )
        .map(|_| c.last_insert_rowid())
    };

    let dosen_demo = tambah_user("dosen", &env("DOSEN_NIDN", "0400000001"), "Dosen Demo", "", &env("DOSEN_PIN", "123456"), None)?;
    let dosen_lain: Vec<i64> = ["Dr. Andi Pratama, M.Kom.", "Siti Rahmawati, M.T.", "Budi Santoso, M.Kom.", "Dewi Lestari, M.Si."]
        .iter()
        .enumerate()
        .map(|(i, n)| tambah_user("dosen", &format!("04000001{i:02}"), n, "", &hex::encode(rand::random::<[u8; 8]>()), None))
        .collect::<Result<_, _>>()?;

    // Satu kelas tiap hari supaya beranda selalu punya isi saat demo.
    let matkul = [
        ("TPL0501", "Pemrograman Web II", 3, dosen_demo, 1, "08:00", "10:30", "V.402"),
        ("TPL0502", "Basis Data II", 3, dosen_lain[0], 1, "13:00", "15:30", "V.305"),
        ("TPL0503", "Rekayasa Perangkat Lunak", 3, dosen_demo, 2, "10:00", "12:30", "V.501"),
        ("TPL0504", "Kecerdasan Buatan", 3, dosen_lain[1], 3, "08:00", "10:30", "V.403"),
        ("TPL0505", "Jaringan Komputer", 3, dosen_lain[2], 4, "13:00", "15:30", "Lab 2"),
        ("TPL0506", "Interaksi Manusia dan Komputer", 2, dosen_lain[3], 5, "08:00", "09:40", "V.304"),
        ("TPL0507", "Metodologi Penelitian", 2, dosen_demo, 6, "08:00", "09:40", "V.401"),
        ("TPL0508", "Statistika", 2, dosen_lain[0], 7, "13:00", "14:40", "V.302"),
    ];
    let mut id_matkul = vec![];
    for (kode, nama, sks, dosen, hari, mulai, selesai, ruang) in matkul {
        c.execute(
            "INSERT INTO matkul(kode,nama,sks,dosen_id,hari,mulai,selesai,ruang) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![kode, nama, sks, dosen, hari, mulai, selesai, ruang],
        )?;
        id_matkul.push(c.last_insert_rowid());
    }

    // Mahasiswa demo: KTM asli didaftarkan ke akun ini lewat halaman daftar (NIM + PIN).
    let demo = tambah_user("mhs", &env("DEMO_NIM", "231011400001"), &env("DEMO_NAMA", "Rahmat"), "Teknik Informatika", &env("DEMO_PIN", "123456"), None)?;
    let mut mhs = vec![demo];
    for (i, n) in ["Andi Wijaya", "Siti Aulia", "Budi Hartono", "Dewi Anggraini", "Fajar Nugroho", "Nadia Putri"].iter().enumerate() {
        let uid = Some(sha(&format!("kartu-contoh-{i}")));
        mhs.push(tambah_user("mhs", &format!("2310114000{:02}", i + 10), n, "Teknik Informatika", &hex::encode(rand::random::<[u8; 8]>()), uid)?);
    }
    for &u in &mhs {
        for &m in &id_matkul {
            c.execute("INSERT INTO ambil(user_id,matkul_id) VALUES(?1,?2)", params![u, m])?;
        }
    }

    // Riwayat 10 pertemuan terakhir tiap matkul (sebelum hari ini).
    let hari_ini = waktu::hari_ke(&kini) as i64;
    for (idx, (_, _, _, _, hari, mulai, selesai, _)) in matkul.iter().enumerate() {
        let mut mundur = (hari_ini - *hari as i64).rem_euclid(7);
        if mundur == 0 {
            mundur = 7;
        }
        for p in 0..10i64 {
            let tgl = (kini - Duration::days(mundur + 7 * (9 - p))).format("%Y-%m-%d").to_string();
            c.execute(
                "INSERT INTO sesi(matkul_id,pertemuan,dibuka,ditutup) VALUES(?1,?2,?3,?4)",
                params![id_matkul[idx], p + 1, format!("{tgl} {mulai}"), format!("{tgl} {selesai}")],
            )?;
            let sesi = c.last_insert_rowid();
            for (k, &u) in mhs.iter().enumerate() {
                let absen = if u == demo {
                    (idx == 1 && [2, 5, 8].contains(&p)) || (idx == 7 && p == 0)
                } else {
                    (p as usize * 3 + k * 5 + idx) % 11 == 0
                };
                if absen {
                    continue;
                }
                let jam = format!("{tgl} {mulai}");
                c.execute(
                    "INSERT INTO presensi(sesi_id,user_id,waktu,bukti,status) VALUES(?1,?2,?3,?4,'riwayat')",
                    params![sesi, u, jam, sha(&format!("riwayat:{sesi}:{u}"))],
                )?;
            }
        }
    }

    let tgl = |hari: i64, jam: &str| format!("{} {jam}", (kini + Duration::days(hari)).format("%Y-%m-%d"));
    let tugas = [
        (1, "ERD Sistem Perpustakaan", "Buat ERD lengkap dengan kardinalitas, kumpulkan dalam PDF.", tgl(1, "23:59")),
        (0, "CRUD PHP + MySQL", "Halaman tambah, ubah, hapus data mahasiswa dengan validasi form.", tgl(3, "23:59")),
        (4, "Konfigurasi VLAN", "Simulasi 3 VLAN di Cisco Packet Tracer, lampirkan file .pkt.", tgl(2, "23:59")),
        (3, "Resume Jurnal AI Agent", "Resume 1 jurnal tentang AI agent (maks 2 halaman).", tgl(5, "21:00")),
        (2, "Dokumen SRS Kelompok", "Spesifikasi kebutuhan perangkat lunak sesuai IEEE 830.", tgl(8, "23:59")),
        (6, "Draft Bab I", "Latar belakang, rumusan masalah, tujuan penelitian.", tgl(-2, "23:59")),
    ];
    for (m, judul, desk, deadline) in &tugas {
        c.execute(
            "INSERT INTO tugas(matkul_id,judul,deskripsi,deadline) VALUES(?1,?2,?3,?4)",
            params![id_matkul[*m], judul, desk, deadline],
        )?;
    }
    let draft: i64 = c.query_row("SELECT id FROM tugas WHERE judul='Draft Bab I'", [], |r| r.get(0))?;
    c.execute("INSERT INTO tugas_selesai(user_id,tugas_id,waktu) VALUES(?1,?2,?3)", params![demo, draft, tgl(-3, "20:00")])?;
    Ok(())
}

#[cfg(test)]
mod tes {
    use super::*;

    #[test]
    fn uid_dinormalisasi() {
        let a = hash_uid("r", "11:00:1C:AA").unwrap();
        assert_eq!(a, hash_uid("r", "11001caa").unwrap());
        assert_ne!(a, hash_uid("lain", "11001caa").unwrap());
        assert!(hash_uid("r", "12").is_none());
    }

    #[test]
    fn pin_diverifikasi() {
        let h = hash_pin("123456");
        assert!(cek_pin("123456", &h));
        assert!(!cek_pin("654321", &h));
    }

    #[test]
    fn data_contoh_lengkap() {
        let c = Connection::open_in_memory().unwrap();
        siapkan(&c).unwrap();
        let n: i64 = c.query_row("SELECT COUNT(*) FROM matkul", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 8);
        let absen_bd: i64 = c
            .query_row(
                "SELECT 10 - COUNT(*) FROM presensi p JOIN sesi s ON s.id=p.sesi_id JOIN matkul m ON m.id=s.matkul_id
                 WHERE m.kode='TPL0502' AND p.user_id=(SELECT id FROM users WHERE nama=?1)",
                [env("DEMO_NAMA", "Rahmat")],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(absen_bd, 3);
    }
}
