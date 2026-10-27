//! Rekap kehadiran satu kelas untuk dosen (dari tap KTM di Smart KTM) dan ekspornya ke Excel.

use rusqlite::{params, Connection};
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Formula, Workbook, XlsxError};
use serde_json::{json, Value};

pub struct Rekap {
    pub matkul: String,
    pub kode: String,
    pub dosen: String,
    pub batas: i64,
    /// (pertemuan ke-, tanggal sesi pertama "YYYY-MM-DD")
    pub pertemuan: Vec<(i64, String)>,
    pub mahasiswa: Vec<Baris>,
}

pub struct Baris {
    pub nim: String,
    pub nama: String,
    /// Per pertemuan: "H" hadir, "-" tidak hadir, "S" disanggah pemilik KTM.
    pub tanda: Vec<&'static str>,
    pub hadir: i64,
    pub absen: i64,
    pub onchain: i64,
    pub status: &'static str,
}

pub fn rekap(c: &Connection, matkul: i64) -> rusqlite::Result<Rekap> {
    let (nama, kode, dosen, batas): (String, String, String, i64) = c.query_row(
        "SELECT m.nama, m.kode, COALESCE(NULLIF(m.dosen_nama, ''), u.nama), m.batas_absen FROM matkul m JOIN users u ON u.id = m.dosen_id WHERE m.id = ?1",
        [matkul],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let pertemuan: Vec<(i64, String)> = c
        .prepare("SELECT pertemuan, MIN(substr(dibuka, 1, 10)) FROM sesi WHERE matkul_id = ?1 GROUP BY pertemuan ORDER BY pertemuan")?
        .query_map([matkul], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    // (mahasiswa, pertemuan) -> (hadir sah?, on-chain?)
    let mut catatan: std::collections::HashMap<(i64, i64), (bool, bool)> = Default::default();
    let mut q = c.prepare(
        "SELECT p.user_id, s.pertemuan, p.status != 'disanggah', p.tx IS NOT NULL AND p.status != 'disanggah'
         FROM presensi p JOIN sesi s ON s.id = p.sesi_id WHERE s.matkul_id = ?1",
    )?;
    for b in q.query_map([matkul], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, bool>(2)?, r.get::<_, bool>(3)?)))? {
        let (u, ke, sah, chain) = b?;
        let e = catatan.entry((u, ke)).or_default();
        e.0 |= sah;
        e.1 |= chain;
    }
    let peserta: Vec<(i64, String, String)> = c
        .prepare("SELECT u.id, u.nim, u.nama FROM ambil a JOIN users u ON u.id = a.user_id WHERE a.matkul_id = ?1 ORDER BY u.nama")?
        .query_map([matkul], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mahasiswa = peserta
        .into_iter()
        .map(|(id, nim, nama)| {
            let tanda: Vec<&'static str> = pertemuan
                .iter()
                .map(|(ke, _)| match catatan.get(&(id, *ke)) {
                    Some((true, _)) => "H",
                    Some((false, _)) => "S",
                    None => "-",
                })
                .collect();
            let hadir = tanda.iter().filter(|t| **t == "H").count() as i64;
            let absen = pertemuan.len() as i64 - hadir;
            let onchain = pertemuan.iter().filter(|(ke, _)| catatan.get(&(id, *ke)).is_some_and(|c| c.1)).count() as i64;
            let status = if absen >= batas {
                "lewat batas"
            } else if absen == batas - 1 {
                "hampir batas"
            } else {
                "aman"
            };
            Baris { nim, nama, tanda, hadir, absen, onchain, status }
        })
        .collect();
    Ok(Rekap { matkul: nama, kode, dosen, batas, pertemuan, mahasiswa })
}

impl Rekap {
    pub fn json(&self) -> Value {
        json!({
            "matkul": self.matkul, "kode": self.kode, "dosen": self.dosen, "batas": self.batas,
            "pertemuan": self.pertemuan.iter().map(|(ke, tgl)| json!({ "ke": ke, "tanggal": tgl })).collect::<Vec<_>>(),
            "mahasiswa": self.mahasiswa.iter().map(|b| json!({
                "nim": b.nim, "nama": b.nama, "tanda": b.tanda, "hadir": b.hadir, "absen": b.absen,
                "onchain": b.onchain, "status": b.status,
            })).collect::<Vec<_>>(),
        })
    }
}

/// Satu sheet "Rekap": identitas kelas, tabel mahasiswa x pertemuan (H / - / S), lalu jumlah hadir,
/// absen, persentase, dan status sebagai rumus Excel supaya ikut berubah kalau dosen mengoreksi tanda.
pub fn excel(r: &Rekap) -> Result<Vec<u8>, XlsxError> {
    use rust_xlsxwriter::utility::column_number_to_name as kol;
    let dasar = || Format::new().set_font_name("Arial").set_font_size(10);
    let judul = dasar().set_bold().set_font_size(14);
    let label = dasar().set_bold();
    let kepala = dasar().set_bold().set_font_color(Color::White).set_background_color(Color::RGB(0x1747C9))
        .set_align(FormatAlign::Center).set_align(FormatAlign::VerticalCenter).set_border(FormatBorder::Thin).set_text_wrap();
    let sel = dasar().set_border(FormatBorder::Thin);
    let tengah = sel.clone().set_align(FormatAlign::Center);
    let tanda_fmt = |t: &str| match t {
        "H" => tengah.clone().set_background_color(Color::RGB(0xDCFCE7)),
        "S" => tengah.clone().set_background_color(Color::RGB(0xFEF3C7)),
        _ => tengah.clone().set_background_color(Color::RGB(0xFFE4E6)),
    };
    let persen = tengah.clone().set_num_format("0%");

    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Rekap")?;
    ws.write_string_with_format(0, 0, format!("Rekap Kehadiran {}", r.matkul), &judul)?;
    let info: [(&str, String); 4] = [
        ("Kode kelas", r.kode.clone()),
        ("Dosen", r.dosen.clone()),
        ("Batas absen", r.batas.to_string()),
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
    // Sel batas absen dan jumlah pertemuan dipakai rumus di bawah.
    let (sel_batas, sel_n) = ("$C$4", "$C$5");

    let h = 6u32;
    let n = r.pertemuan.len() as u16;
    let (k_hadir, k_absen, k_persen, k_status, k_chain) = (3 + n, 4 + n, 5 + n, 6 + n, 7 + n);
    ws.set_row_height(h, 30)?;
    for (c, t) in ["No", "NIM", "Nama"].iter().enumerate() {
        ws.write_string_with_format(h, c as u16, *t, &kepala)?;
    }
    for (i, (ke, tgl)) in r.pertemuan.iter().enumerate() {
        let tgl = format!("{}/{}", &tgl[8..10], &tgl[5..7]);
        ws.write_string_with_format(h, 3 + i as u16, format!("P{ke}\n{tgl}"), &kepala)?;
        ws.set_column_width(3 + i as u16, 7)?;
    }
    for (c, t) in [(k_hadir, "Hadir"), (k_absen, "Absen"), (k_persen, "% Hadir"), (k_status, "Status"), (k_chain, "On-chain")] {
        ws.write_string_with_format(h, c, t, &kepala)?;
    }

    for (i, b) in r.mahasiswa.iter().enumerate() {
        let row = h + 1 + i as u32;
        let x = row + 1; // nomor baris di Excel
        ws.write_number_with_format(row, 0, (i + 1) as f64, &tengah)?;
        ws.write_string_with_format(row, 1, &b.nim, &sel)?;
        ws.write_string_with_format(row, 2, &b.nama, &sel)?;
        for (j, t) in b.tanda.iter().enumerate() {
            ws.write_string_with_format(row, 3 + j as u16, *t, &tanda_fmt(t))?;
        }
        let rentang = if n > 0 { format!("D{x}:{}{x}", kol(2 + n)) } else { format!("D{x}:D{x}") };
        let (ch, ca) = (kol(k_hadir), kol(k_absen));
        ws.write_formula_with_format(row, k_hadir, Formula::new(format!("=COUNTIF({rentang},\"H\")")).set_result(b.hadir.to_string()), &tengah)?;
        ws.write_formula_with_format(row, k_absen, Formula::new(format!("={sel_n}-{ch}{x}")).set_result(b.absen.to_string()), &tengah)?;
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
    ws.write_string_with_format(akhir + 1, 0, "H = hadir (tap KTM di mesin absen), - = tidak hadir, S = disanggah pemilik KTM (tidak dihitung hadir).", &dasar().set_italic())?;
    ws.write_string_with_format(akhir + 2, 0, "On-chain = jumlah kehadiran yang buktinya tercatat di kontrak PresensiKTM (Ethereum Sepolia). Dibuat oleh Smart KTM.", &dasar().set_italic())?;

    ws.set_column_width(0, 5)?;
    ws.set_column_width(1, 15)?;
    ws.set_column_width(2, 28)?;
    ws.set_column_width(k_persen, 9)?;
    ws.set_column_width(k_status, 13)?;
    ws.set_column_width(k_chain, 9)?;
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
        assert_eq!(r.pertemuan.iter().map(|p| p.0).collect::<Vec<_>>(), vec![1, 2, 3]);
        let cari = |n: &str| r.mahasiswa.iter().find(|b| b.nama == n).unwrap();
        assert_eq!((cari("Ani").hadir, cari("Ani").absen, cari("Ani").onchain, cari("Ani").status), (3, 0, 3, "aman"));
        assert_eq!((cari("Budi").absen, cari("Budi").status), (3, "lewat batas"));
        assert_eq!(cari("Caca").tanda, vec!["H", "S", "H"], "presensi disanggah tidak dihitung hadir");
        assert_eq!((cari("Caca").hadir, cari("Caca").status), (2, "aman"));
        assert_eq!(r.json()["mahasiswa"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn excel_bisa_dibuat() {
        let (c, m) = siapkan();
        let b = excel(&rekap(&c, m).unwrap()).unwrap();
        assert_eq!(&b[..2], b"PK", "berkas xlsx adalah zip");
        assert!(b.len() > 3000);
    }
}
