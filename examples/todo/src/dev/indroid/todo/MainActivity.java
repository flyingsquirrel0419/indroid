package dev.indroid.todo;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.DialogInterface;
import android.content.Intent;
import android.content.SharedPreferences;
import android.os.Bundle;
import android.os.Handler;
import android.text.TextUtils;
import android.util.Log;
import android.view.View;
import android.widget.AdapterView;
import android.widget.ArrayAdapter;
import android.widget.Button;
import android.widget.CheckBox;
import android.widget.CompoundButton;
import android.widget.EditText;
import android.widget.ListView;
import android.widget.Spinner;
import android.widget.TextView;
import android.widget.Toast;
import java.util.ArrayList;
import java.util.List;

public class MainActivity extends Activity {
    static final String[] PRIORITIES = { "Normal", "Urgent", "Someday" };
    private final List<Task> tasks = new ArrayList<Task>();
    private ArrayAdapter<Task> adapter;
    private TextView status, empty;
    private EditText input;
    private Spinner priority;
    private CheckBox urgentOnly;
    private final Handler handler = new Handler();
    private int seconds;

    static class Task {
        final String title; final int priority;
        Task(String t, int p) { title = t; priority = p; }
        @Override public String toString() { return (priority == 1 ? "🔥 " : priority == 2 ? "💤 " : "• ") + title; }
    }

    @Override protected void onCreate(Bundle state) {
        super.onCreate(state);
        setContentView(R.layout.activity_main);
        status = (TextView) findViewById(R.id.status);
        empty = (TextView) findViewById(R.id.empty);
        input = (EditText) findViewById(R.id.input);
        priority = (Spinner) findViewById(R.id.priority);
        urgentOnly = (CheckBox) findViewById(R.id.urgent_only);
        ListView list = (ListView) findViewById(R.id.list);

        priority.setAdapter(new ArrayAdapter<String>(this, android.R.layout.simple_spinner_item, PRIORITIES));
        adapter = new ArrayAdapter<Task>(this, android.R.layout.simple_list_item_1, new ArrayList<Task>());
        list.setAdapter(adapter);
        list.setOnItemClickListener(new AdapterView.OnItemClickListener() {
            @Override public void onItemClick(AdapterView<?> parent, View view, int position, long id) { confirmDone(adapter.getItem(position)); }
        });
        ((Button) findViewById(R.id.add)).setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { addTask(); }
        });
        urgentOnly.setOnCheckedChangeListener(new CompoundButton.OnCheckedChangeListener() {
            @Override public void onCheckedChanged(CompoundButton b, boolean checked) { refresh(); }
        });
        findViewById(R.id.about).setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                Intent i = new Intent(MainActivity.this, AboutActivity.class);
                i.putExtra("count", tasks.size());
                startActivity(i);
            }
        });

        SharedPreferences prefs = getSharedPreferences("todo", MODE_PRIVATE);
        int launches = prefs.getInt("launches", 0) + 1;
        prefs.edit().putInt("launches", launches).apply();
        tasks.add(new Task("Try indroid on an iPhone", 1));
        tasks.add(new Task("Star the repo", 0));
        refresh();
        Log.i("Todo", "onCreate done, launch #" + launches);
    }

    @Override protected void onResume() {
        super.onResume();
        handler.postDelayed(tick, 1000);
    }

    @Override protected void onPause() {
        super.onPause();
        handler.removeCallbacks(tick);
    }

    private final Runnable tick = new Runnable() {
        @Override public void run() { seconds++; refresh(); handler.postDelayed(this, 1000); }
    };

    private void addTask() {
        String title = input.getText().toString().trim();
        if (TextUtils.isEmpty(title)) { Toast.makeText(this, "Type something first", Toast.LENGTH_SHORT).show(); return; }
        tasks.add(new Task(title, priority.getSelectedItemPosition()));
        input.setText("");
        refresh();
        Toast.makeText(this, "Added \"" + title + "\"", Toast.LENGTH_SHORT).show();
    }

    private void confirmDone(final Task t) {
        new AlertDialog.Builder(this)
            .setTitle("Done?")
            .setMessage("Mark \"" + t.title + "\" as done?")
            .setPositiveButton("Done", new DialogInterface.OnClickListener() {
                @Override public void onClick(DialogInterface d, int which) { tasks.remove(t); refresh(); }
            })
            .setNegativeButton("Cancel", null)
            .show();
    }

    private void refresh() {
        adapter.setNotifyOnChange(false);
        adapter.clear();
        for (Task t : tasks) if (!urgentOnly.isChecked() || t.priority == 1) adapter.add(t);
        adapter.notifyDataSetChanged();
        empty.setVisibility(adapter.getCount() == 0 ? View.VISIBLE : View.GONE);
        status.setText(String.format("%d open · running for %ds", tasks.size(), seconds));
    }
}
