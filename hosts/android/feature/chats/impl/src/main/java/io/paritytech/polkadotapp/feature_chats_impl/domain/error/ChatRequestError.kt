package io.paritytech.polkadotapp.feature_chats_impl.domain.error

sealed class ChatRequestError(message: String) : Throwable(message) {
    data object ContactNotFound : ChatRequestError("contact is not present in local storage")

    class Unknown(override val cause: Throwable) : ChatRequestError("failed to answer the chat request")
}

fun Throwable.asChatRequestError(): ChatRequestError = this as? ChatRequestError ?: ChatRequestError.Unknown(this)
