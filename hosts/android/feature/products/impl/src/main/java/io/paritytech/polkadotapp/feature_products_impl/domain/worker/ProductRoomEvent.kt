package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage

/** What the user did in a product room, as the product receives it (`ChatEvent`). Message ids are product ids. */
sealed interface ProductRoomEvent {
    /** The user posted into a room that the host stores. The host gave [messageId]. */
    data class MessagePosted(val messageId: String, val content: ChatMessage.Content, val replyTo: String?) : ProductRoomEvent

    data class UserEdited(val messageId: String, val content: ChatMessage.Content) : ProductRoomEvent

    data class UserDeleted(val messageId: String) : ProductRoomEvent

    data class UserReacted(val messageId: String, val emoji: String, val placed: Boolean) : ProductRoomEvent

    data class Seen(val messageId: String) : ProductRoomEvent

    /** A press on a button of an Actions message. */
    data class ActionTriggered(val messageId: String, val actionId: String) : ProductRoomEvent

    /** A slash command that the room declares. */
    data class Command(val name: String, val arguments: String) : ProductRoomEvent

    data class MessageActionTriggered(val messageId: String, val actionId: String) : ProductRoomEvent

    data class ParticipantActionTriggered(val participantId: String, val actionId: String) : ProductRoomEvent

    data class ToolbarActionTriggered(val actionId: String) : ProductRoomEvent

    data class SuggestionSelected(val suggestionId: String) : ProductRoomEvent

    data object RoomOpened : ProductRoomEvent

    data object RoomClosed : ProductRoomEvent
}

class UnsupportedRoomEventException(event: ProductRoomEvent) :
    Exception("this product runtime does not take ${event::class.simpleName} events")
