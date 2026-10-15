package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

/** What the user does to the parts of a product room that its product declares. */
sealed interface ProductRoomAction {
    /** The user confirmed the link of a product action. */
    data class OpenLink(val url: String) : ProductRoomAction

    /** A press on a button of the toolbar. */
    data class Toolbar(val actionId: String) : ProductRoomAction

    /** Opens the list of the participants of the room. */
    data object ShowParticipants : ProductRoomAction

    /** The user came near the end of the list of participants, or tapped the retry of a page that did not load. */
    data object LoadMoreParticipants : ProductRoomAction

    /** The user picked a participant in the list, or tapped the author of a message. */
    data class SelectParticipant(val participantId: String) : ProductRoomAction

    /** An entry of the sheet of the participant that the room shows. */
    data class ParticipantSheet(val entry: ParticipantSheetEntry) : ProductRoomAction
}
