# Smart KTM

**English** · [Bahasa Indonesia](README.id.md)

**AI Agent and Blockchain on the Student ID Card (KTM).** A course project for *Manajemen Proyek Informatika* (IT Project Management) at Universitas Pamulang, Indonesia.

Tap your student ID card on a phone and your day opens up: the next class, today's schedule, assignments, and attendance, plus an AI agent that can answer questions and take actions for you. In class, the lecturer's phone becomes an attendance machine: students tap their cards, and every check-in is anchored on a smart contract as a hash, so attendance records cannot be edited quietly afterwards.

Live demo: **https://ktm.rahmateka.my.id** (tap-to-login needs Chrome on Android with NFC enabled).
Contracts on Ethereum Sepolia: attendance [`0xE3D562D1a8f82e3A31AB956EA81b504d0ac63c8d`](https://sepolia.etherscan.io/address/0xE3D562D1a8f82e3A31AB956EA81b504d0ac63c8d), certificates [`0x4cf209D67BBcd33c8a1Dc84E0428841D3F0adA9b`](https://sepolia.etherscan.io/address/0x4cf209D67BBcd33c8a1Dc84E0428841D3F0adA9b). Students do not need a crypto wallet: the server's operator wallet signs and pays for every check-in.

## Features

- **Tap-to-login with Web NFC.** UNPAM student IDs are MIFARE Classic 1K cards (the same cards used at the campus parking gate). The app only reads the card's serial number (UID) and never writes to the card, so the parking data stays intact. UIDs are stored as salted hashes, never in plain text.
- **Real schedule from MyUnpam.** Each student syncs their own timetable from my.unpam.ac.id with a one-tap bookmarklet (`/sinkron/`). The bookmarklet runs inside the student's own logged-in MyUnpam session and sends only the schedule (course, day, time, room, lecturer); passwords never reach Smart KTM. Classmates are matched by MyUnpam course and class codes, so they automatically share the same class.
- **Time-aware home screen.** Shows the next class with a countdown, the day's timeline, attendance, and a one-sentence AI summary. After midnight it switches to the new day; in the evening it previews tomorrow.
- **AI agent with tool use.** Powered by Groq (`openai/gpt-oss-120b`) with Gemini as a fallback, plus an offline keyword mode when no API key is set. Tools: view schedule, assignments, and attendance; add an assignment from chat ("MPI proposal due Friday"); mark assignments done; create reminders.
- **Lecturer attendance machine.** The lecturer opens a session, and students tap their cards on the lecturer's phone. Each check-in is sent to the `PresensiKTM` contract as `(session hash, student hash, proof hash)`.
- **Public verification.** Anyone can paste a proof code to check that an attendance record exists on-chain (`/verifikasi/`).
- **On-chain event certificates.** Organizers open a check-in for a seminar or workshop; attendees tap their KTM and a certificate is issued on the `SertifikatKTM` contract. Each certificate has a shareable public page that anyone can verify against Etherscan.
- **Lost-card mode.** A student can report their KTM as lost: the card is blocked for login, attendance, and certificates, and whoever finds and taps it sees the owner's message instead. A replacement card can be linked with NIM and PIN.
- **Digital business card.** When someone else taps your card on their phone, they only see your public profile (name, study program, GitHub, portfolio, badges), never your schedule or attendance.

## Architecture

```
 Phone (Chrome Android)                Laptop / server                         Chain
 ┌────────────────────┐   HTTPS   ┌──────────────────────────────┐   cast   ┌──────────────┐
 │ Astro PWA          │ ────────▶ │ Rust (axum) API + static web │ ───────▶ │ PresensiKTM  │
 │ Web NFC: read UID  │           │ SQLite · AI agent (Groq)     │          │ (Solidity)   │
 └────────────────────┘           └──────────────────────────────┘          └──────────────┘
          ▲ bookmarklet runs on my.unpam.ac.id and hands the schedule to /sinkron/
```

| Part | Stack |
|---|---|
| Frontend | Astro (static pages, PWA), Web NFC, Plus Jakarta Sans |
| Backend | Rust: axum, rusqlite (SQLite), argon2 (PINs), reqwest |
| AI | Groq / Gemini function calling |
| Blockchain | Solidity + Foundry; Anvil locally, any EVM testnet in production |
| Hosting | Cloudflare Tunnel |

## Running locally

Requirements: Rust, Node.js, Foundry.

```bash
cp server/.env.contoh server/.env      # add GROQ_API_KEY (optional: GEMINI_API_KEY)
./scripts/chain-lokal.sh                # start Anvil and deploy the PresensiKTM contract
./scripts/jalan.sh                      # build the web app and run the server on :8080
```

Testing on a phone over USB: `adb reverse tcp:8080 tcp:8080`, then open `localhost:8080` in Chrome on the phone (Web NFC needs a secure context, and `localhost` counts as one).

Deploying to a VPS: `./scripts/deploy-vps.sh` (first time: `--pindah` moves the data and the Cloudflare Tunnel from the laptop). Running from the laptop instead: `./scripts/deploy-tunnel.sh`.

Importing a saved MyUnpam schedule from the command line: `cd server && cargo run -- --sinkron schedule.json`.

Demo accounts created on first run: student NIM `231011400001` / PIN `123456`, lecturer NIDN `0400000001` / PIN `123456`. The first MyUnpam sync replaces the sample data.

## Tests

```bash
cd server && cargo test       # schedule logic, PIN/UID hashing, MyUnpam sync, sample data
cd contracts && forge test    # PresensiKTM: record, no double-recording, operator-only
```

## Notes and limits

- MyUnpam and Mentari have no public API. Smart KTM is a standalone prototype: it only reads data the student hands over through their own session, and it never writes anything to campus systems.
- Web NFC works only in Chrome on Android. Brave disables it; the app detects this and offers an "Open in Chrome" button. iPhone and laptop users can log in with NIM and PIN.
- Do not add any feature that writes to the card. KTM cards report `NdefFormatable`, so writing NDEF data would format the card and wipe its parking data.
