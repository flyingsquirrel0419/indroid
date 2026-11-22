package dev.indroid.calc

import java.math.BigDecimal

/** Expression model: a tiny shunting-yard evaluator over BigDecimal-free doubles. */
sealed class Token {
    data class Num(val value: Double) : Token()
    data class Op(val symbol: Char) : Token() {
        val precedence get() = if (symbol == '+' || symbol == '−') 1 else 2
    }
}

class Calculator {
    private val tokens = mutableListOf<Token>()
    private var entry = ""
    var lastResult: Double? = null
        private set
    val history = ArrayDeque<String>()

    val expression: String
        get() = (tokens.map { it.render() } + listOfNotNull(entry.ifEmpty { null })).joinToString(" ")

    val display: String
        get() = when {
            entry.isNotEmpty() -> entry
            lastResult != null && tokens.isEmpty() -> format(lastResult!!)
            else -> tokens.lastOrNull { it is Token.Num }?.render() ?: "0"
        }

    fun digit(d: Char) {
        if (lastResult != null && tokens.isEmpty() && entry.isEmpty()) lastResult = null
        if (d == '.' && '.' in entry) return
        if (entry == "0" && d != '.') entry = ""
        if (entry.length < 15) entry += if (d == '.' && entry.isEmpty()) "0." else d.toString()
    }

    fun op(symbol: Char) {
        when {
            entry.isNotEmpty() -> { tokens += Token.Num(entry.toDouble()); entry = "" }
            tokens.isEmpty() && lastResult != null -> tokens += Token.Num(lastResult!!)
            tokens.lastOrNull() is Token.Op -> tokens.removeAt(tokens.lastIndex)
            tokens.isEmpty() -> tokens += Token.Num(0.0)
        }
        tokens += Token.Op(symbol)
    }

    fun percent() {
        val v = entry.toDoubleOrNull() ?: return
        entry = format(v / 100)
    }

    fun negate() {
        entry = when {
            entry.isEmpty() -> entry
            entry.startsWith("-") -> entry.drop(1)
            else -> "-$entry"
        }
    }

    fun backspace() { entry = entry.dropLast(1) }

    fun clear() { tokens.clear(); entry = ""; lastResult = null }

    fun equals(): String? {
        if (entry.isNotEmpty()) { tokens += Token.Num(entry.toDouble()); entry = "" }
        if (tokens.lastOrNull() is Token.Op) tokens.removeAt(tokens.lastIndex)
        if (tokens.size < 3) return null
        val text = expression
        val result = evaluate(tokens)
        tokens.clear()
        lastResult = result
        val line = "$text = ${format(result)}"
        history.addFirst(line)
        while (history.size > 3) history.removeLast()
        return line
    }

    companion object {
        fun evaluate(input: List<Token>): Double {
            val out = ArrayDeque<Double>()
            val ops = ArrayDeque<Token.Op>()
            fun reduce() {
                val op = ops.removeLast()
                val b = out.removeLast()
                val a = out.removeLast()
                out.addLast(
                    when (op.symbol) {
                        '+' -> a + b
                        '−' -> a - b
                        '×' -> a * b
                        else -> if (b == 0.0) Double.NaN else a / b
                    }
                )
            }
            for (t in input) when (t) {
                is Token.Num -> out.addLast(t.value)
                is Token.Op -> {
                    while (ops.isNotEmpty() && ops.last().precedence >= t.precedence) reduce()
                    ops.addLast(t)
                }
            }
            while (ops.isNotEmpty()) reduce()
            return out.single()
        }

        fun format(v: Double): String = when {
            v.isNaN() -> "Error"
            v == Math.rint(v) && Math.abs(v) < 1e15 -> v.toLong().toString()
            else -> BigDecimal(v).round(java.math.MathContext(12)).stripTrailingZeros().toPlainString()
        }
    }
}

private fun Token.render(): String = when (this) {
    is Token.Num -> Calculator.format(value)
    is Token.Op -> symbol.toString()
}
