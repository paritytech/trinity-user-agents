package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import android.net.Uri
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsNavigationType
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class TrUAPIWorkerNavigationTest {
    private val destination = mockk<Uri>()
    private var chatOnScreen: ChatId? = null
    private val opened = mutableListOf<Uri>()

    // Not mockk: kotlin-reflect cannot map handle, a context-parameter function returning a value class.
    private val deepLinkHandler = object : DeepLinkHandler {
        override suspend fun canHandle(data: Uri) = true

        context(scope: ComputationalScope)
        override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> {
            opened += data
            return Result.success(DeeplinkProcessingOutcome.NoOp)
        }
    }

    @Test
    fun `a card in the product's chat opens the page it names`() = runTest {
        chatOnScreen = ChatId.forExtension(PRODUCT.toChatExtensionId())

        navigate()
        advanceUntilIdle()

        assertEquals(listOf(destination), opened)
    }

    @Test
    fun `a card in a room of the product's chat opens the page it names`() = runTest {
        chatOnScreen = ChatId.forExtensionRoom(PRODUCT.toChatExtensionId(), "lobby")

        navigate()
        advanceUntilIdle()

        assertEquals(listOf(destination), opened)
    }

    @Test
    fun `a worker cannot navigate while no chat is on screen`() = runTest {
        chatOnScreen = null

        assertThrows(IllegalStateException::class.java) { navigate() }
        advanceUntilIdle()

        assertEquals(emptyList<Uri>(), opened)
    }

    @Test
    fun `a worker cannot navigate while another product's chat is on screen`() = runTest {
        chatOnScreen = ChatId.forExtension(ProductId.fromStoredValue("other.dot").toChatExtensionId())

        assertThrows(IllegalStateException::class.java) { navigate() }
        advanceUntilIdle()

        assertEquals(emptyList<Uri>(), opened)
    }

    private fun TestScope.navigate() {
        TrUAPIWorkerNavigation(
            deepLinkHandler = deepLinkHandler,
            chatActiveTracker = mockk { every { getActive() } answers { chatOnScreen } },
            appNotifier = mockk(),
            context = mockk(),
        ).policy(PRODUCT, this).handleNavigation(DotNsNavigationType.SAME_DOTNS_DOMAIN, destination)
    }

    private companion object {
        val PRODUCT = ProductId.fromStoredValue("game.dot")
    }
}
