package io.paritytech.polkadotapp.feature_products_impl.domain.productBotManagement

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.DebugPocketCard
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.DebugPocketCards
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import java.util.concurrent.Executors

class RealProductBotManagementInteractorTest {
    private val ioThread = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "cards-io")
    }

    private val dispatchers = object : CoroutineDispatchers {
        override val main: CoroutineDispatcher = Dispatchers.Unconfined
        override val io: CoroutineDispatcher = ioThread.asCoroutineDispatcher()
        override val computation: CoroutineDispatcher = Dispatchers.Unconfined
    }

    private var readThread: String? = null

    private val debugPocketCards = object : DebugPocketCards {
        override fun get(productId: ProductId): DebugPocketCard? {
            readThread = Thread.currentThread().name

            return CARD
        }

        override fun set(productId: ProductId, card: DebugPocketCard?) = Unit
    }

    private val interactor = RealProductBotManagementInteractor(
        productRepository = mock(),
        integrationRepository = mock(),
        botStateController = mock(),
        resolveProductUseCase = mock(),
        uninstallProductUseCase = mock(),
        dotNsTldProvider = mock(),
        debugPocketCards = debugPocketCards,
        runtimeSettings = mock(),
        dispatchers = dispatchers,
    )

    /**
     * The card lives in preferences, whose first read loads the file from disk. The edit dialog asks
     * for it on the main thread, so read there it stalls the frame that opens the dialog.
     */
    @Test
    fun `the card is read off the caller's thread`() = runBlocking {
        val caller = Thread.currentThread().name

        val card = interactor.getDebugCard(PRODUCT)

        assertEquals(CARD, card)
        // Coroutine debug mode appends "@coroutine#n" to the thread's name.
        assertTrue("read on $readThread", readThread.orEmpty().startsWith("cards-io"))
        assertNotEquals(caller, readThread)
    }

    private companion object {
        val PRODUCT = ProductId.fromStoredValue("coinflip.dot")

        val CARD = DebugPocketCard(
            cardId = PocketCardId("loyalty"),
            title = "Loyalty",
            previewUrl = "http://127.0.0.1:5173/face.json",
        )
    }
}
