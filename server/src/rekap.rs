//! Rekap kehadiran satu kelas untuk dosen (dari tap KTM di Smart KTM) dan ekspornya ke Excel.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Formula, Workbook, XlsxError};
use serde_json::{json, Value};

pub struct Rekap {
    pub matkul: String,
    pub kode: String,
    pub dosen: String,
    pub batas: i64,
    pub pertemuan: Vec<Pertemuan>,
    pub mahasiswa: Vec<Baris>,
}

pub struct Pertemuan {
    pub ke: i64,
    /// Tanggal sesi pertama "YYYY-MM-DD" dan jam dibuka "HH:MM".
    pub tanggal: String,
    pub jam: String,
}

/// Satu sel rekap (mahasiswa x pertemuan).
pub struct Sel {
    /// "H" hadir (tap KTM), "I" izin, "S" sakit, "A" alpa, "D" disanggah pemilik KTM (dihitung alpa).
    pub tanda: &'static str,
    pub waktu: Option<String>,
    pub tx: Option<String>,
    pub bukti: Option<String>,
    pub catatan: Option<String>,
}

pub struct Baris {
    pub nim: String,
    pub nama: String,
    pub sel: Vec<Sel>,
    pub hadir: i64,
    pub izin: i64,
    pub sakit: i64,
    /// Alpa = tidak tap dan tanpa keterangan (termasuk yang disanggah). Hanya ini yang dihitung ke batas absen.
    pub alpa: i64,
    pub onchain: i64,
    pub status: &'static str,
}

pub fn status_absen(alpa: i64, batas: i64) -> &'static str {
    if alpa >= batas {
        "lewat batas"
    } else if alpa == batas - 1 {
        "hampir batas"
    } else {
        "aman"
    }
}

#[derive(Default)]
struct Tap {
    sah: bool,
    waktu: Option<String>,
    tx: Option<String>,
    bukti: Option<String>,
}

pub fn rekap(c: &Connection, matkul: i64) -> rusqlite::Result<Rekap> {
    let (nama, kode, dosen, batas): (String, String, String, i64) = c.query_row(
        "SELECT m.nama, m.kode, COALESCE(NULLIF(m.dosen_nama, ''), u.nama), m.batas_absen FROM matkul m JOIN users u ON u.id = m.dosen_id WHERE m.id = ?1",
        [matkul],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let pertemuan: Vec<Pertemuan> = c
        .prepare("SELECT pertemuan, MIN(dibuka) FROM sesi WHERE matkul_id = ?1 GROUP BY pertemuan ORDER BY pertemuan")?
        .query_map([matkul], |r| {
            let dibuka: String = r.get(1)?;
            Ok(Pertemuan { ke: r.get(0)?, tanggal: dibuka.get(..10).unwrap_or("").into(), jam: dibuka.get(11..16).unwrap_or("").into() })
        })?
        .collect::<rusqlite::Result<_>>()?;
    // (mahasiswa, pertemuan) -> tap pertama yang sah (kalau ada), selain itu tap yang disanggah.
    let mut tap: HashMap<(i64, i64), Tap> = HashMap::new();
    let mut q = c.prepare(
        "SELECT p.user_id, s.pertemuan, p.status != 'disanggah', p.waktu, p.tx, p.bukti
         FROM presensi p JOIN sesi s ON s.id = p.sesi_id WHERE s.matkul_id = ?1 ORDER BY p.waktu",
    )?;
    for b in q.query_map([matkul], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, bool>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, String>(5)?))
    })? {
        let (u, ke, sah, waktu, tx, bukti) = b?;
        let e = tap.entry((u, ke)).or_default();
        if (sah && !e.sah) || e.waktu.is_none() {
            *e = Tap { sah, waktu: Some(waktu), tx: tx.filter(|_| sah), bukti: Some(bukti) };
        }
    }
    let mut ket: HashMap<(i64, i64), (String, String)> = HashMap::new();
    let mut q = c.prepare("SELECT user_id, pertemuan, jenis, catatan FROM keterangan WHERE matkul_id = ?1")?;
    for b in q.query_map([matkul], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))? {
        let (u, ke, jenis, catatan) = b?;
        ket.insert((u, ke), (jenis, catatan));
    }
    let peserta: Vec<(i64, String, String)> = c
        .prepare("SELECT u.id, u.nim, u.nama FROM ambil a JOIN users u ON u.id = a.user_id WHERE a.matkul_id = ?1 ORDER BY u.nama")?
        .query_map([matkul], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mahasiswa = peserta
        .into_iter()
        .map(|(id, nim, nama)| {
            let sel: Vec<Sel> = pertemuan
                .iter()
                .map(|p| {
                    let t = tap.get(&(id, p.ke));
                    let k = ket.get(&(id, p.ke));
                    let tanda = match (t, k) {
                        (Some(t), _) if t.sah => "H",
                        (_, Some((j, _))) if j == "I" => "I",
                        (_, Some(_)) => "S",
                        (Some(_), None) => "D",
                        (None, None) => "A",
                    };
                    Sel {
                        tanda,
                        waktu: t.and_then(|t| t.waktu.clone()),
                        tx: t.and_then(|t| t.tx.clone()),
                        bukti: t.and_then(|t| t.bukti.clone()),
                        catatan: k.map(|k| k.1.clone()).filter(|c| !c.is_empty()),
                    }
                })
                .collect();
            let hitung = |x: &[&str]| sel.iter().filter(|s| x.contains(&s.tanda)).count() as i64;
            let (hadir, izin, sakit, alpa) = (hitung(&["H"]), hitung(&["I"]), hitung(&["S"]), hitung(&["A", "D"]));
            let onchain = sel.iter().filter(|s| s.tanda == "H" && s.tx.is_some()).count() as i64;
            Baris { nim, nama, sel, hadir, izin, sakit, alpa, onchain, status: status_absen(alpa, batas) }
        })
        .collect();
    Ok(Rekap { matkul: nama, kode, dosen, batas, pertemuan, mahasiswa })
}

impl Rekap {
    /// Persentase hadir seluruh kelas (None kalau belum ada pertemuan atau peserta).
    pub fn rata_hadir(&self) -> Option<i64> {
        let slot = (self.pertemuan.len() * self.mahasiswa.len()) as i64;
        (slot > 0).then(|| (self.mahasiswa.iter().map(|b| b.hadir).sum::<i64>() * 100 + slot / 2) / slot)
    }

    /// Mahasiswa yang hampir atau sudah lewat batas alpa, paling banyak alpa dulu.
    pub fn perlu_perhatian(&self) -> Vec<&Baris> {
        let mut v: Vec<&Baris> = self.mahasiswa.iter().filter(|b| b.status != "aman").collect();
        v.sort_by(|a, b| b.alpa.cmp(&a.alpa).then(a.nama.cmp(&b.nama)));
        v
    }

    pub fn json(&self) -> Value {
        json!({
            "matkul": self.matkul, "kode": self.kode, "dosen": self.dosen, "batas": self.batas, "rata_hadir": self.rata_hadir(),
            "pertemuan": self.pertemuan.iter().map(|p| json!({ "ke": p.ke, "tanggal": p.tanggal, "jam": p.jam })).collect::<Vec<_>>(),
            "mahasiswa": self.mahasiswa.iter().map(|b| json!({
                "nim": b.nim, "nama": b.nama,
                "tanda": b.sel.iter().map(|s| s.tanda).collect::<Vec<_>>(),
                "sel": b.sel.iter().map(|s| json!({ "t": s.tanda, "waktu": s.waktu, "tx": s.tx, "bukti": s.bukti, "catatan": s.catatan })).collect::<Vec<_>>(),
                "hadir": b.hadir, "izin": b.izin, "sakit": b.sakit, "alpa": b.alpa, "onchain": b.onchain, "status": b.status,
            })).collect::<Vec<_>>(),
        })
    }
}

/// Dosen menandai mahasiswa izin ("I") atau sakit ("S") di satu pertemuan; jenis "" menghapus keterangan (kembali alpa).
/// Hadir tidak bisa ditulis tangan: hanya dari tap KTM, supaya tetap anti titip absen.
/// Ok(Err(pesan)) = permintaan ditolak (pesan untuk pengguna).
#[allow(clippy::too_many_arguments)]
pub fn atur_keterangan(
    c: &Connection,
    matkul: i64,
    nim: &str,
    pertemuan: i64,
    jenis: &str,
    catatan: &str,
    oleh: i64,
    saat: &str,
) -> rusqlite::Result<Result<(), &'static str>> {
    if !["", "I", "S"].contains(&jenis) {
        return Ok(Err("Pilih izin, sakit, atau alpa"));
    }
    let Some(user) = c
        .query_row("SELECT u.id FROM ambil a JOIN users u ON u.id = a.user_id WHERE a.matkul_id = ?1 AND u.nim = ?2", params![matkul, nim], |r| r.get::<_, i64>(0))
        .optional()?
    else {
        return Ok(Err("Mahasiswa ini tidak terdaftar di kelas"));
    };
    let ada: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM sesi WHERE matkul_id = ?1 AND pertemuan = ?2)", params![matkul, pertemuan], |r| r.get(0))?;
    if !ada {
        return Ok(Err("Pertemuan ini belum pernah dibuka"));
    }
    let hadir: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM presensi p JOIN sesi s ON s.id = p.sesi_id
           WHERE s.matkul_id = ?1 AND s.pertemuan = ?2 AND p.user_id = ?3 AND p.status != 'disanggah')",
        params![matkul, pertemuan, user],
        |r| r.get(0),
    )?;
    if hadir {
        return Ok(Err("Mahasiswa ini sudah hadir lewat tap KTM"));
    }
    if jenis.is_empty() {
        c.execute("DELETE FROM keterangan WHERE matkul_id = ?1 AND user_id = ?2 AND pertemuan = ?3", params![matkul, user, pertemuan])?;
    } else {
        let catatan: String = catatan.trim().chars().take(200).collect();
        c.execute(
            "INSERT INTO keterangan(matkul_id, user_id, pertemuan, jenis, catatan, oleh, waktu) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(matkul_id, user_id, pertemuan) DO UPDATE SET jenis = excluded.jenis, catatan = excluded.catatan, oleh = excluded.oleh, waktu = excluded.waktu",
            params![matkul, user, pertemuan, jenis, catatan, oleh, saat],
        )?;
    }
    Ok(Ok(()))
}

/// Satu sheet "Rekap": identitas kelas, tabel mahasiswa x pertemuan (H / I / S / A / D), lalu jumlah hadir, izin,
/// sakit, alpa, persentase, dan status sebagai rumus Excel supaya ikut berubah kalau dosen mengoreksi tanda.
pub fn excel(r: &Rekap) -> Result<Vec<u8>, XlsxError> {
    use rust_xlsxwriter::utility::column_number_to_name as kol;
    let dasar = || Format::new().set_font_name("Arial").set_font_size(10);
    let judul = dasar().set_bold().set_font_size(14);
    let label = dasar().set_bold();
    let kepala = dasar().set_bold().set_font_color(Color::White).set_background_color(Color::RGB(0x1747C9))
        .set_align(FormatAlign::Center).set_align(FormatAlign::VerticalCenter).set_border(FormatBorder::Thin).set_text_wrap();
    let sel = dasar().set_border(FormatBorder::Thin);
    let tengah = sel.clone().set_align(FormatAlign::Center);
    let tanda_fmt = |t: &str| {
        tengah.clone().set_background_color(Color::RGB(match t {
            "H" => 0xDCFCE7,
            "I" => 0xDBEAFE,
            "S" => 0xEDE9FE,
            "D" => 0xFEF3C7,
            _ => 0xFFE4E6,
        }))
    };
    let persen = tengah.clone().set_num_format("0%");

    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Rekap")?;
    ws.write_string_with_format(0, 0, format!("Rekap Kehadiran {}", r.matkul), &judul)?;
    let info: [(&str, String); 4] = [
        ("Kode kelas", r.kode.clone()),
        ("Dosen", r.dosen.clone()),
        ("Batas alpa", r.batas.to_string()),
        ("Jumlah pertemuan", r.pertemuan.len().to_string()),
    ];
    for (i, (k, v)) in info.iter().enumerate() {
        let baris = 1 + i as u32;
        ws.write_string_with_format(baris, 0, *k, &label)?;
        match v.parse::<f64>() {
            Ok(n) if i >= 2 => ws.write_number_with_format(baris, 2, n, &dasar())?,
            _ => ws.write_string_with_format(baris, 2, v, &dasar())?,
        };
    }
    // Sel batas alpa dan jumlah pertemuan dipakai rumus di bawah.
    let (sel_batas, sel_n) = ("$C$4", "$C$5");

    let h = 6u32;
    let n = r.pertemuan.len() as u16;
    let (k_hadir, k_izin, k_sakit, k_alpa, k_persen, k_status, k_chain) = (3 + n, 4 + n, 5 + n, 6 + n, 7 + n, 8 + n, 9 + n);
    ws.set_row_height(h, 30)?;
    for (c, t) in ["No", "NIM", "Nama"].iter().enumerate() {
        ws.write_string_with_format(h, c as u16, *t, &kepala)?;
    }
    for (i, p) in r.pertemuan.iter().enumerate() {
        let tgl = format!("{}/{}", p.tanggal.get(8..10).unwrap_or(""), p.tanggal.get(5..7).unwrap_or(""));
        ws.write_string_with_format(h, 3 + i as u16, format!("P{}\n{tgl}", p.ke), &kepala)?;
        ws.set_column_width(3 + i as u16, 7)?;
    }
    for (c, t) in [(k_hadir, "Hadir"), (k_izin, "Izin"), (k_sakit, "Sakit"), (k_alpa, "Alpa"), (k_persen, "% Hadir"), (k_status, "Status"), (k_chain, "On-chain")] {
        ws.write_string_with_format(h, c, t, &kepala)?;
    }

    for (i, b) in r.mahasiswa.iter().enumerate() {
        let row = h + 1 + i as u32;
        let x = row + 1; // nomor baris di Excel
        ws.write_number_with_format(row, 0, (i + 1) as f64, &tengah)?;
        ws.write_string_with_format(row, 1, &b.nim, &sel)?;
        ws.write_string_with_format(row, 2, &b.nama, &sel)?;
        for (j, s) in b.sel.iter().enumerate() {
            ws.write_string_with_format(row, 3 + j as u16, s.tanda, &tanda_fmt(s.tanda))?;
        }
        let rentang = if n > 0 { format!("D{x}:{}{x}", kol(2 + n)) } else { format!("D{x}:D{x}") };
        let hitung = |t: &str| format!("COUNTIF({rentang},\"{t}\")");
        let (ch, ca) = (kol(k_hadir), kol(k_alpa));
        for (k, rumus, nilai) in [
            (k_hadir, hitung("H"), b.hadir),
            (k_izin, hitung("I"), b.izin),
            (k_sakit, hitung("S"), b.sakit),
            (k_alpa, format!("{}+{}", hitung("A"), hitung("D")), b.alpa),
        ] {
            ws.write_formula_with_format(row, k, Formula::new(format!("={rumus}")).set_result(nilai.to_string()), &tengah)?;
        }
        let p = if r.pertemuan.is_empty() { 0.0 } else { b.hadir as f64 / r.pertemuan.len() as f64 };
        ws.write_formula_with_format(row, k_persen, Formula::new(format!("=IF({sel_n}=0,0,{ch}{x}/{sel_n})")).set_result(p.to_string()), &persen)?;
        ws.write_formula_with_format(
            row,
            k_status,
            Formula::new(format!("=IF({ca}{x}>={sel_batas},\"lewat batas\",IF({ca}{x}={sel_batas}-1,\"hampir batas\",\"aman\"))")).set_result(b.status),
            &tengah,
        )?;
        ws.write_number_with_format(row, k_chain, b.onchain as f64, &tengah)?;
    }
    let akhir = h + 1 + r.mahasiswa.len() as u32;
    let miring = dasar().set_italic();
    ws.write_string_with_format(akhir + 1, 0, "H = hadir (tap KTM di mesin absen), I = izin, S = sakit, A = alpa, D = disanggah pemilik KTM (dihitung alpa).", &miring)?;
    ws.write_string_with_format(akhir + 2, 0, "Status dihitung dari jumlah alpa terhadap batas alpa; izin dan sakit tidak dihitung alpa.", &miring)?;
    ws.write_string_with_format(akhir + 3, 0, "On-chain = jumlah kehadiran yang buktinya tercatat di kontrak PresensiKTM (Ethereum Sepolia). Dibuat oleh Smart KTM.", &miring)?;

    ws.set_column_width(0, 5)?;
    ws.set_column_width(1, 15)?;
    ws.set_column_width(2, 28)?;
    for k in [k_hadir, k_izin, k_sakit, k_alpa, k_persen, k_chain] {
        ws.set_column_width(k, 8)?;
    }
    ws.set_column_width(k_status, 13)?;
    ws.set_freeze_panes(h + 1, 3)?;
    ws.set_landscape();
    ws.set_print_fit_to_pages(1, 0);
    wb.save_to_buffer()
}

#[cfg(test)]
mod tes {
    use super::*;
    use crate::db;

    /// Kelas pertama demo dengan 3 sesi: A hadir semua, B absen 3 kali (lewat batas 3), C disanggah sekali.
    fn siapkan() -> (Connection, i64) {
        let c = Connection::open_in_memory().unwrap();
        db::siapkan(&c).unwrap();
        let m: i64 = c.query_row("SELECT MIN(id) FROM matkul", [], |r| r.get(0)).unwrap();
        c.execute("UPDATE matkul SET batas_absen = 3 WHERE id = ?1", [m]).unwrap();
        c.execute("DELETE FROM presensi WHERE sesi_id IN (SELECT id FROM sesi WHERE matkul_id = ?1)", [m]).unwrap();
        c.execute("DELETE FROM sesi WHERE matkul_id = ?1", [m]).unwrap();
        c.execute("DELETE FROM ambil WHERE matkul_id = ?1", [m]).unwrap();
        let mhs: Vec<i64> = c.prepare("SELECT id FROM users WHERE peran = 'mhs' ORDER BY id LIMIT 3").unwrap()
            .query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
        for (i, u) in mhs.iter().enumerate() {
            c.execute("UPDATE users SET nama = ?1 WHERE id = ?2", params![["Ani", "Budi", "Caca"][i], u]).unwrap();
            c.execute("INSERT INTO ambil(user_id, matkul_id) VALUES(?1, ?2)", params![u, m]).unwrap();
        }
        for ke in 1..=3 {
            c.execute("INSERT INTO sesi(matkul_id, pertemuan, dibuka) VALUES(?1, ?2, ?3)", params![m, ke, format!("2026-10-0{ke} 08:00")]).unwrap();
            let s = c.last_insert_rowid();
            c.execute("INSERT INTO presensi(sesi_id, user_id, waktu, bukti, tx, status) VALUES(?1, ?2, '2026-10-01 08:01', ?3, '0xabc', 'onchain')", params![s, mhs[0], format!("a{ke}")]).unwrap();
            let status = if ke == 2 { "disanggah" } else { "lokal" };
            c.execute("INSERT INTO presensi(sesi_id, user_id, waktu, bukti, status) VALUES(?1, ?2, '2026-10-01 08:02', ?3, ?4)", params![s, mhs[2], format!("c{ke}"), status]).unwrap();
        }
        (c, m)
    }

    #[test]
    fn rekap_per_pertemuan() {
        let (c, m) = siapkan();
        let r = rekap(&c, m).unwrap();
        assert_eq!(r.pertemuan.iter().map(|p| p.ke).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!((r.pertemuan[0].tanggal.as_str(), r.pertemuan[0].jam.as_str()), ("2026-10-01", "08:00"));
        let cari = |n: &str| r.mahasiswa.iter().find(|b| b.nama == n).unwrap();
        let tanda = |n: &str| cari(n).sel.iter().map(|s| s.tanda).collect::<Vec<_>>();
        assert_eq!((cari("Ani").hadir, cari("Ani").alpa, cari("Ani").onchain, cari("Ani").status), (3, 0, 3, "aman"));
        assert_eq!(cari("Ani").sel[0].waktu.as_deref(), Some("2026-10-01 08:01"));
        assert_eq!(tanda("Budi"), vec!["A", "A", "A"]);
        assert_eq!((cari("Budi").alpa, cari("Budi").status), (3, "lewat batas"));
        assert_eq!(tanda("Caca"), vec!["H", "D", "H"], "presensi disanggah dihitung alpa");
        assert_eq!((cari("Caca").hadir, cari("Caca").alpa, cari("Caca").status), (2, 1, "aman"));
        assert_eq!(r.rata_hadir(), Some(56), "5 hadir dari 9 slot");
        assert_eq!(r.perlu_perhatian().iter().map(|b| b.nama.as_str()).collect::<Vec<_>>(), vec!["Budi"]);
        assert_eq!(r.json()["mahasiswa"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn izin_sakit_tidak_dihitung_alpa() {
        let (c, m) = siapkan();
        let nim = |n: &str| c.query_row("SELECT nim FROM users WHERE nama = ?1", [n], |r| r.get::<_, String>(0)).unwrap();
        let (budi, caca) = (nim("Budi"), nim("Caca"));
        assert_eq!(atur_keterangan(&c, m, &budi, 1, "I", "  acara keluarga ", 1, "2026-10-02 09:00").unwrap(), Ok(()));
        assert_eq!(atur_keterangan(&c, m, &budi, 2, "S", "", 1, "2026-10-02 09:00").unwrap(), Ok(()));
        // Ganti keterangan yang sama = timpa, bukan dobel.
        assert_eq!(atur_keterangan(&c, m, &budi, 2, "S", "surat dokter", 1, "2026-10-02 09:05").unwrap(), Ok(()));
        // Yang disanggah boleh diberi keterangan.
        assert_eq!(atur_keterangan(&c, m, &caca, 2, "I", "", 1, "2026-10-02 09:00").unwrap(), Ok(()));
        let r = rekap(&c, m).unwrap();
        let cari = |n: &str| r.mahasiswa.iter().find(|b| b.nama == n).unwrap();
        let b = cari("Budi");
        assert_eq!(b.sel.iter().map(|s| s.tanda).collect::<Vec<_>>(), vec!["I", "S", "A"]);
        assert_eq!((b.izin, b.sakit, b.alpa, b.status), (1, 1, 1, "aman"));
        assert_eq!(b.sel[0].catatan.as_deref(), Some("acara keluarga"));
        assert_eq!(b.sel[1].catatan.as_deref(), Some("surat dokter"));
        assert_eq!((cari("Caca").sel[1].tanda, cari("Caca").alpa), ("I", 0));

        // Hapus keterangan = kembali alpa.
        assert_eq!(atur_keterangan(&c, m, &budi, 1, "", "", 1, "2026-10-02 09:10").unwrap(), Ok(()));
        assert_eq!(rekap(&c, m).unwrap().mahasiswa.iter().find(|b| b.nama == "Budi").unwrap().sel[0].tanda, "A");
    }

    #[test]
    fn keterangan_ditolak_kalau_tidak_sah() {
        let (c, m) = siapkan();
        let nim = |n: &str| c.query_row("SELECT nim FROM users WHERE nama = ?1", [n], |r| r.get::<_, String>(0)).unwrap();
        let ok = |nim: &str, ke: i64, j: &str| atur_keterangan(&c, m, nim, ke, j, "", 1, "2026-10-02 09:00").unwrap();
        assert_eq!(ok(&nim("Ani"), 1, "S"), Err("Mahasiswa ini sudah hadir lewat tap KTM"), "hadir tidak bisa ditimpa");
        assert_eq!(ok(&nim("Budi"), 9, "I"), Err("Pertemuan ini belum pernah dibuka"));
        assert_eq!(ok(&nim("Budi"), 1, "H"), Err("Pilih izin, sakit, atau alpa"), "hadir hanya dari tap KTM");
        assert_eq!(ok("999", 1, "I"), Err("Mahasiswa ini tidak terdaftar di kelas"));
    }

    #[test]
    fn excel_bisa_dibuat() {
        let (c, m) = siapkan();
        let b = excel(&rekap(&c, m).unwrap()).unwrap();
        assert_eq!(&b[..2], b"PK", "berkas xlsx adalah zip");
        assert!(b.len() > 3000);
    }
}
