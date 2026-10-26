const KUNCI = 'smartktm.token';

export const sesi = {
  ambil(): string | null {
    try { return localStorage.getItem(KUNCI); } catch { return null; }
  },
  simpan(t: string) {
    try { localStorage.setItem(KUNCI, t); } catch { /* mode privat: sesi hanya di memori halaman */ }
  },
  hapus() {
    try { localStorage.removeItem(KUNCI); } catch { /* abaikan */ }
  },
};

export class GalatApi extends Error {
  constructor(message: string, public status: number) { super(message); }
}

export async function api<T = any>(path: string, body?: unknown): Promise<T> {
  const t = sesi.ambil();
  const res = await fetch(`/api${path}`, {
    method: body === undefined ? 'GET' : 'POST',
    headers: { 'content-type': 'application/json', ...(t ? { authorization: `Bearer ${t}` } : {}) },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new GalatApi(data.galat || `Server membalas ${res.status}`, res.status);
  return data as T;
}

export const tujuan = (peran: string) => (peran === 'dosen' ? '/dosen/' : '/beranda/');

/** Halaman yang butuh login: balik ke depan kalau sesi tidak ada atau habis. */
export function tanganiGalat(e: unknown, tampil: (pesan: string) => void) {
  if (e instanceof GalatApi && e.status === 401) {
    sesi.hapus();
    location.href = '/';
    return;
  }
  tampil((e as Error).message);
}

export const esc = (s: unknown) =>
  String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);

export const jam = (waktu: string) => waktu.slice(11, 16).replace(':', '.');

export const pendek = (hash: string) => `${hash.slice(0, 8)}…${hash.slice(-6)}`;
