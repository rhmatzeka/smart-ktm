// Pembaca KTM lewat Web NFC (Chrome Android).
//
// PENTING: modul ini hanya MEMBACA nomor seri kartu. KTM UNPAM adalah MIFARE Classic
// yang "NdefFormatable": menulis NDEF akan memformat kartu dan menghapus data parkirnya.
// Jangan pernah memanggil NDEFReader.write() atau makeReadOnly() di proyek ini.

// Brave (dan browser Chromium lain) bisa punya NDEFReader tapi tidak pernah membaca kartu,
// jadi Brave dianggap tidak mendukung.
export const diBrave = () => typeof navigator !== 'undefined' && 'brave' in navigator;
export const nfcDidukung = () => typeof window !== 'undefined' && 'NDEFReader' in window && !diBrave();
export const diAndroid = () => typeof navigator !== 'undefined' && /Android/i.test(navigator.userAgent);

/** Tautan yang membuka halaman ini di Chrome Android. */
export const tautanChrome = () =>
  `intent://${location.host}${location.pathname}${location.search}#Intent;scheme=https;package=com.android.chrome;end`;

export type Berhenti = () => void;

export async function bacaKtm(onUid: (uid: string) => void, onGalat?: (pesan: string) => void): Promise<Berhenti> {
  const kendali = new AbortController();
  const pembaca = new (window as any).NDEFReader();
  await pembaca.scan({ signal: kendali.signal });

  // Kartu yang ditempel agak lama bisa terbaca berkali-kali; abaikan ulangan dalam 2,5 detik.
  let terakhir = '';
  let kapan = 0;
  pembaca.addEventListener('reading', (e: any) => {
    const uid: string = e.serialNumber || '';
    if (!uid) {
      onGalat?.('Kartu terbaca, tapi nomor serinya kosong.');
      return;
    }
    const kini = Date.now();
    if (uid === terakhir && kini - kapan < 2500) return;
    terakhir = uid;
    kapan = kini;
    navigator.vibrate?.(60);
    onUid(uid);
  });
  pembaca.addEventListener('readingerror', () => onGalat?.('Kartu terdeteksi tapi belum terbaca. Tempel lebih lama di belakang HP.'));
  return () => kendali.abort();
}

export function pesanGalatNfc(e: unknown): string {
  const nama = (e as Error)?.name;
  if (nama === 'NotAllowedError') return 'Izin NFC ditolak. Buka pengaturan situs di Chrome lalu izinkan NFC.';
  if (nama === 'NotSupportedError') return 'NFC di HP ini mati atau tidak tersedia. Nyalakan NFC di pengaturan.';
  return `NFC gagal dimulai: ${(e as Error)?.message ?? e}`;
}
