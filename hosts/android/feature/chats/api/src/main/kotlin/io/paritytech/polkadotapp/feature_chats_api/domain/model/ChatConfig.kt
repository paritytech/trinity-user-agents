package io.paritytech.polkadotapp.feature_chats_api.domain.model

data class ChatConfig(
    val showAvatar: Boolean,
    val showTimestamps: Boolean,
    val showNewMessagesSeparator: Boolean,
) {
    companion object {
        val Default = ChatConfig(
            showAvatar = true,
            showTimestamps = true,
            showNewMessagesSeparator = true,
        )
    }
}
