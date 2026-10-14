package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import dagger.Lazy
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.BindableProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.job
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import java.util.concurrent.atomic.AtomicInteger

class ProductWorkerRefCounterTest {
    private val productId = ProductId.fromStoredValue("coinflip.dot")
    private val hostApiInteractor: HostApiInteractor = mock()

    private class FakeWorker : ProductWorker {
        override suspend fun onUserMessage(roomId: ProductChatIdParameter?, text: String): Result<Unit> = Result.success(Unit)
        override fun renderMessage(roomId: ProductChatIdParameter?, messageId: ChatMessageId, messageType: String, messageData: DataByteArray): Flow<Result<JsWidget>> = emptyFlow()
        override fun dispatchEvent(event: JsUiEvent) = Unit
    }

    private class FakeBootFactory(var gate: CompletableDeferred<Unit>? = null) : WorkerBootFactory {
        val started = AtomicInteger(0)
        val disposed = AtomicInteger(0)
        val bootEntered = CompletableDeferred<Unit>()
        var bootFailure: Throwable? = null

        override suspend fun boot(
            productId: ProductId,
            botApi: BindableProductsBotApi,
            scope: CoroutineScope,
        ): ProductWorker {
            bootEntered.complete(Unit)
            gate?.await()
            bootFailure?.let { throw it }
            started.incrementAndGet()
            scope.coroutineContext.job.invokeOnCompletion { disposed.incrementAndGet() }
            return FakeWorker()
        }
    }

    private fun TestScope.counter(factory: FakeBootFactory): RealProductWorkerRefCounter {
        val dispatcher = StandardTestDispatcher(testScheduler)
        val dispatchers = object : CoroutineDispatchers {
            override val main: CoroutineDispatcher = dispatcher
            override val io: CoroutineDispatcher = dispatcher
            override val computation: CoroutineDispatcher = dispatcher
        }
        return RealProductWorkerRefCounter(hostApiInteractor, Lazy { factory }, dispatchers)
    }

    @Test
    fun `first acquire starts once for concurrent consumers and last release disposes`() = runTest {
        val factory = FakeBootFactory()
        val counter = counter(factory)

        val a = counter.acquire(productId, "a")
        val b = counter.acquire(productId, "b")
        advanceUntilIdle()

        assertEquals(1, factory.started.get())
        assertEquals(0, factory.disposed.get())

        a.release()
        advanceUntilIdle()
        assertEquals(0, factory.disposed.get()) // b still holds

        b.release()
        advanceUntilIdle()
        assertEquals(1, factory.disposed.get())
    }

    @Test
    fun `release is idempotent and does not drop another holder's claim`() = runTest {
        val factory = FakeBootFactory()
        val counter = counter(factory)

        val a = counter.acquire(productId, "a")
        val b = counter.acquire(productId, "b")
        advanceUntilIdle()

        a.release()
        a.release() // double release must be a no-op
        advanceUntilIdle()
        assertEquals("worker still wanted by b", 0, factory.disposed.get())

        b.release()
        advanceUntilIdle()
        assertEquals(1, factory.disposed.get())
    }

    @Test
    fun `churn never leaves started count above disposed count and converges`() = runTest {
        val factory = FakeBootFactory()
        val counter = counter(factory)

        repeat(20) {
            val reference = counter.acquire(productId, "churn")
            advanceUntilIdle()
            reference.release()
            advanceUntilIdle()
        }

        assertEquals(factory.started.get(), factory.disposed.get())
    }

    @Test
    fun `a start that finishes after the product is unwanted disposes the worker`() = runTest {
        val gate = CompletableDeferred<Unit>()
        val factory = FakeBootFactory(gate)
        val counter = counter(factory)

        val reference = counter.acquire(productId, "fast")
        advanceUntilIdle()
        factory.bootEntered.await() // boot is in flight, suspended at the gate
        reference.release() // becomes unwanted before boot finishes
        gate.complete(Unit)
        advanceUntilIdle()

        assertEquals(1, factory.started.get())
        assertEquals(1, factory.disposed.get())
    }

    @Test
    fun `a failed boot surfaces its cause and is not replayed on the next acquire`() = runTest {
        val factory = FakeBootFactory().apply { bootFailure = IllegalStateException("core down") }
        val counter = counter(factory)

        val first = counter.acquire(productId, "open")
        assertEquals("core down", runCatching { first.worker() }.exceptionOrNull()?.cause?.message)
        first.release()
        advanceUntilIdle()

        val gate = CompletableDeferred<Unit>()
        factory.bootFailure = null
        factory.gate = gate
        val second = counter.acquire(productId, "reopen")
        val reopened = async { second.worker() }
        advanceUntilIdle()

        assertTrue(reopened.isActive)
        gate.complete(Unit)
        advanceUntilIdle()
        assertTrue(reopened.await() is FakeWorker)
        second.release()
    }

    @Test
    fun `a cancellation raised by the boot itself does not kill the lifecycle chain`() = runTest {
        // A scope the boot awaits may cancel without this handle being cancelled; that is a boot
        // failure, not our cancellation, and the chain must survive it.
        val factory = FakeBootFactory().apply { bootFailure = CancellationException("provider scope died") }
        val counter = counter(factory)

        val first = counter.acquire(productId, "open")
        assertTrue(runCatching { first.worker() }.exceptionOrNull() is IllegalStateException)
        first.release()
        advanceUntilIdle()

        factory.bootFailure = null
        val second = counter.acquire(productId, "reopen")
        advanceUntilIdle()

        assertTrue(second.worker() is FakeWorker)
        second.release()
    }

    @Test
    fun `a failed boot is not replayed while another holder keeps the count above zero`() = runTest {
        val factory = FakeBootFactory().apply { bootFailure = IllegalStateException("core down") }
        val counter = counter(factory)

        val keepAlive = counter.acquire(productId, "keepalive")
        val first = counter.acquire(productId, "open")
        assertEquals("core down", runCatching { first.worker() }.exceptionOrNull()?.cause?.message)

        // The count never reaches zero, so dispose() never runs and only boot() can clear the failure.
        first.release()
        val gate = CompletableDeferred<Unit>()
        factory.bootFailure = null
        factory.gate = gate
        val second = counter.acquire(productId, "reopen")
        val reopened = async { second.worker() }
        advanceUntilIdle()

        assertTrue(reopened.isActive)
        gate.complete(Unit)
        advanceUntilIdle()
        assertTrue(reopened.await() is FakeWorker)
        second.release()
        keepAlive.release()
    }

    @Test
    fun `a chat closed and reopened in the same tick retries a failed boot`() = runTest {
        val factory = FakeBootFactory().apply { bootFailure = IllegalStateException("core down") }
        val counter = counter(factory)

        val first = counter.acquire(productId, "open")
        assertEquals("core down", runCatching { first.worker() }.exceptionOrNull()?.cause?.message)

        factory.bootFailure = null
        first.release()
        val second = counter.acquire(productId, "reopen")
        advanceUntilIdle()

        assertTrue(second.worker() is FakeWorker)
        second.release()
    }

    @Test
    fun `a product whose worker cannot boot still counts its references`() = runTest {
        val factory = FakeBootFactory().apply { bootFailure = IllegalStateException("no worker script") }
        val counter = counter(factory)

        val reference = counter.acquire(productId, "keepalive")
        advanceUntilIdle()

        assertEquals(0, factory.started.get())
        reference.release()
        advanceUntilIdle()
    }

    @Test
    fun `driving acquisition waits for boot and returns the running worker`() = runTest {
        val factory = FakeBootFactory()
        val counter = counter(factory)

        val reference = counter.acquire(productId, "chat")
        advanceUntilIdle()
        val worker = reference.worker()

        assertNotNull(worker)
        assertEquals(1, factory.started.get())

        reference.release()
        advanceUntilIdle()
        assertEquals(1, factory.disposed.get())
    }

    @Test
    fun `the bot api for a product is the same instance across calls`() = runTest {
        val factory = FakeBootFactory()
        val counter = counter(factory)

        val first = counter.chatMessaging(productId)
        val second = counter.chatMessaging(productId)

        assertSame(first, second)
    }

    @Test
    fun `asking for a bot api does not boot a worker`() = runTest {
        val factory = FakeBootFactory()
        val counter = counter(factory)

        counter.chatMessaging(productId)
        advanceUntilIdle()

        assertEquals(0, factory.started.get())
    }
}
