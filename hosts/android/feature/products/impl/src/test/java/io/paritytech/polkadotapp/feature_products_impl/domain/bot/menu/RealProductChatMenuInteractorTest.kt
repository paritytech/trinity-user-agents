package io.paritytech.polkadotapp.feature_products_impl.domain.bot.menu

import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.ReadOnlyChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.DeleteRoomUseCase
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductIntegrationRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.product.IntegrationType
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertTrue
import org.junit.Test

class RealProductChatMenuInteractorTest {
    private val productId = ProductId.fromStoredValue("jollity.dot")
    private val extensionId = productId.toChatExtensionId()
    private val removedRoom = ChatId.forExtensionRoom(extensionId, "game-1")
    private val otherRoom = ChatId.forExtensionRoom(extensionId, "game-2")

    private val deleteRoomUseCase = mockk<DeleteRoomUseCase>(relaxUnitFun = true)
    private val chatRoomRepository = mockk<ReadOnlyChatRoomRepository>()
    private val integrationRepository = mockk<ProductIntegrationRepository>(relaxUnitFun = true)

    private val interactor = RealProductChatMenuInteractor(deleteRoomUseCase, chatRoomRepository, integrationRepository)

    @Test
    fun `removing one of several rooms keeps the product in chats`() = runTest {
        every { chatRoomRepository.subscribeRoomsByExtension(extensionId) } returns flowOf(listOf(otherRoom))

        assertTrue(interactor.removeChat(productId, removedRoom).isSuccess)

        coVerify { deleteRoomUseCase.invoke(removedRoom) }
        coVerify(exactly = 0) { integrationRepository.uninstall(productId, IntegrationType.Chat) }
    }

    @Test
    fun `removing the last room also removes the product from chats`() = runTest {
        every { chatRoomRepository.subscribeRoomsByExtension(extensionId) } returns flowOf(emptyList())

        assertTrue(interactor.removeChat(productId, removedRoom).isSuccess)

        coVerifyOrder {
            deleteRoomUseCase.invoke(removedRoom)
            integrationRepository.uninstall(productId, IntegrationType.Chat)
        }
    }
}
