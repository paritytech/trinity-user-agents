package io.paritytech.polkadotapp.feature_sso_impl.domain

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionId
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequest
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.coroutines.cancellation.CancellationException

private val SESSION = SsoSessionId("session")

class SsoRequestQueueTest {
    private val incoming = Channel<SsoSessionRequest>(Channel.UNLIMITED)
    private val held = mutableMapOf<String, CompletableDeferred<Unit>>()

    private val served = mutableListOf<String>()
    private val interrupted = mutableListOf<String>()
    private val withdrawn = mutableListOf<String>()
    private val handled = mutableSetOf<String>()

    private val queue = SsoRequestQueue(
        serve = { request ->
            served += request.requestId
            try {
                held[request.requestId]?.await()
            } catch (e: CancellationException) {
                interrupted += request.requestId
                throw e
            }
        },
        onWithdrawn = { withdrawn += it.requestId },
        onCancel = { _, _ -> },
        isHandled = { _, messageId -> messageId in handled },
    )

    @Test
    fun `requests wait their turn when an earlier one is still being served`() = runTest {
        start()
        val first = withHeldRequest("a")

        receive(request("a"), request("b"))
        assertServed("a")

        first.complete(Unit)
        runCurrent()
        assertServed("a", "b")
    }

    @Test
    fun `a request stops and the next one is served when it is withdrawn while being served`() = runTest {
        start()
        withHeldRequest("a")

        receive(request("a"), request("b"))
        receive(cancel("a"))

        assertEquals(listOf("a"), interrupted)
        assertWithdrawn("a")
        assertServed("a", "b")
    }

    @Test
    fun `a queued request is never served when it is withdrawn behind the one being served`() = runTest {
        start()
        val first = withHeldRequest("a")

        receive(request("a"), request("b"), cancel("b"))
        first.complete(Unit)
        runCurrent()

        assertServed("a")
        assertWithdrawn("b")
    }

    @Test
    fun `a request is never served when it is withdrawn before it arrives`() = runTest {
        start()

        receive(cancel("a"), request("a"))

        assertServed()
        assertWithdrawn("a")
    }

    @Test
    fun `a request is still served when the cancel names it from another session`() = runTest {
        start()

        receive(cancel("a", session = SsoSessionId("other")), request("a"))

        assertServed("a")
    }

    @Test
    fun `the oldest early withdrawal is forgotten when more than the bound arrive`() = runTest {
        start()

        receive(*Array(65) { cancel("$it") })
        receive(request("0"), request("64"))

        assertServed("0")
        assertWithdrawn("64")
    }

    @Test
    fun `an early withdrawal survives later Cancels when they name requests already answered`() = runTest {
        start()
        receive(cancel("x"))
        handled += List(64) { "answered-$it" }

        receive(*Array(64) { cancel("answered-$it") })
        receive(request("x"))

        assertServed()
        assertWithdrawn("x")
    }

    /** Each collection serves its own requests, so a second one must not hide what the first is serving. */
    @Test
    fun `a cancel stops the running request when another collection is serving at the same time`() = runTest {
        val other = Channel<SsoSessionRequest>(Channel.UNLIMITED)
        start()
        backgroundScope.launch { queue.process(other.receiveAsFlow()).collect() }
        withHeldRequest("a")
        withHeldRequest("b")

        receive(request("a"))
        other.trySend(request("b"))
        runCurrent()
        receive(cancel("a"))

        assertEquals(listOf("a"), interrupted)
    }

    private fun TestScope.start() {
        backgroundScope.launch { queue.process(incoming.receiveAsFlow()).collect() }
    }

    private fun withHeldRequest(requestId: String) = CompletableDeferred<Unit>().also { held[requestId] = it }

    private fun TestScope.receive(vararg requests: SsoSessionRequest) {
        requests.forEach { incoming.trySend(it) }
        runCurrent()
    }

    private fun assertServed(vararg requestIds: String) = assertEquals(requestIds.toList(), served)

    private fun assertWithdrawn(vararg requestIds: String) = assertEquals(requestIds.toList(), withdrawn)

    private fun request(id: String) = SsoSessionRequest(
        sessionId = SESSION,
        requestId = id,
        content = SsoSessionRequest.Content.ProductSubtreeRequest(ProductId.fromStoredValue("browse.dot")),
    )

    private fun cancel(target: String, session: SsoSessionId = SESSION) = SsoSessionRequest(
        sessionId = session,
        requestId = "cancel-$target",
        content = SsoSessionRequest.Content.Cancel(target),
    )
}
