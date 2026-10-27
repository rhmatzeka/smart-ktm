package id.my.rahmateka.ktm;

import android.app.Activity;
import android.content.Intent;

import androidx.activity.result.ActivityResult;

import com.getcapacitor.JSArray;
import com.getcapacitor.JSObject;
import com.getcapacitor.Plugin;
import com.getcapacitor.PluginCall;
import com.getcapacitor.PluginMethod;
import com.getcapacitor.annotation.ActivityCallback;
import com.getcapacitor.annotation.CapacitorPlugin;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.util.List;

/**
 * ambil({ langkah }): buka situs kampus di dalam aplikasi langkah demi langkah (lihat MyUnpamActivity),
 * lalu kembalikan { hasil: [json|null, ...], data: hasil[0] }. Skrip datang dari halaman Sinkron,
 * jadi perubahan cara ambil data cukup deploy web tanpa APK baru.
 * Versi awal memanggil ambil({ skrip }) untuk MyUnpam saja; itu tetap didukung.
 */
@CapacitorPlugin(name = "MyUnpam")
public class MyUnpamPlugin extends Plugin {
    @PluginMethod
    public void ambil(PluginCall call) {
        JSArray langkah = call.getArray("langkah");
        String skrip = call.getString("skrip");
        try {
            if (langkah == null || langkah.length() == 0) {
                if (skrip == null || skrip.isEmpty()) {
                    call.reject("Skrip sinkron kosong");
                    return;
                }
                JSONObject l = new JSONObject();
                l.put("nama", "MyUnpam");
                l.put("url", "https://my.unpam.ac.id/");
                l.put("host", "my.unpam.ac.id");
                l.put("cekLogin", "localStorage.getItem('access_token')||''");
                l.put("skrip", skrip);
                langkah = new JSArray();
                langkah.put(l);
            }
        } catch (JSONException e) {
            call.reject("Langkah sinkron tidak valid");
            return;
        }
        Intent intent = new Intent(getContext(), MyUnpamActivity.class);
        intent.putExtra(MyUnpamActivity.LANGKAH, langkah.toString());
        startActivityForResult(call, intent, "kembali");
    }

    @ActivityCallback
    private void kembali(PluginCall call, ActivityResult r) {
        if (call == null) return;
        List<String> h = MyUnpamActivity.hasil;
        MyUnpamActivity.hasil = null;
        if (r.getResultCode() != Activity.RESULT_OK || h == null || h.isEmpty() || h.get(0) == null) {
            call.reject("Sinkron dibatalkan");
            return;
        }
        JSONArray semua = new JSONArray();
        for (String s : h) semua.put(s == null ? JSONObject.NULL : s);
        JSObject o = new JSObject();
        o.put("data", h.get(0));
        o.put("hasil", semua);
        call.resolve(o);
    }
}
