// Kotlin language + stdlib smoke test (compared against the JVM).
data class Person(val name: String, val age: Int)

sealed class Shape {
    data class Circle(val r: Double) : Shape()
    data class Rect(val w: Double, val h: Double) : Shape()
    object Empty : Shape()
}

fun Shape.area(): Double = when (this) {
    is Shape.Circle -> 3.0 * r * r
    is Shape.Rect -> w * h
    Shape.Empty -> 0.0
}

enum class Color(val hex: String) { RED("#f00"), GREEN("#0f0"), BLUE("#00f") }

interface Greeter { fun greet(who: String): String = "Hello, $who" }
class Loud : Greeter { override fun greet(who: String) = super.greet(who).uppercase() + "!" }

object Counter {
    var n = 0
    fun next() = ++n
}

class Box<T>(val value: T) {
    companion object { fun <T> of(v: T) = Box(v) }
    override fun toString() = "Box($value)"
}

fun String.shout() = this.uppercase() + "!"
inline fun <T> measure(block: () -> T): T = block()

val lazyValue: String by lazy { "computed once" }

fun main() {
    val people = listOf(Person("Ann", 31), Person("Bob", 25), Person("Cid", 40), Person("Dee", 25))
    println(people.first())
    println(people.sortedBy { it.age }.map { it.name })
    println(people.groupBy { it.age }.mapValues { (_, v) -> v.size })
    println(people.filter { it.age > 26 }.joinToString(", ") { "${it.name}(${it.age})" })
    println(people.maxByOrNull { it.age }?.name)
    println(people.sumOf { it.age } / people.size.toDouble())
    println(people.associateBy { it.name }.keys)
    println(people.partition { it.age < 30 })
    val (n, a) = people[1]
    println("$n is $a")
    println(people[0].copy(age = 32) == Person("Ann", 32))

    val shapes = listOf(Shape.Circle(1.0), Shape.Rect(2.0, 3.5), Shape.Empty)
    println(shapes.map { it.area() })
    println(Color.values().joinToString { it.name + "=" + it.hex })
    println(Color.valueOf("GREEN").ordinal)
    println(Loud().greet("kotlin"))
    println(Counter.next() + Counter.next())
    println(Box.of(42))
    println("indroid".shout())
    println(measure { (1..10).filter { it % 2 == 0 }.sum() })
    println(lazyValue)
    println(lazyValue.length)

    var maybe: String? = null
    println(maybe?.length ?: -1)
    maybe = "four"
    println(maybe?.length ?: -1)
    println(maybe.let { it + it })

    val sb = buildString {
        for (i in 10 downTo 1 step 3) append(i).append(' ')
    }
    println(sb.trim())
    println((1..5).fold(1) { acc, x -> acc * x })
    println(generateSequence(1) { it * 2 }.take(10).toList())
    println(sequenceOf("a", "bb", "ccc").map { it.length }.filter { it > 1 }.toList())
    println(mutableMapOf("x" to 1).apply { put("y", 2); getOrPut("z") { 3 } })
    println(setOf(3, 1, 2) + setOf(2, 5))
    println(listOf(1, 2, 3).zip(listOf("a", "b", "c")))
    println(listOf(5, 2, 9).sortedDescending().windowed(2))
    println("a,b,,c".split(",").filter(String::isNotEmpty))
    println(String.format("%.2f|%5d|%s", 3.14159, 42, "x"))
    println("Kotlin".reversed() + " " + "abc".padStart(5, '*'))
    println(listOf(1, 2, 3, 4).chunked(3))
    println(arrayOf(3, 1, 2).also { it.sort() }.contentToString())
    println(IntArray(5) { it * it }.toList())
    println('a' + 2)
    println(10.coerceIn(0, 5))
    println(Long.MAX_VALUE / 3)
    println(0.1 + 0.2)
    println(7 / 2.0)
    println(Int.MAX_VALUE + 1)

    try {
        require(people.size > 10) { "need more people" }
    } catch (e: IllegalArgumentException) {
        println("caught: ${e.message}")
    }
    try {
        val s: String? = null
        s!!.length
    } catch (e: NullPointerException) {
        println("npe")
    }
    val result = runCatching { "12x".toInt() }
    println(result.isFailure)
    println("12".toIntOrNull()?.plus(1))
    val r = when (val x = people.size) {
        in 0..2 -> "few"
        in 3..10 -> "some ($x)"
        else -> "many"
    }
    println(r)
    repeat(2) { println("repeat $it") }
    println(listOf("b", "a", "c").sortedWith(compareBy { it }).asReversed())
    println(mapOf(1 to "one", 2 to "two").entries.joinToString { "${it.key}:${it.value}" })
    println(Pair(1, "x").toList())
    println(Triple(1, 2, 3))
    println("end")
}
