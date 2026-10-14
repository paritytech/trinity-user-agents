package io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.webView

import android.webkit.JavascriptInterface
import kotlinx.coroutines.runBlocking
import java.util.concurrent.ConcurrentHashMap

/**
 * Handler for native functions called from JavaScript.
 */
fun interface JsNativeHandler {
    suspend fun invoke(args: String): String
}

/**
 * JavaScript interface bridge for WebView.
 * Exposes native functions to JavaScript via @JavascriptInterface.
 *
 * Communication goes through the ContainerBridge pattern:
 * JS calls `Android.call("__container__", json)` -> dispatches to registered handlers.
 */
class WebViewJsBridge {
    private val handlers = ConcurrentHashMap<String, JsNativeHandler>()

    fun registerFunction(name: String, handler: JsNativeHandler) {
        handlers[name] = handler
    }

    /**
     * Called from JS: Android.call("functionName", "argsJson")
     */
    @JavascriptInterface
    fun call(functionName: String, argsJson: String): String {
        val handler = handlers[functionName] ?: return """{"error": "Unknown function: $functionName"}"""
        return runBlocking {
            try {
                handler.invoke(argsJson)
            } catch (e: Exception) {
                """{"error": "${e.message?.replace("\"", "\\\"")}"}"""
            }
        }
    }
}
