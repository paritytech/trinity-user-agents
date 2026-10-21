package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call.ProductRoomCallAction

/** What the user does to the parts of a product room that its product declares. */
sealed interface ProductRoomAction {
    /** The user confirmed the link of a product action or of a message. */
    data class OpenLink(val url: String) : ProductRoomAction

    /** The user tapped a link in a message. The link opens only after the user confirms it (02-DESIGN §6.1). */
    data class ConfirmLink(val url: String) : ProductRoomAction

    /** Opens the list of the participants of the room. */
    data object ShowParticipants : ProductRoomAction

    /** The user came near the end of the list of participants, or tapped the retry of a page that did not load. */
    data object LoadMoreParticipants : ProductRoomAction

    /** The user picked a participant in the list, or tapped the author of a message. */
    data class SelectParticipant(val participantId: String) : ProductRoomAction

    /** An entry of the sheet of the participant that the room shows. */
    data class ParticipantSheet(val entry: ParticipantSheetEntry) : ProductRoomAction

    /** "Stop" in the footer while a message speaks (07-LATER §9.5). */
    data object StopReadAloud : ProductRoomAction

    /** The user acts on the call of the room (07-LATER §11.2). */
    data class Call(val action: ProductRoomCallAction) : ProductRoomAction
}
