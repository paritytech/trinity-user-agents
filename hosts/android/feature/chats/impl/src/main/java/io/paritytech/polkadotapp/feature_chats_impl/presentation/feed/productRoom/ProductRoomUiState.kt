package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatFooterRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call.ProductRoomCallUiState
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.ImmutableMap
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.persistentMapOf

/** What the room screen shows of the parts of a product room that its product declares. */
@Immutable
data class ProductRoomUiState(
    val toolbar: ProductToolbarUiModel,
    val participants: ProductParticipantsUiState,
    val participantSheet: ParticipantSheetUiState?,
    /** The footer that the product draws in place of the input (`ChatRoomFooter::Custom`), or null. */
    val customFooter: CustomChatFooterRenderer?,
    val extras: ProductRoomExtrasUiState,
    /** The thread chips, branch controls and reasoning rows of the messages (07-LATER §9). */
    val decorations: ImmutableMap<ChatMessageId, MessageDecorationUiModel> = persistentMapOf(),
    /** The header and the pinned root of a thread room, or null in another room. */
    val thread: ThreadRoomUiState? = null,
    /** A message of this room speaks, so the footer shows "Stop". */
    val readingAloud: Boolean = false,
    /** The call buttons of the header, the ringing banner and the call bar (07-LATER §11.2). */
    val call: ProductRoomCallUiState = ProductRoomCallUiState.None,
) {
    companion object {
        val None = ProductRoomUiState(
            toolbar = ProductToolbarUiModel(inline = persistentListOf(), overflow = persistentListOf()),
            participants = ProductParticipantsUiState.None,
            participantSheet = null,
            customFooter = null,
            extras = ProductRoomExtrasUiState.None,
        )
    }
}

/** A button of the toolbar of a product room. [icon] is what the image loader draws, or null until it loads. */
@Immutable
data class ProductToolbarItemUiModel(val actionId: String, val label: String, val icon: Any?)

/** The buttons that the bar shows, and the buttons of the overflow menu, in the order of the product. */
@Immutable
data class ProductToolbarUiModel(
    val inline: ImmutableList<ProductToolbarItemUiModel>,
    val overflow: ImmutableList<ProductToolbarItemUiModel>,
) {
    companion object {
        /** The number of buttons that the bar shows (02-DESIGN §6.6). */
        const val INLINE_COUNT = 2
    }
}
