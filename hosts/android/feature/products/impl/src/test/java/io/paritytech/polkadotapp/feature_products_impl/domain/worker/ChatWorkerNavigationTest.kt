package io.paritytech.polkadotapp.feature_products_impl.domain.worker

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
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ChatWorkerNavigationTest {
    private val game = ProductId.fromStoredValue("game.dot")
    private val destination = mockk<Uri>()
    private val handled = mutableListOf<Uri>()
    private var activeChat: ChatId? = ChatId.forExtension(game.toChatExtensionId())

    // A card button is tapped from inside the product's chat, so its worker may move the person.
    @Test
    fun `a worker of the chat on screen navigates through the deeplink handler`() = runTest {
        navigation().policy(game, this).handleNavigation(DotNsNavigationType.CROSS_DOTNS_DOMAIN, destination)
        advanceUntilIdle()

        assertEquals(listOf(destination), handled)
    }

    // The script keeps running while its chat is closed; a background bot must not pull the person
    // off another screen.
    @Test
    fun `a worker whose chat is not on screen navigates nowhere`() = runTest {
        activeChat = ChatId.forExtension(ProductId.fromStoredValue("other.dot").toChatExtensionId())

        navigation().policy(game, this).handleNavigation(DotNsNavigationType.CROSS_DOTNS_DOMAIN, destination)
        advanceUntilIdle()

        assertEquals(emptyList<Uri>(), handled)
    }

    private fun navigation() = ChatWorkerNavigation(
        deepLinkHandler = recordingHandler,
        chatActiveTracker = mockk { every { getActive() } answers { activeChat } },
        appNotifier = mockk(),
        context = mockk(),
    )

    // Hand-written: `handle` takes a context receiver, which mockk cannot stub.
    private val recordingHandler = object : DeepLinkHandler {
        override suspend fun canHandle(data: Uri) = true

        context(scope: ComputationalScope)
        override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> {
            handled += data
            return Result.success(DeeplinkProcessingOutcome.NoOp)
        }
    }
}
