package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatReactionDetailModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import org.junit.Assert.assertEquals
import org.junit.Test

class ProductReactionDetailsTest {
    private val hostStore = ChatRoomPolicy.Defaults
    private val productSource = ChatRoomPolicy.Defaults.copy(messageStore = ChatRoomPolicy.MessageStore.PRODUCT)

    @Test
    fun `a declared room asks its product until the product answers NotAvailable`() {
        val declared = productSource.copy(offersReactionDetails = true)

        assertEquals(ReactionDetailsRoute.PRODUCT, declared.reactionDetailsRoute(productAnsweredNotAvailable = false))
        assertEquals(ReactionDetailsRoute.NONE, declared.reactionDetailsRoute(productAnsweredNotAvailable = true))
    }

    @Test
    fun `a room that declares nothing lists the rows of the host only when the host keeps the messages`() {
        assertEquals(ReactionDetailsRoute.LOCAL_ROWS, (null as ChatRoomPolicy?).reactionDetailsRoute(productAnsweredNotAvailable = false))
        assertEquals(ReactionDetailsRoute.LOCAL_ROWS, hostStore.reactionDetailsRoute(productAnsweredNotAvailable = false))
        assertEquals(ReactionDetailsRoute.NONE, productSource.reactionDetailsRoute(productAnsweredNotAvailable = false))
    }

    @Test
    fun `the reactions are grouped by emoji in the order of the product`() {
        val reactions = listOf(
            ChatReactionDetailModel("alice", "👍"),
            ChatReactionDetailModel("user", "🎉"),
            ChatReactionDetailModel("bob", "👍"),
        )
        val participants = listOf("alice", "bob", "user").associateWith { participant(it) }

        val details = reactions.toReactionDetails("m1", timestamp = 5L, participants)

        assertEquals(3, details.totalReactionsCount)
        assertEquals(listOf("👍", "🎉"), details.reactionsByEmoji.map { it.emoji })
        assertEquals(listOf("alice", "bob"), details.reactionsByEmoji[0].reactions.map { it.authorName })
        assertEquals(true, details.reactionsByEmoji[1].reactions.single().isUser)
    }

    private fun participant(id: String) =
        ChatParticipantUiModel(id, id, detail = "", avatar = AvatarUiModel.Name(id, AvatarColorScheme.entries.first()))
}
