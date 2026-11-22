package dev.indroid.calc

import android.app.Activity
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.view.Gravity
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

class MainActivity : Activity() {
    private val calc = Calculator()
    private lateinit var display: TextView
    private lateinit var expr: TextView
    private lateinit var history: TextView

    private val rows = listOf(
        listOf("AC", "±", "%", "÷"),
        listOf("7", "8", "9", "×"),
        listOf("4", "5", "6", "−"),
        listOf("1", "2", "3", "+"),
        listOf("0", ".", "⌫", "="),
    )

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        display = findViewById(R.id.display)
        expr = findViewById(R.id.expr)
        history = findViewById(R.id.history)
        val keys = findViewById<LinearLayout>(R.id.keys)
        for (row in rows) {
            val line = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
            for (label in row) line.addView(key(label), LinearLayout.LayoutParams(0, 72.dp, 1f).apply { setMargins(4.dp, 4.dp, 4.dp, 4.dp) })
            keys.addView(line, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT))
        }
        savedInstanceState?.getString("history")?.let { history.text = it }
        refresh()
    }

    private val Int.dp: Int get() = (this * resources.displayMetrics.density).toInt()

    private fun key(label: String) = Button(this).apply {
        text = label
        textSize = 26f
        gravity = Gravity.CENTER
        setTextColor(Color.WHITE)
        background = GradientDrawable().apply {
            cornerRadius = 36f.dpf
            setColor(
                when (label) {
                    "÷", "×", "−", "+", "=" -> resources.getColor(R.color.op)
                    "AC", "±", "%" -> resources.getColor(R.color.fn)
                    else -> resources.getColor(R.color.key)
                }
            )
        }
        setOnClickListener { press(label) }
    }

    private val Float.dpf: Float get() = this * resources.displayMetrics.density

    private fun press(label: String) {
        when (label) {
            "AC" -> calc.clear()
            "±" -> calc.negate()
            "%" -> calc.percent()
            "⌫" -> calc.backspace()
            "=" -> calc.equals()
            "÷", "×", "−", "+" -> calc.op(label[0])
            else -> calc.digit(label[0])
        }
        refresh()
    }

    private fun refresh() {
        display.text = calc.display
        expr.text = calc.expression
        history.text = calc.history.joinToString("\n")
    }
}
