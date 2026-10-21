package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatReactionDetailModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.EmojiReactionGroup
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessagePopUpUiState
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ReactionDetail
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import kotlinx.collections.immutable.toImmutableList

/** How a long press on the reactions of a message shows who reacted (02-DESIGN §6.5, scenario S32). */
enum class ReactionDetailsRoute {
    /** The host lists the reactions that it keeps. */
    LOCAL_ROWS,

    /** The host asks the product with `reactions_query`. */
    PRODUCT,

    /** The room does not offer who reacted. */
    NONE,
}

/**
 * A person chat and a room that the host stores list the reactions that the host keeps. A room that declares
 * `reaction_details` asks its product, until the product answers `NotAvailable`. A room that the product keeps and
 * that declares nothing offers no list (as iOS).
 */
internal fun ChatRoomPolicy?.reactionDetailsRoute(productAnsweredNotAvailable: Boolean): ReactionDetailsRoute = when {
    this == null -> ReactionDetailsRoute.LOCAL_ROWS
    offersReactionDetails -> if (productAnsweredNotAvailable) ReactionDetailsRoute.NONE else ReactionDetailsRoute.PRODUCT
    messageStore == ChatRoomPolicy.MessageStore.HOST -> ReactionDetailsRoute.LOCAL_ROWS
    else -> ReactionDetailsRoute.NONE
}

/** The reactions grouped by emoji, in the order of the product, with the participants that the room knows. */
internal fun List<ChatReactionDetailModel>.toReactionDetails(
    messageId: ChatMessageId,
    timestamp: Long,
    participants: Map<String, ChatParticipantUiModel>,
): MessagePopUpUiState.ReactionsDetails {
    val groups = groupBy { it.emoji }.map { (emoji, reactions) ->
        EmojiReactionGroup(
            emoji = emoji,
            reactions = reactions.map { reaction ->
                val participant = participants.getValue(reaction.participantId)
                ReactionDetail(
                    emoji = emoji,
                    timestamp = timestamp,
                    isUser = reaction.participantId == ProductParticipant.USER,
                    authorName = participant.name,
                    avatarModel = participant.avatar,
                )
            }.toImmutableList(),
        )
    }

    return MessagePopUpUiState.ReactionsDetails(messageId = messageId, reactionsByEmoji = groups.toImmutableList(), totalReactionsCount = size)
}
