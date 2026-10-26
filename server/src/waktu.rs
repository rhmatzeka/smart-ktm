//! Semua waktu disimpan sebagai teks "YYYY-MM-DD HH:MM" dalam WIB (UTC+7).

use chrono::{DateTime, Datelike, FixedOffset, NaiveDateTime, Timelike, Utc};

pub const FORMAT: &str = "%Y-%m-%d %H:%M";
pub const HARI: [&str; 7] = ["Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu", "Minggu"];
const BULAN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des"];

pub fn wib() -> FixedOffset {
    FixedOffset::east_opt(7 * 3600).unwrap()
}

pub fn sekarang() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&wib())
}

pub fn teks(t: &DateTime<FixedOffset>) -> String {
    t.format(FORMAT).to_string()
}

pub fn baca(s: &str) -> Option<DateTime<FixedOffset>> {
    NaiveDateTime::parse_from_str(s, FORMAT).ok()?.and_local_timezone(wib()).single()
}

/// 1 = Senin ... 7 = Minggu
pub fn hari_ke(t: &DateTime<FixedOffset>) -> u32 {
    t.weekday().number_from_monday()
}

pub fn nama_hari(hari: u32) -> &'static str {
    HARI[((hari + 6) % 7) as usize]
}

/// "Selasa, 27 Okt · 07.35"
pub fn tanggal_panjang(t: &DateTime<FixedOffset>) -> String {
    format!(
        "{}, {} {} · {:02}.{:02}",
        nama_hari(hari_ke(t)),
        t.day(),
        BULAN[t.month0() as usize],
        t.hour(),
        t.minute()
    )
}

/// "08:00" -> menit sejak tengah malam
pub fn menit(jam: &str) -> u32 {
    let (h, m) = jam.split_once(':').unwrap_or(("0", "0"));
    h.parse::<u32>().unwrap_or(0) * 60 + m.parse::<u32>().unwrap_or(0)
}

pub fn menit_sekarang(t: &DateTime<FixedOffset>) -> u32 {
    t.hour() * 60 + t.minute()
}

/// Teks sisa waktu ke deadline: "lewat", "hari ini 23.59", "besok 23.59", "3 hari lagi".
pub fn sisa(deadline: &str, kini: &DateTime<FixedOffset>) -> (String, &'static str) {
    let Some(d) = baca(deadline) else { return ("-".into(), "biasa") };
    if d < *kini {
        return ("lewat".into(), "merah");
    }
    let jam = format!("{:02}.{:02}", d.hour(), d.minute());
    let selisih_hari = (d.date_naive() - kini.date_naive()).num_days();
    match selisih_hari {
        0 => (format!("hari ini {jam}"), "merah"),
        1 => (format!("besok {jam}"), "merah"),
        2..=3 => (format!("{selisih_hari} hari lagi"), "kuning"),
        _ => (format!("{selisih_hari} hari lagi"), "biasa"),
    }
}

/// Momen hari ini menentukan kartu apa yang paling atas di beranda.
/// `kelas` = (mulai, selesai) dalam menit, kelas hari ini saja.
pub fn momen(kini: u32, kelas: &[(u32, u32)]) -> &'static str {
    if kelas.iter().any(|&(m, s)| m <= kini && kini < s) {
        return "kelas";
    }
    // Lewat tengah malam dihitung hari baru: kelas pagi nanti yang ditampilkan.
    if kini >= 18 * 60 {
        return "malam";
    }
    if kelas.iter().any(|&(m, _)| kini < m) {
        return "sebelum";
    }
    if kelas.iter().any(|&(_, s)| s <= kini) {
        return "setelah";
    }
    "bebas"
}

#[cfg(test)]
mod tes {
    use super::*;

    #[test]
    fn momen_sesuai_jam() {
        let kelas = [(8 * 60, 10 * 60 + 30), (13 * 60, 15 * 60 + 30)];
        assert_eq!(momen(7 * 60, &kelas), "sebelum");
        assert_eq!(momen(9 * 60, &kelas), "kelas");
        assert_eq!(momen(11 * 60, &kelas), "sebelum");
        assert_eq!(momen(16 * 60, &kelas), "setelah");
        assert_eq!(momen(20 * 60, &kelas), "malam");
        assert_eq!(momen(12 * 60, &[]), "bebas");
        assert_eq!(momen(60, &kelas), "sebelum"); // jam 01.00
        assert_eq!(momen(60, &[]), "bebas");
    }

    #[test]
    fn sisa_deadline() {
        let kini = baca("2026-10-27 07:00").unwrap();
        assert_eq!(sisa("2026-10-26 23:59", &kini).0, "lewat");
        assert_eq!(sisa("2026-10-27 23:59", &kini).0, "hari ini 23.59");
        assert_eq!(sisa("2026-10-28 23:59", &kini), ("besok 23.59".into(), "merah"));
        assert_eq!(sisa("2026-10-30 23:59", &kini), ("3 hari lagi".into(), "kuning"));
    }

    #[test]
    fn tanggal_indonesia() {
        let t = baca("2026-10-27 07:35").unwrap();
        assert_eq!(tanggal_panjang(&t), "Selasa, 27 Okt · 07.35");
    }
}
