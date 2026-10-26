//! AI agent dengan function calling ke data akademik mahasiswa.
//! Urutan penyedia: Groq (gpt-oss, API gaya OpenAI) lalu Gemini. Tanpa kunci,
//! atau saat semuanya gagal, jatuh ke mode offline berbasis kata kunci.

use std::time::Duration;

use anyhow::bail;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{api::Pengguna, data, waktu, App};

const GROQ_URL: &str = "https://api.groq.com/openai/v1/chat/completions";

pub struct Konfig {
    pub kunci: Option<String>,
    pub model: String,
    pub groq_kunci: Option<String>,
    pub groq_model: String,
}

impl Konfig {
    pub fn dari_env() -> Self {
        let ambil = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Self {
            kunci: ambil("GEMINI_API_KEY"),
            model: ambil("GEMINI_MODEL").unwrap_or_else(|| "gemini-2.5-flash".into()),
            groq_kunci: ambil("GROQ_API_KEY"),
            groq_model: ambil("GROQ_MODEL").unwrap_or_else(|| "openai/gpt-oss-120b".into()),
        }
    }

    pub fn aktif(&self) -> bool {
        self.groq_kunci.is_some() || self.kunci.is_some()
    }

    pub fn label(&self) -> String {
        let mut p = vec![];
        if self.groq_kunci.is_some() {
            p.push(format!("Groq {}", self.groq_model));
        }
        if self.kunci.is_some() {
            p.push(format!("Gemini {}", self.model));
        }
        if p.is_empty() {
            "offline (GROQ_API_KEY dan GEMINI_API_KEY kosong)".into()
        } else {
            p.join(" -> ")
        }
    }

    fn url(&self) -> String {
        format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent", self.model)
    }

    /// Flash tanpa "thinking" supaya balasan cepat.
    fn generation_config(&self) -> Value {
        if self.model.contains("flash") {
            json!({ "thinkingConfig": { "thinkingBudget": 0 } })
        } else {
            json!({})
        }
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Pesan {
    pub peran: String,
    pub teks: String,
}

fn alat() -> Value {
    json!([{ "functionDeclarations": [
        { "name": "lihat_jadwal", "description": "Melihat jadwal kuliah mahasiswa (matkul, jam, ruang, dosen).",
          "parameters": { "type": "object", "properties": { "hari": { "type": "string", "description": "'hari ini', 'besok', nama hari (Senin..Minggu), atau 'semua'" } } } },
        { "name": "lihat_tugas", "description": "Melihat tugas dan deadline. Hasil berisi id tugas.",
          "parameters": { "type": "object", "properties": { "termasuk_selesai": { "type": "boolean" } } } },
        { "name": "lihat_presensi", "description": "Rekap kehadiran per mata kuliah: hadir, total pertemuan, absen, batas absen, jumlah tercatat on-chain." },
        { "name": "tandai_tugas_selesai", "description": "Menandai tugas sudah dikumpulkan. Pakai hanya kalau mahasiswa bilang tugasnya sudah selesai.",
          "parameters": { "type": "object", "properties": { "tugas_id": { "type": "integer" } }, "required": ["tugas_id"] } },
        { "name": "buat_pengingat", "description": "Membuat pengingat untuk mahasiswa.",
          "parameters": { "type": "object", "properties": {
              "teks": { "type": "string" },
              "waktu": { "type": "string", "description": "Format YYYY-MM-DD HH:MM, zona WIB" } }, "required": ["teks", "waktu"] } },
        { "name": "lihat_pengingat", "description": "Melihat pengingat yang sudah dibuat." }
    ]}])
}

fn pilih_hari(teks: &str) -> Option<u32> {
    let t = teks.to_lowercase();
    let hari_ini = waktu::hari_ke(&waktu::sekarang());
    if t.contains("semua") || t.contains("minggu ini") || t.contains("seminggu") {
        return None;
    }
    if t.contains("lusa") {
        return Some((hari_ini + 1) % 7 + 1);
    }
    if t.contains("besok") {
        return Some(hari_ini % 7 + 1);
    }
    waktu::HARI
        .iter()
        .position(|h| t.contains(&h.to_lowercase()))
        .map(|i| i as u32 + 1)
        .or(Some(hari_ini))
}

/// Menjalankan satu alat. Mengembalikan hasil untuk model + keterangan aksi (kalau mengubah data).
pub fn jalankan(app: &App, u: &Pengguna, nama: &str, args: &Value) -> (Value, Option<String>) {
    let c = app.db.lock().unwrap();
    let kini = waktu::sekarang();
    let hasil = (|| -> rusqlite::Result<(Value, Option<String>)> {
        Ok(match nama {
            "lihat_jadwal" => {
                let hari = pilih_hari(args["hari"].as_str().unwrap_or("hari ini"));
                let kelas: Vec<Value> = data::jadwal(&c, u.id, hari)?.iter().map(|k| k.json()).collect();
                let label = hari.map(waktu::nama_hari).unwrap_or("semua hari");
                (json!({ "hari": label, "kelas": kelas }), None)
            }
            "lihat_tugas" => (json!({ "tugas": data::tugas(&c, u.id, &kini, args["termasuk_selesai"].as_bool().unwrap_or(false))? }), None),
            "lihat_presensi" => (json!({ "presensi": data::presensi(&c, u.id, &u.dibuat)? }), None),
            "tandai_tugas_selesai" => {
                let id = args["tugas_id"].as_i64().unwrap_or(0);
                let judul: Option<String> = c
                    .query_row(
                        "SELECT t.judul FROM tugas t JOIN ambil a ON a.matkul_id = t.matkul_id AND a.user_id = ?1 WHERE t.id = ?2",
                        params![u.id, id],
                        |r| r.get(0),
                    )
                    .optional()?;
                match judul {
                    Some(j) => {
                        c.execute(
                            "INSERT OR IGNORE INTO tugas_selesai(user_id, tugas_id, waktu) VALUES(?1, ?2, ?3)",
                            params![u.id, id, waktu::teks(&kini)],
                        )?;
                        (json!({ "ok": true, "judul": j }), Some(format!("Tugas \"{j}\" ditandai selesai")))
                    }
                    None => (json!({ "galat": "tugas tidak ditemukan" }), None),
                }
            }
            "buat_pengingat" => {
                let teks = args["teks"].as_str().unwrap_or("").trim().chars().take(200).collect::<String>();
                let saat = args["waktu"].as_str().unwrap_or("");
                match waktu::baca(saat) {
                    Some(t) if !teks.is_empty() => {
                        c.execute(
                            "INSERT INTO pengingat(user_id, teks, waktu, dibuat) VALUES(?1, ?2, ?3, ?4)",
                            params![u.id, teks, waktu::teks(&t), waktu::teks(&kini)],
                        )?;
                        (json!({ "ok": true }), Some(format!("Pengingat {}: {teks}", waktu::tanggal_panjang(&t))))
                    }
                    _ => (json!({ "galat": "teks kosong atau format waktu bukan YYYY-MM-DD HH:MM" }), None),
                }
            }
            "lihat_pengingat" => (json!({ "pengingat": data::pengingat(&c, u.id)? }), None),
            _ => (json!({ "galat": "alat tidak dikenal" }), None),
        })
    })();
    hasil.unwrap_or_else(|e| (json!({ "galat": e.to_string() }), None))
}

pub async fn chat(app: &App, u: &Pengguna, pesan: &str, riwayat: &[Pesan]) -> Value {
    let mut aksi = vec![];
    if let Some(kunci) = &app.ai.groq_kunci {
        match groq(app, kunci, u, pesan, riwayat, &mut aksi).await {
            Ok(t) => return json!({ "balasan": t, "aksi": aksi, "mode": "groq" }),
            Err(e) => eprintln!("groq gagal: {e:#}"),
        }
    }
    if let Some(kunci) = &app.ai.kunci {
        match gemini(app, kunci, u, pesan, riwayat, &mut aksi).await {
            Ok(t) => return json!({ "balasan": t, "aksi": aksi, "mode": "gemini" }),
            Err(e) => eprintln!("gemini gagal: {e:#}"),
        }
    }
    json!({ "balasan": offline(app, u, pesan), "aksi": aksi, "mode": "offline" })
}

/// Alat yang sama dalam format OpenAI (dipakai Groq).
fn alat_openai() -> Value {
    let deklarasi = alat()[0]["functionDeclarations"].as_array().cloned().unwrap_or_default();
    Value::Array(
        deklarasi
            .into_iter()
            .map(|f| {
                let parameter = f.get("parameters").cloned().unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
                json!({ "type": "function", "function": { "name": f["name"], "description": f["description"], "parameters": parameter } })
            })
            .collect(),
    )
}

async fn groq(app: &App, kunci: &str, u: &Pengguna, pesan: &str, riwayat: &[Pesan], aksi: &mut Vec<String>) -> anyhow::Result<String> {
    let mut isi = vec![json!({ "role": "system", "content": sistem(u) })];
    for p in riwayat.iter().rev().take(10).rev() {
        isi.push(json!({ "role": if p.peran == "model" { "assistant" } else { "user" }, "content": p.teks }));
    }
    isi.push(json!({ "role": "user", "content": pesan }));

    for _ in 0..5 {
        let badan = json!({
            "model": app.ai.groq_model, "messages": isi, "tools": alat_openai(),
            "tool_choice": "auto", "reasoning_effort": "low", "temperature": 0.4,
        });
        let res = app.http.post(GROQ_URL).bearer_auth(kunci).json(&badan).timeout(Duration::from_secs(40)).send().await?;
        let status = res.status();
        let v: Value = res.json().await?;
        if !status.is_success() {
            bail!("{status} {}", v["error"]["message"]);
        }
        let mut pesan_ai = v["choices"][0]["message"].clone();
        let panggilan = pesan_ai["tool_calls"].as_array().cloned().unwrap_or_default();
        if panggilan.is_empty() {
            let teks = pesan_ai["content"].as_str().unwrap_or("").trim().to_string();
            if teks.is_empty() {
                bail!("jawaban kosong: {v}");
            }
            return Ok(teks);
        }
        // Field "reasoning" dari gpt-oss tidak diterima balik sebagai input.
        if let Some(o) = pesan_ai.as_object_mut() {
            o.remove("reasoning");
        }
        isi.push(pesan_ai);
        for p in panggilan {
            let nama = p["function"]["name"].as_str().unwrap_or("");
            let args: Value = serde_json::from_str(p["function"]["arguments"].as_str().unwrap_or("{}")).unwrap_or(json!({}));
            let (hasil, a) = jalankan(app, u, nama, &args);
            aksi.extend(a);
            isi.push(json!({ "role": "tool", "tool_call_id": p["id"], "content": hasil.to_string() }));
        }
    }
    bail!("terlalu banyak langkah alat")
}

fn sistem(u: &Pengguna) -> String {
    let kini = waktu::sekarang();
    format!(
        "Kamu AI agent akademik di aplikasi Smart KTM untuk mahasiswa Universitas Pamulang. \
         Pengguna: {} (NIM {}, {}). Sekarang {} WIB, hari {}. \
         Jawab dalam bahasa Indonesia santai, singkat dan jelas (maksimal sekitar 120 kata). \
         Selalu pakai alat untuk data jadwal, tugas, presensi dan pengingat; jangan mengarang data. \
         Kalau mahasiswa minta diingatkan, pakai buat_pengingat. Kalau mahasiswa bilang tugasnya sudah selesai, cari id-nya lalu pakai tandai_tugas_selesai. \
         Untuk pertanyaan materi kuliah umum, jawab dari pengetahuanmu. Pakai daftar dengan '•' bila perlu, tanpa markdown tebal atau judul.",
        u.nama,
        u.nim,
        u.prodi,
        waktu::teks(&kini),
        waktu::nama_hari(waktu::hari_ke(&kini))
    )
}

async fn gemini(app: &App, kunci: &str, u: &Pengguna, pesan: &str, riwayat: &[Pesan], aksi: &mut Vec<String>) -> anyhow::Result<String> {
    let sistem = sistem(u);
    let mut isi: Vec<Value> = riwayat
        .iter()
        .rev()
        .take(10)
        .rev()
        .map(|p| json!({ "role": if p.peran == "model" { "model" } else { "user" }, "parts": [{ "text": p.teks }] }))
        .collect();
    isi.push(json!({ "role": "user", "parts": [{ "text": pesan }] }));

    for _ in 0..5 {
        let badan = json!({
            "systemInstruction": { "parts": [{ "text": sistem }] },
            "contents": isi,
            "tools": alat(),
            "generationConfig": app.ai.generation_config(),
        });
        let res = app.http.post(app.ai.url()).header("x-goog-api-key", kunci).json(&badan).timeout(Duration::from_secs(40)).send().await?;
        let status = res.status();
        let v: Value = res.json().await?;
        if !status.is_success() {
            bail!("{status} {}", v["error"]["message"]);
        }
        let konten = v["candidates"][0]["content"].clone();
        let parts = konten["parts"].as_array().cloned().unwrap_or_default();
        let panggilan: Vec<&Value> = parts.iter().filter(|p| p.get("functionCall").is_some()).collect();
        if panggilan.is_empty() {
            let teks: String = parts.iter().filter(|p| p["thought"] != true).filter_map(|p| p["text"].as_str()).collect();
            if teks.trim().is_empty() {
                bail!("jawaban kosong: {v}");
            }
            return Ok(teks.trim().to_string());
        }
        let mut jawaban = vec![];
        for p in panggilan {
            let f = &p["functionCall"];
            let nama = f["name"].as_str().unwrap_or("");
            let (hasil, a) = jalankan(app, u, nama, &f["args"]);
            aksi.extend(a);
            jawaban.push(json!({ "functionResponse": { "name": nama, "response": { "hasil": hasil } } }));
        }
        isi.push(konten);
        isi.push(json!({ "role": "user", "parts": jawaban }));
    }
    bail!("terlalu banyak langkah alat")
}

fn offline(app: &App, u: &Pengguna, pesan: &str) -> String {
    let p = pesan.to_lowercase();
    let ada = |kata: &[&str]| kata.iter().any(|k| p.contains(k));
    let c = app.db.lock().unwrap();
    let kini = waktu::sekarang();
    let hasil = (|| -> rusqlite::Result<String> {
        if ada(&["tugas", "deadline", "dl", "kumpul"]) {
            let t = data::tugas(&c, u.id, &kini, false)?;
            if t.is_empty() {
                return Ok("Semua tugas sudah beres 🎉".into());
            }
            let baris: Vec<String> = t.iter().map(|t| format!("• {} ({}), {}", t["judul"].as_str().unwrap(), t["matkul"].as_str().unwrap(), t["sisa"].as_str().unwrap())).collect();
            return Ok(format!("Tugas yang belum selesai:\n{}", baris.join("\n")));
        }
        if ada(&["absen", "presensi", "hadir", "bolos"]) {
            let baris: Vec<String> = data::presensi(&c, u.id, &u.dibuat)?
                .iter()
                .map(|x| format!("• {}: hadir {}/{}, absen {} (batas {})", x["matkul"].as_str().unwrap(), x["hadir"], x["total"], x["absen"], x["batas"]))
                .collect();
            return Ok(format!("Rekap presensi kamu:\n{}", baris.join("\n")));
        }
        if ada(&["jadwal", "kelas", "jam", "ruang", "kuliah", "besok", "hari ini"]) {
            let hari = pilih_hari(&p);
            let k = data::jadwal(&c, u.id, hari)?;
            let label = hari.map(waktu::nama_hari).unwrap_or("semua hari");
            if k.is_empty() {
                return Ok(format!("Tidak ada kelas ({label})."));
            }
            let baris: Vec<String> = k.iter().map(|k| format!("• {} {}–{} {} di {}", waktu::nama_hari(k.hari), k.mulai, k.selesai, k.nama, k.ruang)).collect();
            return Ok(format!("Jadwal {label}:\n{}", baris.join("\n")));
        }
        Ok("Aku lagi mode offline (kunci AI belum diatur), jadi baru bisa jawab soal jadwal, tugas, dan presensi. Coba tanya: \"tugas apa aja yang belum selesai?\"".into())
    })();
    hasil.unwrap_or_else(|e| format!("Maaf, ada galat: {e}"))
}

/// Ringkasan beranda versi AI (cache per jam). None = pakai ringkasan aturan.
pub async fn briefing(app: &App, user: i64, beranda: &Value) -> Option<String> {
    if !app.ai.aktif() {
        return None;
    }
    let kunci_cache = format!("{user}:{}", waktu::sekarang().format("%Y-%m-%d %H"));
    if let Some(s) = app.cache.lock().unwrap().get(&kunci_cache) {
        return Some(s.clone());
    }
    let prompt = format!(
        "Tulis SATU kalimat pendek (maks 16 kata) bahasa Indonesia santai untuk mahasiswa: \
         hal paling penting saat ini sesuai 'momen' (kelas berikutnya, tugas paling mendesak, atau peringatan absen; pilih maksimal dua). \
         Tanpa salam, tanpa markdown, tanpa emoji. Data: {beranda}"
    );
    let teks = match &app.ai.groq_kunci {
        Some(kunci) => {
            let badan = json!({ "model": app.ai.groq_model, "messages": [{ "role": "user", "content": prompt }], "reasoning_effort": "low" });
            let res = app.http.post(GROQ_URL).bearer_auth(kunci).json(&badan).timeout(Duration::from_secs(8)).send().await.ok()?;
            let v: Value = res.json().await.ok()?;
            v["choices"][0]["message"]["content"].as_str()?.to_string()
        }
        None => {
            let kunci = app.ai.kunci.as_ref()?;
            let badan = json!({ "contents": [{ "role": "user", "parts": [{ "text": prompt }] }], "generationConfig": app.ai.generation_config() });
            let res = app.http.post(app.ai.url()).header("x-goog-api-key", kunci).json(&badan).timeout(Duration::from_secs(8)).send().await.ok()?;
            let v: Value = res.json().await.ok()?;
            v["candidates"][0]["content"]["parts"].as_array()?.iter().filter(|p| p["thought"] != true).filter_map(|p| p["text"].as_str()).collect()
        }
    };
    let teks = teks.trim().to_string();
    if teks.is_empty() {
        return None;
    }
    app.cache.lock().unwrap().insert(kunci_cache, teks.clone());
    Some(teks)
}
