// Notifikasi HP (aplikasi Android saja): kelas 15 menit sebelum mulai, tenggat tugas, dan pengingat
// dari Asisten KTM. Jadwalnya dihitung server (/api/notifikasi); di sini hanya dipasang sebagai
// notifikasi lokal lewat plugin LocalNotifications, jadi tetap muncul walau aplikasi tertutup.
import { api } from './api';

type Cap = {
  PluginHeaders?: { name: string }[];
  nativePromise: (plugin: string, method: string, opsi?: object) => Promise<any>;
};
const cap = (): Cap | undefined => (typeof window !== 'undefined' ? (window as any).Capacitor : undefined);
const P = 'LocalNotifications';

export const bisaNotif = () => !!cap()?.PluginHeaders?.some((p) => p.name === P);

/** Status izin: 'granted' | 'denied' | 'prompt' | 'tidak-ada' (bukan di aplikasi). */
export async function izinNotif(): Promise<string> {
  const c = cap();
  if (!c || !bisaNotif()) return 'tidak-ada';
  try {
    return (await c.nativePromise(P, 'checkPermissions', {})).display ?? 'prompt';
  } catch {
    return 'tidak-ada';
  }
}

/** Ambil jadwal dari server lalu pasang ulang semua notifikasi. `minta` = boleh memunculkan dialog izin. */
export async function jadwalkanNotif(minta = true): Promise<number> {
  const c = cap();
  if (!c || !bisaNotif()) return 0;
  try {
    let izin = await izinNotif();
    if (izin !== 'granted') {
      if (!minta || izin === 'denied') return 0;
      izin = (await c.nativePromise(P, 'requestPermissions', {})).display;
      if (izin !== 'granted') return 0;
    }
    const { notifikasi } = await api<{ notifikasi: { id: number; waktu: string; judul: string; isi: string }[] }>('/notifikasi');
    const lama = await c.nativePromise(P, 'getPending', {});
    if (lama.notifications?.length) {
      await c.nativePromise(P, 'cancel', { notifications: lama.notifications.map((n: { id: number }) => ({ id: n.id })) });
    }
    if (notifikasi.length) {
      await c.nativePromise(P, 'schedule', {
        notifications: notifikasi.map((n) => ({
          id: n.id,
          title: n.judul,
          body: n.isi,
          schedule: { at: new Date(n.waktu).toISOString(), allowWhileIdle: true },
        })),
      });
    }
    return notifikasi.length;
  } catch {
    return 0; // notifikasi hanya pelengkap; jangan ganggu halaman kalau gagal
  }
}
