# Smart KTM

[English](README.md) · **Bahasa Indonesia**

**AI Agent dan Blockchain pada KTM.** Tugas mata kuliah Manajemen Proyek Informatika, Universitas Pamulang.

Tap KTM (kartu MIFARE Classic yang juga dipakai untuk parkir) ke HP, lalu jadwal, tugas, dan kehadiran langsung terbuka, lengkap dengan AI agent yang bisa menjawab dan bertindak (membuat pengingat, menandai tugas selesai). Presensi kelas dilakukan dengan tap KTM di HP dosen, dan setiap kehadiran dicatat sebagai hash di smart contract sehingga tidak bisa diubah diam-diam.

## Fitur

- **Tap KTM lewat Web NFC** (Chrome Android): aplikasi hanya membaca nomor seri (UID) kartu dan tidak pernah menulis ke kartu, jadi data parkir aman. UID disimpan sebagai hash bergaram.
- **Jadwal asli dari MyUnpam**: tiap mahasiswa menyinkronkan jadwalnya sendiri lewat bookmark di my.unpam.ac.id (halaman `/sinkron/`); teman sekelas otomatis masuk kelas yang sama.
- **Beranda menyesuaikan waktu**: kelas berikutnya, jadwal hari ini, tugas, dan kehadiran.
- **AI agent** (Groq `gpt-oss-120b`, cadangan Gemini) dengan function calling: lihat jadwal, tugas, presensi, buat pengingat, tandai tugas selesai.
- **Mesin absen dosen**: HP dosen menerima tap KTM; kehadiran dikirim ke kontrak `PresensiKTM`.
- **Verifikasi publik**: siapa pun bisa mengecek kode bukti kehadiran terhadap blockchain.
- **Kartu nama digital**: KTM ditap di HP orang lain hanya menampilkan profil publik.

## Teknologi

| Bagian | Teknologi |
|---|---|
| Frontend | Astro (halaman statis + PWA), Web NFC |
| Backend | Rust (axum, SQLite/rusqlite) |
| AI | Groq / Gemini, function calling |
| Blockchain | Solidity + Foundry (Anvil lokal atau testnet) |

## Menjalankan

```bash
cp server/.env.contoh server/.env      # isi GROQ_API_KEY
./scripts/chain-lokal.sh                # Anvil + deploy kontrak
./scripts/jalan.sh                      # build web, jalankan server di :8080
```

Tes di HP lewat kabel: `adb reverse tcp:8080 tcp:8080`, lalu buka `localhost:8080` di Chrome HP.
Online lewat Cloudflare Tunnel: `./scripts/deploy-tunnel.sh`.

Tes: `cd server && cargo test`, `cd contracts && forge test`.

## Catatan

Aplikasi ini prototipe yang berdiri sendiri: MyUnpam dan Mentari tidak menyediakan API publik, dan aplikasi tidak menulis apa pun ke sistem kampus.
