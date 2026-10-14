package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi

import com.google.gson.Gson
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.common.utils.invokeOnCompletion
import io.paritytech.polkadotapp.feature_products_impl.data.storage.ContainerScriptProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerTransport
import io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime.JsRuntime
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.job

/**
 * Orchestrates the host API for a single product environment.
 * Owns the runtime, bridge, and session scope. Tied to the supplied [scope] —
 * when [scope] is cancelled, the session disposes automatically.
 *
 * After [initialize]: all handlers registered, scripts injected (or hooks active),
 * [evaluateScript] is safe to call.
 */
class HostApiSession @AssistedInject constructor(
    private val containerScriptProvider: ContainerScriptProvider,
    private val gson: Gson,
    @Assisted private val environment: HostApiEnvironment,
    @Assisted private val runtime: JsRuntime,
    @Assisted private val transport: ContainerTransport,
    @Assisted private val scope: CoroutineScope,
) {
    @AssistedFactory
    interface Factory {
        fun create(
            environment: HostApiEnvironment,
            runtime: JsRuntime,
            transport: ContainerTransport,
            scope: CoroutineScope,
        ): HostApiSession
    }

    private val sessionScope = CoroutineScope(scope.coroutineContext + SupervisorJob(scope.coroutineContext.job) + Dispatchers.Default)
    private val bridge = ContainerBridge(transport, sessionScope, gson)

    init {
        scope.invokeOnCompletion { dispose() }
    }

    suspend fun initialize() {
        runtime.initialize()

        environment.handlerGroups.forEach { it.registerOn(bridge) }

        val payload = ScriptPayload(
            bridgeJs = containerScriptProvider.loadBridgeLibrary(),
            containerJs = containerScriptProvider.loadContainerScript()
        )

        environment.injectionStrategy.setup(payload, runtime) {
            runtime.loadInitialPage()
        }
    }

    suspend fun evaluateScript(script: String): Result<String> = runtime.evaluate(script)

    suspend fun loadEntryModule(srcPath: String): Result<Unit> = runtime.loadEntryModule(srcPath)

    private fun dispose() {
        bridge.dispose()
        runtime.dispose()
    }
}
