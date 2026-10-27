# Smart KTM

**English** · [Bahasa Indonesia](README.id.md)

**AI Agent and Blockchain on the Student ID Card (KTM).** A course project for *Manajemen Proyek Informatika* (IT Project Management) at Universitas Pamulang, Indonesia.

Tap your student ID card on a phone and your day opens up: the next class, the week's schedule, assignments, and attendance, plus an AI agent that answers questions and takes actions for you. In class, the lecturer's phone becomes an attendance machine: students tap their cards, their photo appears for the lecturer to check, and every check-in is anchored on a smart contract, so attendance cannot be edited quietly afterwards.

- **Live app:** https://ktm.rahmateka.my.id
- **Android app:** https://ktm.rahmateka.my.id/unduh/SmartKTM.apk?v=2
- **Contracts on Ethereum Sepolia:** attendance [`PresensiKTM`](https://sepolia.etherscan.io/address/0xE3D562D1a8f82e3A31AB956EA81b504d0ac63c8d), certificates [`SertifikatKTM`](https://sepolia.etherscan.io/address/0x4cf209D67BBcd33c8a1Dc84E0428841D3F0adA9b)

## Get the app

| Device | How | Tap KTM |
|---|---|---|
| **Android (recommended)** | Download the [APK](https://ktm.rahmateka.my.id/unduh/SmartKTM.apk?v=2) (≈14 MB), open it, allow "install from this source", and choose *Install anyway* if Play Protect warns (the app is not on the Play Store yet). | ✅ Native NFC. Holding your KTM to the phone opens the app even when it is closed. |
| Android, Chrome | Open https://ktm.rahmateka.my.id in **Chrome** (not Brave). Optional: menu → *Add to Home screen*. | ✅ Web NFC, while the page is open. |
| iPhone | Open the site in Safari → *Share* → *Add to Home Screen*. Sign in with NIM and PIN. | ❌ iPhone NFC cannot read MIFARE Classic cards. |
| Laptop | Open the site in any browser. Sign in with NIM and PIN. | ❌ |

The Android app loads the live site, so new features arrive without reinstalling.

## How to use

**Students**
1. Tap your KTM. The first time, enter your NIM and a 6-digit PIN to link the card. After that, a tap signs you in on that phone.
2. Sync your schedule: *Profile → Sync MyUnpam schedule*. Save the bookmark once, sign in at my.unpam.ac.id, then open the bookmark. Your timetable, per-meeting attendance, and photo come over; your password never leaves MyUnpam.
3. Use the home screen (next class, today and this week, missed meetings), *Ask* (the AI agent), *Kenalan* (tap a friend's KTM to see their public card), and *Profile* (certificates, PIN, lost card).

**Lecturers**: sign in with NIDN and PIN, pick a class, and press *Start accepting KTM taps*. Each tap records attendance and sends a proof to `PresensiKTM`.

**Event organizers** (lecturer account): *Events & on-chain certificates* → create an event → start check-in. Each attendee tap issues a certificate on `SertifikatKTM` with a shareable public page.

**Lost card**: *Profile → Report KTM lost*. The card is blocked for sign-in, attendance, and certificates; whoever finds and taps it sees your message. Link a replacement card by tapping it and entering your NIM and PIN.

## Features

- **Tap-to-sign-in with NFC.** UNPAM KTMs are MIFARE Classic 1K cards (the same cards used at the parking gate). The app only reads the card's serial number (UID) and never writes to the card, so the parking data stays intact. UIDs are stored as salted hashes.
- **Real data from MyUnpam.** Schedule (course, day, time, room, lecturer), attendance status for every meeting, and profile photo, synced per student with a bookmarklet that runs in the student's own MyUnpam session.
- **Time-aware home screen.** Next class with a countdown, a weekly timeline, missed meetings, and a one-sentence AI summary.
- **AI agent with tool use.** Groq (`openai/gpt-oss-120b`), Gemini as fallback, offline keyword mode without keys. Tools: schedule, assignments, attendance, add assignment from chat ("MPI proposal due Friday"), mark assignments done, reminders.
- **Attendance on-chain.** The lecturer's phone is the reader; the student's photo appears on each tap; a hash of every check-in goes to `PresensiKTM`. Students need no crypto wallet: the server's operator wallet signs and pays gas. Anyone can verify a proof code against Etherscan.
- **On-chain event certificates** with a public verification page.
- **Lost-card mode** and **digital business card**.
- **Android app** (Capacitor) with a native read-only NFC plugin; **PWA** for every other phone.

## Security

- **Read-only NFC.** KTMs report `NdefFormatable`; writing would wipe the parking data. No write path exists in the web or native code.
- **Trusted phones.** A KTM tap signs in without a PIN only on a phone that has signed in with the PIN once. Knowing someone's UID is not enough. Owners can require the PIN on every tap.
- **Rate limits and lockout.** 5 wrong PINs lock the account for 15 minutes; card checks, sign-in, registration, taps, chat, and sync are rate-limited per IP or account.
- **Sessions expire** after 30 days. Changing the PIN revokes other trusted phones.
- **Privacy on-chain.** Only hashes go to the blockchain: no names, NIMs, or UIDs.

Known limits: a MIFARE Classic UID can be read and cloned by anyone holding the card for a moment, so a cloned card on a trusted phone, or a dishonest lecturer account, could still fake attendance. The photo shown on every tap is the current safeguard; a full fix needs cards with cryptographic authentication (e.g. DESFire). MyUnpam has no public API, and Smart KTM never writes to campus systems.

## Architecture

```
 Phone (Android app / Chrome)          VPS                                      Ethereum Sepolia
 ┌───────────────────────────┐ HTTPS ┌──────────────────────────────┐  cast   ┌───────────────┐
 │ Astro PWA + native NFC    │──────▶│ Rust (axum) API + static web │───────▶ │ PresensiKTM   │
 │ reads KTM UID only        │       │ SQLite · AI agent (Groq)     │         │ SertifikatKTM │
 └───────────────────────────┘       └──────────────────────────────┘         └───────────────┘
        ▲ bookmarklet on my.unpam.ac.id hands the student's schedule to /sinkron/
```

| Part | Stack |
|---|---|
| Web | Astro (static pages, PWA), Web NFC |
| Mobile | Capacitor 8 (Android), custom Java NFC plugin, APK built by GitHub Actions |
| Backend | Rust: axum, rusqlite, argon2, reqwest |
| AI | Groq / Gemini function calling |
| Blockchain | Solidity + Foundry, Ethereum Sepolia |
| Hosting | VPS + Cloudflare Tunnel |

```
contracts/   Solidity contracts and Foundry tests
server/      Rust API (src/api.rs, ai.rs, sinkron.rs, chain.rs, batas.rs …)
web/         Astro pages (src/pages) and shared UI
mobile/      Capacitor Android project and native NFC plugin
scripts/     run locally, deploy to VPS, deploy contracts
```

## Running locally

Requirements: Rust, Node.js, Foundry.

```bash
cp server/.env.contoh server/.env      # add GROQ_API_KEY (optional: GEMINI_API_KEY)
./scripts/chain-lokal.sh                # local Anvil chain + deploy PresensiKTM
./scripts/jalan.sh                      # build the web app and run the server on :8080
```

On first run the database is filled with sample data and two demo accounts (student NIM `231011400001`, lecturer NIDN `0400000001`, PIN from `DEMO_PIN` / `DOSEN_PIN` in `.env`). Change these PINs on any server that is reachable from the internet.

Testing on a phone over USB: `adb reverse tcp:8080 tcp:8080`, then open `localhost:8080` in Chrome on the phone.

| Task | Command |
|---|---|
| Deploy to the VPS | `./scripts/deploy-vps.sh` (first time: `--pindah`) |
| Deploy contracts to Sepolia | `./scripts/deploy-sepolia.sh` |
| Import a saved MyUnpam schedule | `cd server && cargo run -- --sinkron schedule.json` |
| Build the Android APK | push changes under `mobile/` (GitHub Actions publishes release `apk-terbaru`) |

## Tests

```bash
cd server && cargo test       # 13 tests: schedule logic, hashing, MyUnpam sync, lost card, trusted phones, rate limits
cd contracts && forge test    # 6 tests: PresensiKTM and SertifikatKTM
```

## Project management

Group 11 (07TPLP003): Rahmat Eka Satria (PM, backend, blockchain, AI) and Andika Agung Triadha (frontend, mobile, QA).

- Task board and roadmap: [GitHub Project #3](https://github.com/users/rhmatzeka/projects/3)
- Plan, budget (RAB) and schedule (Gantt): [`docs/Rencana_Anggaran_Jadwal_Smart_KTM.xlsx`](docs/Rencana_Anggaran_Jadwal_Smart_KTM.xlsx)
