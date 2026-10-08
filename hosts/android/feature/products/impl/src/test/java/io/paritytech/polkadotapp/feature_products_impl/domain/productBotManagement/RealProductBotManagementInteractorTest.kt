package io.paritytech.polkadotapp.feature_products_impl.domain.productBotManagement

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.DebugPocketCard
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.DebugPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.product.UninstallProductUseCase
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
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
    private var storedAppUrl: String? = null

    private val debugPocketCards = object : DebugPocketCards {
        override fun get(productId: ProductId): DebugPocketCard? {
            readThread = Thread.currentThread().name

            return CARD
        }

        override fun set(productId: ProductId, card: DebugPocketCard?) = Unit

        override fun appUrl(productId: ProductId): String? {
            readThread = Thread.currentThread().name

            return storedAppUrl
        }

        override fun setAppUrl(productId: ProductId, appUrl: String?) {
            storedAppUrl = appUrl
        }
    }

    private val uninstallProductUseCase: UninstallProductUseCase = mock()

    private val interactor = RealProductBotManagementInteractor(
        productRepository = mock(),
        integrationRepository = mock(),
        botStateController = mock(),
        resolveProductUseCase = mock(),
        uninstallProductUseCase = uninstallProductUseCase,
        dotNsTldProvider = mock(),
        debugPocketCards = debugPocketCards,
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

    @Test
    fun `the app url is read off the caller's thread`() = runBlocking {
        storedAppUrl = APP_URL

        val appUrl = interactor.getAppUrl(PRODUCT)

        assertEquals(APP_URL, appUrl)
        assertTrue("read on $readThread", readThread.orEmpty().startsWith("cards-io"))
    }

    /** The page is what the card opens, so saving the product must save where that page comes from. */
    @Test
    fun `updating a product stores the app url it was given`() = runBlocking {
        interactor.updateProduct(PRODUCT, WORKER_URL, "Coinflip", CARD, APP_URL)

        assertEquals(APP_URL, storedAppUrl)
    }

    @Test
    fun `clearing the app url removes it`() = runBlocking {
        storedAppUrl = APP_URL

        interactor.updateProduct(PRODUCT, WORKER_URL, "Coinflip", CARD, appUrl = null)

        assertNull(storedAppUrl)
    }

    @Test
    fun `adding a product stores the app url it was given`() = runBlocking {
        interactor.upsertProduct(PRODUCT, WORKER_URL, "Coinflip", CARD, APP_URL)

        assertEquals(APP_URL, storedAppUrl)
    }

    // Cleartext loads only from 127.0.0.1, so any other url would save cleanly and then never be served.
    @Test
    fun `updating a product with an app url the device cannot load is refused`() = runBlocking {
        val result = interactor.updateProduct(PRODUCT, WORKER_URL, "Coinflip", CARD, "http://localhost:5173/")

        assertTrue(result.isFailure)
        assertNull(storedAppUrl)
    }

    @Test
    fun `adding a product with an app url the device cannot load is refused`() = runBlocking {
        val result = interactor.upsertProduct(PRODUCT, WORKER_URL, "Coinflip", CARD, "https://127.0.0.1:5173/")

        assertTrue(result.isFailure)
        assertNull(storedAppUrl)
    }

    // The app url is read for every page of its product id, so one left behind would keep serving a
    // deleted debug product from the developer's machine.
    @Test
    fun `deleting a product forgets its app url`() = runBlocking {
        storedAppUrl = APP_URL
        whenever(uninstallProductUseCase(PRODUCT)).thenReturn(Result.success(Unit))

        interactor.deleteProduct(PRODUCT)

        assertNull(storedAppUrl)
    }

    private companion object {
        const val APP_URL = "http://127.0.0.1:5173/"
        const val WORKER_URL = "http://127.0.0.1:5173/worker.js"

        val PRODUCT = ProductId.fromStoredValue("coinflip.dot")

        val CARD = DebugPocketCard(
            cardId = PocketCardId("loyalty"),
            title = "Loyalty",
            previewUrl = "http://127.0.0.1:5173/face.json",
            faceShown = true,
        )
    }
}
