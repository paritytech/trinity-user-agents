package io.paritytech.polkadotapp.feature_statement_store_impl.data

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

class RealStatementStorePeerTest {
    private val peer = peer(GRACE)

    private fun peer(grace: Duration) = RealStatementStorePeer(
        KnownChains(people = "people", assetHub = "hub", bulletIn = "bulletin", hydration = null),
        handoverGrace = grace,
    )

    @Test
    fun `an open subscription that has sent nothing is not answered`() = runBlocking<Unit> {
        val subscription = Subscription()

        subscription.open {
            assertFalse(answered())
        }
    }

    @Test
    fun `a page from the peer answers the subscription`() = runBlocking<Unit> {
        val subscription = Subscription()

        subscription.open {
            subscription.sendPage()

            assertTrue(answered())
        }
    }

    @Test
    fun `a failed page does not answer the subscription`() = runBlocking<Unit> {
        val subscription = Subscription()

        subscription.open {
            subscription.fail()

            assertFalse(answered())
        }
    }

    @Test
    fun `closing the last answered subscription leaves nothing answered`() = runBlocking<Unit> {
        val subscription = Subscription()

        subscription.open { subscription.sendPage() }

        assertFalse(answered())
    }

    @Test
    fun `a second page does not answer the subscription again`() = runBlocking<Unit> {
        val subscription = Subscription()

        subscription.open {
            subscription.sendPage()
            subscription.sendPage()
        }

        assertFalse(answered())
    }

    @Test
    fun `one answered subscription is enough while another is still waiting`() = runBlocking<Unit> {
        val answering = Subscription()
        val quiet = Subscription()

        answering.open {
            quiet.open {
                answering.sendPage()

                assertTrue(answered())
            }

            assertTrue(answered())
        }

        assertFalse(answered())
    }

    @Test
    fun `the first reading does not wait out the grace`() = runBlocking<Unit> {
        val slowToGiveUp = peer(grace = TIMEOUT * 2)

        assertFalse(withTimeout(TIMEOUT) { slowToGiveUp.observeAnswered().first() })
    }

    @Test
    fun `a subscription handing over to its replacement is not a loss`() = runBlocking<Unit> {
        val first = Subscription()
        val second = Subscription()
        val attached = CompletableDeferred<Unit>()
        first.start(this)
        first.sendPage()

        val seen = mutableListOf<Boolean>()
        val watching = launch { peer.observeAnswered().collect { seen += it; attached.complete(Unit) } }
        attached.await()

        first.stop()
        second.start(this)
        second.sendPage()
        delay(GRACE * 3)

        assertEquals(listOf(true), seen)

        second.stop()
        delay(GRACE * 3)
        watching.cancelAndJoin()

        assertEquals(listOf(true, false), seen)
    }

    private suspend fun answered(): Boolean = peer.observeAnswered().first()

    // Driven page by page so every assertion runs after the page landed.
    private inner class Subscription {
        private val pages = MutableSharedFlow<Result<String>>()
        private var delivered = CompletableDeferred<Unit>()
        private var collection: Job? = null

        suspend fun open(assertions: suspend () -> Unit) = coroutineScope {
            start(this)

            try {
                assertions()
            } finally {
                stop()
            }
        }

        fun start(scope: CoroutineScope) {
            collection = scope.launch { peer.track(pages).collect { delivered.complete(Unit) } }
        }

        suspend fun awaitOpen() = pages.subscriptionCount.first { it > 0 }

        suspend fun stop() {
            collection?.let { close(it) }
            collection = null
        }

        suspend fun sendPage() {
            awaitOpen()
            deliver(Result.success("page"))
        }

        suspend fun fail() {
            awaitOpen()
            deliver(Result.failure(IllegalStateException("unreadable page")))
        }

        private suspend fun deliver(page: Result<String>) {
            delivered = CompletableDeferred()
            pages.emit(page)
            delivered.await()
        }

        private suspend fun close(collection: Job) {
            collection.cancel()
            collection.join()
        }
    }

    private companion object {
        // An order above the coroutine handover it has to outlast, so a JIT or GC stall cannot fail it.
        val GRACE = 300.milliseconds
        val TIMEOUT = 5.seconds
    }
}
