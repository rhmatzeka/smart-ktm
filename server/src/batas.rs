//! Pembatas percobaan dalam memori: batas permintaan per kunci (mis. IP) dan
//! penguncian akun setelah PIN salah berulang. Hilang saat server restart, cukup
//! untuk menahan tebakan PIN/UID otomatis.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Pembatas {
    peta: Mutex<HashMap<String, (u32, Instant)>>,
}

impl Pembatas {
    /// Catat satu permintaan; false kalau sudah melewati `maks` dalam `jendela`.
    pub fn izin(&self, kunci: &str, maks: u32, jendela: Duration) -> bool {
        let mut p = self.peta.lock().unwrap();
        let kini = Instant::now();
        // Bersihkan entri kedaluwarsa sesekali supaya peta tidak membengkak.
        if p.len() > 10_000 {
            p.retain(|_, (_, mulai)| kini.duration_since(*mulai) < Duration::from_secs(3600));
        }
        let e = p.entry(kunci.to_string()).or_insert((0, kini));
        if kini.duration_since(e.1) >= jendela {
            *e = (0, kini);
        }
        e.0 += 1;
        e.0 <= maks
    }

    /// Sudah terkunci karena `maks` kegagalan dalam `jendela`? (tidak menambah hitungan)
    pub fn terkunci(&self, kunci: &str, maks: u32, jendela: Duration) -> bool {
        let p = self.peta.lock().unwrap();
        p.get(kunci).is_some_and(|(n, mulai)| *n >= maks && mulai.elapsed() < jendela)
    }

    /// Catat satu kegagalan (mis. PIN salah).
    pub fn gagal(&self, kunci: &str, jendela: Duration) {
        let mut p = self.peta.lock().unwrap();
        let e = p.entry(kunci.to_string()).or_insert((0, Instant::now()));
        if e.1.elapsed() >= jendela {
            *e = (0, Instant::now());
        }
        e.0 += 1;
    }

    pub fn hapus(&self, kunci: &str) {
        self.peta.lock().unwrap().remove(kunci);
    }
}

#[cfg(test)]
mod tes {
    use super::*;

    #[test]
    fn batas_permintaan() {
        let b = Pembatas::default();
        let j = Duration::from_secs(60);
        assert!((0..3).all(|_| b.izin("ip", 3, j)));
        assert!(!b.izin("ip", 3, j));
        assert!(b.izin("ip-lain", 3, j));
    }

    #[test]
    fn kunci_setelah_gagal() {
        let b = Pembatas::default();
        let j = Duration::from_secs(900);
        for _ in 0..4 {
            b.gagal("pin:1", j);
        }
        assert!(!b.terkunci("pin:1", 5, j));
        b.gagal("pin:1", j);
        assert!(b.terkunci("pin:1", 5, j));
        b.hapus("pin:1");
        assert!(!b.terkunci("pin:1", 5, j));
    }
}
