package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy

/** What the room screen shows from the policy of a product room. */
@Immutable
data class ChatRoomUiState(
    /** The description of the room under its name. */
    val subtitle: String?,
    /** Shows while the room has no messages. Null for a person chat, which shows nothing. */
    val emptyPlaceholder: EmptyRoomPlaceholder?,
    /** A tap on a link of a message asks the user to confirm the link first (02-DESIGN §6.1). */
    val confirmsLinks: Boolean,
) {
    companion object {
        val PersonChat = ChatRoomUiState(subtitle = null, emptyPlaceholder = null, confirmsLinks = false)
    }
}

sealed interface EmptyRoomPlaceholder {
    data object HostText : EmptyRoomPlaceholder

    data class ProductText(val text: String) : EmptyRoomPlaceholder
}

fun ChatRoomPolicy?.toRoomUiState(): ChatRoomUiState {
    if (this == null) return ChatRoomUiState.PersonChat

    return ChatRoomUiState(
        subtitle = description?.takeIf { it.isNotBlank() },
        emptyPlaceholder = placeholder?.takeIf { it.isNotBlank() }?.let { EmptyRoomPlaceholder.ProductText(it) } ?: EmptyRoomPlaceholder.HostText,
        confirmsLinks = true,
    )
}
