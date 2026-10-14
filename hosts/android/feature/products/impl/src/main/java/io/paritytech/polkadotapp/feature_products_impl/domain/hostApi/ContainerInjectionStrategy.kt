package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi

import io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime.JsRuntime
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import timber.log.Timber

/**
 * The scripts that need to be injected into a [JsRuntime].
 */
class ScriptPayload(
    val bridgeJs: String,
    val containerJs: String,
)

/**
 * "When to inject" — bridges the session ("what") with the runtime ("where").
 *
 * Contract: after [setup] returns, the environment is ready for its intended use pattern.
 * - [ExplicitInjection]: scripts ARE injected, `evaluateScript()` is safe to call immediately.
 * - [PageLoadInjection]: content is loaded and injection hooks are active.
 *
 * The strategy controls when [loadInitialPage] is called relative to callback wiring,
 * ensuring correct ordering without the caller needing to think about it.
 */
interface ContainerInjectionStrategy {
    suspend fun setup(
        scriptPayload: ScriptPayload,
        runtime: JsRuntime,
        loadInitialPage: suspend () -> Unit
    )
}

/**
 * Narrow interface for observing page lifecycle events.
 * Implemented by BrowserWebViewProvider. Avoids [PageLoadInjection] holding the full provider reference.
 */
interface PageLifecycleSource {
    fun addOnPageStartedListener(listener: (url: String) -> Unit)

    fun addOnPageFinishedListener(listener: () -> Unit)
}

/**
 * Chat: inject once, explicitly, after the runtime is ready.
 *
 * Sequence: loadInitialPage (empty HTML shell) → waitForReady → inject bridgeJs → inject containerJs.
 * After [setup] returns, the container is fully loaded and `evaluateScript()` is safe.
 */
class ExplicitInjection : ContainerInjectionStrategy {
    override suspend fun setup(scriptPayload: ScriptPayload, runtime: JsRuntime, loadInitialPage: suspend () -> Unit) {
        loadInitialPage()
        runtime.waitForReady()
        runtime.evaluate(scriptPayload.bridgeJs)
        runtime.evaluate(scriptPayload.containerJs)
    }
}

/**
 * SPA/Explore: install the host bridge/container before product scripts run.
 *
 * Preferred path is document-start injection ([JsRuntime.injectDocumentStartScript]): the bridge +
 * container run before any page script on every navigation, so the product can never call into the
 * host API before it loads. If the runtime can't support document-start (e.g. an old WebView provider), fall back to the
 * legacy [PageLifecycleSource] callback, which re-injects on each `onPageStarted` but races with the
 * page's own scripts.
 *
 * After [setup] returns, the first page has started loading with injection active.
 *
 * @param pageLifecycleSource page lifecycle hook used only by the fallback path
 * @param coroutineScope scope for launching suspend work from the synchronous fallback callback
 */
class PageLoadInjection(
    private val pageLifecycleSource: PageLifecycleSource,
    private val coroutineScope: CoroutineScope,
) : ContainerInjectionStrategy {
    override suspend fun setup(scriptPayload: ScriptPayload, runtime: JsRuntime, loadInitialPage: suspend () -> Unit) {
        val combinedScript = scriptPayload.bridgeJs + "\n" + scriptPayload.containerJs
        val documentStartActive = runtime.injectDocumentStartScript(combinedScript, ALLOWED_ORIGIN_RULES)

        if (documentStartActive) {
            Timber.d("Using document-start injection (DOCUMENT_START_SCRIPT supported)")
        } else {
            Timber.w("Document-start unsupported, falling back to onPageStarted injection")
            pageLifecycleSource.addOnPageStartedListener { url ->
                coroutineScope.launch {
                    runtime.evaluate(scriptPayload.bridgeJs)
                    runtime.evaluate(scriptPayload.containerJs)
                }
            }
        }

        loadInitialPage()
    }

    private companion object {
        // Host-trusted bridge injected into every navigated product page, regardless of origin.
        val ALLOWED_ORIGIN_RULES = setOf("*")
    }
}
