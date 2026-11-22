package dev.indroid.todo;

import android.app.Activity;
import android.os.Build;
import android.os.Bundle;
import android.view.View;
import android.widget.TextView;

public class AboutActivity extends Activity {
    @Override protected void onCreate(Bundle state) {
        super.onCreate(state);
        setContentView(R.layout.activity_about);
        setTitle("About");
        int count = getIntent().getIntExtra("count", 0);
        ((TextView) findViewById(R.id.stats)).setText("Open tasks: " + count + "\nAndroid API " + Build.VERSION.SDK_INT + " on " + System.getProperty("java.vm.name"));
        findViewById(R.id.done).setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { finish(); }
        });
    }
}
