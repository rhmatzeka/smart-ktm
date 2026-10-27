//! Jadwal kuliah dan tenggat tugas sebagai kalender iCalendar (.ics) untuk Google Calendar / kalender HP.
//!
//! Kalau mahasiswa sudah sinkron MyUnpam, setiap pertemuan memakai tanggal aslinya dari MyUnpam
//! (minggu libur/UTS tidak ikut). Kalau belum, kelas dibuat berulang mingguan 16 kali.

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, Utc};
use rusqlite::{params, Connection};

use crate::{data, waktu};

pub fn ics(c: &Connection, user: i64, kini: &DateTime<FixedOffset>) -> rusqlite::Result<String> {
    let mut acara: Vec<Vec<String>> = vec![];
    let utc = |t: DateTime<FixedOffset>| t.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ").to_string();
    let jam = |tgl: NaiveDate, j: &str| waktu::baca(&format!("{} {j}", tgl.format("%Y-%m-%d")));

    let kelas: Vec<(i64, String, u32, String, String, String, Option<String>)> = c
        .prepare("SELECT m.id, m.nama, m.hari, m.mulai, m.selesai, m.ruang, m.dosen_nama FROM matkul m JOIN ambil a ON a.matkul_id = m.id WHERE a.user_id = ?1")?
        .query_map([user], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (id, nama, hari, mulai, selesai, ruang, dosen) in kelas {
        let ket = dosen.filter(|d| !d.is_empty()).map(|d| format!("Dosen: {d}")).unwrap_or_default();
        let pertemuan: Vec<(i64, String)> = c
            .prepare("SELECT ke, mulai FROM pertemuan_unpam WHERE user_id = ?1 AND matkul_id = ?2 ORDER BY ke")?
            .query_map(params![user, id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let isi = |uid: String, judul: String, tgl: NaiveDate| -> Option<Vec<String>> {
            Some(vec![
                format!("UID:{uid}"),
                format!("DTSTART:{}", utc(jam(tgl, &mulai)?)),
                format!("DTEND:{}", utc(jam(tgl, &selesai)?)),
                format!("SUMMARY:{}", teks(&judul)),
                format!("LOCATION:{}", teks(&ruang)),
                format!("DESCRIPTION:{}", teks(&ket)),
            ])
        };
        if pertemuan.is_empty() {
            // Belum sinkron MyUnpam: mulai pekan ini, berulang mingguan.
            let senin = kini.date_naive() - chrono::Duration::days(kini.weekday().num_days_from_monday() as i64);
            if let Some(mut e) = isi(format!("kelas-{id}-u{user}@ktm.rahmateka.my.id"), nama.clone(), senin + chrono::Duration::days(hari as i64 - 1)) {
                e.push("RRULE:FREQ=WEEKLY;COUNT=16".into());
                e.extend(alarm("-PT15M", &nama));
                acara.push(e);
            }
        }
        for (ke, awal_pekan) in pertemuan {
            let Ok(awal) = NaiveDate::parse_from_str(&awal_pekan, "%Y-%m-%d") else { continue };
            let geser = (hari as i64 - 1 - awal.weekday().num_days_from_monday() as i64).rem_euclid(7);
            if let Some(mut e) = isi(format!("kelas-{id}-p{ke}-u{user}@ktm.rahmateka.my.id"), format!("{nama} (P{ke})"), awal + chrono::Duration::days(geser)) {
                e.extend(alarm("-PT15M", &nama));
                acara.push(e);
            }
        }
    }

    for t in data::tugas(c, user, kini, false)? {
        let (Some(id), Some(judul), Some(Some(tenggat))) = (t["id"].as_i64(), t["judul"].as_str(), t["deadline"].as_str().map(waktu::baca)) else { continue };
        let judul = format!("Tenggat: {judul}");
        let mut e = vec![
            format!("UID:tugas-{id}-u{user}@ktm.rahmateka.my.id"),
            format!("DTSTART:{}", utc(tenggat - chrono::Duration::minutes(30))),
            format!("DTEND:{}", utc(tenggat)),
            format!("SUMMARY:{}", teks(&judul)),
            format!("DESCRIPTION:{}", teks(t["matkul"].as_str().unwrap_or_default())),
        ];
        if let Some(u) = t["url"].as_str() {
            e.push(format!("URL:{u}"));
        }
        e.extend(alarm("-P1D", &judul));
        e.extend(alarm("-PT3H", &judul));
        acara.push(e);
    }

    let stempel = format!("DTSTAMP:{}", utc(*kini));
    let mut baris = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".into(),
        "PRODID:-//Smart KTM//Jadwal Kuliah//ID".into(),
        "CALSCALE:GREGORIAN".into(),
        "METHOD:PUBLISH".into(),
        "X-WR-CALNAME:Kuliah (Smart KTM)".into(),
        "X-WR-TIMEZONE:Asia/Jakarta".into(),
        "REFRESH-INTERVAL;VALUE=DURATION:PT6H".into(),
    ];
    for e in acara {
        baris.push("BEGIN:VEVENT".into());
        baris.push(stempel.clone());
        baris.extend(e);
        baris.push("END:VEVENT".into());
    }
    baris.push("END:VCALENDAR".into());
    Ok(baris.iter().map(|b| lipat(b)).collect::<String>())
}

fn alarm(kapan: &str, judul: &str) -> Vec<String> {
    vec![
        "BEGIN:VALARM".into(),
        "ACTION:DISPLAY".into(),
        format!("TRIGGER:{kapan}"),
        format!("DESCRIPTION:{}", teks(judul)),
        "END:VALARM".into(),
    ]
}

/// Escape teks iCalendar (RFC 5545 3.3.11).
fn teks(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace('\n', "\\n")
}

/// Lipat baris lebih dari 75 oktet (RFC 5545 3.1) tanpa memotong karakter UTF-8, akhiri dengan CRLF.
fn lipat(baris: &str) -> String {
    let mut hasil = String::new();
    let mut panjang = 0;
    for ch in baris.chars() {
        if panjang + ch.len_utf8() > 75 {
            hasil.push_str("\r\n ");
            panjang = 1;
        }
        hasil.push(ch);
        panjang += ch.len_utf8();
    }
    hasil.push_str("\r\n");
    hasil
}

#[cfg(test)]
mod tes {
    use super::*;
    use crate::db;

    #[test]
    fn kelas_dan_tenggat_jadi_acara_kalender() {
        let c = Connection::open_in_memory().unwrap();
        db::siapkan(&c).unwrap();
        let id: i64 = c.query_row("SELECT id FROM users WHERE nim = '231011400001'", [], |r| r.get(0)).unwrap();
        let ms: Vec<i64> = c.prepare("SELECT matkul_id FROM ambil WHERE user_id = ?1 ORDER BY matkul_id LIMIT 2").unwrap()
            .query_map([id], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
        c.execute("DELETE FROM ambil WHERE user_id = ?1 AND matkul_id NOT IN (?2, ?3)", params![id, ms[0], ms[1]]).unwrap();
        c.execute("DELETE FROM tugas", []).unwrap();
        // Kelas 1: sudah sinkron MyUnpam, pertemuan 8 di pekan 26 Okt, kelasnya Rabu 08.00.
        c.execute("UPDATE matkul SET nama = 'Etika, Profesi', hari = 3, mulai = '08:00', selesai = '09:40', ruang = 'V.630' WHERE id = ?1", [ms[0]]).unwrap();
        c.execute("INSERT INTO pertemuan_unpam(user_id, matkul_id, ke, mulai, akhir) VALUES(?1, ?2, 8, '2026-10-26', '2026-10-31')", params![id, ms[0]]).unwrap();
        c.execute("INSERT INTO tugas(matkul_id, judul, deskripsi, deadline) VALUES(?1, 'Buat WBS', '', '2026-10-30 23:59')", [ms[0]]).unwrap();

        let kini = waktu::baca("2026-10-27 10:00").unwrap();
        let s = ics(&c, id, &kini).unwrap();
        assert!(s.starts_with("BEGIN:VCALENDAR\r\n") && s.ends_with("END:VCALENDAR\r\n"));
        assert!(s.contains("DTSTART:20261028T010000Z"), "Rabu 08.00 WIB = 01.00 UTC");
        assert!(s.contains("SUMMARY:Etika\\, Profesi (P8)"), "koma di-escape");
        assert!(s.contains("RRULE:FREQ=WEEKLY;COUNT=16"), "kelas yang belum sinkron berulang mingguan");
        assert!(s.contains("SUMMARY:Tenggat: Buat WBS"));
        assert!(s.contains("DTEND:20261030T165900Z"));
        assert_eq!(s.matches("BEGIN:VEVENT").count(), 3);
        assert!(s.split("\r\n").all(|l| l.len() <= 75), "baris dilipat maks 75 oktet");
    }
}
