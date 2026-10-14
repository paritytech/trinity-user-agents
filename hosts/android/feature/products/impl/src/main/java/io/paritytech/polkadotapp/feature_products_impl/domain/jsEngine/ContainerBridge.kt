package io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine

import com.google.gson.Gson
import com.google.gson.JsonObject
import com.google.gson.JsonParser
import com.google.gson.JsonSyntaxException
import com.google.gson.reflect.TypeToken
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch
import timber.log.Timber
import java.lang.reflect.Type
import java.util.concurrent.ConcurrentHashMap

/**
 * Abstraction over the JS-Native transport layer.
 * Different implementations for WebView (@JavascriptInterface) vs QuickJS (native bindings).
 */
interface ContainerTransport {
    fun registerIncomingHandler(handler: (json: String) -> Unit)
    suspend fun evaluateJs(script: String)
}

/**
 * Bridge for typed request-response and subscription communication
 * between the container JS layer and native Kotlin handlers.
 *
 * JS sends messages via the transport's incoming handler.
 * Kotlin responds via the transport's evaluateJs method.
 */
class ContainerBridge(
    private val transport: ContainerTransport,
    private val scope: CoroutineScope,
    private val gson: Gson,
) {
    init {
        transport.registerIncomingHandler { json -> handleMessage(json) }
    }

    private class RequestEntry(
        val paramsType: Type,
        val handler: suspend (Any?) -> Result<Any?>,
    )

    private class SubscriptionEntry(
        val paramsType: Type,
        val handler: (Any?) -> Flow<*>,
    )

    private val requestHandlers = mutableMapOf<String, RequestEntry>()
    private val subscriptionHandlers = mutableMapOf<String, SubscriptionEntry>()
    private val activeSubscriptions = ConcurrentHashMap<String, Job>()

    // ---- Type-safe registration via reified generics ----

    inline fun <reified P, R> registerHandler(
        method: String,
        noinline handler: suspend (P) -> Result<R>,
    ) {
        val paramsType = object : TypeToken<P>() {}.type
        registerHandler(method, paramsType, handler)
    }

    fun <P, R> registerHandler(
        method: String,
        paramType: Type,
        handler: suspend (P) -> Result<R>,
    ) {
        @Suppress("UNCHECKED_CAST")
        requestHandlers[method] = RequestEntry(paramType) { params ->
            handler(params as P)
        }
    }

    inline fun <reified P, reified E> registerSubscription(
        method: String,
        noinline handler: (P) -> Flow<E>,
    ) {
        val paramsType = object : TypeToken<P>() {}.type
        registerSubscription(method, paramsType, handler)
    }

    fun <P, E> registerSubscription(
        method: String,
        paramType: Type,
        handler: (P) -> Flow<E>,
    ) {
        @Suppress("UNCHECKED_CAST")
        subscriptionHandlers[method] = SubscriptionEntry(paramType) { params ->
            handler(params as P)
        }
    }

    // ---- Lifecycle ----

    fun dispose() {
        activeSubscriptions.values.forEach { it.cancel() }
        activeSubscriptions.clear()
    }

    // ---- Message routing ----

    private fun handleMessage(json: String) {
        Timber.d("Received message: $json")

        try {
            val msg = JsonParser.parseString(json).asJsonObject
            when (msg.get("type").asString) {
                "request" -> handleRequest(msg)
                "subscribe" -> handleSubscription(msg)
                "unsubscribe" -> handleUnsubscribe(msg)
                else -> Timber.w("ContainerBridge: unknown message type in: $json")
            }
        } catch (e: Exception) {
            Timber.e(e, "ContainerBridge: failed to parse message: $json")
        }
    }

    private fun handleRequest(msg: JsonObject) {
        val id = msg.get("id").asString
        val method = msg.get("method").asString
        val paramsElement = msg.get("params")

        val entry = requestHandlers[method]
        if (entry == null) {
            respondError(id, "Unknown method: $method")
            return
        }

        scope.launch {
            try {
                val params = gson.fromJson<Any>(paramsElement, entry.paramsType)
                entry.handler(params)
                    .onSuccess { success -> respondValue(id, gson.toJson(success)) }
                    .onFailure { respondError(id, it) }
            } catch (e: JsonSyntaxException) {
                Timber.e(e, "ContainerBridge: failed to parse params for $method")
                respondError(id, "Invalid params: ${e.message}")
            } catch (e: Exception) {
                Timber.e(e, "ContainerBridge: handler failed for $method")
                respondError(id, e)
            }
        }
    }

    private fun handleSubscription(msg: JsonObject) {
        val id = msg.get("id").asString
        val method = msg.get("method").asString
        val paramsElement = msg.get("params")

        val entry = subscriptionHandlers[method]
        if (entry == null) {
            respondError(id, "Unknown subscription: $method")
            return
        }

        try {
            val params = gson.fromJson<Any>(paramsElement, entry.paramsType)
            val flow = entry.handler(params)

            val job = flow
                .onEach { value -> sendUpdate(id, gson.toJson(value)) }
                .onCompletion { cause ->
                    if (cause == null) sendComplete(id)
                    activeSubscriptions.remove(id)
                }
                // The Throwable overload, so a HostCallException thrown while subscribing keeps its
                // code: a subscription that can fail for more than one reason is unreadable without it.
                .catch { respondError(id, it) }
                .launchIn(scope)

            activeSubscriptions[id] = job
        } catch (e: JsonSyntaxException) {
            Timber.e(e, "ContainerBridge: failed to parse subscription params for $method")
            respondError(id, "Invalid params: ${e.message}")
        } catch (e: Exception) {
            Timber.e(e, "ContainerBridge: subscription setup failed for $method")
            respondError(id, e)
        }
    }

    private fun handleUnsubscribe(msg: JsonObject) {
        val id = msg.get("id").asString
        activeSubscriptions.remove(id)?.cancel()
    }

    // ---- JS callback helpers ----

    private fun respondValue(id: String, resultJson: String) {
        Timber.d("Responding to request $id: $resultJson")
        callbackToJs(id, """{"value":$resultJson}""")
    }

    private fun respondError(id: String, error: Throwable) {
        respondError(id, (error as? HostCallException)?.code, error.message ?: "Unknown error")
    }

    private fun respondError(id: String, message: String) {
        respondError(id, code = null, message = message)
    }

    private fun respondError(id: String, code: String?, message: String) {
        Timber.d("Responding to request $id: Error [$code] $message")
        val errorObject = JsonObject().apply {
            if (code != null) addProperty("code", code)
            addProperty("message", message)
        }
        callbackToJs(id, """{"error":${gson.toJson(errorObject)}}""")
    }

    private fun sendUpdate(id: String, updateJson: String) {
        Timber.d("Sending subscription update for $id: $updateJson")
        callbackToJs(id, """{"update":$updateJson}""")
    }

    private fun sendComplete(id: String) {
        Timber.d("Completing subscription $id")
        callbackToJs(id, """{"complete":true}""")
    }

    private fun callbackToJs(id: String, payloadJson: String) {
        val script = "window.__container_callback__(${gson.toJson(id)}, ${gson.toJson(payloadJson)})"
        scope.launch {
            transport.evaluateJs(script)
        }
    }
}
