package io.paritytech.polkadotapp.feature_chats_api.presentation.model

import androidx.compose.runtime.Immutable

@Immutable
data class ChatMessageGrouping(
    val isTopAttached: Boolean,
    val isBottomAttached: Boolean
) {
    companion object {
        val Standalone = ChatMessageGrouping(isTopAttached = false, isBottomAttached = false)
    }
}
