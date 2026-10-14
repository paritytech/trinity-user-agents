package io.paritytech.polkadotapp.feature_chats_api.domain.chatRequest

interface ChatRequestServiceCoordinator {
    suspend fun runChatRequestServices()
}
