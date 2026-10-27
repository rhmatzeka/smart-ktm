package id.my.rahmateka.ktm;

import android.content.Intent;
import android.nfc.NfcAdapter;
import android.nfc.Tag;
import android.os.Build;

import com.getcapacitor.JSObject;
import com.getcapacitor.Plugin;
import com.getcapacitor.PluginCall;
import com.getcapacitor.PluginMethod;
import com.getcapacitor.annotation.CapacitorPlugin;

/**
 * Membaca nomor seri (UID) KTM lewat NFC native dan mengirimnya ke halaman web sebagai event "ktm".
 *
 * PENTING: plugin ini hanya MEMBACA UID. KTM UNPAM adalah MIFARE Classic yang "NdefFormatable":
 * menulis NDEF akan memformat kartu dan menghapus data parkirnya. Jangan menambah operasi tulis.
 *
 * Dua jalur:
 *  - aplikasi terbuka dan halaman memanggil mulai(): reader mode, tap langsung dikirim;
 *  - aplikasi tertutup: intent TECH_DISCOVERED (lihat res/xml/nfc_tech_filter.xml) membuka aplikasi,
 *    UID disimpan sebagai event yang menunggu sampai halaman memasang listener.
 */
@CapacitorPlugin(name = "KtmNfc")
public class KtmNfcPlugin extends Plugin {
    private static final int FLAG_BACA =
        NfcAdapter.FLAG_READER_NFC_A | NfcAdapter.FLAG_READER_NFC_B | NfcAdapter.FLAG_READER_SKIP_NDEF_CHECK;

    private NfcAdapter adapter;
    private boolean membaca = false;

    @Override
    public void load() {
        adapter = NfcAdapter.getDefaultAdapter(getContext());
        kirimDariIntent(getActivity().getIntent());
    }

    @PluginMethod
    public void status(PluginCall call) {
        JSObject r = new JSObject();
        r.put("ada", adapter != null);
        r.put("aktif", adapter != null && adapter.isEnabled());
        call.resolve(r);
    }

    @PluginMethod
    public void mulai(PluginCall call) {
        if (adapter == null) {
            call.reject("HP ini tidak punya NFC");
            return;
        }
        if (!adapter.isEnabled()) {
            call.reject("NFC mati. Nyalakan NFC di pengaturan HP.");
            return;
        }
        getActivity().runOnUiThread(() -> {
            nyalakanReader();
            membaca = true;
            call.resolve();
        });
    }

    @PluginMethod
    public void berhenti(PluginCall call) {
        getActivity().runOnUiThread(() -> {
            membaca = false;
            if (adapter != null) adapter.disableReaderMode(getActivity());
            call.resolve();
        });
    }

    @Override
    protected void handleOnNewIntent(Intent intent) {
        super.handleOnNewIntent(intent);
        kirimDariIntent(intent);
    }

    @Override
    protected void handleOnResume() {
        super.handleOnResume();
        // Reader mode hanya berlaku selama activity tampil; pasang lagi setelah kembali.
        if (membaca) nyalakanReader();
    }

    @Override
    protected void handleOnPause() {
        super.handleOnPause();
        if (adapter != null) adapter.disableReaderMode(getActivity());
    }

    private void nyalakanReader() {
        if (adapter != null && adapter.isEnabled()) {
            adapter.enableReaderMode(getActivity(), this::kirim, FLAG_BACA, null);
        }
    }

    @SuppressWarnings("deprecation")
    private void kirimDariIntent(Intent intent) {
        if (intent == null) return;
        String aksi = intent.getAction();
        if (!NfcAdapter.ACTION_TECH_DISCOVERED.equals(aksi) && !NfcAdapter.ACTION_TAG_DISCOVERED.equals(aksi)) return;
        Tag tag = Build.VERSION.SDK_INT >= 33
            ? intent.getParcelableExtra(NfcAdapter.EXTRA_TAG, Tag.class)
            : intent.getParcelableExtra(NfcAdapter.EXTRA_TAG);
        if (tag != null) kirim(tag);
        // Jangan diproses ulang saat activity dibuat ulang.
        intent.setAction(Intent.ACTION_MAIN);
    }

    private void kirim(Tag tag) {
        byte[] id = tag.getId();
        if (id == null || id.length == 0) return;
        StringBuilder uid = new StringBuilder();
        for (int i = 0; i < id.length; i++) {
            if (i > 0) uid.append(':');
            uid.append(String.format("%02x", id[i] & 0xff));
        }
        JSObject data = new JSObject();
        data.put("uid", uid.toString());
        data.put("waktu", System.currentTimeMillis());
        // retainUntilConsumed: event dari tap yang membuka aplikasi tetap sampai halaman siap mendengar.
        notifyListeners("ktm", data, true);
    }
}
