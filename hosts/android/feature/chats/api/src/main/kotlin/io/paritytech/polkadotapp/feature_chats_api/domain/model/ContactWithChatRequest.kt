package io.paritytech.polkadotapp.feature_chats_api.domain.model

data class ContactWithChatRequest(
    val contact: Contact,
    val pendingChatRequest: ChatRequest?
)

fun ContactWithChatRequest.hasPendingIncomingRequest(): Boolean {
    return pendingChatRequest != null && pendingChatRequest.isPendingIncoming()
}

fun ContactWithChatRequest.hasPendingOutgoingRequest(): Boolean {
    return pendingChatRequest != null && pendingChatRequest.isPendingOutgoing()
}

fun ContactWithChatRequest.hasDeclinedIncomingRequest(): Boolean {
    return pendingChatRequest != null && pendingChatRequest.userDeclinedIncomingRequest()
}
