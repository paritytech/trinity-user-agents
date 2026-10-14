package io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot

data class ChatOverlay(
    val renderer: CustomChatOverlayRenderer,
    val ownedFragmentClasses: Set<String>,
)
