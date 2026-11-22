package dev.indroid.showcase

data class Planet(val name: String, val emoji: String, val moons: Int, val dayHours: Double, var favorite: Boolean = false) {
    val info: String get() = "$moons moon${if (moons == 1) "" else "s"} · day ${"%.1f".format(dayHours)} h"
}

object Planets {
    val all = listOf(
        Planet("Mercury", "☿️", 0, 4222.6),
        Planet("Venus", "♀️", 0, 2802.0),
        Planet("Earth", "🌍", 1, 24.0, favorite = true),
        Planet("Mars", "🔴", 2, 24.7),
        Planet("Jupiter", "🟠", 95, 9.9),
        Planet("Saturn", "🪐", 146, 10.7, favorite = true),
        Planet("Uranus", "🔵", 28, 17.2),
        Planet("Neptune", "🌊", 16, 16.1),
    )
    fun byName(name: String) = all.first { it.name == name }
}
