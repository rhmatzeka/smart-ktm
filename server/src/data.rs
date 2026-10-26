//! Query data akademik yang dipakai beranda dan alat AI agent.

use chrono::{DateTime, FixedOffset, Timelike};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::waktu::{self, menit};

pub struct Kelas {
    pub id: i64,
    pub kode: String,
    pub nama: String,
    pub hari: u32,
    pub mulai: String,
    pub selesai: String,
    pub ruang: String,
    pub dosen: String,
}

impl Kelas {
    pub fn json(&self) -> Value {
        json!({
            "id": self.id, "kode": self.kode, "nama": self.nama, "hari": waktu::nama_hari(self.hari),
            "mulai": self.mulai, "selesai": self.selesai, "ruang": self.ruang, "dosen": self.dosen,
        })
    }
}

/// `hari` None = semua hari.
pub fn jadwal(c: &Connection, user: i64, hari: Option<u32>) -> rusqlite::Result<Vec<Kelas>> {
    let mut q = c.prepare(
        "SELECT m.id, m.kode, m.nama, m.hari, m.mulai, m.selesai, m.ruang, COALESCE(NULLIF(m.dosen_nama, ''), d.nama) FROM matkul m
         JOIN ambil a ON a.matkul_id = m.id JOIN users d ON d.id = m.dosen_id
         WHERE a.user_id = ?1 AND (?2 IS NULL OR m.hari = ?2) ORDER BY m.hari, m.mulai",
    )?;
    let baris = q.query_map(params![user, hari], |r| {
        Ok(Kelas {
            id: r.get(0)?,
            kode: r.get(1)?,
            nama: r.get(2)?,
            hari: r.get(3)?,
            mulai: r.get(4)?,
            selesai: r.get(5)?,
            ruang: r.get(6)?,
            dosen: r.get(7)?,
        })
    })?;
    baris.collect()
}

pub fn tugas(c: &Connection, user: i64, kini: &DateTime<FixedOffset>, termasuk_selesai: bool) -> rusqlite::Result<Vec<Value>> {
    let mut q = c.prepare(
        "SELECT t.id, t.judul, t.deskripsi, m.nama, t.deadline,
                EXISTS(SELECT 1 FROM tugas_selesai s WHERE s.tugas_id = t.id AND s.user_id = ?1)
         FROM tugas t JOIN matkul m ON m.id = t.matkul_id JOIN ambil a ON a.matkul_id = m.id AND a.user_id = ?1
         ORDER BY t.deadline",
    )?;
    let baris = q.query_map([user], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, bool>(5)?))
    })?;
    let mut hasil = vec![];
    for b in baris {
        let (id, judul, deskripsi, matkul, deadline, selesai) = b?;
        if selesai && !termasuk_selesai {
            continue;
        }
        let (sisa, tingkat) = waktu::sisa(&deadline, kini);
        hasil.push(json!({
            "id": id, "judul": judul, "deskripsi": deskripsi, "matkul": matkul, "deadline": deadline,
            "sisa": sisa, "tingkat": if selesai { "selesai" } else { tingkat }, "selesai": selesai,
        }));
    }
    Ok(hasil)
}

/// Rekap per matkul. Pertemuan dihitung sejak akun dibuat; sesi yang masih
/// terbuka baru dihitung kalau mahasiswa sudah tap.
pub fn presensi(c: &Connection, user: i64, dibuat: &str) -> rusqlite::Result<Vec<Value>> {
    let mut q = c.prepare(
        "SELECT m.nama, m.batas_absen,
           (SELECT COUNT(*) FROM sesi s WHERE s.matkul_id = m.id AND s.dibuka >= ?2 AND
              (s.ditutup IS NOT NULL OR EXISTS(SELECT 1 FROM presensi p WHERE p.sesi_id = s.id AND p.user_id = ?1))),
           (SELECT COUNT(*) FROM presensi p JOIN sesi s ON s.id = p.sesi_id WHERE s.matkul_id = m.id AND p.user_id = ?1),
           (SELECT COUNT(*) FROM presensi p JOIN sesi s ON s.id = p.sesi_id WHERE s.matkul_id = m.id AND p.user_id = ?1 AND p.tx IS NOT NULL)
         FROM matkul m JOIN ambil a ON a.matkul_id = m.id WHERE a.user_id = ?1 ORDER BY m.hari, m.mulai",
    )?;
    let baris = q.query_map(params![user, dibuat], |r| {
        let (nama, batas, total, hadir, onchain): (String, i64, i64, i64, i64) = (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?);
        let absen = (total - hadir).max(0);
        let status = if absen >= batas {
            "lewat batas"
        } else if absen == batas - 1 {
            "hampir batas"
        } else {
            "aman"
        };
        Ok(json!({ "matkul": nama, "hadir": hadir, "total": total, "absen": absen, "batas": batas, "status": status, "onchain": onchain }))
    })?;
    baris.collect()
}

pub fn pengingat(c: &Connection, user: i64) -> rusqlite::Result<Vec<Value>> {
    let mut q = c.prepare("SELECT teks, waktu FROM pengingat WHERE user_id = ?1 ORDER BY waktu")?;
    let baris = q.query_map([user], |r| Ok(json!({ "teks": r.get::<_, String>(0)?, "waktu": r.get::<_, String>(1)? })))?;
    baris.collect()
}

pub fn beranda(c: &Connection, user: i64, nama: &str, dibuat: &str, kini: &DateTime<FixedOffset>) -> rusqlite::Result<Value> {
    let hari = waktu::hari_ke(kini);
    let m = waktu::menit_sekarang(kini);
    let hari_ini = jadwal(c, user, Some(hari))?;
    let besok = jadwal(c, user, Some(hari % 7 + 1))?;
    // Kelas yang jamnya belum diisi (data impor) tidak ikut hitungan waktu.
    let berjam: Vec<&Kelas> = hari_ini.iter().filter(|k| !k.mulai.is_empty()).collect();
    let rentang: Vec<(u32, u32)> = berjam.iter().map(|k| (menit(&k.mulai), menit(&k.selesai))).collect();
    let momen = waktu::momen(m, &rentang);

    let jadwal_json: Vec<Value> = hari_ini
        .iter()
        .map(|k| {
            let (a, b) = (menit(&k.mulai), menit(&k.selesai));
            let mut v = k.json();
            v["status"] = json!(if k.mulai.is_empty() { "tanpa_jam" } else if m >= b { "selesai" } else if m >= a { "berlangsung" } else { "nanti" });
            v
        })
        .collect();
    let berikut = berjam.iter().find(|k| m < menit(&k.mulai)).map(|k| {
        let mut v = k.json();
        v["menit_lagi"] = json!(menit(&k.mulai) - m);
        v
    });
    let berlangsung = berjam.iter().find(|k| menit(&k.mulai) <= m && m < menit(&k.selesai)).map(|k| {
        let mut v = k.json();
        v["menit_tersisa"] = json!(menit(&k.selesai) - m);
        v
    });
    let selesai_hari_ini: Vec<&str> = berjam.iter().filter(|k| menit(&k.selesai) <= m).map(|k| k.nama.as_str()).collect();

    let tugas = tugas(c, user, kini, false)?;
    let tugas_kelas_tadi: Vec<&Value> = tugas.iter().filter(|t| selesai_hari_ini.contains(&t["matkul"].as_str().unwrap_or(""))).collect();
    let presensi = presensi(c, user, dibuat)?;
    let onchain: i64 = presensi.iter().map(|p| p["onchain"].as_i64().unwrap_or(0)).sum();

    let sapaan = match kini.hour() {
        4..=10 => "Pagi",
        11..=14 => "Siang",
        15..=17 => "Sore",
        _ => "Malam",
    };

    // Ringkasan berbasis aturan; diganti versi AI kalau Gemini tersedia.
    let mut kalimat = vec![];
    match momen {
        "kelas" => {
            let k = berlangsung.as_ref().unwrap();
            kalimat.push(format!("Sekarang lagi {} di {}, selesai {}.", k["nama"].as_str().unwrap(), k["ruang"].as_str().unwrap(), k["selesai"].as_str().unwrap()));
        }
        "sebelum" => {
            let k = berikut.as_ref().unwrap();
            kalimat.push(format!("Hari ini {} kelas. Berikutnya {} jam {} di {}.", hari_ini.len(), k["nama"].as_str().unwrap(), k["mulai"].as_str().unwrap(), k["ruang"].as_str().unwrap()));
        }
        "setelah" => kalimat.push("Kelas hari ini sudah selesai.".into()),
        "malam" => match besok.first() {
            Some(k) => kalimat.push(format!("Besok {} kelas, mulai {} jam {}.", besok.len(), k.nama, k.mulai)),
            None => kalimat.push("Besok tidak ada kelas.".into()),
        },
        _ if !hari_ini.is_empty() => kalimat.push(format!("Hari ini ada {}.", hari_ini.iter().map(|k| k.nama.as_str()).collect::<Vec<_>>().join(", "))),
        _ => kalimat.push("Hari ini tidak ada kelas.".into()),
    }
    if let Some(t) = tugas.iter().find(|t| t["tingkat"] == "merah") {
        kalimat.push(format!("Tugas {} ({}) deadline {}.", t["judul"].as_str().unwrap(), t["matkul"].as_str().unwrap(), t["sisa"].as_str().unwrap()));
    }
    if let Some(p) = presensi.iter().find(|p| p["status"] != "aman") {
        kalimat.push(format!("Absen {} sudah {}x, batasnya {}.", p["matkul"].as_str().unwrap(), p["absen"], p["batas"]));
    }

    Ok(json!({
        "nama": nama,
        "sapaan": sapaan,
        "sekarang": waktu::tanggal_panjang(kini),
        "momen": momen,
        "ringkasan": kalimat.join(" "),
        "kelas_berikut": berikut,
        "kelas_berlangsung": berlangsung,
        "jadwal_hari_ini": jadwal_json,
        "jadwal_besok": besok.iter().map(|k| k.json()).collect::<Vec<_>>(),
        "tugas": tugas,
        "tugas_kelas_tadi": tugas_kelas_tadi,
        "presensi": presensi,
        "onchain": onchain,
    }))
}

#[cfg(test)]
mod tes {
    use super::*;
    use crate::db;

    #[test]
    fn beranda_mahasiswa_demo() {
        let c = Connection::open_in_memory().unwrap();
        db::siapkan(&c).unwrap();
        let (id, dibuat): (i64, String) = c.query_row("SELECT id, dibuat FROM users WHERE nim = '231011400001'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        let kini = waktu::sekarang();
        let b = beranda(&c, id, "Rahmat", &dibuat, &kini).unwrap();
        assert_eq!(b["jadwal_hari_ini"].as_array().unwrap().len() >= 1, true);
        assert_eq!(b["tugas"].as_array().unwrap().len(), 5); // Draft Bab I sudah selesai
        let bd = b["presensi"].as_array().unwrap().iter().find(|p| p["matkul"] == "Basis Data II").unwrap();
        assert_eq!(bd["absen"], 3);
        assert_eq!(bd["status"], "hampir batas");
    }
}
