#!/usr/bin/env python3
"""Ubah logs/mentari-kelas.json (hasil Mentaribo) jadi server/data/akademik.json.

Mentari hanya punya nama, SKS, hari dan dosen. Jam dan ruang diisi dari jadwal
MyUnpam di file hasil (kolom "mulai", "selesai", "ruang"), lalu impor dengan
`cargo run -- --impor data/akademik.json`. Data peserta lain tidak ikut diambil.
"""
import json, sys, pathlib

sumber = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else pathlib.Path.home() / "Projects/mentaribo/logs/mentari-kelas.json")
tujuan = pathlib.Path(__file__).resolve().parent.parent / "server/data/akademik.json"
nim = sys.argv[2] if len(sys.argv) > 2 else "231011402890"

data = json.loads(sumber.read_text())
lama = {m["nama"]: m for m in json.loads(tujuan.read_text())["matkul"]} if tujuan.exists() else {}
urut = ["Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu", "Minggu"]

mahasiswa = None
matkul = []
for m in sorted(data["matkul"], key=lambda m: (urut.index(m["hari"]) if m["hari"] in urut else 9, m["nama"])):
    nama = m["nama"].title().replace(" Dan ", " dan ").replace(" Ii", " II")
    for p in m.get("peserta", []):
        if p.get("nim") == nim and not mahasiswa:
            mahasiswa = {"nim": nim, "nama": p["nama"].title()}
    if not mahasiswa:
        for pt in m.get("pertemuan", []):
            for q in pt.get("quiz", []):
                for n in q.get("nilai", []) + q.get("belum", []):
                    if n.get("nim") == nim:
                        mahasiswa = {"nim": nim, "nama": n["nama"].title()}
    pertemuan = [{"ke": p["pertemuan"], "topik": p.get("topik", "")} for p in m.get("pertemuan", [])]
    sebelum = lama.get(nama, {})
    matkul.append({
        "nama": nama, "sks": m["sks"], "hari": m["hari"], "dosen": m["dosen"],
        "mulai": sebelum.get("mulai", ""), "selesai": sebelum.get("selesai", ""), "ruang": sebelum.get("ruang", ""),
        "pertemuan": pertemuan,
    })

hasil = {"sumber": f"Mentari UNPAM ({data.get('waktu', '')[:10]})", "mahasiswa": mahasiswa or {"nim": nim, "nama": ""},
         "prodi": "Teknik Informatika", "matkul": matkul}
tujuan.write_text(json.dumps(hasil, ensure_ascii=False, indent=1))
kosong = [m["nama"] for m in matkul if not m["mulai"]]
print(f"{len(matkul)} matkul ditulis ke {tujuan}")
if kosong:
    print("Jam/ruang belum diisi:", ", ".join(kosong))
