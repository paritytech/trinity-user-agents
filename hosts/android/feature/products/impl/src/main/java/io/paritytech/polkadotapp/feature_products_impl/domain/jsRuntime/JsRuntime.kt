package io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime

import kotlinx.coroutines.flow.StateFlow

/**
 * Isolated JS execution context for one product.
 * Decoupled from the host environment (WebView vs QuickJS).
 *
 * Each [JsRuntime] represents a single product's sandboxed execution context.
 * Multiple runtimes may share the same underlying host (e.g., QuickJS contexts inside a pooled WebView).
 *
 * Lifecycle: [initialize] (setup) → [loadContent] (trigger page load) → [evaluate] / [waitForReady]
 */
interface JsRuntime {
    val state: StateFlow<RuntimeState>

    /**
     * Setup the runtime (create WebView, add JS interface, wire callbacks).
     * Does NOT trigger content loading — call [loadContent] separately.
     */
    suspend fun initialize()

    /**
     * Load the initial page into the runtime.
     * - For browsers: `loadUrl(productUrl)` — loads the product page
     * - For chat: `loadDataWithBaseURL(emptyHtml)` — loads an empty shell for script evaluation
     *
     * Must be called after [initialize]. Separated so that injection strategies can wire
     * callbacks before page load triggers lifecycle events.
     */
    suspend fun loadInitialPage()

    suspend fun evaluate(js: String): Result<String>

    /**
     * Load an entry module relative to the runtime's origin (`<script type="module" src>`), so the
     * module and its relative imports are fetched from that origin's served archive.
     */
    suspend fun loadEntryModule(srcPath: String): Result<Unit>

    suspend fun waitForReady()

    fun dispose()
}
