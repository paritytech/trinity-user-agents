package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatFooterRenderer
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call.ProductRoomCallUiState

/** What the room screen shows of the parts of a product room that its product declares. */
@Immutable
data class ProductRoomUiState(
    val participants: ProductParticipantsUiState,
    val participantSheet: ParticipantSheetUiState?,
    /** The footer that the product draws in place of the input (`ChatRoomFooter::Custom`), or null. */
    val customFooter: CustomChatFooterRenderer?,
    /** A message of this room speaks, so the footer shows "Stop". */
    val readingAloud: Boolean = false,
    /** The call buttons of the header, the ringing banner and the call bar (07-LATER §11.2). */
    val call: ProductRoomCallUiState = ProductRoomCallUiState.None,
) {
    companion object {
        val None = ProductRoomUiState(
            participants = ProductParticipantsUiState.None,
            participantSheet = null,
            customFooter = null,
        )
    }
}
