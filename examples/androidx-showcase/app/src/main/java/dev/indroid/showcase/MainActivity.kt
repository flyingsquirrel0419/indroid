package dev.indroid.showcase

import android.content.Intent
import android.os.Bundle
import android.view.LayoutInflater
import android.view.MenuItem
import android.view.View
import android.view.ViewGroup
import android.widget.CheckBox
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.core.widget.doAfterTextChanged
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.appbar.MaterialToolbar
import com.google.android.material.chip.Chip
import com.google.android.material.floatingactionbutton.ExtendedFloatingActionButton
import com.google.android.material.materialswitch.MaterialSwitch
import com.google.android.material.snackbar.Snackbar
import com.google.android.material.textfield.TextInputEditText

class MainActivity : AppCompatActivity() {
    private val adapter = PlanetAdapter { planet -> openDetail(planet) }
    private var query = ""
    private var favoritesOnly = false
    private var sortByMoons = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        val toolbar = findViewById<MaterialToolbar>(R.id.toolbar)
        setSupportActionBar(toolbar)
        toolbar.inflateMenu(R.menu.main)
        toolbar.setOnMenuItemClickListener { onOptionsItemSelected(it) }

        val list = findViewById<RecyclerView>(R.id.list)
        list.layoutManager = LinearLayoutManager(this)
        list.adapter = adapter

        findViewById<TextInputEditText>(R.id.search).doAfterTextChanged {
            query = it?.toString().orEmpty()
            refresh()
        }
        findViewById<MaterialSwitch>(R.id.favorites).setOnCheckedChangeListener { _, checked ->
            favoritesOnly = checked
            refresh()
        }
        findViewById<ExtendedFloatingActionButton>(R.id.fab).setOnClickListener { v ->
            val p = Planets.all.random()
            Snackbar.make(v, "How about ${p.name}?", Snackbar.LENGTH_SHORT).setAction("Open") { openDetail(p) }.show()
        }
        refresh()
    }

    override fun onCreateOptionsMenu(menu: android.view.Menu): Boolean {
        menuInflater.inflate(R.menu.main, menu)
        return true
    }

    override fun onOptionsItemSelected(item: MenuItem): Boolean = when (item.itemId) {
        R.id.action_sort -> { sortByMoons = !sortByMoons; refresh(); true }
        else -> super.onOptionsItemSelected(item)
    }

    private fun refresh() {
        var items = Planets.all.filter { it.name.contains(query, ignoreCase = true) }
        if (favoritesOnly) items = items.filter { it.favorite }
        if (sortByMoons) items = items.sortedByDescending { it.moons }
        adapter.submit(items)
        findViewById<Chip>(R.id.count).text = "${items.size} planet${if (items.size == 1) "" else "s"}"
    }

    private fun openDetail(p: Planet) {
        startActivity(Intent(this, DetailActivity::class.java).putExtra("name", p.name))
    }

    inner class PlanetAdapter(private val onClick: (Planet) -> Unit) : RecyclerView.Adapter<PlanetAdapter.Holder>() {
        private var items: List<Planet> = emptyList()

        fun submit(newItems: List<Planet>) {
            items = newItems
            notifyDataSetChanged()
        }

        inner class Holder(view: View) : RecyclerView.ViewHolder(view) {
            val emoji: TextView = view.findViewById(R.id.emoji)
            val name: TextView = view.findViewById(R.id.name)
            val info: TextView = view.findViewById(R.id.info)
            val fav: CheckBox = view.findViewById(R.id.fav)
        }

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int) =
            Holder(LayoutInflater.from(parent.context).inflate(R.layout.item_planet, parent, false))

        override fun getItemCount() = items.size

        override fun onBindViewHolder(holder: Holder, position: Int) {
            val p = items[position]
            holder.emoji.text = p.emoji
            holder.name.text = p.name
            holder.info.text = p.info
            holder.fav.setOnCheckedChangeListener(null)
            holder.fav.isChecked = p.favorite
            holder.fav.setOnCheckedChangeListener { _, checked -> p.favorite = checked; if (favoritesOnly) refresh() }
            holder.itemView.setOnClickListener { onClick(p) }
        }
    }
}
