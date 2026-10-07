package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.chat

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class TrUAPIChatHostTest {
    private val events = mutableListOf<String>()
    private val surfaces = TrUAPIChatSurfaces()
    private val bridge = surfaces.bridgeFor(PRODUCT)

    private val runtime = mockk<TrUAPIHostRuntime> {
        every { acquireWorker(PRODUCT.value) } answers {
            // The reference boots the worker, whose first chat call must already find its chat.
            val chatBound = runCatching { runBlocking { bridge.listRooms() } }.isSuccess
            events += if (chatBound) "acquire:bound" else "acquire:unbound"
        }
        every { releaseWorker(PRODUCT.value) } answers { events += "release" }
    }

    private val messaging = mockk<ProductChatMessaging> {
        every { subscribeChatRooms() } returns flowOf(emptyList())
    }

    private val host = TrUAPIChatHost(
        runtimeProvider = mockk<TrUAPIHostRuntimeProvider> { coEvery { runtime() } returns Result.success(runtime) },
        workers = mockk(),
        surfaces = surfaces,
    )

    @Test
    fun `serving holds one worker reference with the chat bound, and stopping returns both`() = runTest {
        var attached: ProductWorker? = null

        val serving = launch { host.serve(PRODUCT, PRODUCT.toChatExtensionId(), messaging) { attached = it } }
        advanceUntilIdle()

        assertEquals(listOf("acquire:bound"), events)
        assertNotNull(attached)

        serving.cancel()
        advanceUntilIdle()

        assertEquals(listOf("acquire:bound", "release"), events)
        assertNull(attached)
        assertTrue(runCatching { bridge.listRooms() }.isFailure)
    }

    private companion object {
        val PRODUCT = ProductId.fromStoredValue("counter.paseo")
    }
}
