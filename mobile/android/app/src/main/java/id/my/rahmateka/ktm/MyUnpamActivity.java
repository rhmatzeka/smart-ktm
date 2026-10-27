package id.my.rahmateka.ktm;

import android.annotation.SuppressLint;
import android.graphics.Color;
import android.net.Uri;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.util.TypedValue;
import android.view.Gravity;
import android.webkit.CookieManager;
import android.webkit.JavascriptInterface;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;

import androidx.activity.OnBackPressedCallback;
import androidx.appcompat.app.AppCompatActivity;

/**
 * Sinkron MyUnpam satu tombol: membuka my.unpam.ac.id di dalam aplikasi, pengguna login sendiri
 * (kata sandi hanya diketik di halaman MyUnpam, tidak pernah dibaca aplikasi), lalu skrip pengumpul
 * dari halaman Sinkron Smart KTM dijalankan di sesi MyUnpam itu dan hasilnya dikembalikan ke plugin.
 *
 * Sesi MyUnpam tersimpan di WebView, jadi sinkron berikutnya biasanya tidak perlu login lagi.
 */
public class MyUnpamActivity extends AppCompatActivity {
    static final String SKRIP = "skrip";
    static final String MYUNPAM = "https://my.unpam.ac.id/";
    /** Hasil terakhir (JSON). Lewat field, bukan Intent, karena bisa lebih besar dari batas Binder. */
    static String hasil;

    private final Handler jam = new Handler(Looper.getMainLooper());
    private WebView web;
    private TextView status;
    private String skrip;
    private boolean berjalan = false;
    private String tokenGagal = "";

    @SuppressLint({"SetJavaScriptEnabled", "AddJavascriptInterface"})
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        hasil = null;
        skrip = getIntent().getStringExtra(SKRIP);

        LinearLayout akar = new LinearLayout(this);
        akar.setOrientation(LinearLayout.VERTICAL);

        LinearLayout bilah = new LinearLayout(this);
        bilah.setOrientation(LinearLayout.HORIZONTAL);
        bilah.setGravity(Gravity.CENTER_VERTICAL);
        bilah.setBackgroundColor(Color.parseColor("#1747C9"));
        bilah.setPadding(dp(16), dp(10), dp(8), dp(10));

        LinearLayout teks = new LinearLayout(this);
        teks.setOrientation(LinearLayout.VERTICAL);
        TextView judul = new TextView(this);
        judul.setText("Sinkron MyUnpam");
        judul.setTextColor(Color.WHITE);
        judul.setTextSize(TypedValue.COMPLEX_UNIT_SP, 16);
        judul.setTypeface(judul.getTypeface(), android.graphics.Typeface.BOLD);
        status = new TextView(this);
        status.setTextColor(Color.parseColor("#DCE5FF"));
        status.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12);
        status.setText("Membuka MyUnpam…");
        teks.addView(judul);
        teks.addView(status);
        bilah.addView(teks, new LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f));

        Button tutup = new Button(this);
        tutup.setText("Batal");
        tutup.setAllCaps(false);
        tutup.setTextColor(Color.WHITE);
        tutup.setBackgroundColor(Color.TRANSPARENT);
        tutup.setOnClickListener(v -> batal());
        bilah.addView(tutup);

        web = new WebView(this);
        WebSettings s = web.getSettings();
        s.setJavaScriptEnabled(true);
        s.setDomStorageEnabled(true);
        CookieManager.getInstance().setAcceptCookie(true);
        // Login SSO (satu.unpam.ac.id) dan verifikasi Cloudflare memakai cookie lintas domain.
        CookieManager.getInstance().setAcceptThirdPartyCookies(web, true);
        web.addJavascriptInterface(new Jembatan(), "SmartKTM");
        web.setWebViewClient(new WebViewClient() {
            @Override
            public boolean shouldOverrideUrlLoading(WebView v, WebResourceRequest r) {
                return false; // login dan redirect SSO tetap di dalam aplikasi
            }

            @Override
            public void onPageFinished(WebView v, String url) {
                if (!berjalan) status.setText(diMyUnpam(url) ? "Login MyUnpam seperti biasa. Data diambil otomatis." : "Login MyUnpam seperti biasa.");
            }
        });

        akar.addView(bilah, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT));
        akar.addView(web, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));
        setContentView(akar);

        getOnBackPressedDispatcher().addCallback(this, new OnBackPressedCallback(true) {
            @Override
            public void handleOnBackPressed() {
                if (web.canGoBack()) web.goBack();
                else batal();
            }
        });

        if (skrip == null || skrip.isEmpty()) {
            status.setText("Skrip sinkron tidak ada. Perbarui halaman lalu coba lagi.");
            return;
        }
        web.loadUrl(MYUNPAM);
        jam.postDelayed(this::periksa, 1500);
    }

    /** Tiap 1,5 detik: kalau sudah login di MyUnpam (ada access_token), jalankan skrip pengumpul. */
    private void periksa() {
        if (isFinishing()) return;
        if (!berjalan && diMyUnpam(web.getUrl())) {
            web.evaluateJavascript("localStorage.getItem('access_token')||''", nilai -> {
                String t = nilai == null ? "" : nilai.replace("\"", "");
                if (t.isEmpty() || t.equals("null") || t.equals(tokenGagal) || berjalan) return;
                berjalan = true;
                status.setText("Mengambil jadwal dan presensi…");
                web.evaluateJavascript(skrip, null);
            });
        }
        jam.postDelayed(this::periksa, 1500);
    }

    private static boolean diMyUnpam(String url) {
        if (url == null) return false;
        String host = Uri.parse(url).getHost();
        return "my.unpam.ac.id".equals(host);
    }

    private void batal() {
        setResult(RESULT_CANCELED);
        finish();
    }

    @Override
    protected void onDestroy() {
        jam.removeCallbacksAndMessages(null);
        CookieManager.getInstance().flush();
        web.destroy();
        super.onDestroy();
    }

    private int dp(int n) {
        return Math.round(n * getResources().getDisplayMetrics().density);
    }

    /** Dipanggil skrip pengumpul dari halaman MyUnpam. */
    private class Jembatan {
        @JavascriptInterface
        public void selesai(String json) {
            runOnUiThread(() -> {
                hasil = json;
                setResult(RESULT_OK);
                finish();
            });
        }

        /** `login` = sesi MyUnpam habis (skrip sudah mengarahkan ke halaman login); selain itu pesan galat. */
        @JavascriptInterface
        public void gagal(String alasan, String token) {
            runOnUiThread(() -> {
                berjalan = false;
                if ("login".equals(alasan)) {
                    status.setText("Sesi MyUnpam habis. Login lagi ya.");
                } else {
                    tokenGagal = token == null ? "" : token;
                    status.setText("Gagal mengambil data (" + alasan + "). Coba login ulang atau tekan Batal.");
                }
            });
        }
    }
}
