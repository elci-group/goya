package android.widget

import android.content.Context

open class TextView(@Suppress("UNUSED_PARAMETER") context: Context) {
    var text: CharSequence = ""
}

open class Button(context: Context) : TextView(context) {
    fun setOnClickListener(@Suppress("UNUSED_PARAMETER") listener: (Button) -> Unit) = Unit
}
