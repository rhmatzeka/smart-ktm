package id.my.rahmateka.ktm;

import android.annotation.SuppressLint;
import android.graphics.Color;
import android.graphics.Typeface;
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

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.util.ArrayList;
import java.util.List;

/**
 * Sinkron kampus satu tombol: membuka situs kampus (MyUnpam, lalu Mentari) di dalam aplikasi.
 * Pengguna login sendiri (kata sandi hanya diketik di halaman kampus, tidak pernah dibaca aplikasi),
 * lalu skrip pengumpul dari halaman Sinkron Smart KTM dijalankan di sesi itu dan hasilnya dikembalikan.
 *
 * Tiap langkah: { nama, url, host, cekLogin (JS -> token), skrip, bolehLewat }.
 * Sesi tersimpan di WebView, jadi sinkron berikutnya biasanya tidak perlu login lagi.
 */
public class MyUnpamActivity extends AppCompatActivity {
    static final String LANGKAH = "langkah";
    /** Hasil per langkah (JSON, null = dilewati). Lewat field, bukan Intent, karena bisa melebihi batas Binder. */
    static List<String> hasil;

    private final Handler jam = new Handler(Looper.getMainLooper());
    private final List<String> kumpulan = new ArrayList<>();
    private JSONArray langkah;
    private int ke = 0;
    private WebView web;
    private TextView judul;
    private TextView status;
    private Button tombol;
    private boolean berjalan = false;
    private String tokenGagal = "";

    @SuppressLint({"SetJavaScriptEnabled", "AddJavascriptInterface"})
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        hasil = null;
        try {
            langkah = new JSONArray(getIntent().getStringExtra(LANGKAH));
        } catch (JSONException | NullPointerException e) {
            langkah = new JSONArray();
        }

        LinearLayout akar = new LinearLayout(this);
        akar.setOrientation(LinearLayout.VERTICAL);

        LinearLayout bilah = new LinearLayout(this);
        bilah.setOrientation(LinearLayout.HORIZONTAL);
        bilah.setGravity(Gravity.CENTER_VERTICAL);
        bilah.setBackgroundColor(Color.parseColor("#1747C9"));
        bilah.setPadding(dp(16), dp(10), dp(8), dp(10));

        LinearLayout teks = new LinearLayout(this);
        teks.setOrientation(LinearLayout.VERTICAL);
        judul = new TextView(this);
        judul.setTextColor(Color.WHITE);
        judul.setTextSize(TypedValue.COMPLEX_UNIT_SP, 16);
        judul.setTypeface(judul.getTypeface(), Typeface.BOLD);
        status = new TextView(this);
        status.setTextColor(Color.parseColor("#DCE5FF"));
        status.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12);
        teks.addView(judul);
        teks.addView(status);
        bilah.addView(teks, new LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f));

        tombol = new Button(this);
        tombol.setAllCaps(false);
        tombol.setTextColor(Color.WHITE);
        tombol.setBackgroundColor(Color.TRANSPARENT);
        tombol.setOnClickListener(v -> lewatiAtauBatal());
        bilah.addView(tombol);

        web = new WebView(this);
        WebSettings s = web.getSettings();
        s.setJavaScriptEnabled(true);
        s.setDomStorageEnabled(true);
        CookieManager.getInstance().setAcceptCookie(true);
        // Login SSO dan verifikasi Cloudflare memakai cookie lintas domain.
        CookieManager.getInstance().setAcceptThirdPartyCookies(web, true);
        web.addJavascriptInterface(new Jembatan(), "SmartKTM");
        web.setWebViewClient(new WebViewClient() {
            @Override
            public boolean shouldOverrideUrlLoading(WebView v, WebResourceRequest r) {
                return false; // login dan redirect SSO tetap di dalam aplikasi
            }
        });

        akar.addView(bilah, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT));
        akar.addView(web, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));
        setContentView(akar);

        getOnBackPressedDispatcher().addCallback(this, new OnBackPressedCallback(true) {
            @Override
            public void handleOnBackPressed() {
                if (web.canGoBack()) web.goBack();
                else lewatiAtauBatal();
            }
        });

        if (langkah.length() == 0) {
            judul.setText("Sinkron");
            status.setText("Skrip sinkron tidak ada. Perbarui halaman lalu coba lagi.");
            tombol.setText("Tutup");
            return;
        }
        mulaiLangkah();
        jam.postDelayed(this::periksa, 1500);
    }

    private JSONObject sekarang() {
        return langkah.optJSONObject(ke);
    }

    private void mulaiLangkah() {
        JSONObject l = sekarang();
        berjalan = false;
        tokenGagal = "";
        String nama = l.optString("nama", "Kampus");
        judul.setText(langkah.length() > 1 ? "Sinkron " + nama + " (" + (ke + 1) + "/" + langkah.length() + ")" : "Sinkron " + nama);
        status.setText("Login " + nama + " seperti biasa. Data diambil otomatis.");
        tombol.setText(l.optBoolean("bolehLewat") ? "Lewati" : "Batal");
        web.loadUrl(l.optString("url"));
    }

    /** Tiap 1,5 detik: kalau sudah login (cekLogin memberi token), jalankan skrip pengumpul langkah ini. */
    private void periksa() {
        if (isFinishing()) return;
        JSONObject l = sekarang();
        if (!berjalan && l != null && diHost(web.getUrl(), l.optString("host"))) {
            web.evaluateJavascript(l.optString("cekLogin", "''"), nilai -> {
                String t = nilai == null ? "" : nilai.replace("\"", "");
                if (t.isEmpty() || t.equals("null") || t.equals(tokenGagal) || berjalan) return;
                berjalan = true;
                status.setText("Mengambil data " + l.optString("nama") + "…");
                web.evaluateJavascript(l.optString("skrip"), null);
            });
        }
        jam.postDelayed(this::periksa, 1500);
    }

    private static boolean diHost(String url, String host) {
        return url != null && host != null && host.equals(Uri.parse(url).getHost());
    }

    private void lanjut(String json) {
        kumpulan.add(json);
        ke++;
        if (ke < langkah.length()) {
            mulaiLangkah();
            return;
        }
        selesai();
    }

    private void selesai() {
        hasil = new ArrayList<>(kumpulan);
        setResult(kumpulan.isEmpty() || kumpulan.get(0) == null ? RESULT_CANCELED : RESULT_OK);
        finish();
    }

    /** Langkah opsional (Mentari) dilewati; langkah wajib (MyUnpam) membatalkan sinkron. */
    private void lewatiAtauBatal() {
        JSONObject l = sekarang();
        if (l != null && l.optBoolean("bolehLewat")) lanjut(null);
        else {
            setResult(RESULT_CANCELED);
            finish();
        }
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

    /** Dipanggil skrip pengumpul dari halaman kampus. */
    private class Jembatan {
        @JavascriptInterface
        public void selesai(String json) {
            runOnUiThread(() -> lanjut(json));
        }

        /** `login` = sesi habis (skrip sudah mengarahkan ke halaman login); selain itu pesan galat. */
        @JavascriptInterface
        public void gagal(String alasan, String token) {
            runOnUiThread(() -> {
                berjalan = false;
                if ("login".equals(alasan)) {
                    status.setText("Sesi habis. Login lagi ya.");
                } else {
                    tokenGagal = token == null ? "" : token;
                    status.setText("Gagal mengambil data (" + alasan + "). Login ulang atau tekan " + tombol.getText() + ".");
                }
            });
        }
    }
}
