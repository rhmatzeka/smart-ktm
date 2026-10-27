//! Akun dosen sungguhan: daftar dengan kode undangan dari dosen yang sudah terdaftar, lalu kelas
//! yang dosen pengampunya (menurut MyUnpam) bernama sama otomatis pindah ke akun itu.
//! Kode undangan mencegah orang sembarangan mengaku dosen dan membuka presensi palsu.

use anyhow::bail;
use chrono::{DateTime, FixedOffset};
use rusqlite::{params, Connection, OptionalExtension};

use crate::{db, waktu};

pub const UNDANGAN_HARI: i64 = 7;

/// "Dr. MUHAMMAD YUNUS RANGKUTI, S.Kom., M.Kom." -> "muhammad yunus rangkuti" (gelar dibuang).
pub fn nama_inti(s: &str) -> String {
    let depan = s.split(',').next().unwrap_or_default();
    depan
        .split_whitespace()
        .filter(|w| !w.contains('.'))
        .map(|w| w.chars().filter(|c| c.is_alphabetic() || *c == '\'').collect::<String>().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Kode undangan 8 huruf/angka (tanpa 0/O/1/I yang mudah tertukar), sekali pakai, berlaku 7 hari.
pub fn buat_undangan(c: &Connection, dosen: i64, kini: &DateTime<FixedOffset>) -> rusqlite::Result<String> {
    const HURUF: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let kode: String = rand::random::<[u8; 8]>().iter().map(|b| HURUF[*b as usize % HURUF.len()] as char).collect();
    c.execute(
        "INSERT INTO undangan_dosen(kode, dibuat_oleh, dibuat, kedaluwarsa) VALUES(?1, ?2, ?3, ?4)",
        params![kode, dosen, waktu::teks(kini), waktu::teks(&(*kini + chrono::Duration::days(UNDANGAN_HARI)))],
    )?;
    Ok(kode)
}

/// Daftarkan dosen baru. Mengembalikan (id akun, jumlah kelas yang otomatis terhubung).
pub fn daftar(c: &mut Connection, kode: &str, nidn: &str, nama: &str, pin: &str, kini: &DateTime<FixedOffset>) -> anyhow::Result<(i64, usize)> {
    let (kode, nidn, nama) = (kode.trim().to_uppercase(), nidn.trim(), nama.trim());
    // NIDN 10 digit; NUPTK/NIDK juga angka, jadi diterima 8-16 digit.
    if !(8..=16).contains(&nidn.len()) || !nidn.chars().all(|c| c.is_ascii_digit()) {
        bail!("NIDN harus 8-16 angka");
    }
    if nama.chars().count() < 3 {
        bail!("Nama lengkap wajib diisi");
    }
    if pin.len() != 6 || !pin.chars().all(|c| c.is_ascii_digit()) {
        bail!("PIN harus 6 angka");
    }
    let tx = c.transaction()?;
    let kedaluwarsa: Option<(String, Option<i64>)> = tx
        .query_row("SELECT kedaluwarsa, dipakai_oleh FROM undangan_dosen WHERE kode = ?1", [&kode], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    match kedaluwarsa {
        None => bail!("Kode undangan tidak dikenal. Minta kode ke dosen yang sudah memakai Smart KTM."),
        Some((_, Some(_))) => bail!("Kode undangan sudah dipakai. Minta kode baru."),
        Some((k, None)) if k < waktu::teks(kini) => bail!("Kode undangan sudah kedaluwarsa. Minta kode baru."),
        _ => {}
    }
    let ada: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM users WHERE nim = ?1)", [nidn], |r| r.get(0))?;
    if ada {
        bail!("NIDN ini sudah terdaftar. Masuk pakai NIDN dan PIN.");
    }
    let nama: String = nama.chars().take(80).collect();
    tx.execute(
        "INSERT INTO users(peran, nim, nama, prodi, pin_hash, dibuat) VALUES('dosen', ?1, ?2, '', ?3, ?4)",
        params![nidn, nama, db::hash_pin(pin), waktu::teks(kini)],
    )?;
    let id = tx.last_insert_rowid();
    tx.execute("UPDATE undangan_dosen SET dipakai_oleh = ?1 WHERE kode = ?2", params![id, kode])?;
    let kelas = klaim_kelas(&tx, id)?;
    tx.commit()?;
    Ok((id, kelas))
}

/// Pindahkan ke dosen ini kelas yang dosen pengampunya bernama sama tapi masih dipegang akun lain
/// (biasanya akun Dosen Demo). Mengembalikan jumlah kelas yang pindah.
pub fn klaim_kelas(c: &Connection, dosen: i64) -> rusqlite::Result<usize> {
    let saya = nama_inti(&c.query_row("SELECT nama FROM users WHERE id = ?1", [dosen], |r| r.get::<_, String>(0))?);
    if saya.is_empty() {
        return Ok(0);
    }
    let kelas: Vec<(i64, String, String)> = c
        .prepare("SELECT m.id, COALESCE(m.dosen_nama, ''), u.nama FROM matkul m JOIN users u ON u.id = m.dosen_id WHERE m.dosen_id != ?1")?
        .query_map([dosen], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut n = 0;
    for (id, pengampu, pemegang) in kelas {
        // Hanya ambil dari akun yang bukan pengampu aslinya (mis. Dosen Demo), bukan dari dosen sungguhan.
        if nama_inti(&pengampu) == saya && nama_inti(&pemegang) != saya {
            c.execute("UPDATE matkul SET dosen_id = ?1 WHERE id = ?2", params![dosen, id])?;
            n += 1;
        }
    }
    Ok(n)
}

/// Akun dosen untuk kelas baru hasil sinkron: dosen terdaftar yang namanya cocok, kalau tidak ada `cadangan`.
pub fn dosen_untuk(c: &Connection, dosen_nama: &str, cadangan: i64) -> rusqlite::Result<i64> {
    let cari = nama_inti(dosen_nama);
    if cari.is_empty() {
        return Ok(cadangan);
    }
    let dosen: Vec<(i64, String)> = c
        .prepare("SELECT id, nama FROM users WHERE peran = 'dosen' AND id != ?1 ORDER BY id")?
        .query_map([cadangan], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(dosen.into_iter().find(|(_, n)| nama_inti(n) == cari).map(|(id, _)| id).unwrap_or(cadangan))
}

#[cfg(test)]
mod tes {
    use super::*;

    fn siap() -> (Connection, i64, DateTime<FixedOffset>) {
        let c = Connection::open_in_memory().unwrap();
        db::siapkan(&c).unwrap();
        let demo: i64 = c.query_row("SELECT id FROM users WHERE peran = 'dosen' ORDER BY id LIMIT 1", [], |r| r.get(0)).unwrap();
        let m: i64 = c.query_row("SELECT MIN(id) FROM matkul", [], |r| r.get(0)).unwrap();
        c.execute("UPDATE matkul SET dosen_id = ?1", [demo]).unwrap();
        c.execute("UPDATE matkul SET dosen_nama = 'Dr. JULI GUNAWAN, S.T., M.Kom.' WHERE id = ?1", [m]).unwrap();
        (c, demo, waktu::baca("2026-10-27 10:00").unwrap())
    }

    #[test]
    fn gelar_dibuang_dari_nama() {
        assert_eq!(nama_inti("Dr. MUHAMMAD YUNUS RANGKUTI, S.Kom., M.Kom."), "muhammad yunus rangkuti");
        assert_eq!(nama_inti("S.Sos. SYAFRIL CHAIRIANSYAH, S.Sos., M.Kom."), "syafril chairiansyah");
        assert_eq!(nama_inti("juli  gunawan"), "juli gunawan");
    }

    #[test]
    fn daftar_dengan_undangan_dan_kelas_terhubung() {
        let (mut c, demo, kini) = siap();
        let kode = buat_undangan(&c, demo, &kini).unwrap();
        assert_eq!(kode.len(), 8);
        assert!(daftar(&mut c, "SALAH123", "0412345678", "Juli Gunawan", "112233", &kini).is_err(), "kode salah ditolak");
        let (id, kelas) = daftar(&mut c, &kode.to_lowercase(), "0412345678", "Juli Gunawan, S.T., M.Kom.", "112233", &kini).unwrap();
        assert_eq!(kelas, 1, "kelas dengan nama dosen yang sama pindah ke akun baru");
        let pemilik: i64 = c.query_row("SELECT dosen_id FROM matkul WHERE dosen_nama LIKE '%JULI%'", [], |r| r.get(0)).unwrap();
        assert_eq!(pemilik, id);
        let peran: String = c.query_row("SELECT peran FROM users WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(peran, "dosen");
        assert!(daftar(&mut c, &kode, "0499999999", "Orang Lain", "112233", &kini).is_err(), "kode hanya sekali pakai");
    }

    #[test]
    fn undangan_kedaluwarsa_dan_nidn_ganda_ditolak() {
        let (mut c, demo, kini) = siap();
        let kode = buat_undangan(&c, demo, &kini).unwrap();
        let nanti = kini + chrono::Duration::days(UNDANGAN_HARI + 1);
        assert!(daftar(&mut c, &kode, "0412345678", "Juli Gunawan", "112233", &nanti).is_err());
        let kode = buat_undangan(&c, demo, &kini).unwrap();
        assert!(daftar(&mut c, &kode, "0400000001", "Dosen Kembar", "112233", &kini).is_err(), "NIDN sudah dipakai");
        assert!(daftar(&mut c, &kode, "12ab", "Juli", "112233", &kini).is_err(), "NIDN harus angka");
    }

    #[test]
    fn kelas_baru_ikut_dosen_yang_cocok() {
        let (mut c, demo, kini) = siap();
        let kode = buat_undangan(&c, demo, &kini).unwrap();
        let (id, _) = daftar(&mut c, &kode, "0412345678", "Juli Gunawan", "112233", &kini).unwrap();
        assert_eq!(dosen_untuk(&c, "JULI GUNAWAN, S.T., M.Kom.", demo).unwrap(), id);
        assert_eq!(dosen_untuk(&c, "BUDI APRIYANTO, S.Kom", demo).unwrap(), demo);
    }
}
