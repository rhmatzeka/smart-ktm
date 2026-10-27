package id.my.rahmateka.ktm;

import android.os.Bundle;

import com.getcapacitor.BridgeActivity;

public class MainActivity extends BridgeActivity {
    @Override
    public void onCreate(Bundle savedInstanceState) {
        registerPlugin(KtmNfcPlugin.class);
        super.onCreate(savedInstanceState);
    }
}
