package io.paritytech.polkadotapp.feature_chats_api.domain.model

data class ChatRequest(
    val welcomeMessageId: ChatMessageId,
    val timestamp: Long,
    val direction: Direction,
    val status: Status,
    val delivery: Delivery,
) {
    val id: ChatRequestId = welcomeMessageId

    enum class Direction {
        INCOMING,
        OUTGOING
    }

    enum class Status {
        PENDING,
        ACCEPTED,
        DECLINED
    }

    sealed interface Delivery {
        /** Recorded locally, not yet on the statement store. */
        data object Undelivered : Delivery

        /** Can never be delivered, e.g. too large for any statement account. */
        data object Failed : Delivery

        /** On the statement store, signed by an account linkable to us (incoming and legacy requests too). */
        data object Delivered : Delivery

        /** On the statement store, signed by the per-[period] notification account; needs renewal each period. */
        data class DeliveredAnonymously(val period: UInt) : Delivery
    }
}

fun ChatRequest.isOutgoing(): Boolean {
    return direction == ChatRequest.Direction.OUTGOING
}

fun ChatRequest.isIncoming(): Boolean {
    return direction == ChatRequest.Direction.INCOMING
}

fun ChatRequest.userDeclinedIncomingRequest(): Boolean {
    return isIncoming() && status == ChatRequest.Status.DECLINED
}

fun ChatRequest.isPendingIncoming(): Boolean {
    return isIncoming() && status == ChatRequest.Status.PENDING
}

fun ChatRequest.isPendingOutgoing(): Boolean {
    return isOutgoing() && status == ChatRequest.Status.PENDING
}
