package android.app

import android.content.Context
import android.os.Bundle
import android.widget.TextView

open class Activity : Context() {
    open fun onCreate(savedInstanceState: Bundle?) = Unit

    fun setContentView(@Suppress("UNUSED_PARAMETER") view: TextView) = Unit
}
