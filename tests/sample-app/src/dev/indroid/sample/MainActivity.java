package dev.indroid.sample;

import android.app.Activity;
import android.os.Bundle;
import android.util.Log;
import android.view.View;
import android.widget.Button;
import android.widget.EditText;
import android.widget.TextView;

public class MainActivity extends Activity {
    private int count;
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        setContentView(R.layout.activity_main);
        final TextView title = findViewById(R.id.title);
        Button button = findViewById(R.id.button);
        final EditText name = findViewById(R.id.name);
        button.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View view) {
                String text = "Tapped " + (++count);
                String entered = name.getText().toString();
                if (!entered.isEmpty()) text += ", " + entered;
                title.setText(text);
                Log.i("Sample", text);
            }
        });
    }
}
