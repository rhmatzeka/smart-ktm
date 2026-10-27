package id.my.rahmateka.ktm;

import android.app.Activity;
import android.content.Intent;

import androidx.activity.result.ActivityResult;

import com.getcapacitor.JSObject;
import com.getcapacitor.Plugin;
import com.getcapacitor.PluginCall;
import com.getcapacitor.PluginMethod;
import com.getcapacitor.annotation.ActivityCallback;
import com.getcapacitor.annotation.CapacitorPlugin;

/**
 * ambil({ skrip }): buka MyUnpam di dalam aplikasi, tunggu pengguna login, jalankan skrip pengumpul,
 * lalu kembalikan { data: "<json>" }. Skrip datang dari halaman Sinkron, jadi perubahan cara ambil data
 * cukup deploy web tanpa APK baru.
 */
@CapacitorPlugin(name = "MyUnpam")
public class MyUnpamPlugin extends Plugin {
    @PluginMethod
    public void ambil(PluginCall call) {
        String skrip = call.getString("skrip");
        if (skrip == null || skrip.isEmpty()) {
            call.reject("Skrip sinkron kosong");
            return;
        }
        Intent intent = new Intent(getContext(), MyUnpamActivity.class);
        intent.putExtra(MyUnpamActivity.SKRIP, skrip);
        startActivityForResult(call, intent, "kembali");
    }

    @ActivityCallback
    private void kembali(PluginCall call, ActivityResult r) {
        if (call == null) return;
        String data = MyUnpamActivity.hasil;
        MyUnpamActivity.hasil = null;
        if (r.getResultCode() != Activity.RESULT_OK || data == null) {
            call.reject("Sinkron dibatalkan");
            return;
        }
        JSObject o = new JSObject();
        o.put("data", data);
        call.resolve(o);
    }
}
