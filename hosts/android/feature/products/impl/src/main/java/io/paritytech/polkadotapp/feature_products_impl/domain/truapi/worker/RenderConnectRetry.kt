package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.retryWhen
import timber.log.Timber
import uniffi.truapi.ProductRendererRenderRequest
import uniffi.truapi.ProductRuntimeException
import uniffi.truapi.RenderContext
import uniffi.truapi.RendererNode
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

private const val CONNECT_ATTEMPTS = 40L
private val CONNECT_RETRY_DELAY = 250.milliseconds

// Doubling from a second up to half a minute: a worker that is simply slow is picked up at once,
// and one that is broken is not asked on a loop for as long as its body is on screen.
private val REOPEN_DELAY = 1.seconds
private val MAX_REOPEN_DELAY = 30.seconds
private const val MAX_BACKOFF_SHIFT = 5L

/** The canonical nodes one render context draws, waiting out the worker connection. */
internal fun TrUAPIProductExecution.renderNodes(context: RenderContext, payload: ByteArray): Flow<RendererNode> =
    render(ProductRendererRenderRequest(context, payload))
        .retryWhileConnecting(CONNECT_ATTEMPTS, CONNECT_RETRY_DELAY)

/** The widgets one render context draws on this worker execution, waiting out its connect. */
internal fun TrUAPIProductExecution.renderWidgets(context: RenderContext, payload: ByteArray): Flow<JsWidget> =
    renderNodes(context, payload)
        .map { it.toJsWidget() }

/**
 * A worker's script is loaded before its client has opened the socket to the core, so a render
 * asked for in that window is refused as not connected. Such a refusal is retried; anything else
 * is a failure of the stream.
 */
internal fun <T> Flow<T>.retryWhileConnecting(attempts: Long, delay: Duration): Flow<T> =
    retryWhen { cause, attempt ->
        val connecting = cause is ProductRuntimeException.NotConnected && attempt < attempts
        if (connecting) delay(delay)
        connecting
    }

/**
 * A render that fails is opened again on the same worker. Ending instead would leave the body
 * static for the worker's whole life: a new render only opens when a different execution is
 * published, and a stop publishes none.
 */
internal fun <T> Flow<T>.reopenAfterStreamEnd(label: String): Flow<T> = retryWhen { failure, attempt ->
    Timber.w(failure, "%s stream ended, reopening", label)
    delay(reopenDelay(attempt))
    true
}

private fun reopenDelay(attempt: Long): Duration =
    minOf(REOPEN_DELAY * (1 shl attempt.coerceAtMost(MAX_BACKOFF_SHIFT).toInt()), MAX_REOPEN_DELAY)
