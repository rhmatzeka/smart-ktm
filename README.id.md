# Smart KTM

[English](README.md) · **Bahasa Indonesia**

**AI Agent dan Blockchain pada KTM.** Tugas mata kuliah Manajemen Proyek Informatika, Universitas Pamulang.

Tap KTM ke HP dan harimu langsung terbuka: kelas berikutnya, jadwal seminggu, tugas, dan kehadiran, ditambah AI agent yang menjawab sekaligus bertindak. Di kelas, HP dosen jadi mesin absen: mahasiswa tap KTM, fotonya muncul untuk dicocokkan, dan setiap kehadiran dicatat di smart contract sehingga tidak bisa diubah diam-diam.

- **Aplikasi web:** https://ktm.rahmateka.my.id
- **Aplikasi Android:** https://ktm.rahmateka.my.id/unduh/SmartKTM.apk?v=2
- **Kontrak di Ethereum Sepolia:** presensi [`PresensiKTM`](https://sepolia.etherscan.io/address/0xE3D562D1a8f82e3A31AB956EA81b504d0ac63c8d), sertifikat [`SertifikatKTM`](https://sepolia.etherscan.io/address/0x4cf209D67BBcd33c8a1Dc84E0428841D3F0adA9b)

## Cara memasang

| Perangkat | Caranya | Tap KTM |
|---|---|---|
| **Android (disarankan)** | Unduh [APK](https://ktm.rahmateka.my.id/unduh/SmartKTM.apk?v=2) (±14 MB), buka file-nya, izinkan "instal dari sumber ini", lalu pilih *Tetap instal* kalau Play Protect memberi peringatan (aplikasinya belum ada di Play Store). | ✅ NFC native. Menempel KTM membuka aplikasi walau sedang tertutup. |
| Android, Chrome | Buka https://ktm.rahmateka.my.id di **Chrome** (bukan Brave). Opsional: menu → *Tambahkan ke layar utama*. | ✅ Web NFC, selama halaman terbuka. |
| iPhone | Buka situsnya di Safari → *Bagikan* → *Tambahkan ke Layar Utama*. Masuk pakai NIM dan PIN. | ❌ NFC iPhone tidak bisa membaca kartu MIFARE Classic. |
| Laptop | Buka situsnya di browser apa saja. Masuk pakai NIM dan PIN. | ❌ |

Aplikasi Android memuat situs yang live, jadi fitur baru langsung muncul tanpa pasang ulang.

## Cara pakai

**Mahasiswa**
1. Tap KTM. Pertama kali, isi NIM dan PIN 6 angka untuk menautkan kartu. Setelah itu, tap KTM langsung masuk di HP tersebut.
2. Sinkronkan jadwal: *Profil → Sinkron jadwal MyUnpam*. Simpan bookmark sekali, login di my.unpam.ac.id, lalu buka bookmark itu. Jadwal, presensi per pertemuan, dan foto ikut terbawa; kata sandi tidak pernah keluar dari MyUnpam.
3. Pakai beranda (kelas berikutnya, hari ini dan seminggu, pertemuan yang terlewat), *Tanya* (AI agent), *Kenalan* (tap KTM teman untuk melihat kartu namanya), dan *Profil* (sertifikat, PIN, KTM hilang).

**Dosen**: masuk dengan NIDN dan PIN, pilih kelas, tekan *Mulai terima tap KTM*. Setiap tap mencatat kehadiran dan mengirim bukti ke `PresensiKTM`.

**Panitia acara** (akun dosen): *Acara & sertifikat on-chain* → buat acara → mulai check-in. Setiap peserta yang tap mendapat sertifikat di `SertifikatKTM` dengan halaman publik yang bisa dibagikan.

**KTM hilang**: *Profil → Laporkan KTM hilang*. Kartu terblokir untuk masuk, presensi, dan sertifikat; penemu yang men-tap kartu melihat pesanmu. KTM pengganti ditautkan dengan tap lalu isi NIM dan PIN.

## Fitur

- **Masuk dengan tap KTM (NFC).** KTM UNPAM adalah MIFARE Classic 1K (kartu yang sama untuk parkir). Aplikasi hanya membaca nomor seri (UID) dan tidak pernah menulis ke kartu, jadi data parkir aman. UID disimpan sebagai hash bergaram.
- **Data asli dari MyUnpam dan Mentari, satu tombol.** Di aplikasi Android, *Sinkron sekarang* membuka MyUnpam lalu Mentari di dalam aplikasi; mahasiswa login seperti biasa dan aplikasi mengambil jadwal (matkul, hari, jam, ruang, dosen), presensi tiap pertemuan, foto, serta tugas dan aktivitas Mentari yang belum dikerjakan. Kata sandi hanya diketik di halaman kampus. Di browser, MyUnpam disinkron lewat bookmark.
- **Beranda yang menyesuaikan waktu**: kelas berikutnya dengan hitung mundur, jadwal seminggu, pertemuan terlewat, ringkasan AI satu kalimat.
- **Notifikasi HP** (Android): 15 menit sebelum kelas, H-1 dan 3 jam sebelum tenggat tugas, serta pengingat yang dibuat AI agent.
- **AI agent dengan alat**: Groq (`openai/gpt-oss-120b`), cadangan Gemini, mode offline tanpa kunci. Bisa melihat jadwal, tugas, presensi, mencatat tugas dari obrolan, menandai tugas selesai, dan membuat pengingat.
- **Presensi on-chain**: HP dosen sebagai pembaca, foto mahasiswa muncul setiap tap, hash kehadiran dikirim ke `PresensiKTM`. Mahasiswa tidak perlu wallet: wallet operator di server yang menandatangani dan membayar gas. Siapa pun bisa memverifikasi kode bukti di Etherscan, dan kode kedua kontrak terverifikasi di [Sourcify](https://repo.sourcify.dev/11155111/0xE3D562D1a8f82e3A31AB956EA81b504d0ac63c8d) (exact match).
- **Rekap kehadiran untuk dosen** per kelas (mahasiswa x pertemuan, peringatan batas absen) dengan **unduhan Excel** (lengkap dengan rumus).
- **Jadwal ke kalender**: semua pertemuan (tanggal asli MyUnpam) dan tenggat tugas sebagai berkas `.ics` atau link langganan Google Calendar pribadi.
- **Akun dosen sungguhan** lewat kode undangan dari dosen terdaftar (sekali pakai, 7 hari); kelas terhubung otomatis kalau namanya sama dengan dosen pengampu di MyUnpam.
- **Halaman privasi** ([/privasi/](https://ktm.rahmateka.my.id/privasi/)) dan **hapus akun**: akun dan semua data pribadi dihapus dari server.
- **Sertifikat acara on-chain** dengan halaman verifikasi publik.
- **Mode KTM hilang** dan **kartu nama digital**.
- **Aplikasi Android** (Capacitor) dengan plugin NFC native yang hanya membaca; **PWA** untuk HP lain.

## Keamanan

- **NFC hanya membaca.** KTM bertipe `NdefFormatable`; menulis akan menghapus data parkir. Tidak ada jalur tulis di kode web maupun native.
- **HP tepercaya.** Tap KTM tanpa PIN hanya berlaku di HP yang pernah masuk dengan PIN sekali. Sekadar tahu UID seseorang tidak cukup. Pemilik bisa mewajibkan PIN di setiap tap.
- **Batas percobaan.** 5 kali PIN salah mengunci akun 15 menit; cek kartu, masuk, daftar, tap, chat, dan sinkron dibatasi per IP atau per akun.
- **Sesi kedaluwarsa** setelah 30 hari. Mengganti PIN mencabut HP tepercaya lainnya.
- **Riwayat pemakaian KTM.** Setiap pemakaian KTM (masuk, presensi, check-in acara, ditempel orang) tercatat untuk pemiliknya. Tombol *Bukan saya* memblokir kartu dan menandai presensi itu ke dosen.
- **Deteksi kartu ganda.** KTM yang sama dipakai presensi di kelas lain dalam 20 menit ditolak dan dicatat, karena hampir pasti kartu salinan.
- **Privasi on-chain.** Hanya hash yang dikirim ke blockchain: tanpa nama, NIM, atau UID.

Keterbatasan yang diketahui: UID MIFARE Classic bisa dibaca dan disalin oleh siapa pun yang sempat memegang kartu, jadi kartu salinan di HP tepercaya, atau akun dosen yang tidak jujur, masih bisa memalsukan kehadiran. Pengaman saat ini adalah foto yang muncul di setiap tap, deteksi kartu ganda, dan riwayat pemakaian untuk pemilik; solusi tuntas butuh kartu dengan autentikasi kriptografis (mis. DESFire). MyUnpam tidak punya API publik, dan Smart KTM tidak pernah menulis ke sistem kampus.

## Arsitektur

```
 HP (aplikasi Android / Chrome)        VPS                                      Ethereum Sepolia
 ┌───────────────────────────┐ HTTPS ┌──────────────────────────────┐  cast   ┌───────────────┐
 │ Astro PWA + NFC native    │──────▶│ API Rust (axum) + web statis │───────▶ │ PresensiKTM   │
 │ hanya membaca UID KTM     │       │ SQLite · AI agent (Groq)     │         │ SertifikatKTM │
 └───────────────────────────┘       └──────────────────────────────┘         └───────────────┘
        ▲ bookmark di my.unpam.ac.id menyerahkan jadwal mahasiswa ke /sinkron/
```

| Bagian | Teknologi |
|---|---|
| Web | Astro (halaman statis, PWA), Web NFC |
| Mobile | Capacitor 8 (Android), plugin NFC Java buatan sendiri, APK dibuat GitHub Actions |
| Backend | Rust: axum, rusqlite, argon2, reqwest |
| AI | Groq / Gemini function calling |
| Blockchain | Solidity + Foundry, Ethereum Sepolia |
| Hosting | VPS + Cloudflare Tunnel |

```
contracts/   kontrak Solidity dan tes Foundry
server/      API Rust (src/api.rs, ai.rs, sinkron.rs, chain.rs, batas.rs …)
web/         halaman Astro (src/pages) dan UI bersama
mobile/      proyek Android Capacitor dan plugin NFC native
scripts/     jalankan lokal, deploy ke VPS, deploy kontrak
```

## Menjalankan sendiri

Butuh: Rust, Node.js, Foundry.

```bash
cp server/.env.contoh server/.env      # isi GROQ_API_KEY (opsional: GEMINI_API_KEY)
./scripts/chain-lokal.sh                # blockchain lokal Anvil + deploy PresensiKTM
./scripts/jalan.sh                      # build web lalu jalankan server di :8080
```

Saat pertama jalan, database diisi data contoh dan dua akun demo (mahasiswa NIM `231011400001`, dosen NIDN `0400000001`, PIN dari `DEMO_PIN` / `DOSEN_PIN` di `.env`). Ganti PIN ini di server yang bisa diakses dari internet.

Tes di HP lewat kabel: `adb reverse tcp:8080 tcp:8080`, lalu buka `localhost:8080` di Chrome HP.

| Tugas | Perintah |
|---|---|
| Deploy ke VPS | `./scripts/deploy-vps.sh` (pertama kali: `--pindah`) |
| Deploy kontrak ke Sepolia | `./scripts/deploy-sepolia.sh` |
| Impor jadwal MyUnpam yang tersimpan | `cd server && cargo run -- --sinkron jadwal.json` |
| Build APK Android | push perubahan di `mobile/` (GitHub Actions menerbitkan rilis `apk-terbaru`) |

## Tes

```bash
cd server && cargo test       # 27 tes: logika jadwal, hash, sinkron MyUnpam & Mentari, notifikasi, KTM hilang, HP tepercaya, kartu ganda, rekap & Excel, kalender, link bertanda tangan, undangan dosen, hapus akun, batas percobaan
cd contracts && forge test    # 6 tes: PresensiKTM dan SertifikatKTM
```

## Manajemen proyek

Kelompok 11 (07TPLP003): Rahmat Eka Satria (PM, backend, blockchain, AI) dan Andika Agung Triadha (frontend, mobile, QA).

- Papan tugas dan roadmap: [GitHub Project #3](https://github.com/users/rhmatzeka/projects/3)
- Rencana proyek, anggaran (RAB), dan jadwal (Gantt): [`docs/Rencana_Anggaran_Jadwal_Smart_KTM.xlsx`](docs/Rencana_Anggaran_Jadwal_Smart_KTM.xlsx)
