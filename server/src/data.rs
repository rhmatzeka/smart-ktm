//! Query data akademik yang dipakai beranda dan alat AI agent.

use chrono::{DateTime, FixedOffset, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
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
    pub sks: i64,
}

impl Kelas {
    pub fn json(&self) -> Value {
        json!({
            "id": self.id, "kode": self.kode, "nama": self.nama, "hari": waktu::nama_hari(self.hari),
            "mulai": self.mulai, "selesai": self.selesai, "ruang": self.ruang, "dosen": self.dosen,
            "hari_ke": self.hari, "sks": self.sks,
        })
    }
}

/// `hari` None = semua hari.
pub fn jadwal(c: &Connection, user: i64, hari: Option<u32>) -> rusqlite::Result<Vec<Kelas>> {
    let mut q = c.prepare(
        "SELECT m.id, m.kode, m.nama, m.hari, m.mulai, m.selesai, m.ruang, COALESCE(NULLIF(m.dosen_nama, ''), d.nama), m.sks FROM matkul m
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
            sks: r.get(8)?,
        })
    })?;
    baris.collect()
}

pub fn tugas(c: &Connection, user: i64, kini: &DateTime<FixedOffset>, termasuk_selesai: bool) -> rusqlite::Result<Vec<Value>> {
    let mut q = c.prepare(
        "SELECT t.id, t.judul, t.deskripsi, m.nama, t.deadline,
                EXISTS(SELECT 1 FROM tugas_selesai s WHERE s.tugas_id = t.id AND s.user_id = ?1),
                t.jenis, t.url, t.sumber IS NOT NULL
         FROM tugas t JOIN matkul m ON m.id = t.matkul_id JOIN ambil a ON a.matkul_id = m.id AND a.user_id = ?1
         ORDER BY t.deadline = '', t.deadline",
    )?;
    type Baris = (i64, String, String, String, String, bool, Option<String>, Option<String>, bool);
    let baris = q.query_map([user], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?))
    })?;
    // Aktivitas Mentari yang tenggatnya lewat lebih dari seminggu sudah tidak bisa dikerjakan: tidak ditampilkan.
    let basi = waktu::teks(&(*kini - chrono::Duration::days(7)));
    let mut hasil = vec![];
    for b in baris {
        let (id, judul, deskripsi, matkul, deadline, selesai, jenis, url, dari_mentari): Baris = b?;
        if selesai && !termasuk_selesai {
            continue;
        }
        if dari_mentari && !selesai && !deadline.is_empty() && deadline < basi {
            continue;
        }
        let (sisa, tingkat) = waktu::sisa(&deadline, kini);
        hasil.push(json!({
            "id": id, "judul": judul, "deskripsi": deskripsi, "matkul": matkul, "deadline": deadline,
            "sisa": sisa, "tingkat": if selesai { "selesai" } else { tingkat }, "selesai": selesai,
            "jenis": jenis.unwrap_or_else(|| "Tugas".into()), "url": url,
        }));
    }
    Ok(hasil)
}

/// Rekap kehadiran per matkul. Kalau mahasiswa sudah sinkron MyUnpam, angka diambil dari
/// status presensi per pertemuan di MyUnpam; kalau belum, dari sesi tap KTM di Smart KTM
/// (pertemuan dihitung sejak akun dibuat, sesi terbuka baru dihitung kalau sudah tap).
pub fn presensi(c: &Connection, user: i64, dibuat: &str) -> rusqlite::Result<Vec<Value>> {
    let hari_ini = waktu::sekarang().format("%Y-%m-%d").to_string();
    let mut q = c.prepare(
        "SELECT m.id, m.nama, m.batas_absen,
           (SELECT COUNT(*) FROM sesi s WHERE s.matkul_id = m.id AND s.dibuka >= ?2 AND
              (s.ditutup IS NOT NULL OR EXISTS(SELECT 1 FROM presensi p WHERE p.sesi_id = s.id AND p.user_id = ?1))),
           (SELECT COUNT(*) FROM presensi p JOIN sesi s ON s.id = p.sesi_id WHERE s.matkul_id = m.id AND p.user_id = ?1),
           (SELECT COUNT(*) FROM presensi p JOIN sesi s ON s.id = p.sesi_id WHERE s.matkul_id = m.id AND p.user_id = ?1 AND p.tx IS NOT NULL)
         FROM matkul m JOIN ambil a ON a.matkul_id = m.id WHERE a.user_id = ?1 ORDER BY m.hari, m.mulai",
    )?;
    let baris: Vec<(i64, String, i64, i64, i64, i64)> = q
        .query_map(params![user, dibuat], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut q_unpam = c.prepare("SELECT ke, mulai, akhir, status FROM pertemuan_unpam WHERE user_id = ?1 AND matkul_id = ?2 ORDER BY ke")?;

    let mut hasil = vec![];
    for (id, nama, batas, total_tap, hadir_tap, onchain) in baris {
        let unpam: Vec<(i64, String, String, Option<String>)> =
            q_unpam.query_map(params![user, id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
        let (hadir, total, absen, belum, bisa, jumlah, sumber) = if unpam.is_empty() {
            (hadir_tap, total_tap, (total_tap - hadir_tap).max(0), vec![], vec![], 0, "smart-ktm")
        } else {
            // Periode presensi yang sudah lewat tanpa status = terlewat; yang sedang berjalan = masih bisa.
            let lewat: Vec<_> = unpam.iter().filter(|p| p.2 < hari_ini).collect();
            let berjalan: Vec<_> = unpam.iter().filter(|p| p.1 <= hari_ini && hari_ini <= p.2).collect();
            let hadir = |p: &(i64, String, String, Option<String>)| p.3.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("hadir"));
            let belum: Vec<i64> = lewat.iter().filter(|p| p.3.is_none()).map(|p| p.0).collect();
            let bisa: Vec<i64> = berjalan.iter().filter(|p| p.3.is_none()).map(|p| p.0).collect();
            let n_hadir = lewat.iter().chain(berjalan.iter()).filter(|p| hadir(p)).count() as i64;
            let total = lewat.len() as i64 + berjalan.iter().filter(|p| p.3.is_some()).count() as i64;
            let absen = lewat.iter().filter(|p| !hadir(p)).count() as i64;
            (n_hadir, total, absen, belum, bisa, unpam.len() as i64, "myunpam")
        };
        let status = if absen >= batas {
            "lewat batas"
        } else if absen == batas - 1 {
            "hampir batas"
        } else {
            "aman"
        };
        hasil.push(json!({
            "id": id, "matkul": nama, "hadir": hadir, "total": total, "absen": absen, "batas": batas, "status": status,
            "onchain": onchain, "belum_presensi": belum, "bisa_presensi": bisa, "jumlah_pertemuan": jumlah, "sumber": sumber,
        }));
    }
    Ok(hasil)
}

/// Rincian per pertemuan satu matkul untuk mahasiswa (data MyUnpam + tap KTM di Smart KTM).
pub fn kehadiran(c: &Connection, user: i64, matkul: i64) -> rusqlite::Result<Option<Value>> {
    let info: Option<(String, String, String)> = c
        .query_row(
            "SELECT m.nama, COALESCE(NULLIF(m.dosen_nama, ''), d.nama), m.ruang FROM matkul m JOIN users d ON d.id = m.dosen_id
             JOIN ambil a ON a.matkul_id = m.id AND a.user_id = ?1 WHERE m.id = ?2",
            params![user, matkul],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((nama, dosen, ruang)) = info else { return Ok(None) };
    let hari_ini = waktu::sekarang().format("%Y-%m-%d").to_string();
    let mut q = c.prepare("SELECT ke, jenis, mulai, akhir, status, waktu FROM pertemuan_unpam WHERE user_id = ?1 AND matkul_id = ?2 ORDER BY ke")?;
    let pertemuan: Vec<Value> = q
        .query_map(params![user, matkul], |r| {
            let (ke, jenis, mulai, akhir, status, saat): (i64, String, String, String, Option<String>, Option<String>) =
                (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?);
            let keadaan = match status.as_deref() {
                Some(s) if s.eq_ignore_ascii_case("hadir") => "hadir",
                Some(_) => "absen",
                None if akhir < hari_ini => "terlewat",
                None if mulai <= hari_ini => "minggu_ini",
                None => "nanti",
            };
            Ok(json!({ "ke": ke, "jenis": jenis, "mulai": mulai, "akhir": akhir, "status": status, "waktu": saat, "keadaan": keadaan }))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut q = c.prepare(
        "SELECT s.pertemuan, p.waktu, p.status, p.tx, p.bukti FROM presensi p JOIN sesi s ON s.id = p.sesi_id
         WHERE p.user_id = ?1 AND s.matkul_id = ?2 ORDER BY p.waktu DESC",
    )?;
    let tap: Vec<Value> = q
        .query_map(params![user, matkul], |r| {
            Ok(json!({ "pertemuan": r.get::<_, i64>(0)?, "waktu": r.get::<_, String>(1)?, "status": r.get::<_, String>(2)?,
                       "tx": r.get::<_, Option<String>>(3)?, "bukti": r.get::<_, String>(4)? }))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Some(json!({ "matkul": nama, "dosen": dosen, "ruang": ruang, "pertemuan": pertemuan, "tap": tap })))
}

/// Jadwal notifikasi HP untuk 7 hari ke depan: kelas (15 menit sebelum mulai), tenggat tugas
/// (H-1 dan 3 jam sebelumnya), dan pengingat dari Asisten KTM. Aplikasi Android menjadwalkannya
/// sebagai notifikasi lokal; `id` stabil supaya penjadwalan ulang menimpa, bukan menggandakan.
pub fn notifikasi(c: &Connection, user: i64, kini: &DateTime<FixedOffset>) -> rusqlite::Result<Vec<Value>> {
    let batas = *kini + chrono::Duration::days(7);
    let mut n: Vec<(DateTime<FixedOffset>, i64, String, String)> = vec![];
    let mut tambah = |t: DateTime<FixedOffset>, id: i64, judul: String, isi: String| {
        if t > *kini && t <= batas {
            n.push((t, id, judul, isi));
        }
    };

    let mut q = c.prepare("SELECT m.id, m.nama, m.hari, m.mulai, m.ruang FROM matkul m JOIN ambil a ON a.matkul_id = m.id WHERE a.user_id = ?1")?;
    let kelas: Vec<(i64, String, u32, String, String)> =
        q.query_map([user], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<rusqlite::Result<_>>()?;
    for (id, nama, hari, mulai, ruang) in kelas {
        for geser in 0..8 {
            let tgl = *kini + chrono::Duration::days(geser);
            if waktu::hari_ke(&tgl) != hari {
                continue;
            }
            let Some(awal) = waktu::baca(&format!("{} {mulai}", tgl.format("%Y-%m-%d"))) else { continue };
            tambah(awal - chrono::Duration::minutes(15), 1_000_000 + id * 10 + geser, format!("15 menit lagi: {nama}"), format!("{ruang}, mulai {}", mulai.replace(':', ".")));
        }
    }

    for t in tugas(c, user, kini, false)? {
        let (Some(id), Some(judul), Some(Some(tenggat))) = (t["id"].as_i64(), t["judul"].as_str(), t["deadline"].as_str().map(waktu::baca)) else { continue };
        let matkul = t["matkul"].as_str().unwrap_or_default();
        let jam = tenggat.format("%H.%M");
        tambah(tenggat - chrono::Duration::days(1), 2_000_000 + id * 10, format!("Tenggat besok: {judul}"), format!("{matkul}, {jam}"));
        tambah(tenggat - chrono::Duration::hours(3), 2_000_000 + id * 10 + 1, format!("3 jam lagi: {judul}"), format!("{matkul}, tenggat {jam}"));
    }

    let mut q = c.prepare("SELECT id, teks, waktu FROM pengingat WHERE user_id = ?1")?;
    let ingat: Vec<(i64, String, String)> = q.query_map([user], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
    for (id, teks, w) in ingat {
        if let Some(t) = waktu::baca(&w) {
            tambah(t, 3_000_000 + id, "Pengingat".into(), teks);
        }
    }

    n.sort_by_key(|x| x.0);
    // Android membatasi jumlah alarm per aplikasi; 60 cukup untuk seminggu.
    Ok(n.into_iter()
        .take(60)
        .map(|(t, id, judul, isi)| json!({ "id": id, "waktu": t.format("%Y-%m-%dT%H:%M:%S%:z").to_string(), "judul": judul, "isi": isi }))
        .collect())
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

    #[test]
    fn notifikasi_kelas_tugas_dan_pengingat() {
        let c = Connection::open_in_memory().unwrap();
        db::siapkan(&c).unwrap();
        let id: i64 = c.query_row("SELECT id FROM users WHERE nim = '231011400001'", [], |r| r.get(0)).unwrap();
        c.execute("DELETE FROM tugas", []).unwrap();
        let m: i64 = c.query_row("SELECT matkul_id FROM ambil WHERE user_id = ?1 LIMIT 1", [id], |r| r.get(0)).unwrap();
        c.execute("UPDATE matkul SET hari = 3, mulai = '08:00' WHERE id = ?1", [m]).unwrap();
        c.execute("DELETE FROM ambil WHERE user_id = ?1 AND matkul_id != ?2", params![id, m]).unwrap();
        c.execute("INSERT INTO tugas(matkul_id, judul, deskripsi, deadline) VALUES(?1, 'WBS', '', '2026-10-30 23:59')", [m]).unwrap();
        c.execute("INSERT INTO pengingat(user_id, teks, waktu, dibuat) VALUES(?1, 'Bawa laptop', '2026-10-28 06:30', '2026-10-27 09:00')", [id]).unwrap();
        c.execute("INSERT INTO pengingat(user_id, teks, waktu, dibuat) VALUES(?1, 'Sudah lewat', '2026-10-26 06:30', '2026-10-25 09:00')", [id]).unwrap();

        // Selasa 27 Okt 2026 10.00: kelas Rabu 08.00 -> diingatkan Rabu 07.45.
        let kini = waktu::baca("2026-10-27 10:00").unwrap();
        let n = notifikasi(&c, id, &kini).unwrap();
        let waktu_: Vec<&str> = n.iter().map(|x| x["waktu"].as_str().unwrap()).collect();
        assert!(waktu_.contains(&"2026-10-28T07:45:00+07:00"), "kelas 15 menit sebelum mulai: {waktu_:?}");
        assert!(waktu_.contains(&"2026-10-29T23:59:00+07:00"), "tugas H-1");
        assert!(waktu_.contains(&"2026-10-30T20:59:00+07:00"), "tugas 3 jam sebelum tenggat");
        assert!(waktu_.contains(&"2026-10-28T06:30:00+07:00"), "pengingat dari AI");
        assert!(!n.iter().any(|x| x["isi"].as_str().unwrap().contains("Sudah lewat")));
        let mut urut = waktu_.clone();
        urut.sort();
        assert_eq!(waktu_, urut, "urut waktu");
        let mut id_: Vec<i64> = n.iter().map(|x| x["id"].as_i64().unwrap()).collect();
        id_.dedup();
        assert_eq!(id_.len(), n.len(), "id notifikasi unik");

        // Tugas yang sudah selesai tidak diingatkan.
        c.execute("INSERT INTO tugas_selesai(user_id, tugas_id, waktu) SELECT ?1, id, '2026-10-27 10:00' FROM tugas", [id]).unwrap();
        assert!(!notifikasi(&c, id, &kini).unwrap().iter().any(|x| x["judul"].as_str().unwrap().contains("WBS")));
    }
}
