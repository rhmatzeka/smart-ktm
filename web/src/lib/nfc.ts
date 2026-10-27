// Pembaca KTM: plugin native di aplikasi Android (Capacitor), Web NFC di Chrome Android.
//
// PENTING: modul ini hanya MEMBACA nomor seri kartu. KTM UNPAM adalah MIFARE Classic
// yang "NdefFormatable": menulis NDEF akan memformat kartu dan menghapus data parkirnya.
// Jangan pernah memanggil NDEFReader.write() atau makeReadOnly() di proyek ini.

type Cap = {
  PluginHeaders?: { name: string }[];
  nativePromise: (plugin: string, method: string, opsi?: object) => Promise<any>;
  addListener: (plugin: string, event: string, cb: (data: any) => void) => { remove: () => Promise<void> };
};
const cap = (): Cap | undefined => (typeof window !== 'undefined' ? (window as any).Capacitor : undefined);

/** Berjalan di dalam aplikasi Smart KTM yang punya plugin NFC native. */
export const diAplikasi = () => !!cap()?.PluginHeaders?.some((p) => p.name === 'KtmNfc');

// Brave (dan browser Chromium lain) bisa punya NDEFReader tapi tidak pernah membaca kartu,
// jadi Brave dianggap tidak mendukung.
export const diBrave = () => typeof navigator !== 'undefined' && 'brave' in navigator;
const webNfc = () => typeof window !== 'undefined' && 'NDEFReader' in window && !diBrave();
export const nfcDidukung = () => diAplikasi() || webNfc();
export const diAndroid = () => typeof navigator !== 'undefined' && /Android/i.test(navigator.userAgent);
export const diIphone = () => typeof navigator !== 'undefined' && /iPhone|iPad|iPod/i.test(navigator.userAgent);

/** Tautan yang membuka halaman ini di Chrome Android. */
export const tautanChrome = () =>
  `intent://${location.host}${location.pathname}${location.search}#Intent;scheme=https;package=com.android.chrome;end`;

export type Berhenti = () => void;

export async function bacaKtm(onUid: (uid: string) => void, onGalat?: (pesan: string) => void): Promise<Berhenti> {
  // Kartu yang ditempel agak lama bisa terbaca berkali-kali; abaikan ulangan dalam 2,5 detik.
  let terakhir = '';
  let kapan = 0;
  const terima = (uid: string) => {
    if (!uid) return;
    const kini = Date.now();
    if (uid === terakhir && kini - kapan < 2500) return;
    terakhir = uid;
    kapan = kini;
    navigator.vibrate?.(60);
    onUid(uid);
  };

  const c = cap();
  if (diAplikasi() && c) {
    // Event yang tertahan (tap saat aplikasi di latar) diabaikan kalau sudah lebih dari 15 detik,
    // supaya tap lama tidak terputar ulang, misalnya tercatat sebagai presensi.
    const dengar = c.addListener('KtmNfc', 'ktm', (d) => {
      if (d?.waktu && Date.now() - d.waktu > 15000) return;
      terima(d?.uid ?? '');
    });
    try {
      await c.nativePromise('KtmNfc', 'mulai', {});
    } catch (e) {
      dengar.remove();
      throw e;
    }
    return () => {
      dengar.remove();
      c.nativePromise('KtmNfc', 'berhenti', {}).catch(() => {});
    };
  }

  const kendali = new AbortController();
  const pembaca = new (window as any).NDEFReader();
  await pembaca.scan({ signal: kendali.signal });
  pembaca.addEventListener('reading', (e: any) => {
    if (!e.serialNumber) {
      onGalat?.('Kartu terbaca, tapi nomor serinya kosong.');
      return;
    }
    terima(e.serialNumber);
  });
  pembaca.addEventListener('readingerror', () => onGalat?.('Kartu terdeteksi tapi belum terbaca. Tempel lebih lama di belakang HP.'));
  return () => kendali.abort();
}

export function pesanGalatNfc(e: unknown): string {
  const nama = (e as Error)?.name;
  if (nama === 'NotAllowedError') return 'Izin NFC ditolak. Buka pengaturan situs di Chrome lalu izinkan NFC.';
  if (nama === 'NotSupportedError') return 'NFC di HP ini mati atau tidak tersedia. Nyalakan NFC di pengaturan.';
  const pesan = (e as Error)?.message ?? String(e);
  return pesan.startsWith('NFC') || pesan.startsWith('HP') ? pesan : `NFC gagal dimulai: ${pesan}`;
}
