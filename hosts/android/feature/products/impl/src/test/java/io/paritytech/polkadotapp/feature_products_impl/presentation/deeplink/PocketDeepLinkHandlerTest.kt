package io.paritytech.polkadotapp.feature_products_impl.presentation.deeplink

import android.content.Context
import android.net.Uri
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.PocketDeeplink
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.PocketDeeplinkAction
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.PocketDeeplinkParser
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ProductId
import java.io.IOException

class PocketDeepLinkHandlerTest {
    private val collectionLink = "polkadotapp://pocket"
    private val malformedLink = "polkadotapp://pocket/add"
    private val cardLink = "polkadotapp://pocket/add?product=game.paseo&card=loyalty"

    // A fresh install with the People chain unreachable: no TLD is known yet.
    private val tldProvider = mockk<DotNsTldProvider> {
        every { currentTldOrNull() } returns null
        coEvery { getTld() } returns Result.failure(IOException("people chain unreachable"))
    }

    private fun handler(canParse: Boolean = true) = PocketDeepLinkHandler(
        dispatchers = TestCoroutineDispatchers(Dispatchers.Unconfined),
        dotNsTldProvider = tldProvider,
        parser = mockk<PocketDeeplinkParser> {
            every { this@mockk.canParse } returns canParse
            every { parse(collectionLink) } returns PocketDeeplink.Collection
            every { parse(malformedLink) } returns null
            every { parse(cardLink) } returns PocketDeeplink.Card(
                productId = ProductId.parse("game.paseo").getOrThrow(),
                action = PocketDeeplinkAction.ADD,
                cardId = "loyalty",
                pageQuery = "",
            )
        },
        publishedCards = mockk(),
        collection = mockk(),
        cardOpenRequests = mockk(),
        gate = mockk(),
        accountRepository = mockk<AccountRepository>(relaxed = true),
        router = mockk(),
        context = mockk<Context> { every { getString(any()) } returns "malformed" },
    )

    @Test
    fun `the collection opens before the network's TLD is known`() = runBlocking {
        val handler = handler()
        val data = pocketUri(collectionLink)

        assertTrue(handler.canHandle(data))
        assertTrue(handle(handler, data).getOrNull() is DeeplinkProcessingOutcome.Navigate)
        coVerify(exactly = 0) { tldProvider.getTld() }
    }

    @Test
    fun `a malformed Pocket link is answered before the network's TLD is known`() = runBlocking {
        val handler = handler()
        val data = pocketUri(malformedLink)

        assertTrue(handler.canHandle(data))
        assertTrue(handle(handler, data).getOrNull() is DeeplinkProcessingOutcome.ShowMessage)
    }

    // A card names a product, which only the network's TLD can resolve, so it waits for that TLD.
    @Test
    fun `a card link is declined before the network's TLD is known`() = runBlocking {
        assertFalse(handler().canHandle(pocketUri(cardLink)))
    }

    @Test
    fun `no Pocket link is claimed without the bundled stack`() = runBlocking {
        assertFalse(handler(canParse = false).canHandle(pocketUri(collectionLink)))
    }

    private fun pocketUri(link: String) = mockk<Uri> {
        every { scheme } returns DeepLinkHandler.APP_SCHEME
        every { host } returns "pocket"
        every { this@mockk.toString() } returns link
    }

    private suspend fun handle(handler: PocketDeepLinkHandler, data: Uri) =
        coroutineScope { with(ComputationalScope(this)) { handler.handle(data) } }
}
