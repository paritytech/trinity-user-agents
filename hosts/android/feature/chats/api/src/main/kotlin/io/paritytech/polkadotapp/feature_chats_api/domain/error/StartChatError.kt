package io.paritytech.polkadotapp.feature_chats_api.domain.error

sealed class StartChatError(message: String) : Throwable(message) {
    data object PeerNotRegistered : StartChatError("peer has no consumer info registered on chain")

    class Unknown(override val cause: Throwable) : StartChatError("failed to resolve chat start data")
}

fun Throwable.asStartChatError(): StartChatError = this as? StartChatError ?: StartChatError.Unknown(this)
