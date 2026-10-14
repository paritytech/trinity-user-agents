package io.paritytech.polkadotapp.common.utils

import android.view.View
import android.view.ViewGroup
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.webkit.WebView
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlin.coroutines.resume

fun View.makeGone() {
    this.visibility = View.GONE
}

fun <V : View> V.applyMatchParent(): V {
    layoutParams = ViewGroup.LayoutParams(MATCH_PARENT, MATCH_PARENT)
    return this
}

fun <V : View> V.applyMatchParentHorizontally(): V {
    layoutParams = ViewGroup.LayoutParams(MATCH_PARENT, WRAP_CONTENT)
    return this
}

suspend fun WebView.evaluateJavascript(script: String): String = suspendCancellableCoroutine { continuation ->
    evaluateJavascript(script) { result ->
        continuation.resume(result)
    }
}
