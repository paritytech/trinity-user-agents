package io.paritytech.polkadotapp.feature_products_impl.domain.bot.menu

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.ReadOnlyChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.DeleteRoomUseCase
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductIntegrationRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.product.IntegrationType
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RealProductChatMenuInteractorTest {
    private val productId = ProductId.fromStoredValue("jollity.dot")
    private val extensionId = productId.toChatExtensionId()
    private val removedRoom = ChatId.forExtensionRoom(extensionId, "game-1")
    private val otherRoom = ChatId.forExtensionRoom(extensionId, "game-2")

    private val deleteRoomUseCase = mockk<DeleteRoomUseCase>()
    private val chatRoomRepository = mockk<ReadOnlyChatRoomRepository>()
    private val integrationRepository = mockk<ProductIntegrationRepository>()

    private val interactor = RealProductChatMenuInteractor(deleteRoomUseCase, chatRoomRepository, integrationRepository)

    private var inTransaction = false
    private val calls = mutableListOf<Pair<String, Boolean>>()

    init {
        coEvery { integrationRepository.withTransaction<Unit>(any()) } coAnswers {
            inTransaction = true
            try {
                firstArg<suspend () -> Unit>().invoke()
            } finally {
                inTransaction = false
            }
        }
        coEvery { deleteRoomUseCase.invoke(removedRoom) } coAnswers { calls += "deleteRoom" to inTransaction }
        coEvery { integrationRepository.uninstall(productId, IntegrationType.Chat) } coAnswers {
            calls += "uninstall" to inTransaction
        }
    }

    @Test
    fun `removing one of several rooms keeps the product in chats`() = runTest {
        coEvery { chatRoomRepository.getRoomsByExtension(extensionId) } coAnswers {
            calls += "getRooms" to inTransaction
            listOf(otherRoom)
        }

        assertTrue(interactor.removeChat(productId, removedRoom).isSuccess)

        assertEquals(listOf("deleteRoom" to true, "getRooms" to true), calls)
    }

    /**
     * The check and both writes share one transaction: an integration left without rooms would boot
     * the product's worker only to recreate the room the user just removed.
     */
    @Test
    fun `removing the last room also removes the product from chats in the same transaction`() = runTest {
        coEvery { chatRoomRepository.getRoomsByExtension(extensionId) } coAnswers {
            calls += "getRooms" to inTransaction
            emptyList()
        }

        assertTrue(interactor.removeChat(productId, removedRoom).isSuccess)

        assertEquals(listOf("deleteRoom" to true, "getRooms" to true, "uninstall" to true), calls)
    }
}
