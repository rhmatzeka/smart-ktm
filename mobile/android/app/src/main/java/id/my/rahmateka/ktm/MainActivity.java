package id.my.rahmateka.ktm;

import android.app.DownloadManager;
import android.net.Uri;
import android.os.Bundle;
import android.os.Environment;
import android.webkit.URLUtil;
import android.widget.Toast;

import com.getcapacitor.BridgeActivity;

public class MainActivity extends BridgeActivity {
    @Override
    public void onCreate(Bundle savedInstanceState) {
        registerPlugin(KtmNfcPlugin.class);
        registerPlugin(MyUnpamPlugin.class);
        super.onCreate(savedInstanceState);
        // WebView tidak mengunduh berkas sendiri: rekap Excel dan kalender .ics diteruskan ke DownloadManager
        // (masuk folder Download + notifikasi). Link unduhan Smart KTM sudah bertanda tangan, jadi tanpa cookie.
        getBridge().getWebView().setDownloadListener((url, agen, disposisi, mime, panjang) -> {
            String nama = URLUtil.guessFileName(url, disposisi, mime);
            DownloadManager.Request r = new DownloadManager.Request(Uri.parse(url))
                .setMimeType(mime)
                .addRequestHeader("User-Agent", agen)
                .setTitle(nama)
                .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
                .setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, nama);
            DownloadManager dm = getSystemService(DownloadManager.class);
            if (dm == null) return;
            dm.enqueue(r);
            Toast.makeText(this, "Mengunduh " + nama, Toast.LENGTH_SHORT).show();
        });
    }
}
