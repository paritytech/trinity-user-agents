package io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime

import android.os.Handler
import android.os.Looper
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import io.paritytech.polkadotapp.common.utils.evaluateJavascript
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerTransport
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.webView.WebViewJsBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.WebViewProvider
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import timber.log.Timber
import java.util.UUID

/**
 * [JsRuntime] implementation backed by a [WebViewProvider].
 *
 * Each instance is 1:1 with a [WebViewProvider]. For the future QuickJS pooling,
 * a different [JsRuntime] implementation would be used instead.
 */
class WebViewRuntime(
    private val webViewProvider: WebViewProvider,
) : JsRuntime {
    private val _state = MutableStateFlow<RuntimeState>(RuntimeState.NotInitialized)
    override val state: StateFlow<RuntimeState> = _state

    private val jsBridge = WebViewJsBridge()
    private var initialized = false

    override suspend fun initialize() {
        if (initialized) return

        try {
            jsBridge.registerFunction("__initial_theme_delivered__") {
                webViewProvider.onInitialThemeDelivered()
                ""
            }
            webViewProvider.addOnPageFinishedListener {
                _state.value = RuntimeState.Ready
            }
            webViewProvider.addOnWebViewDestroyedListener {
                _state.value = RuntimeState.Error("Render process gone")
            }

            webViewProvider.addWebViewSetup {
                it.addJavascriptInterface(jsBridge, "Android")
            }

            initialized = true
        } catch (e: Exception) {
            Timber.e(e, "WebViewRuntime: initialization failed")
            _state.value = RuntimeState.Error(e.message ?: "Unknown error")
        }
    }

    override suspend fun loadInitialPage() {
        check(initialized) { "Runtime must be initialized before loading initial page" }
        webViewProvider.loadInitialContent()
    }

    override suspend fun evaluate(js: String): Result<String> {
        if (!initialized) return Result.failure(IllegalStateException("Runtime not initialized"))

        return runCatching {
            val result = webViewProvider.accessWebView { it.evaluateJavascript(js) }
            parseJsResult(result)
        }
    }

    override suspend fun loadEntryModule(srcPath: String): Result<Unit> = injectEntryModule(srcPath, "")

    private suspend fun injectEntryModule(srcPath: String, observe: String): Result<Unit> {
        if (!initialized) return Result.failure(IllegalStateException("Runtime not initialized"))

        return runCatching {
            // The path crosses the dotNS trust boundary, so it is encoded rather than interpolated.
            val srcLiteral = srcPath.toJsStringLiteral()
            val injector = """
                (function() {
                    var script = document.createElement('script');
                    script.type = 'module';
                    script.src = $srcLiteral;
                    $observe
                    document.head.appendChild(script);
                })();
            """.trimIndent()
            webViewProvider.accessWebView { it.evaluateJavascript(injector) }
        }
    }

    /** Wait for module loading and initial execution. Top-level await may need a host call. */
    suspend fun startWorkerModule(srcPath: String) {
        val loaded = CompletableDeferred<Unit>()
        val callback = "worker-module-${UUID.randomUUID()}"
        jsBridge.registerFunction(callback) { error ->
            if (error.isEmpty()) loaded.complete(Unit) else loaded.completeExceptionally(IllegalStateException(error))
            ""
        }
        try {
            val observe = """
                const finish = error => {
                    script.onload = script.onerror = null;
                    window.removeEventListener("error", failed);
                    Android.call("$callback", error);
                };
                const failed = event => finish(event.message || "Worker module failed");
                window.addEventListener("error", failed);
                script.onerror = () => finish("Could not load worker module");
                script.onload = () => finish("");
            """.trimIndent()
            injectEntryModule(srcPath, observe).getOrThrow()
            loaded.await()
        } finally {
            jsBridge.unregisterFunction(callback)
        }
    }

    override suspend fun injectDocumentStartScript(js: String, allowedOriginRules: Set<String>): Boolean {
        if (!WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) return false

        webViewProvider.addWebViewSetup {
            WebViewCompat.addDocumentStartJavaScript(it, js, allowedOriginRules)
        }
        return true
    }

    override suspend fun waitForReady() {
        state.first { it == RuntimeState.Ready }
    }

    /**
     * Creates a [ContainerTransport] backed by this runtime's WebView.
     * Used to construct a [ContainerBridge] that communicates through this runtime.
     */
    fun createTransport(): ContainerTransport = object : ContainerTransport {
        override fun registerIncomingHandler(handler: (json: String) -> Unit) {
            jsBridge.registerFunction("__container__") { argsJson ->
                handler(argsJson)
                ""
            }
        }

        override suspend fun evaluateJs(script: String) {
            evaluate(script)
        }
    }

    override fun dispose() {
        if (initialized) {
            webViewProvider.getWebViewOrNull()?.let { view ->
                val main = Looper.getMainLooper()
                if (Looper.myLooper() == main) view.destroy() else Handler(main).post { view.destroy() }
            }
        }
        _state.value = RuntimeState.Error("Disposed")
    }

    private fun parseJsResult(result: String?): String {
        return when {
            result == null || result == "null" -> ""
            result.startsWith("\"") && result.endsWith("\"") -> {
                result.drop(1).dropLast(1)
                    .replace("\\\"", "\"")
                    .replace("\\n", "\n")
                    .replace("\\\\", "\\")
            }
            else -> result
        }
    }
}
