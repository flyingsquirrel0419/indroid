package dev.indroid.showcase

import android.os.Bundle
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import com.google.android.material.button.MaterialButton

class DetailActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_detail)
        val p = Planets.byName(intent.getStringExtra("name") ?: "Earth")
        title = p.name
        findViewById<TextView>(R.id.big).text = p.emoji
        findViewById<TextView>(R.id.title).text = p.name
        findViewById<TextView>(R.id.body).text = "${p.name} has ${p.info}.${if (p.favorite) "\n★ One of your favorites." else ""}"
        findViewById<MaterialButton>(R.id.back).setOnClickListener { finish() }
    }
}
