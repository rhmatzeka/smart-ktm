//! Sinkron jadwal kuliah dari MyUnpam (per mahasiswa).
//!
//! Data dikirim oleh bookmark "Sinkron Smart KTM" yang dijalankan pengguna di
//! my.unpam.ac.id setelah login (lihat web/src/pages/sinkron.astro). Kelas
//! dicocokkan lewat kode matkul + kode kelas MyUnpam, jadi teman sekelas yang
//! sinkron otomatis masuk kelas yang sama.
//!
//! Di aplikasi Android, sinkron yang sama juga mengambil tugas & aktivitas Mentari
//! (lihat [`sinkron_mentari`]); tugas dicocokkan ke matkul lewat nama.

use anyhow::{bail, Context};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;

use crate::waktu;

#[derive(Deserialize)]
pub struct DataUnpam {
    pub nim: String,
    #[serde(default)]
    pub nama: String,
    #[serde(default)]
    pub prodi: String,
    #[serde(default)]
    pub semester: String,
    /// Foto profil MyUnpam sebagai data URL JPEG kecil (diperkecil di bookmark).
    #[serde(default)]
    pub foto: String,
    pub kelas: Vec<KelasUnpam>,
    /// Tugas & aktivitas Mentari yang belum dikerjakan (kosong kalau Mentari dilewati).
    #[serde(default)]
    pub mentari: Vec<TugasMentari>,
}

#[derive(Deserialize)]
pub struct KelasUnpam {
    pub kode: String,
    #[serde(default)]
    pub kelas: String,
    pub nama: String,
    #[serde(default)]
    pub sks: i64,
    pub hari: String,
    /// Seperti di MyUnpam: "08.50 - 10.30"
    pub jam: String,
    #[serde(default)]
    pub ruang: String,
    #[serde(default)]
    pub dosen: String,
    /// Status presensi per pertemuan dari menu Presensi MyUnpam.
    #[serde(default)]
    pub pertemuan: Vec<PertemuanUnpam>,
}

#[derive(Deserialize)]
pub struct PertemuanUnpam {
    pub ke: i64,
    #[serde(default)]
    pub jenis: String,
    /// Periode presensi: tanggal "YYYY-MM-DD"
    pub mulai: String,
    pub akhir: String,
    /// "hadir", status lain dari MyUnpam, atau null kalau belum presensi
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub waktu: Option<String>,
}

/// "08.50 - 10.30" -> ("08:50", "10:30")
#[derive(Deserialize)]
pub struct TugasMentari {
    pub id: String,
    pub matkul: String,
    pub ke: i64,
    /// Tugas, Pretest, Posttest, Forum, Kuesioner
    pub jenis: String,
    pub judul: String,
    #[serde(default)]
    pub isi: String,
    #[serde(default)]
    pub selesai: bool,
    /// Tenggat dari Mentari kalau ada (ISO/UTC atau "YYYY-MM-DD HH:MM").
    #[serde(default)]
    pub batas: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

/// Simpan tugas Mentari ke matkul yang diambil pengguna. Tenggat yang tidak diberikan Mentari
/// diperkirakan dari akhir pekan pertemuan itu di MyUnpam. Mengembalikan jumlah tugas yang tersimpan.
pub fn sinkron_mentari(c: &Connection, user: i64, tugas: &[TugasMentari]) -> anyhow::Result<usize> {
    let kunci = |s: &str| s.to_lowercase().split(|ch: char| !ch.is_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    let matkul: Vec<(i64, String)> = c
        .prepare("SELECT m.id, m.nama FROM matkul m JOIN ambil a ON a.matkul_id = m.id WHERE a.user_id = ?1")?
        .query_map([user], |r| Ok((r.get(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut n = 0;
    for t in tugas {
        let Some(m) = matkul.iter().find(|(_, nama)| kunci(nama) == kunci(&t.matkul)).map(|(id, _)| *id) else { continue };
        let tenggat = match t.batas.as_deref().and_then(baca_tenggat) {
            Some(b) => b,
            None => c
                .query_row("SELECT akhir FROM pertemuan_unpam WHERE user_id = ?1 AND matkul_id = ?2 AND ke = ?3", params![user, m, t.ke], |r| r.get::<_, String>(0))
                .optional()?
                .map(|a| format!("{a} 23:59"))
                .unwrap_or_default(),
        };
        let sumber = format!("mentari:{}", t.id);
        c.execute(
            "INSERT INTO tugas(matkul_id, judul, deskripsi, deadline, sumber, jenis, url) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(sumber) DO UPDATE SET judul = excluded.judul, deskripsi = excluded.deskripsi,
               deadline = excluded.deadline, jenis = excluded.jenis, url = excluded.url",
            params![m, t.judul.trim(), t.isi.trim(), tenggat, sumber, t.jenis, t.url],
        )?;
        if t.selesai {
            c.execute(
                "INSERT OR IGNORE INTO tugas_selesai(user_id, tugas_id, waktu) SELECT ?1, id, ?2 FROM tugas WHERE sumber = ?3",
                params![user, waktu::teks(&waktu::sekarang()), sumber],
            )?;
        }
        n += 1;
    }
    Ok(n)
}

/// "2026-10-31T16:59:00.000Z" -> "2026-10-31 23:59" (WIB); "2026-10-31 23:59[:00]" dan "2026-10-31" juga diterima.
fn baca_tenggat(s: &str) -> Option<String> {
    let s = s.trim();
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(d.with_timezone(&chrono::FixedOffset::east_opt(7 * 3600)?).format("%Y-%m-%d %H:%M").to_string());
    }
    let t = s.replace('T', " ");
    match t.len() {
        10 if chrono::NaiveDate::parse_from_str(&t, "%Y-%m-%d").is_ok() => Some(format!("{t} 23:59")),
        16.. if chrono::NaiveDateTime::parse_from_str(&t[..16], "%Y-%m-%d %H:%M").is_ok() => Some(t[..16].to_string()),
        _ => None,
    }
}

pub fn baca_jam(s: &str) -> Option<(String, String)> {
    let (a, b) = s.split_once('-')?;
    let rapi = |t: &str| -> Option<String> {
        let t = t.trim().replace('.', ":");
        let (h, m) = t.split_once(':')?;
        let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
        (h < 24 && m < 60).then(|| format!("{h:02}:{m:02}"))
    };
    Some((rapi(a)?, rapi(b)?))
}

/// "MANAJEMEN PROYEK INFORMATIKA" -> "Manajemen Proyek Informatika"
pub fn rapikan(s: &str) -> String {
    s.split_whitespace()
        .map(|k| match k.to_uppercase().as_str() {
            "DAN" | "DI" | "KE" | "DARI" | "YANG" => k.to_lowercase(),
            r if r.chars().all(|c| "IVX".contains(c)) || r == "QA" || r == "TI" => r.to_string(),
            _ => {
                let mut c = k.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase()).unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn sinkron(c: &mut Connection, user: i64, d: &DataUnpam) -> anyhow::Result<usize> {
    let nim = d.nim.trim();
    if nim.len() < 6 || !nim.chars().all(|c| c.is_ascii_digit()) {
        bail!("NIM dari MyUnpam tidak valid");
    }
    if d.kelas.is_empty() {
        bail!("Jadwal kosong: pastikan semester aktif sudah punya jadwal di MyUnpam");
    }
    let tx = c.transaction()?;
    let dipakai: Option<i64> = tx.query_row("SELECT id FROM users WHERE nim = ?1 AND id != ?2", params![nim, user], |r| r.get(0)).optional()?;
    if dipakai.is_some() {
        bail!("NIM {nim} sudah dipakai akun Smart KTM lain");
    }
    let dosen_demo: i64 = tx.query_row("SELECT id FROM users WHERE peran = 'dosen' ORDER BY id LIMIT 1", [], |r| r.get(0))?;

    let mut id_matkul = vec![];
    for k in &d.kelas {
        let hari = waktu::HARI
            .iter()
            .position(|h| h.eq_ignore_ascii_case(k.hari.trim()))
            .with_context(|| format!("hari '{}' tidak dikenal", k.hari))? as u32
            + 1;
        let (mulai, selesai) = baca_jam(&k.jam).with_context(|| format!("jam '{}' untuk {} tidak terbaca", k.jam, k.nama))?;
        let kode = if k.kelas.is_empty() { k.kode.clone() } else { format!("{}-{}", k.kode, k.kelas) };
        tx.execute(
            "INSERT INTO matkul(kode, nama, sks, dosen_id, dosen_nama, hari, mulai, selesai, ruang) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(kode) DO UPDATE SET nama = excluded.nama, sks = excluded.sks, dosen_nama = excluded.dosen_nama,
               hari = excluded.hari, mulai = excluded.mulai, selesai = excluded.selesai, ruang = excluded.ruang",
            params![kode, rapikan(&k.nama), k.sks, crate::dosen::dosen_untuk(&tx, &k.dosen, dosen_demo)?, k.dosen.trim(), hari, mulai, selesai, k.ruang.trim()],
        )?;
        id_matkul.push(tx.query_row("SELECT id FROM matkul WHERE kode = ?1", [&kode], |r| r.get::<_, i64>(0))?);
    }
    tx.execute("DELETE FROM pertemuan_unpam WHERE user_id = ?1", [user])?;
    for (k, m) in d.kelas.iter().zip(&id_matkul) {
        for p in &k.pertemuan {
            tx.execute(
                "INSERT OR REPLACE INTO pertemuan_unpam(user_id, matkul_id, ke, jenis, mulai, akhir, status, waktu) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![user, m, p.ke, p.jenis, p.mulai, p.akhir, p.status.as_deref().filter(|s| !s.is_empty()), p.waktu],
            )?;
        }
    }
    tx.execute("DELETE FROM ambil WHERE user_id = ?1", [user])?;
    for m in &id_matkul {
        tx.execute("INSERT OR IGNORE INTO ambil(user_id, matkul_id) VALUES(?1, ?2)", params![user, m])?;
    }
    tx.execute("UPDATE users SET nim = ?1 WHERE id = ?2", params![nim, user])?;
    if !d.nama.trim().is_empty() {
        tx.execute("UPDATE users SET nama = ?1 WHERE id = ?2", params![rapikan(&d.nama), user])?;
    }
    if !d.prodi.trim().is_empty() {
        tx.execute("UPDATE users SET prodi = ?1 WHERE id = ?2", params![rapikan(d.prodi.trim_end_matches(" S1").trim()), user])?;
    }
    if !d.semester.trim().is_empty() {
        tx.execute("UPDATE users SET semester = ?1 WHERE id = ?2", params![d.semester.trim().trim_start_matches('0'), user])?;
    }
    if d.foto.starts_with("data:image/") && d.foto.len() < 300_000 {
        tx.execute("UPDATE users SET foto = ?1 WHERE id = ?2", params![d.foto, user])?;
    }
    tx.commit()?;
    Ok(id_matkul.len())
}

/// Hapus data contoh bawaan (matkul TPL05xx, teman sekelas & dosen fiktif) beserta riwayatnya.
pub fn bersihkan_contoh(c: &mut Connection, kecuali: i64) -> anyhow::Result<()> {
    let tx = c.transaction()?;
    tx.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS m_contoh AS SELECT id FROM matkul WHERE kode LIKE 'TPL05%';
         DELETE FROM presensi WHERE sesi_id IN (SELECT id FROM sesi WHERE matkul_id IN (SELECT id FROM m_contoh));
         DELETE FROM sesi WHERE matkul_id IN (SELECT id FROM m_contoh);
         DELETE FROM tugas_selesai WHERE tugas_id IN (SELECT id FROM tugas WHERE matkul_id IN (SELECT id FROM m_contoh));
         DELETE FROM tugas WHERE matkul_id IN (SELECT id FROM m_contoh);
         DELETE FROM ambil WHERE matkul_id IN (SELECT id FROM m_contoh);
         DROP TABLE m_contoh;",
    )?;
    tx.execute("DELETE FROM matkul WHERE kode LIKE 'TPL05%'", [])?;
    let palsu = "(SELECT id FROM users WHERE id != ?1 AND ((peran = 'mhs' AND nim LIKE '2310114000__') OR (peran = 'dosen' AND nim LIKE '04000001%')))";
    for t in ["presensi", "ambil", "sesi_login", "tugas_selesai", "pengingat", "pertemuan_unpam"] {
        tx.execute(&format!("DELETE FROM {t} WHERE user_id IN {palsu}"), [kecuali])?;
    }
    tx.execute(&format!("DELETE FROM users WHERE id IN {palsu}"), [kecuali])?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tes {
    use super::*;
    use crate::db;

    fn data(nim: &str, kelas: &str) -> DataUnpam {
        serde_json::from_value(serde_json::json!({
            "nim": nim, "nama": "SITI NUR AINI", "prodi": "TEKNIK INFORMATIKA S1",
            "kelas": [
                {"kode": "22TIF0312", "kelas": kelas, "nama": "MANAJEMEN PROYEK INFORMATIKA", "sks": 2, "hari": "Selasa", "jam": "08.50 - 10.30", "ruang": "VIKTOR V.630", "dosen": "DOSEN SATU, S.Kom., M.Kom.",
                 "pertemuan": [{"ke": 1, "mulai": "2026-08-31", "akhir": "2026-09-05", "status": "hadir"}, {"ke": 2, "mulai": "2026-09-07", "akhir": "2026-09-12", "status": null}]},
                {"kode": "22TIF0412", "kelas": kelas, "nama": "TESTING DAN QA PERANGKAT LUNAK", "sks": 2, "hari": "Senin", "jam": "10.30 - 12.10", "ruang": "VIKTOR V.630", "dosen": "DOSEN DUA"}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn jam_dan_nama_dirapikan() {
        assert_eq!(baca_jam("08.50 - 10.30"), Some(("08:50".into(), "10:30".into())));
        assert_eq!(baca_jam("7.10-8.50"), Some(("07:10".into(), "08:50".into())));
        assert_eq!(baca_jam("pagi"), None);
        assert_eq!(rapikan("TESTING DAN QA PERANGKAT LUNAK"), "Testing dan QA Perangkat Lunak");
        assert_eq!(rapikan("PEMROGRAMAN WEB II"), "Pemrograman Web II");
    }

    #[test]
    fn teman_sekelas_berbagi_kelas() {
        let mut c = Connection::open_in_memory().unwrap();
        db::siapkan(&c).unwrap();
        let demo: i64 = c.query_row("SELECT id FROM users WHERE nim = '231011400001'", [], |r| r.get(0)).unwrap();
        bersihkan_contoh(&mut c, demo).unwrap();
        assert_eq!(sinkron(&mut c, demo, &data("231011400777", "07TPLP003")).unwrap(), 2);

        c.execute("INSERT INTO users(peran, nim, nama, pin_hash, dibuat) VALUES('mhs', '231011409999', 'Teman', 'x', '2026-10-01 00:00')", []).unwrap();
        let teman = c.last_insert_rowid();
        sinkron(&mut c, teman, &data("231011409999", "07TPLP003")).unwrap();

        let matkul: i64 = c.query_row("SELECT COUNT(*) FROM matkul", [], |r| r.get(0)).unwrap();
        assert_eq!(matkul, 2, "data contoh terhapus dan kelas yang sama tidak dobel");
        let sekelas: i64 = c.query_row("SELECT COUNT(*) FROM ambil WHERE matkul_id = (SELECT id FROM matkul WHERE kode = '22TIF0312-07TPLP003')", [], |r| r.get(0)).unwrap();
        assert_eq!(sekelas, 2);
        let (nim, nama): (String, String) = c.query_row("SELECT nim, nama FROM users WHERE id = ?1", [demo], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((nim.as_str(), nama.as_str()), ("231011400777", "Siti Nur Aini"));
        assert!(sinkron(&mut c, teman, &data("231011400777", "07TPLP003")).is_err(), "NIM orang lain ditolak");
        let pt: i64 = c.query_row("SELECT COUNT(*) FROM pertemuan_unpam WHERE user_id = ?1", [demo], |r| r.get(0)).unwrap();
        assert_eq!(pt, 2);
    }

    #[test]
    fn tugas_mentari_masuk_ke_matkul() {
        let mut c = Connection::open_in_memory().unwrap();
        db::siapkan(&c).unwrap();
        let demo: i64 = c.query_row("SELECT id FROM users WHERE nim = '231011400001'", [], |r| r.get(0)).unwrap();
        bersihkan_contoh(&mut c, demo).unwrap();
        sinkron(&mut c, demo, &data("231011400777", "07TPLP003")).unwrap();
        let tugas: Vec<TugasMentari> = serde_json::from_value(serde_json::json!([
            // tanpa tenggat: diperkirakan dari akhir pertemuan ke-2 di MyUnpam
            {"id": "a1", "matkul": "MANAJEMEN PROYEK INFORMATIKA", "ke": 2, "jenis": "Tugas", "judul": "Buat WBS", "isi": "Kumpulkan PDF", "selesai": false, "url": "https://mentari.unpam.ac.id/u-courses/X/assignment/a1"},
            {"id": "a2", "matkul": "Testing dan QA Perangkat Lunak", "ke": 3, "jenis": "Forum", "judul": "Forum P3", "selesai": true, "batas": "2026-10-31T16:59:00.000Z"},
            {"id": "a3", "matkul": "MATKUL LAIN", "ke": 1, "jenis": "Tugas", "judul": "Tidak cocok", "selesai": false}
        ]))
        .unwrap();
        assert_eq!(sinkron_mentari(&c, demo, &tugas).unwrap(), 2, "tugas matkul yang tidak diambil dilewati");
        assert_eq!(sinkron_mentari(&c, demo, &tugas).unwrap(), 2);
        let semua: Vec<(String, String, String, Option<String>)> = c
            .prepare("SELECT judul, deadline, jenis, url FROM tugas ORDER BY sumber").unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap()
            .collect::<rusqlite::Result<_>>().unwrap();
        assert_eq!(semua.len(), 2, "sinkron ulang tidak membuat tugas dobel");
        assert_eq!(semua[0].0, "Buat WBS");
        assert_eq!(semua[0].1, "2026-09-12 23:59");
        assert_eq!(semua[1].1, "2026-10-31 23:59", "tenggat UTC diubah ke WIB");
        assert_eq!(semua[1].2, "Forum");
        let selesai: i64 = c.query_row("SELECT COUNT(*) FROM tugas_selesai WHERE user_id = ?1", [demo], |r| r.get(0)).unwrap();
        assert_eq!(selesai, 1, "status selesai dari Mentari ikut");
    }
}
