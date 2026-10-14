package io.parity.truapi

import android.content.Context

/** Shared browser container bundled with the native host adapter. */
object ContainerScriptBundle {
    /** Load before creating the product execution, so a missing asset cannot leave it running. */
    fun load(context: Context): String =
        context.assets.open("truapi-container.js").bufferedReader().use { it.readText() }
}
